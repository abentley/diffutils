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
    ConflictMineYours,
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
    fn calculate_merge(&self, concrete: ConcreteResolution) -> MergeOutcome {
        use MergeOutcome::*;
        if self.my_lines == self.old_lines {
            concrete.only_your
        } else if self.your_lines == self.old_lines {
            MyWins
        } else if self.your_lines == self.my_lines {
            concrete.conflict
        } else {
            concrete.overlap
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
            ConflictOldYours =>write_conflict(
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
    fn write_conflict(
        &self,
        labels: &MergeLabels,
        overlap: bool,
        include_old: bool,
        stdout: &mut impl Write,
    ) -> Result<(), std::io::Error> {
        let middle = match include_old && overlap {
            true => Some((&self.old_lines, &labels.old)),
            false => None,
        };
        // For overlap conflicts, the first set of lines is MINE but for other conflicts, it's OLD
        let first = match overlap {
            true => (&self.my_lines, &labels.mine),
            false => (&self.old_lines, &labels.old),
        };
        write_conflict(first, middle, (&self.your_lines, &labels.yours), stdout)
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

fn write_conflict<T: AsRef<Vec<u8>>>(
    first: (&Vec<T>, &OsString),
    middle: Option<(&Vec<T>, &OsString)>,
    last: (&Vec<T>, &OsString),
    stdout: &mut impl Write,
) -> Result<(), std::io::Error> {
    stdout.write_all(b"<<<<<<< ")?;
    stdout.write_all(first.1.as_bytes())?;
    stdout.write_all(b"\n")?;
    for line in first.0 {
        stdout.write_all(line.as_ref())?;
    }
    if let Some(middle) = middle {
        stdout.write_all(b"||||||| ")?;
        stdout.write_all(middle.1.as_bytes())?;
        stdout.write_all(b"\n")?;
        for line in middle.0 {
            stdout.write_all(line.as_ref())?;
        }
    }
    stdout.write_all(b"=======\n")?;
    for line in last.0 {
        stdout.write_all(line.as_ref())?;
    }
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
    Ed(Resolution),
    Merge(Resolution),
}

fn operation(resolution: Resolution, merge: bool) -> Operation {
    if merge {
        Operation::Merge(resolution)
    } else {
        Operation::Ed(resolution)
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
        (true, false, false, false, false, false, merge) => operation(Resolution::PickYour, merge),
        (false, true, false, false, false, false, merge) => {
            operation(Resolution::BracketAll, merge)
        }
        (false, false, true, false, false, false, merge) => {
            operation(Resolution::BracketOverlap, merge)
        }
        (false, false, false, true, false, false, merge) => {
            operation(Resolution::PickOverlap, merge)
        }
        (false, false, false, false, true, false, merge) => {
            operation(Resolution::PickOverlapBracketConflicts, merge)
        }
        (false, false, false, false, false, true, merge) => {
            operation(Resolution::PickNonOverlap, merge)
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
    match operation {
        Operation::Merge(resolution) => {
            write_merge(merged, &files, resolution, &mut std::io::stdout())?;
        }
        Operation::Normal | Operation::Ed(_) => {
            todo!()
        }
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
}
