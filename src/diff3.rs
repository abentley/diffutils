use std::env::ArgsOs;
use std::ffi::OsString;
use std::fmt::Display;
use std::fs;
use std::io::{BufRead, BufReader, Read};
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

#[derive(Debug, PartialEq)]
struct Lines<T: PartialEq> {
    lines: Vec<T>,
    versions: MatchingVersions,
}

fn group_lines<T: PartialEq>(src: Vec<Line<T>>) -> Vec<Lines<T>> {
    let mut output: Vec<_> = vec![];
    for line in src {
        let mut tcur: Option<&mut Lines<T>> = output.last_mut();
        if let Some(x) = &tcur {
            if line.versions != x.versions {
                tcur = None
            }
        }
        let cur = if let Some(cur) = tcur {
            cur
        } else {
            output.push(Lines {
                versions: line.versions,
                lines: vec![],
            });
            output
                .last_mut()
                .expect("The item we just pushed should still be there.")
        };
        cur.lines.push(line.line);
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
                "Should not have anything other than right lines or we missed a match.  Got{:?}",
                result
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
    IOError(std::io::Error),
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Error {
        Error::IOError(e)
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
            Error::IOError(e) => {
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

fn bsplit(theirs: &OsString) -> Result<Vec<Vec<u8>>, Error> {
    let contents = fs::read(theirs)?;
    Ok(contents
        .split_inclusive(|x| *x == b'\n')
        .map(|x| x.to_owned())
        .collect())
}

fn real_main(opts: Peekable<ArgsOs>) -> Result<(), Error> {
    let opts: Vec<_> = opts.collect();
    let mut opts_iter = opts.into_iter();
    opts_iter.next();
    let mine = next_file(&mut opts_iter)?;
    let old = next_file(&mut opts_iter)?;
    let theirs = next_file(&mut opts_iter)?;
    eprintln!("{:?} {:?} {:?}", mine, old, theirs);
    let mine_lines = bsplit(&mine)?;
    let old_lines = bsplit(&old)?;
    let theirs_lines = bsplit(&theirs)?;
    let matches = match_sequence(&mine_lines, &old_lines, &theirs_lines);
    for match_ in matches {
        eprint!("{}", match_)
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
    fn test_group_lines_consolidates() {
        assert_eq!(
            vec![Lines {
                lines: vec!["a", "b"],
                versions: MyOldYour
            }],
            group_lines(vec![
                Line {
                    line: "a",
                    versions: MyOldYour
                },
                Line {
                    line: "b",
                    versions: MyOldYour
                },
            ])
        )
    }
    #[test]
    fn test_group_lines_different_versions() {
        assert_eq!(
            vec![
                Lines {
                    lines: vec!["a", "b"],
                    versions: MyOldYour
                },
                Lines {
                    lines: vec!["c"],
                    versions: My
                },
            ],
            group_lines(vec![
                Line {
                    line: "a",
                    versions: MyOldYour
                },
                Line {
                    line: "b",
                    versions: MyOldYour
                },
                Line {
                    line: "c",
                    versions: My
                },
            ])
        )
    }
}
