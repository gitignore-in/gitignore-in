use crate::script::{
    Comment, Echo, Gi, Gibo, GitIgnoreIn, GitIgnoreStatement, Invalid, Meaningless,
};

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
    if let Some(stripped) = text.strip_prefix("gibo dump ") {
        return match parse_template_target("gibo dump", stripped) {
            Ok(target) => GitIgnoreStatement::Gibo(Gibo::Target(target)),
            Err(reason) => GitIgnoreStatement::Invalid(Invalid::Line {
                content: text.to_string(),
                reason,
            }),
        };
    }
    if let Some(stripped) = text.strip_prefix("gi ") {
        return match parse_template_target("gi", stripped) {
            Ok(target) => GitIgnoreStatement::Gi(Gi::Target(target)),
            Err(reason) => GitIgnoreStatement::Invalid(Invalid::Line {
                content: text.to_string(),
                reason,
            }),
        };
    }
    if let Some(stripped) = text.strip_prefix("echo ") {
        return GitIgnoreStatement::Echo(Echo::Content(remove_shell_quote(stripped)));
    }
    if text.starts_with('#') {
        return GitIgnoreStatement::Comment(Comment::Content(text.to_string()));
    }
    GitIgnoreStatement::Meaningless(Meaningless::Content(text.to_string()))
}

fn parse_template_target(command: &str, text: &str) -> Result<String, String> {
    let Some(parts) = shlex::split(text) else {
        return Err(format!("{command} has invalid shell quoting: {text:?}"));
    };

    match parts.as_slice() {
        [target] if !target.is_empty() => Ok(target.clone()),
        [] | [_] => Err(format!("{command} expects one non-empty template name")),
        _ => Err(format!(
            "{command} expects one template per line; found {} template names: {}",
            parts.len(),
            parts.join(", ")
        )),
    }
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

    #[test]
    fn template_commands_reject_multiple_unquoted_targets() {
        assert_eq!(
            parse_line("gibo dump Rust macOS"),
            GitIgnoreStatement::Invalid(Invalid::Line {
                content: "gibo dump Rust macOS".to_string(),
                reason:
                    "gibo dump expects one template per line; found 2 template names: Rust, macOS"
                        .to_string(),
            })
        );
        assert_eq!(
            parse_line("gi Rust macOS"),
            GitIgnoreStatement::Invalid(Invalid::Line {
                content: "gi Rust macOS".to_string(),
                reason: "gi expects one template per line; found 2 template names: Rust, macOS"
                    .to_string(),
            })
        );
    }

    #[test]
    fn template_commands_accept_quoted_single_target() {
        assert_eq!(
            parse_line("gibo dump \"Rust macOS\""),
            GitIgnoreStatement::Gibo(Gibo::Target("Rust macOS".to_string()))
        );
        assert_eq!(
            parse_line("gi \"Rust macOS\""),
            GitIgnoreStatement::Gi(Gi::Target("Rust macOS".to_string()))
        );
    }
}
