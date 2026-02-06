use std::env::ArgsOs;
use std::ffi::OsString;
use std::fmt::Display;
use std::fs;
use std::io::Write;
use std::iter::Peekable;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::process::ExitCode;
use std::vec::Vec;

#[derive(Copy, Clone, Debug, PartialEq)]
enum MatchingVersions {
    MyOldYour,
    OldYour,
    MyYour,
    MyOld,
    My,
    Old,
    Your,
}

impl MatchingVersions {
    fn as_tuple(&self) -> (bool, bool, bool) {
        use MatchingVersions::*;
        match &self {
            MyOldYour => (true, true, true),
            OldYour => (false, true, true),
            MyYour => (true, false, true),
            MyOld => (true, true, false),
            My => (true, false, false),
            Old => (false, true, false),
            Your => (false, false, true),
        }
    }
}

#[derive(Debug, PartialEq)]
struct Line<T: PartialEq> {
    line: T,
    versions: MatchingVersions,
}

impl Display for Line<&Vec<u8>> {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        let (mine, old, yours) = self.versions.as_tuple();
        let m_s = if mine { "<" } else { " " };
        let o_s = if old { "!" } else { " " };
        let t_s = if yours { ">" } else { " " };
        let l2 = OsString::from_vec(self.line.clone());
        write!(fmt, "{m_s}{o_s}{t_s} {}", l2.to_string_lossy())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum MergeOutcome {
    MyWins,
    YourWins,
    /// A Conflict is either when all sides disagree (an overlap conflict) or when old disagrees
    /// with the other two.
    ConflictOldYours,
    ConflictAll,
    /// All and MineYours represent the same situation but a different output.
    ConflictMineYours,
}

enum Changed {
    Your,
    Mine,
    YourMine,
    // This case is actually as if *old* had changed, but that implies time going backwards.
    YourMineSame,
}

#[derive(Debug)]
struct LineVariants<T: PartialEq> {
    my_lines: Vec<T>,
    old_lines: Vec<T>,
    your_lines: Vec<T>,
}
impl<T: PartialEq> LineVariants<T> {
    fn has_conflict(&self) -> bool {
        self.my_lines != vec![] || self.old_lines != vec![] || self.your_lines != vec![]
    }
    /// Infer which lines changed based on which match each other.
    /// Assumes at least one set of lines doesn't match the other, or why are we even doing this?
    fn infer_changes(&self) -> Changed {
        if self.my_lines == self.old_lines {
            Changed::Your
        } else if self.your_lines == self.old_lines {
            Changed::Mine
        } else if self.your_lines == self.my_lines {
            Changed::YourMineSame
        } else {
            Changed::YourMine
        }
    }
    /// Based on what changed and what resolution we're doing, choose a merge outcome.
    fn calculate_merge(&self, concrete: ConcreteResolution) -> MergeOutcome {
        use MergeOutcome::*;
        match self.infer_changes() {
            Changed::Your => concrete.only_your,
            Changed::Mine => MyWins,
            Changed::YourMineSame => concrete.conflict,
            Changed::YourMine => concrete.overlap,
        }
    }
    fn as_ref(&self) -> LineVariants<&T> {
        LineVariants::<&T> {
            my_lines: self.my_lines.iter().collect(),
            old_lines: self.old_lines.iter().collect(),
            your_lines: self.your_lines.iter().collect(),
        }
    }
}

impl<T: AsRef<Vec<u8>> + PartialEq> LineVariants<T> {
    fn dump(
        &self,
        labels: &MergeLabels,
        merge_outcome: MergeOutcome,
        stdout: &mut impl Write,
    ) -> Result<(), std::io::Error> {
        use MergeOutcome::*;
        match merge_outcome {
            MyWins => {
                for line in &self.my_lines {
                    stdout.write_all(line.as_ref())?;
                }
            }
            ConflictOldYours => write_conflict(
                (&self.old_lines, &labels.old),
                None,
                (&self.your_lines, &labels.yours),
                stdout,
            )?,
            ConflictMineYours => write_conflict(
                (&self.my_lines, &labels.mine),
                None,
                (&self.your_lines, &labels.yours),
                stdout,
            )?,
            ConflictAll => write_conflict(
                (&self.my_lines, &labels.mine),
                Some((&self.old_lines, &labels.old)),
                (&self.your_lines, &labels.yours),
                stdout,
            )?,
            YourWins => {
                for line in &self.your_lines {
                    stdout.write_all(line.as_ref())?;
                }
            }
        };
        Ok(())
    }
}

#[derive(Debug)]
struct MergeLines<T: PartialEq> {
    common_lines: Vec<T>,
    variants: LineVariants<T>,
}

impl<T: PartialEq> MergeLines<T> {
    fn new() -> Self {
        Self {
            common_lines: vec![],
            variants: LineVariants::<T> {
                my_lines: vec![],
                old_lines: vec![],
                your_lines: vec![],
            },
        }
    }
    fn as_ref(&self) -> MergeLines<&T> {
        let ml = MergeLines::<&T> {
            common_lines: self.common_lines.iter().collect(),
            variants: self.variants.as_ref(),
        };
        ml
    }
}

impl<T: AsRef<Vec<u8>> + PartialEq> MergeLines<T> {
    fn dump(
        &self,
        labels: &MergeLabels,
        merge_outcome: MergeOutcome,
        stdout: &mut impl Write,
    ) -> Result<(), std::io::Error> {
        for line in &self.common_lines {
            stdout.write_all(line.as_ref())?;
        }
        if !self.variants.has_conflict() {
            return Ok(());
        }
        self.variants.dump(labels, merge_outcome, stdout)?;
        Ok(())
    }
}

/// Write a series of lines with a prefix.
fn write_lines<T: AsRef<Vec<u8>>>(
    lines: &Vec<T>,
    prefix: &[u8],
    output: &mut impl Write,
) -> Result<(), std::io::Error> {
    for line in lines {
        output.write_all(prefix)?;
        output.write_all(line.as_ref())?;
    }
    Ok(())
}

fn write_conflict<T: AsRef<Vec<u8>>>(
    first: (&Vec<T>, &OsString),
    middle: Option<(&Vec<T>, &OsString)>,
    last: (&Vec<T>, &OsString),
    stdout: &mut impl Write,
) -> Result<(), std::io::Error> {
    stdout.write_all(b"<<<<<<< ")?;
    stdout.write_all(first.1.as_bytes())?;
    stdout.write_all(b"\n")?;
    write_lines(first.0, b"", stdout)?;
    if let Some(middle) = middle {
        stdout.write_all(b"||||||| ")?;
        stdout.write_all(middle.1.as_bytes())?;
        stdout.write_all(b"\n")?;
        write_lines(middle.0, b"", stdout)?;
    }
    stdout.write_all(b"=======\n")?;
    write_lines(last.0, b"", stdout)?;
    stdout.write_all(b">>>>>>> ")?;
    stdout.write_all(last.1.as_bytes())?;
    stdout.write_all(b"\n")?;
    Ok(())
}

fn make_merged<T: PartialEq + Copy>(lines: Vec<Line<T>>) -> Vec<MergeLines<T>> {
    use MatchingVersions::*;
    let mut output: Vec<_> = vec![];
    for line in lines {
        let mut cur: Option<&mut MergeLines<T>> = output.last_mut();
        if let Some(ref lcur) = cur {
            if line.versions == MyOldYour && lcur.variants.has_conflict() {
                cur = None;
            }
        }
        if cur.is_none() {
            output.push(MergeLines::new());
            cur = output.last_mut();
        }
        let cur: &mut MergeLines<T> = cur.expect("There must be something by now.");
        if line.versions == MyOldYour {
            cur.common_lines.push(line.line);
        } else {
            let (my_b, old_b, your_b) = line.versions.as_tuple();
            if my_b {
                cur.variants.my_lines.push(line.line)
            }
            if old_b {
                cur.variants.old_lines.push(line.line)
            }
            if your_b {
                cur.variants.your_lines.push(line.line)
            }
        }
    }
    output
}

fn match_sequence<'a, T: PartialEq + std::fmt::Debug>(
    myfile: &'a [T],
    oldfile: &'a [T],
    yourfile: &'a [T],
) -> Vec<Line<&'a T>> {
    use diff::Result::*;
    use MatchingVersions::*;
    let mut output = vec![];
    let mut old_your = diff::slice(oldfile, yourfile).into_iter();
    let mut left_lines = vec![];
    let mut right_lines = vec![];
    for result in diff::slice(myfile, oldfile) {
        let (maybe_combiner, line) = match result {
            Left(line) => (None, line),
            Right(line) => (Some(false), line),
            Both(line, _) => (Some(true), line),
        };
        if let Some(has_my) = maybe_combiner {
            for r_line in &mut old_your {
                let versions = match r_line {
                    Right(x) => {
                        right_lines.push(x);
                        continue;
                    }
                    Left(_) if has_my => MyOld,
                    Left(_) => Old,
                    Both(_, _) if has_my => MyOldYour,
                    Both(_, _) => OldYour,
                };
                for sides_result in diff::slice(&left_lines, &right_lines) {
                    output.push(match sides_result {
                        Left(line) => Line {
                            line: *line,
                            versions: My,
                        },
                        Right(line) => Line {
                            line: *line,
                            versions: Your,
                        },
                        Both(line, _) => Line {
                            line: *line,
                            versions: MyYour,
                        },
                    });
                }
                left_lines.clear();
                right_lines.clear();
                output.push(Line { line, versions });
                break;
            }
        } else {
            left_lines.push(line)
        }
    }
    for result in &mut old_your {
        let Right(x) = result else {
            panic!(
                "Should not have anything other than right lines or we missed a match.  Got{result:?}",
            );
        };
        right_lines.push(x);
    }
    for sides_result in diff::slice(&left_lines, &right_lines) {
        output.push(match sides_result {
            Left(line) => Line {
                line: *line,
                versions: My,
            },
            Right(line) => Line {
                line: *line,
                versions: Your,
            },
            Both(line, _) => Line {
                line: *line,
                versions: MyYour,
            },
        });
    }
    output
}

enum Error {
    MissingOperand,
    NoSuch(OsString),
    IO(std::io::Error),
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Error {
        Error::IO(e)
    }
}

impl Display for Error {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        match &self {
            Error::MissingOperand => {
                write!(fmt, "missing operand")?;
            }
            Error::NoSuch(file) => {
                write!(fmt, "{file:?}: No such file or directory")?;
            }
            Error::IO(e) => {
                e.fmt(fmt)?;
            }
        }
        Ok(())
    }
}

fn next_file<T: Iterator<Item = OsString>>(opts_iter: &mut T) -> Result<OsString, Error> {
    let Some(x) = opts_iter.next() else {
        return Err(Error::MissingOperand);
    };
    Ok(x)
}

fn split(contents: &[u8]) -> Vec<Vec<u8>> {
    contents
        .split_inclusive(|x| *x == b'\n')
        .map(|x| x.to_owned())
        .collect()
}

fn bsplit(filename: &OsString) -> Result<Vec<Vec<u8>>, Error> {
    let contents = fs::read(filename)?;
    Ok(split(&contents))
}

struct MergeLabels {
    mine: OsString,
    old: OsString,
    yours: OsString,
}

#[derive(Copy, Clone)]
enum Resolution {
    PickNonOverlap,              // -3, incorporate non-overlap conflicts
    BracketOverlap,              // -E, incorporate conflicts, but bracket overlaps
    PickYour,    // -e, incorporate all changes from your, including overlapped changes.
    PickOverlap, // -x
    PickOverlapBracketConflicts, // -X
    BracketAll,  // -A, incorporate changes from your, but bracket all conflicts
}

#[derive(Copy, Clone)]
struct ConcreteResolution {
    only_your: MergeOutcome,
    conflict: MergeOutcome,
    overlap: MergeOutcome,
}

impl From<Resolution> for ConcreteResolution {
    fn from(resolution: Resolution) -> ConcreteResolution {
        use MergeOutcome::*;
        use Resolution::*;
        let only_your = match resolution {
            PickOverlap | PickOverlapBracketConflicts => MyWins,
            _ => YourWins,
        };
        let conflict = match resolution {
            PickOverlap => MyWins,
            BracketOverlap | PickNonOverlap | PickYour | PickOverlapBracketConflicts => YourWins,
            BracketAll => ConflictOldYours,
        };
        let overlap = match resolution {
            PickNonOverlap => MyWins,
            PickOverlap | PickOverlapBracketConflicts | PickYour => YourWins,
            BracketOverlap => ConflictMineYours,
            BracketAll => ConflictAll,
        };
        ConcreteResolution {
            only_your,
            conflict,
            overlap,
        }
    }
}

enum Operation {
    Normal,
    Ed(Resolution, bool),
    Merge(Resolution),
}

fn operation(resolution: Resolution, merge: bool, wq: bool) -> Operation {
    if merge {
        Operation::Merge(resolution)
    } else {
        Operation::Ed(resolution, wq)
    }
}

fn load(
    mut opts_iter: impl Iterator<Item = OsString>,
) -> Result<(MergeLabels, LineVariants<Vec<u8>>, Operation), Error> {
    let mut show_overlap = false;
    let mut ed = false;
    let mut merge = false;
    let mut show_all = false;
    let mut overlap_only = false;
    let mut overlap_only_bracket = false;
    let mut easy_only = false;
    let mut ed_wq = false;
    let mut file_list = vec![];
    while file_list.len() < 3 {
        let arg = next_file(&mut opts_iter)?;
        match arg.as_bytes() {
            b"-A" | b"--show-all" => {
                show_all = true;
            }
            b"-e" | b"--ed" => {
                ed = true;
            }
            b"-E" | b"--show-overlap" => {
                show_overlap = true;
            }
            b"-m" | b"--merge" => {
                merge = true;
            }
            b"-3" | b"--easy-only" => {
                easy_only = true;
            }
            b"-x" | b"--overlap-only" => {
                overlap_only = true;
            }
            b"-X" => {
                overlap_only_bracket = true;
            }
            b"-i" => {
                ed_wq = true;
            }
            _ => file_list.push(arg),
        }
    }
    let operation = match (
        ed,
        show_all,
        show_overlap,
        overlap_only,
        overlap_only_bracket,
        easy_only,
        merge,
    ) {
        (false, false, false, false, false, false, false) => Operation::Normal,
        (false, false, false, false, false, false, true) => {
            Operation::Merge(Resolution::BracketAll)
        }
        (true, false, false, false, false, false, merge) => {
            operation(Resolution::PickYour, merge, ed_wq)
        }
        (false, true, false, false, false, false, merge) => {
            operation(Resolution::BracketAll, merge, ed_wq)
        }
        (false, false, true, false, false, false, merge) => {
            operation(Resolution::BracketOverlap, merge, ed_wq)
        }
        (false, false, false, true, false, false, merge) => {
            operation(Resolution::PickOverlap, merge, ed_wq)
        }
        (false, false, false, false, true, false, merge) => {
            operation(Resolution::PickOverlapBracketConflicts, merge, ed_wq)
        }
        (false, false, false, false, false, true, merge) => {
            operation(Resolution::PickNonOverlap, merge, ed_wq)
        }
        x => {
            panic!("incompatible options: {x:?}")
        }
    };
    let files = MergeLabels {
        mine: file_list[0].clone(),
        old: file_list[1].clone(),
        yours: file_list[2].clone(),
    };
    let variants = LineVariants::<Vec<u8>> {
        my_lines: bsplit(&files.mine)?,
        old_lines: bsplit(&files.old)?,
        your_lines: bsplit(&files.yours)?,
    };
    Ok((files, variants, operation))
}

fn real_main(opts: Peekable<ArgsOs>) -> Result<(), Error> {
    let opts: Vec<_> = opts.collect();
    let mut opts_iter = opts.into_iter();
    opts_iter.next();
    let (files, variants, operation) = load(opts_iter)?;
    let matches = match_sequence(
        &variants.my_lines,
        &variants.old_lines,
        &variants.your_lines,
    );
    let merged = make_merged(matches);
    let mut stdout = std::io::stdout();
    match operation {
        Operation::Merge(resolution) => {
            write_merge(merged, &files, resolution, &mut stdout)?;
        }
        Operation::Normal => {
            NormalWriter {
                merged: &merged,
                output: &mut stdout,
                my_line_n: 0,
                old_line_n: 0,
                your_line_n: 0,
            }
            .write_normal()?;
        }
        Operation::Ed(resolution) => EdWriter {
            merged: merged,
            output: &mut stdout,
        }
        .write(&files, resolution.into()),
    }
    Ok(())
}

fn write_merge(
    merged: Vec<MergeLines<&Vec<u8>>>,
    labels: &MergeLabels,
    resolution: Resolution,
    stdout: &mut impl Write,
) -> Result<(), std::io::Error> {
    let concrete = resolution.into();
    for match_ in merged {
        match_.dump(labels, match_.variants.calculate_merge(concrete), stdout)?;
    }
    Ok(())
}

struct NormalWriter<'a, T: Write, T1: PartialEq> {
    merged: &'a Vec<MergeLines<&'a T1>>,
    output: T,
    my_line_n: usize,
    old_line_n: usize,
    your_line_n: usize,
}

impl<T: Write, T1: PartialEq + AsRef<Vec<u8>>> NormalWriter<'_, T, T1> {
    /// Write the normal, default diff3 output format.
    fn write_normal(&mut self) -> Result<(), std::io::Error> {
        for match_ in self.merged {
            self.write_merge_lines(match_)?;
            self.my_line_n += match_.variants.my_lines.len();
            self.old_line_n += match_.variants.old_lines.len();
            self.your_line_n += match_.variants.your_lines.len();
        }
        Ok(())
    }
    /// Write the "normal" output format (diff3 default).
    fn write_merge_lines(&mut self, match_: &MergeLines<&'_ T1>) -> Result<(), std::io::Error> {
        self.my_line_n += match_.common_lines.len();
        self.old_line_n += match_.common_lines.len();
        self.your_line_n += match_.common_lines.len();
        if !match_.variants.has_conflict() {
            return Ok(());
        }
        let changes = match_.variants.infer_changes();
        match changes {
            Changed::Mine => {
                self.output.write_all(b"====1\n")?;
            }
            Changed::Your => {
                self.output.write_all(b"====3\n")?;
            }
            Changed::YourMine | Changed::YourMineSame => {
                self.output.write_all(b"====\n")?;
            }
        }
        self.write_header(1, match_.variants.my_lines.len(), self.my_line_n)?;
        if !matches!(changes, Changed::Your) {
            write_lines(&match_.variants.my_lines, b"  ", &mut self.output)?;
        }
        self.write_header(2, match_.variants.old_lines.len(), self.old_line_n)?;
        if !matches!(changes, Changed::Mine) {
            write_lines(&match_.variants.old_lines, b"  ", &mut self.output)?;
        }
        self.write_header(3, match_.variants.your_lines.len(), self.your_line_n)?;
        write_lines(&match_.variants.your_lines, b"  ", &mut self.output)?;
        Ok(())
    }
    /// Write a "normal output" hunk header
    fn write_header(
        &mut self,
        i: usize,
        line_count: usize,
        n: usize,
    ) -> Result<(), std::io::Error> {
        write!(self.output, "{i}:")?;
        match line_count {
            0 => EdOperation::Add(n),
            count => EdOperation::Change(n, count - 1),
        }
        .write_header(&mut self.output)
    }
}

#[derive(Debug, PartialEq)]
enum EdOperation {
    Add(usize),
    Change(usize, usize),
    Delete(usize, usize),
}

impl EdOperation {
    fn write_header(&self, mut output: impl Write) -> Result<(), std::io::Error> {
        use EdOperation::*;
        match &self {
            Add(pos) => {
                writeln!(output, "{pos}a")?;
            }
            Change(pos, 0) => {
                writeln!(output, "{}c", pos + 1)?;
            }
            Change(pos, count) => {
                writeln!(output, "{},{}c", pos + 1, pos + count + 1)?;
            }
            Delete(pos, 0) => {
                writeln!(output, "{}d", pos + 1)?;
            }
            Delete(pos, count) => {
                writeln!(output, "{},{}d", pos + 1, pos + count + 1)?;
            }
        }
        Ok(())
    }
}

struct EdWriter<T: Write, T1: PartialEq> {
    merged: Vec<MergeLines<T1>>,
    output: T,
}

impl<T: Write, T1: PartialEq + AsRef<Vec<u8>>> EdWriter<T, T1> {
    fn write(&mut self, labels: &MergeLabels, concrete: ConcreteResolution) {
        use EdOperation::*;
        use MergeOutcome::*;
        let mut pos: usize = 0;
        for ml in &self.merged {
            pos += ml.common_lines.len();
            let merge = ml.variants.calculate_merge(concrete);
            let (offset, right_lines) = match merge {
                MyWins => (ml.variants.my_lines.len(), 0),
                YourWins => {
                    let your_count = ml.variants.your_lines.len();
                    (your_count, your_count)
                }
                ConflictOldYours => {
                    let con_count = ml.variants.your_lines.len() + ml.variants.old_lines.len() + 3;
                    (con_count, con_count)
                }
                ConflictMineYours => {
                    let con_count = ml.variants.your_lines.len() + ml.variants.my_lines.len() + 3;
                    (con_count, con_count)
                }
                ConflictAll => {
                    let con_count = (ml.variants.your_lines.len()
                        + ml.variants.my_lines.len()
                        + ml.variants.old_lines.len()
                        + 4);
                    (con_count, con_count)
                }
            };
            if !matches!(merge, MyWins) {
                let op = make_operation(pos, ml.variants.my_lines.len(), right_lines == 0);
                op.write_header(&mut self.output);
                ml.variants.dump(labels, merge, &mut self.output);
                writeln!(self.output, ".");
            }
            pos += offset;
        }
    }
}

fn make_operation(pos: usize, left_lines: usize, right_empty: bool) -> EdOperation {
    use EdOperation::*;
    match (left_lines, right_empty) {
        (0, _) => Add(pos),
        (count, true) => Delete(pos, count - 1),
        (count, false) => Change(pos, count - 1),
    }
}

pub fn main(opts: Peekable<ArgsOs>) -> ExitCode {
    if let Err(e) = real_main(opts) {
        eprintln!("{e}");
        ExitCode::from(1)
    } else {
        ExitCode::from(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::indoc;
    use MatchingVersions::*;

    fn input(ink: &str) -> Vec<char> {
        ink.chars().collect()
    }

    #[test]
    fn test_match_sequence_no_yours() {
        assert_eq!(
            vec![
                Line {
                    line: &'a',
                    versions: Old
                },
                Line {
                    line: &'b',
                    versions: MyOld
                },
                Line {
                    line: &'c',
                    versions: My
                },
            ],
            match_sequence(&input("bc"), &input("ab"), &[])
        )
    }
    #[test]
    fn test_match_sequence() {
        assert_eq!(
            vec![
                Line {
                    line: &'a',
                    versions: OldYour
                },
                Line {
                    line: &'b',
                    versions: MyOldYour
                },
                Line {
                    line: &'c',
                    versions: My
                },
            ],
            match_sequence(&input("bc"), &input("ab"), &input("ab"))
        )
    }
    #[test]
    fn test_match_ends_in_your() {
        assert_eq!(
            vec![
                Line {
                    line: &'a',
                    versions: MyOldYour
                },
                Line {
                    line: &'b',
                    versions: Your
                },
                Line {
                    line: &'c',
                    versions: Your
                },
            ],
            match_sequence(&input("a"), &input("a"), &input("abc"))
        )
    }
    #[test]
    fn test_match_my_your() {
        assert_eq!(
            vec![
                Line {
                    line: &'b',
                    versions: MyYour
                },
                Line {
                    line: &'c',
                    versions: MyOldYour
                },
            ],
            match_sequence(&input("bc"), &input("c"), &input("bc"))
        )
    }
    fn make_ml(common: &[u8], my: &[u8], old: &[u8], your: &[u8]) -> MergeLines<Vec<u8>> {
        MergeLines::<Vec<u8>> {
            common_lines: split(common),
            variants: LineVariants::<Vec<u8>> {
                my_lines: split(my),
                old_lines: split(old),
                your_lines: split(your),
            },
        }
    }
    #[test]
    fn dump_conflict_old() {
        let ml = make_ml(b"common\n", b"my\n", b"old\n", b"your\n");
        let mut result = vec![];
        ml.dump(&merge_labels(), MergeOutcome::ConflictAll, &mut result)
            .expect("Succeeds because result is a Vec.");
        assert_eq!(
            String::from_utf8_lossy(&result),
            indoc! {
                "common
                <<<<<<< my_label
                my
                ||||||| old_label
                old
                =======
                your
                >>>>>>> your_label
                "
            }
        );
    }
    #[test]
    fn dump_conflict_no_old() {
        let ml = make_ml(b"common\n", b"my\n", b"old\n", b"your\n");
        let mut result = vec![];
        ml.dump(
            &merge_labels(),
            MergeOutcome::ConflictMineYours,
            &mut result,
        )
        .expect("Succeeds because result is a Vec.");
        assert_eq!(
            String::from_utf8_lossy(&result),
            indoc! {
                "common
                <<<<<<< my_label
                my
                =======
                your
                >>>>>>> your_label
                "
            }
        );
    }
    #[test]
    fn dump_my_wins() {
        let ml = make_ml(b"common\n", b"my\n", b"old\n", b"your\n");
        let mut result = vec![];
        ml.dump(&merge_labels(), MergeOutcome::MyWins, &mut result)
            .expect("Succeeds because result is a Vec.");
        assert_eq!(
            String::from_utf8_lossy(&result),
            indoc! {
                "common
                my
                "
            }
        );
    }
    #[test]
    fn dump_your_wins() {
        let ml = make_ml(b"common\n", b"my\n", b"old\n", b"your\n");
        let mut result = vec![];
        ml.dump(&merge_labels(), MergeOutcome::YourWins, &mut result)
            .expect("Succeeds because result is a Vec.");
        assert_eq!(
            String::from_utf8_lossy(&result),
            indoc! {
                "common
                your
                "
            }
        );
    }
    #[test]
    fn dump_fluke_agreement() {
        let ml = make_ml(b"common\n", b"my\n", b"old\n", b"your\n");
        let mut result = vec![];
        ml.dump(&merge_labels(), MergeOutcome::ConflictOldYours, &mut result)
            .expect("Succeeds because result is a Vec.");
        assert_eq!(
            String::from_utf8_lossy(&result),
            indoc! {
                "common
                <<<<<<< old_label
                old
                =======
                your
                >>>>>>> your_label
                "
            }
        );
    }
    #[test]
    fn dump_fluke_agreement_no_old() {
        let ml = make_ml(b"common\n", b"my\n", b"old\n", b"your\n");
        let mut result = vec![];
        ml.dump(&merge_labels(), MergeOutcome::ConflictOldYours, &mut result)
            .expect("Succeeds because result is a Vec.");
        assert_eq!(
            String::from_utf8_lossy(&result),
            indoc! {
                "common
                <<<<<<< old_label
                old
                =======
                your
                >>>>>>> your_label
                "
            }
        );
    }
    #[test]
    fn calculate_merge() {
        let ml = make_ml(b"common\n", b"a\n", b"b\n", b"c\n");
        assert_eq!(
            ml.variants.calculate_merge(Resolution::BracketAll.into()),
            MergeOutcome::ConflictAll
        );
        let ml = make_ml(b"common\n", b"a\n", b"a\n", b"c\n");
        assert_eq!(
            ml.variants.calculate_merge(Resolution::BracketAll.into()),
            MergeOutcome::YourWins
        );
        let ml = make_ml(b"common\n", b"a\n", b"b\n", b"b\n");
        assert_eq!(
            ml.variants.calculate_merge(Resolution::BracketAll.into()),
            MergeOutcome::MyWins
        );
        let ml = make_ml(b"common\n", b"a\n", b"b\n", b"a\n");
        assert_eq!(
            ml.variants.calculate_merge(Resolution::BracketAll.into()),
            MergeOutcome::ConflictOldYours
        );
        let ml = make_ml(b"common\n", b"a\n", b"a\n", b"a\n");
        assert_eq!(
            ml.variants.calculate_merge(Resolution::BracketAll.into()),
            MergeOutcome::YourWins
        );
    }
    fn merge_labels() -> MergeLabels {
        MergeLabels {
            mine: "my_label".into(),
            old: "old_label".into(),
            yours: "your_label".into(),
        }
    }
    #[test]
    fn normal_writer() {
        let ml = make_ml(b"common\n", b"my\n", b"old\n", b"your\n");
        let ml2 = ml.as_ref();
        let mut result = vec![];
        let merged = vec![ml2];
        let writer = NormalWriter {
            merged: &merged,
            output: &mut result,
            my_line_n: 0,
            old_line_n: 0,
            your_line_n: 0,
        }
        .write_normal();
        assert_eq!(
            String::from_utf8_lossy(&result),
            indoc! {
                "====
                1:2c
                  my
                2:2c
                  old
                3:2c
                  your
                "
            }
        );
    }
    #[test]
    fn normal_writer_range() {
        let ml = make_ml(b"common\n", b"my\nthing\n", b"old\n", b"your\n");
        let ml2 = ml.as_ref();
        let mut result = vec![];
        let merged = vec![ml2];
        let writer = NormalWriter {
            merged: &merged,
            output: &mut result,
            my_line_n: 0,
            old_line_n: 0,
            your_line_n: 0,
        }
        .write_normal();
        assert_eq!(
            String::from_utf8_lossy(&result),
            indoc! {
                "====
                1:2,3c
                  my
                  thing
                2:2c
                  old
                3:2c
                  your
                "
            }
        );
    }
    #[test]
    fn normal_writer_add() {
        let ml = make_ml(b"common\n", b"", b"old\n", b"your\n");
        let ml2 = ml.as_ref();
        let mut result = vec![];
        let merged = vec![ml2];
        let writer = NormalWriter {
            merged: &merged,
            output: &mut result,
            my_line_n: 0,
            old_line_n: 0,
            your_line_n: 0,
        }
        .write_normal();
        assert_eq!(
            String::from_utf8_lossy(&result),
            indoc! {
                "====
                1:1a
                2:2c
                  old
                3:2c
                  your
                "
            }
        );
    }
    #[test]
    fn test_write_header_add() {
        let mut result = vec![];
        EdOperation::Add(1).write_header(&mut result);
        assert_eq!(String::from_utf8_lossy(&result), "1a\n");
    }
    #[test]
    fn test_write_header_one_change() {
        let mut result = vec![];
        EdOperation::Change(1, 0).write_header(&mut result);
        assert_eq!(String::from_utf8_lossy(&result), "2c\n");
    }
    #[test]
    fn test_write_header_range_change() {
        let mut result = vec![];
        EdOperation::Change(1, 1).write_header(&mut result);
        assert_eq!(String::from_utf8_lossy(&result), "2,3c\n");
    }
    #[test]
    fn test_write_header_one_delete() {
        let mut result = vec![];
        EdOperation::Delete(1, 0).write_header(&mut result);
        assert_eq!(String::from_utf8_lossy(&result), "2d\n");
    }
    #[test]
    fn test_write_header_range_delete() {
        let mut result = vec![];
        EdOperation::Delete(1, 1).write_header(&mut result);
        assert_eq!(String::from_utf8_lossy(&result), "2,3d\n");
    }
    #[test]
    fn test_make_operation_add() {
        assert_eq!(make_operation(5, 0, false), EdOperation::Add(5));
    }
    #[test]
    fn test_make_operation_delete() {
        assert_eq!(make_operation(5, 4, true), EdOperation::Delete(5, 3));
    }
    #[test]
    fn test_make_operation_change() {
        assert_eq!(make_operation(5, 4, false), EdOperation::Change(5, 3));
    }
    #[test]
    fn ed_writer_aab() {
        let ml = make_ml(b"common\n", b"a\n", b"a\n", b"b\n");
        let ml2 = ml.as_ref();
        let mut result = vec![];
        let merged = vec![ml2];
        let writer = EdWriter {
            merged: merged,
            output: &mut result,
        }
        .write(&merge_labels(), Resolution::BracketAll.into());
        assert_eq!(
            String::from_utf8_lossy(&result),
            indoc! {
                "2c
                  b
                  .
                "
            }
        );
    }
    #[test]
    fn ed_writer_range_aab() {
        let ml = make_ml(b"common\n", b"a\na\n", b"a\na\n", b"b\n");
        let ml2 = ml.as_ref();
        let mut result = vec![];
        let merged = vec![ml2];
        let writer = EdWriter {
            merged: merged,
            output: &mut result,
        }
        .write(&merge_labels(), Resolution::BracketAll.into());
        assert_eq!(
            String::from_utf8_lossy(&result),
            indoc! {
                "2,3c
                  b
                  .
                "
            }
        );
    }
    #[test]
    fn ed_writer_aa_null() {
        let ml = make_ml(b"common\n", b"a\n", b"a\n", b"");
        let ml2 = ml.as_ref();
        let mut result = vec![];
        let merged = vec![ml2];
        let writer = EdWriter {
            merged: merged,
            output: &mut result,
        }
        .write(&merge_labels(), Resolution::BracketAll.into());
        assert_eq!(String::from_utf8_lossy(&result), "2d\n.\n",);
    }
    #[test]
    fn ed_writer_aa_null_range() {
        let ml = make_ml(b"common\n", b"a\na\n", b"a\na\n", b"");
        let ml2 = ml.as_ref();
        let mut result = vec![];
        let merged = vec![ml2];
        let writer = EdWriter {
            merged: merged,
            output: &mut result,
        }
        .write(&merge_labels(), Resolution::BracketAll.into());
        assert_eq!(String::from_utf8_lossy(&result), "2,3d\n.\n",);
    }
    #[test]
    fn ed_writer_abc() {
        let ml = make_ml(b"common\n", b"a\n", b"b\n", b"c\n");
        let ml2 = ml.as_ref();
        let mut result = vec![];
        let merged = vec![ml2];
        let writer = EdWriter {
            merged: merged,
            output: &mut result,
        }
        .write(&merge_labels(), Resolution::BracketAll.into());
        assert_eq!(
            String::from_utf8_lossy(&result),
            indoc! {
                "2c
                <<<<<<< my_label
                a
                ||||||| old_label
                b
                =======
                c
                >>>>>>> your_label
                .
                "
            }
        );
    }
}
