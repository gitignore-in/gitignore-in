use crate::script::{Comment, Echo, Gi, Gibo, GitIgnoreIn, GitIgnoreStatement, Meaningless};

pub fn parse_text(text: &str) -> GitIgnoreIn {
    let mut result = Vec::new();
    let lines = text.lines();
    for line in lines {
        let command = parse_line(line);
        result.push(command);
    }
    GitIgnoreIn { content: result }
}

pub fn parse_line(text: &str) -> GitIgnoreStatement {
    // TODO: support quoted string like gibo dump "C++"
    if let Some(stripped) = text.strip_prefix("gibo dump ") {
        return GitIgnoreStatement::Gibo(Gibo::Target(stripped.to_string()));
    }
    if let Some(stripped) = text.strip_prefix("gi ") {
        return GitIgnoreStatement::Gi(Gi::Target(stripped.to_string()));
    }
    if let Some(stripped) = text.strip_prefix("echo ") {
        return GitIgnoreStatement::Echo(Echo::Content(stripped.to_string()));
    }
    if text.starts_with('#') {
        return GitIgnoreStatement::Comment(Comment::Content(text.to_string()));
    }
    GitIgnoreStatement::Meaningless(Meaningless::Content(text.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_text() {
        let text = r#"# comment
function meaningless() { echo "meaningless" }
gibo dump C++
gi C++
echo hello
"#;
        let result = parse_text(text);
        let expected = GitIgnoreIn {
            content: vec![
                GitIgnoreStatement::Comment(Comment::Content("# comment".to_string())),
                GitIgnoreStatement::Meaningless(Meaningless::Content(
                    r#"function meaningless() { echo "meaningless" }"#.to_string(),
                )),
                GitIgnoreStatement::Gibo(Gibo::Target("C++".to_string())),
                GitIgnoreStatement::Gi(Gi::Target("C++".to_string())),
                GitIgnoreStatement::Echo(Echo::Content("hello".to_string())),
            ],
        };
        assert_eq!(result, expected);
    }
}
