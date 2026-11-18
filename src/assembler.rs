// build .gitignore from .gitignore.in script

use crate::{
    format::{GENERATED_HEADER_LINES, SEPARATOR},
    gi::gi_command,
    gibo::gibo_command,
    script::{Comment, Echo, Gi, Gibo, GitIgnoreIn, GitIgnoreStatement, Invalid, Meaningless},
};

/// Pre-fetched template content that can be reused across phases.
#[derive(Debug, Default)]
pub(crate) struct TemplateCache {
    pub gibo: std::collections::HashMap<String, String>,
    pub gi: std::collections::HashMap<String, String>,
}

const MAX_TEMPLATE_CACHE_BYTES: usize = 200 * 1024 * 1024;
const MAX_GENERATED_OUTPUT_BYTES: usize = 200 * 1024 * 1024;

#[derive(Clone, Copy)]
struct BuildLimits {
    cache_bytes: usize,
    output_bytes: usize,
}

impl Default for BuildLimits {
    fn default() -> Self {
        Self {
            cache_bytes: MAX_TEMPLATE_CACHE_BYTES,
            output_bytes: MAX_GENERATED_OUTPUT_BYTES,
        }
    }
}

fn build_with(
    script: GitIgnoreIn,
    load_gibo: impl Fn(&str) -> std::io::Result<String>,
    load_gi: impl Fn(&str) -> std::io::Result<String>,
    seed: TemplateCache,
) -> std::io::Result<String> {
    build_with_limits(script, load_gibo, load_gi, seed, BuildLimits::default())
}

fn build_with_limits(
    script: GitIgnoreIn,
    load_gibo: impl Fn(&str) -> std::io::Result<String>,
    load_gi: impl Fn(&str) -> std::io::Result<String>,
    seed: TemplateCache,
    limits: BuildLimits,
) -> std::io::Result<String> {
    let mut result = String::new();
    for line in GENERATED_HEADER_LINES {
        append_generated_output(&mut result, line, limits.output_bytes)?;
        append_generated_output(&mut result, "\n", limits.output_bytes)?;
    }
    let mut gibo_cache = seed.gibo;
    let mut gi_cache = seed.gi;
    let mut cache_bytes = template_cache_bytes(&gibo_cache, &gi_cache);
    check_aggregate_size(cache_bytes, limits.cache_bytes, "template cache")?;
    for statement in script.content {
        match statement {
            GitIgnoreStatement::Comment(Comment::Content(c)) => {
                // `c` already contains the leading `#` (e.g. `# my comment`).
                // Prefixing `# ` produces `# # my comment` in the output
                // .gitignore.  This double-hash is the encoding contract that
                // restore() relies on to distinguish user comments from
                // section headers (`# gibo dump …` / `# gi …`).
                append_generated_output(&mut result, &format!("# {c}\n"), limits.output_bytes)?;
            }
            GitIgnoreStatement::Meaningless(Meaningless::Content(m)) => {
                if m.is_empty() {
                    append_generated_output(&mut result, "\n", limits.output_bytes)?;
                }
            }
            GitIgnoreStatement::Gibo(Gibo::Target(target)) => {
                let content = match gibo_cache.entry(target.clone()) {
                    std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
                    std::collections::hash_map::Entry::Vacant(entry) => {
                        let fetched = load_gibo(&target)?;
                        validate_generated_section_content("gibo dump", &target, &fetched)?;
                        cache_bytes = checked_aggregate_size(
                            cache_bytes,
                            target.len().saturating_add(fetched.len()),
                            limits.cache_bytes,
                            "template cache",
                        )?;
                        entry.insert(fetched)
                    }
                };
                append_generated_output(&mut result, &separator(), limits.output_bytes)?;
                append_generated_output(
                    &mut result,
                    &format!("# gibo dump {target}\n"),
                    limits.output_bytes,
                )?;
                push_external_content(&mut result, content, limits.output_bytes)?;
            }
            GitIgnoreStatement::Gi(Gi::Target(target)) => {
                let content = match gi_cache.entry(target.clone()) {
                    std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
                    std::collections::hash_map::Entry::Vacant(entry) => {
                        let fetched = load_gi(&target)?;
                        validate_generated_section_content("gi", &target, &fetched)?;
                        cache_bytes = checked_aggregate_size(
                            cache_bytes,
                            target.len().saturating_add(fetched.len()),
                            limits.cache_bytes,
                            "template cache",
                        )?;
                        entry.insert(fetched)
                    }
                };
                append_generated_output(&mut result, &separator(), limits.output_bytes)?;
                append_generated_output(
                    &mut result,
                    &format!("# gi {target}\n"),
                    limits.output_bytes,
                )?;
                push_external_content(&mut result, content, limits.output_bytes)?;
            }
            GitIgnoreStatement::Echo(Echo::Content(echo)) => {
                if echo.starts_with("# gibo dump ") || echo.starts_with("# gi ") {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        format!(
                            "echo content {echo:?} starts with a reserved section header prefix; \
                             use a different prefix to avoid ambiguity during restore"
                        ),
                    ));
                }
                append_generated_output(&mut result, &separator(), limits.output_bytes)?;
                append_generated_output(&mut result, &format!("{echo}\n"), limits.output_bytes)?;
            }
            GitIgnoreStatement::Invalid(Invalid::Line { reason, .. }) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    reason,
                ));
            }
        }
    }
    Ok(result)
}

pub(crate) fn build(script: GitIgnoreIn) -> std::io::Result<String> {
    build_with(script, gibo_command, gi_command, Default::default())
}

pub(crate) fn build_with_seed(script: GitIgnoreIn, seed: TemplateCache) -> std::io::Result<String> {
    build_with(script, gibo_command, gi_command, seed)
}

fn push_external_content(
    result: &mut String,
    content: &str,
    output_limit: usize,
) -> std::io::Result<()> {
    append_generated_output(result, content, output_limit)?;
    if !content.ends_with('\n') {
        append_generated_output(result, "\n", output_limit)?;
    }
    Ok(())
}

fn separator() -> String {
    format!("{SEPARATOR}\n")
}

fn template_cache_bytes(
    gibo_cache: &std::collections::HashMap<String, String>,
    gi_cache: &std::collections::HashMap<String, String>,
) -> usize {
    gibo_cache
        .iter()
        .chain(gi_cache)
        .fold(0, |total, (target, content)| {
            total.saturating_add(target.len().saturating_add(content.len()))
        })
}

fn append_generated_output(
    result: &mut String,
    content: &str,
    output_limit: usize,
) -> std::io::Result<()> {
    check_aggregate_size(
        result.len().saturating_add(content.len()),
        output_limit,
        "generated output",
    )?;
    result.push_str(content);
    Ok(())
}

fn checked_aggregate_size(
    current: usize,
    added: usize,
    limit: usize,
    subject: &str,
) -> std::io::Result<usize> {
    let total = current.saturating_add(added);
    check_aggregate_size(total, limit, subject)?;
    Ok(total)
}

fn check_aggregate_size(total: usize, limit: usize, subject: &str) -> std::io::Result<()> {
    if total > limit {
        return Err(std::io::Error::other(format!(
            "aggregate {subject} exceeds {limit} bytes"
        )));
    }
    Ok(())
}

fn validate_generated_section_content(
    command: &str,
    target: &str,
    content: &str,
) -> std::io::Result<()> {
    if content.lines().any(|line| line == SEPARATOR) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("{command} {target}: template output contains the reserved section separator"),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::parser::parse_text;

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
        let result = build(result).unwrap();
        assert!(result.contains("# gi C++"));
        assert!(result.contains("# Created by https://www.toptal.com/developers/gitignore/api/C++"));
        assert!(
            result.contains("# Edit at https://www.toptal.com/developers/gitignore?templates=C++")
        );
        assert!(result.contains("# gibo dump C++"));
        assert!(result.contains("# Generated by gibo (https://github.com/simonwhitaker/gibo)"));
        assert!(result.contains("hello"));
    }

    #[test]
    fn empty_lines_pass_through() {
        let script = GitIgnoreIn {
            content: vec![
                GitIgnoreStatement::Comment(crate::script::Comment::Content(
                    "# section".to_string(),
                )),
                GitIgnoreStatement::Meaningless(Meaningless::Content(String::new())),
                GitIgnoreStatement::Echo(Echo::Content("hello".to_string())),
            ],
        };
        let result = build(script).unwrap();
        // The empty Meaningless line becomes a newline in the output.
        assert!(result.contains("\n\n"));
    }

    #[test]
    fn non_empty_meaningless_lines_are_dropped() {
        let script = GitIgnoreIn {
            content: vec![GitIgnoreStatement::Meaningless(Meaningless::Content(
                "function foo() {}".to_string(),
            ))],
        };
        let result = build(script).unwrap();
        assert!(!result.contains("function foo()"));
    }

    #[test]
    fn generated_header_includes_official_site() {
        let result = build(GitIgnoreIn {
            content: Vec::new(),
        })
        .unwrap();
        assert!(result.contains("# See https://gitignore.in/"));
    }

    #[test]
    fn repeated_gi_target_dedup_uses_cache() {
        // Verifies dedup logic offline: the loader is called once, cached content
        // appears twice in the output.
        let text = "gi Rust\ngi Rust\n";
        let script = parse_text(text);
        let result = build_with(
            script,
            |_| Err(std::io::Error::other("no gibo")),
            |target| Ok(format!("# {target} content\n")),
            Default::default(),
        )
        .unwrap();
        assert_eq!(result.matches("# gi Rust").count(), 2);
    }

    #[test]
    fn repeated_gibo_target_dedup_uses_cache() {
        // Verifies dedup logic offline: the loader is called once, cached content
        // appears twice in the output.
        let text = "gibo dump Rust\ngibo dump Rust\n";
        let script = parse_text(text);
        let result = build_with(
            script,
            |target| Ok(format!("# {target} content\n")),
            |_| Err(std::io::Error::other("no gi")),
            Default::default(),
        )
        .unwrap();
        assert_eq!(result.matches("# gibo dump Rust").count(), 2);
    }

    #[test]
    fn cache_is_scoped_to_a_single_build() {
        let gi_loads = std::cell::Cell::new(0);
        for _ in 0..2 {
            let script = parse_text("gi Rust\n");
            build_with(
                script,
                |_| Err(std::io::Error::other("no gibo")),
                |_| {
                    gi_loads.set(gi_loads.get() + 1);
                    Ok("# Rust content\n".to_string())
                },
                Default::default(),
            )
            .unwrap();
        }
        assert_eq!(gi_loads.get(), 2);
    }

    #[test]
    fn build_rejects_seed_cache_over_aggregate_limit() {
        let seed = TemplateCache {
            gibo: std::collections::HashMap::from([("Rust".to_string(), "content".to_string())]),
            gi: Default::default(),
        };
        let err = build_with_limits(
            parse_text(""),
            |_| Err(std::io::Error::other("no gibo")),
            |_| Err(std::io::Error::other("no gi")),
            seed,
            BuildLimits {
                cache_bytes: 10,
                output_bytes: usize::MAX,
            },
        )
        .unwrap_err();

        assert!(err.to_string().contains("aggregate template cache exceeds"));
    }

    #[test]
    fn build_rejects_new_cache_entry_over_aggregate_limit() {
        let err = build_with_limits(
            parse_text("gi Rust\n"),
            |_| Err(std::io::Error::other("no gibo")),
            |_| Ok("content".to_string()),
            Default::default(),
            BuildLimits {
                cache_bytes: 10,
                output_bytes: usize::MAX,
            },
        )
        .unwrap_err();

        assert!(err.to_string().contains("aggregate template cache exceeds"));
    }

    #[test]
    fn build_rejects_output_over_aggregate_limit() {
        let header = build_with_limits(
            parse_text(""),
            |_| Err(std::io::Error::other("no gibo")),
            |_| Err(std::io::Error::other("no gi")),
            Default::default(),
            BuildLimits {
                cache_bytes: usize::MAX,
                output_bytes: usize::MAX,
            },
        )
        .unwrap();
        let err = build_with_limits(
            parse_text("echo hello\n"),
            |_| Err(std::io::Error::other("no gibo")),
            |_| Err(std::io::Error::other("no gi")),
            Default::default(),
            BuildLimits {
                cache_bytes: usize::MAX,
                output_bytes: header.len(),
            },
        )
        .unwrap_err();

        assert!(err
            .to_string()
            .contains("aggregate generated output exceeds"));
    }

    #[test]
    fn gibo_loader_error_propagates() {
        let text = "gibo dump Rust\n";
        let script = parse_text(text);
        let err = build_with(
            script,
            |_| Err(std::io::Error::other("gibo unavailable")),
            |_| Ok(String::new()),
            Default::default(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("gibo unavailable"));
    }

    #[test]
    fn gi_loader_error_propagates() {
        let text = "gi Rust\n";
        let script = parse_text(text);
        let err = build_with(
            script,
            |_| Ok(String::new()),
            |_| Err(std::io::Error::other("gi unavailable")),
            Default::default(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("gi unavailable"));
    }

    #[test]
    #[ignore = "requires network access to gitignore.io and gibo"]
    fn test_repeated_gi_target_dedup() {
        // Same target listed twice: build must succeed and emit both header lines,
        // producing identical content for each occurrence (cache hit on second call).
        let text = "gi C++\ngi C++\n";
        let script = parse_text(text);
        let result = build(script).unwrap();
        assert_eq!(result.matches("# gi C++").count(), 2);
    }

    #[test]
    #[ignore = "requires network access to gitignore.io and gibo"]
    fn test_repeated_gibo_target_dedup() {
        let text = "gibo dump C++\ngibo dump C++\n";
        let script = parse_text(text);
        let result = build(script).unwrap();
        assert_eq!(result.matches("# gibo dump C++").count(), 2);
    }

    #[test]
    fn external_content_without_trailing_newline_stays_line_delimited() {
        let mut result = String::new();
        push_external_content(&mut result, "*.swp", usize::MAX).unwrap();
        result.push_str(&separator());

        assert_eq!(result, "*.swp\n# -----------------------------------------------------------------------------\n");
    }

    #[test]
    fn external_content_with_trailing_newline_is_not_changed() {
        let mut result = String::new();
        push_external_content(&mut result, "*.swp\n", usize::MAX).unwrap();
        result.push_str(&separator());

        assert_eq!(result, "*.swp\n# -----------------------------------------------------------------------------\n");
    }

    #[test]
    fn test_echo_with_gibo_prefix_is_rejected() {
        let text = "echo '# gibo dump Rust'\n";
        let script = parse_text(text);
        let err = build(script).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
        assert!(err.to_string().contains("reserved section header prefix"));
    }

    #[test]
    fn test_echo_with_gi_prefix_is_rejected() {
        let text = "echo '# gi Rust'\n";
        let script = parse_text(text);
        let err = build(script).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
        assert!(err.to_string().contains("reserved section header prefix"));
    }

    #[test]
    fn test_generated_template_content_with_separator_line_is_rejected() {
        let content = format!("first\n{SEPARATOR}\nsecond\n");
        let err = validate_generated_section_content("gibo dump", "Rust", &content).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        assert!(err
            .to_string()
            .contains("template output contains the reserved section separator"));
    }

    #[test]
    fn test_generated_template_content_with_separator_substring_is_allowed() {
        let content = format!("before {SEPARATOR} after\n");
        validate_generated_section_content("gi", "Rust", &content).unwrap();
    }

    #[test]
    fn test_multi_template_gibo_line_is_rejected() {
        let text = "gibo dump Rust macOS\n";
        let script = parse_text(text);
        let err = build(script).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
        assert!(err.to_string().contains("one template per line"));
    }

    #[test]
    fn test_multi_template_gi_line_is_rejected() {
        let text = "gi Rust macOS\n";
        let script = parse_text(text);
        let err = build(script).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
        assert!(err.to_string().contains("one template per line"));
    }
}
