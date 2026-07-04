/// Always wraps text in single quotes with proper single-quote escaping.
/// Use this when the argument must be quoted unconditionally (e.g. `echo` lines).
pub(crate) fn shell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', r#"'\''"#))
}

/// Returns the minimally-quoted form of text using POSIX rules via shlex.
/// Returns the bare text when no quoting is required, and single-quoted
/// form otherwise. Use this for template targets (gibo/gi).
pub(crate) fn shell_word(text: &str) -> String {
    match shlex::try_quote(text) {
        Ok(quoted) => quoted.into_owned(),
        Err(_) => format!("'{}'", text.replace('\'', r#"'\''"#)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_word_is_unchanged() {
        assert_eq!(shell_word("Rust"), "Rust");
    }

    #[test]
    fn word_with_space_is_single_quoted() {
        assert_eq!(shell_word("Visual Studio"), "'Visual Studio'");
    }

    #[test]
    fn dollar_sign_is_quoted() {
        assert_eq!(shell_word("$VAR"), "'$VAR'");
    }

    #[test]
    fn backtick_is_quoted() {
        assert_eq!(shell_word("`cmd`"), "'`cmd`'");
    }

    #[test]
    fn single_quote_in_text_is_quoted() {
        // shlex uses double-quoting when the text contains a single quote
        assert_eq!(shell_word("it's"), r#""it's""#);
    }
}
