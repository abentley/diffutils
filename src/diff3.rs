use diff::Result as DiffResult;

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

fn match_sequence<'a, T: PartialEq + std::fmt::Debug>(myfile: &'a [T], oldfile:  &'a [T], yourfile:  &'a [T]) -> Vec<Line<&'a T>> {
    use diff::Result::*;
    use MatchingVersions::*;
    let mut output = vec![];
    let mut old_your = diff::slice(oldfile, yourfile).into_iter();
    for result in diff::slice(myfile, oldfile) {
        let (maybe_combiner, line) = match result {
            Left(line) => (None, line),
            Right(line) => (Some(false), line),
            Both(line, _) => (Some(true), line),
        };
        if let Some(combiner) = maybe_combiner {
            for r_line in &mut old_your {
                let versions = match r_line {
                    Right(x) => {
                        output.push(Line{line: x, versions: Your});
                        continue
                    },
                    Left(_) if combiner => MyOld,
                    Left(_) => Old,
                    Both(_,_) if combiner => MyOldYour,
                    Both(_,_) => OldYour,
                };
                output.push(Line{line, versions});
                break
            }
        } else {
            output.push(Line{line, versions: My});
        }
    }
    for result in old_your {
        let Right(line) = result else {
            panic!("Should not have anything other than right lines or we missed a match.  Got{:?}", result);
        };
        output.push(Line{line: line, versions: Your});
    }
    output
}


#[cfg(test)]
mod tests {
    use super::*;
    use diff::Result as DiffResult;
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
}
