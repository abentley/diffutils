use std::env::ArgsOs;
use std::ffi::OsString;
use std::fmt::Display;
use std::fs;
use std::io::Write;
use std::iter::Peekable;
use std::os::unix::ffi::OsStringExt;
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
        let (mine, old, theirs) = self.versions.as_tuple();
        let m_s = if mine { "<" } else { " " };
        let o_s = if old { "!" } else { " " };
        let t_s = if theirs { ">" } else { " " };
        let l2 = OsString::from_vec(self.line.clone());
        write!(fmt, "{m_s}{o_s}{t_s} {}", l2.to_string_lossy())
    }
}

#[derive(Debug)]
struct MergeLines<T: PartialEq> {
    common_lines: Vec<T>,
    my_lines: Vec<T>,
    old_lines: Vec<T>,
    your_lines: Vec<T>,
}

impl<T: PartialEq> MergeLines<T> {
    fn has_conflict(&self) -> bool {
        self.my_lines != vec![] || self.old_lines != vec![] || self.your_lines != vec![]
    }
    fn new() -> Self {
        Self {
            common_lines: vec![],
            my_lines: vec![],
            old_lines: vec![],
            your_lines: vec![],
        }
    }
}

impl MergeLines<&Vec<u8>> {
    fn dump(&self, stdout: &mut impl Write) -> Result<(), std::io::Error> {
        for line in &self.common_lines {
            stdout.write_all(line)?;
        }
        if !self.has_conflict() {
            return Ok(());
        }
        stdout.write_all(b"<<<<<<<\n")?;
        for line in &self.my_lines {
            stdout.write_all(line)?;
        }
        stdout.write_all(b"!!!!!!!\n")?;
        for line in &self.old_lines {
            stdout.write_all(line)?;
        }
        stdout.write_all(b"=======\n")?;
        for line in &self.your_lines {
            stdout.write_all(line)?;
        }
        stdout.write_all(b">>>>>>>\n")?;
        Ok(())
    }
}

fn make_merged<T: PartialEq + Copy>(lines: Vec<Line<T>>) -> Vec<MergeLines<T>> {
    use MatchingVersions::*;
    let mut output: Vec<_> = vec![];
    for line in lines {
        let mut cur: Option<&mut MergeLines<T>> = output.last_mut();
        if let Some(ref lcur) = cur {
            if line.versions == MyOldYour && lcur.has_conflict() {
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
                cur.my_lines.push(line.line)
            }
            if old_b {
                cur.old_lines.push(line.line)
            }
            if your_b {
                cur.your_lines.push(line.line)
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

fn split(contents: &Vec<u8>) -> impl Iterator<Item = &[u8]>{
    contents
        .split_inclusive(|x| *x == b'\n')
}

fn vsplit(contents: &Vec<u8>) -> Vec<Vec<u8>> {
        split(&contents)
        .map(|x| x.to_owned())
        .collect()
}

fn bsplit(theirs: &OsString) -> Result<Vec<Vec<u8>>, Error> {
    let contents = fs::read(theirs)?;
    Ok(vsplit(&contents))
}

fn real_main(opts: Peekable<ArgsOs>) -> Result<(), Error> {
    let opts: Vec<_> = opts.collect();
    let mut opts_iter = opts.into_iter();
    opts_iter.next();
    let mine = next_file(&mut opts_iter)?;
    let old = next_file(&mut opts_iter)?;
    let theirs = next_file(&mut opts_iter)?;
    eprintln!("{mine:?} {old:?} {theirs:?}");
    let mine_lines = bsplit(&mine)?;
    let old_lines = bsplit(&old)?;
    let theirs_lines = bsplit(&theirs)?;
    let matches = match_sequence(&mine_lines, &old_lines, &theirs_lines);
    let merged = make_merged(matches);
    for match_ in merged {
        match_.dump(&mut std::io::stdout())?
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
    use MatchingVersions::*;
    use indoc::indoc;

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
    #[test]
    fn dump_basic(){
        let common_lines:Vec<u8> = b"common\n".to_owned().into_iter().collect();
        let my_lines:Vec<u8> = b"my\n".to_owned().into_iter().collect();
        let old_lines:Vec<u8> = b"old\n".to_owned().into_iter().collect();
        let your_lines:Vec<u8> = b"your\n".to_owned().into_iter().collect();
        let ml = MergeLines::<&Vec<u8>> {
            common_lines: vec![&common_lines],
            my_lines: vec![&my_lines],
            old_lines: vec![&old_lines],
            your_lines: vec![&your_lines],
        };
        let mut result = vec![];
        ml.dump(&mut result).expect("Succeeds because result is a Vec.");
        assert_eq!(String::from_utf8_lossy(&result), String::from(indoc!("
            common
            <<<<<<<
            my
            !!!!!!!
            old
            =======
            your
            >>>>>>>
        ")));
    }
}
