use diff::Result as DiffResult;

#[derive(Debug, PartialEq)]
enum MatchingVersions {
    All,
    MyOld,
    OldYour,
    MyYour,
    My,
    Old,
    Your,
}

#[derive(Debug, PartialEq)]
struct Line<T: PartialEq>  {
    line: T,
    versions: MatchingVersions
}

fn match_sequence<'a, T: PartialEq>(myfile: &'a [T], oldfile:  &'a [T], _yourfile:  &'a [T]) -> Vec<Line<&'a T>> {
    use diff::Result::*;
    use MatchingVersions::*;
    let mut output = vec![];
    for result in diff::slice(myfile, oldfile) {
        output.push(match result {
            Left(line) => Line {line, versions: My},
            Both(line, _) => Line {line, versions: MyOld},
            Right(line) => Line {line, versions: Old},
        })
    }
    output
}


#[cfg(test)]
mod tests {
    use super::*;
    use diff::Result as DiffResult;
    use MatchingVersions::*;
    #[test]
    fn test_match_sequence() {
        assert_eq!(vec![
            Line {line: &"a", versions: Old},
            Line {line: &"b", versions: MyOld},
            Line {line: &"c", versions: My},
        ], match_sequence(&["b", "c"], &["a", "b"], &[]))
    }
}
