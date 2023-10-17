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
        return GitIgnoreStatement::Gibo(Gibo::Target(remove_shell_quote(stripped)));
    }
    if let Some(stripped) = text.strip_prefix("gi ") {
        return GitIgnoreStatement::Gi(Gi::Target(remove_shell_quote(stripped)));
    }
    if let Some(stripped) = text.strip_prefix("echo ") {
        return GitIgnoreStatement::Echo(Echo::Content(remove_shell_quote(stripped)));
    }
    if text.starts_with('#') {
        return GitIgnoreStatement::Comment(Comment::Content(text.to_string()));
    }
    GitIgnoreStatement::Meaningless(Meaningless::Content(text.to_string()))
}

fn remove_shell_quote(text: &str) -> String {
    let split = shlex::split(text);
    if let Some(sp) = split {
        return sp.join(" ");
    }
    text.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_text() {
        let text = r#"# comment
function meaningless() { echo "meaningless" }
gibo dump C++
gibo dump "C++"
gi C++
gi "C++"
echo hello
echo '!.gitignore'
"#;
        let result = parse_text(text);
        let expected = GitIgnoreIn {
            content: vec![
                GitIgnoreStatement::Comment(Comment::Content("# comment".to_string())),
                GitIgnoreStatement::Meaningless(Meaningless::Content(
                    r#"function meaningless() { echo "meaningless" }"#.to_string(),
                )),
                GitIgnoreStatement::Gibo(Gibo::Target("C++".to_string())),
                GitIgnoreStatement::Gibo(Gibo::Target("C++".to_string())),
                GitIgnoreStatement::Gi(Gi::Target("C++".to_string())),
                GitIgnoreStatement::Gi(Gi::Target("C++".to_string())),
                GitIgnoreStatement::Echo(Echo::Content("hello".to_string())),
                GitIgnoreStatement::Echo(Echo::Content("!.gitignore".to_string())),
            ],
        };
        assert_eq!(result, expected);
    }
}
