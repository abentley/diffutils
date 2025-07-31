use std::env::ArgsOs;
use std::process::{ExitCode};
use std::iter::Peekable;

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

#[derive(Debug, PartialEq)]
struct Line<T: PartialEq>  {
    line: T,
    versions: MatchingVersions
}

fn match_sequence<'a, T: PartialEq + std::fmt::Debug>(
    myfile: &'a [T], oldfile:  &'a [T], yourfile:  &'a [T]) -> Vec<Line<&'a T>> {
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
                        continue
                    },
                    Left(_) if has_my => MyOld,
                    Left(_) => Old,
                    Both(_,_) if has_my => MyOldYour,
                    Both(_,_) => OldYour,
                };
                eprintln!("ll {:?}", left_lines);
                eprintln!("rl {:?}", right_lines);
                eprintln!("ver {:?}", versions);
                for sides_result in diff::slice(&left_lines, &right_lines) {
                    output.push(match sides_result {
                        Left(line) => Line{line: *line, versions: My},
                        Right(line) => Line{line: *line, versions: Your},
                        Both(line, _) => Line{line: *line, versions: MyYour},
                    });
                }
                left_lines.clear();
                right_lines.clear();
                output.push(Line{line, versions});
                break
            }
        } else {
            left_lines.push(line)
        }
    }
    for result in &mut old_your {
        let Right(x) = result else {
            panic!("Should not have anything other than right lines or we missed a match.  Got{:?}", result);
        };
        right_lines.push(x);
    }
    eprintln!("ll {:?}", left_lines);
    eprintln!("rl {:?}", right_lines);
    for sides_result in diff::slice(&left_lines, &right_lines) {
        output.push(match sides_result {
            Left(line) => Line{line: *line, versions: My},
            Right(line) => Line{line: *line, versions: Your},
            Both(line, _) => Line{line: *line, versions: MyYour},
        });
    }
    output
}

pub fn main(opts: Peekable<ArgsOs>) -> ExitCode {
    let opts: Vec<_> = opts.collect();
    let mut opts_iter = opts.into_iter();
    opts_iter.next();
    let mine = opts_iter.next().unwrap();
    let old = opts_iter.next().unwrap();
    let theirs = &opts_iter.next().unwrap();
    eprintln!("{:?} {:?} {:?}", mine, old, theirs);
    ExitCode::from(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use MatchingVersions::*;

    fn input(ink: &str) -> Vec<char>{
        ink.chars().collect()
    }

    #[test]
    fn test_match_sequence_no_yours() {
        assert_eq!(vec![
            Line {line: &'a', versions: Old},
            Line {line: &'b', versions: MyOld},
            Line {line: &'c', versions: My},
        ], match_sequence(&input("bc"), &input("ab"), &[]))
    }
    #[test]
    fn test_match_sequence() {
        assert_eq!(vec![
            Line {line: &'a', versions: OldYour},
            Line {line: &'b', versions: MyOldYour},
            Line {line: &'c', versions: My},
        ], match_sequence(&input("bc"), &input("ab"), &input("ab")))
    }
    #[test]
    fn test_match_ends_in_your() {
        assert_eq!(vec![
            Line {line: &'a', versions: MyOldYour},
            Line {line: &'b', versions: Your},
            Line {line: &'c', versions: Your},
        ], match_sequence(&input("a"), &input("a"), &input("abc")))
    }
    #[test]
    fn test_match_my_your() {
        assert_eq!(vec![
            Line {line: &'b', versions: MyYour},
            Line {line: &'c', versions: MyOldYour},
        ], match_sequence(&input("bc"), &input("c"), &input("bc")))
    }
}
