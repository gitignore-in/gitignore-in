use std::{collections::HashMap, io};

use crate::{
    gi::gi_list,
    gibo::gibo_list,
    script::{Comment, Echo, Gi, Gibo, GitIgnoreIn, GitIgnoreStatement, Invalid, Meaningless},
    shell::shell_word,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Provider {
    Gibo,
    Gi,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TemplateRef {
    pub(crate) provider: Provider,
    pub(crate) target: String,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Catalog {
    entries: HashMap<String, Vec<TemplateRef>>,
}

impl Catalog {
    pub(crate) fn load() -> io::Result<Self> {
        Self::load_from(gibo_list, gi_list, &mut io::stderr())
    }

    fn load_from(
        load_gibo: impl FnOnce() -> io::Result<Vec<String>>,
        load_gi: impl FnOnce() -> io::Result<Vec<String>>,
        warn: &mut impl io::Write,
    ) -> io::Result<Self> {
        let mut catalog = Self::default();
        let mut errors = Vec::new();

        match load_gibo() {
            Ok(targets) => {
                for target in targets {
                    catalog.insert(TemplateRef {
                        provider: Provider::Gibo,
                        target,
                    });
                }
            }
            Err(error) => errors.push(("gibo", error)),
        }

        match load_gi() {
            Ok(targets) => {
                for target in targets {
                    catalog.insert(TemplateRef {
                        provider: Provider::Gi,
                        target,
                    });
                }
            }
            Err(error) => errors.push(("gitignore.io", error)),
        }

        if catalog.entries.is_empty() && !errors.is_empty() {
            let kind = errors[0].1.kind();
            let details = errors
                .into_iter()
                .map(|(provider, error)| format!("{provider}: {error}"))
                .collect::<Vec<_>>()
                .join("; ");
            return Err(io::Error::new(
                kind,
                format!("Failed to load templates from any provider ({details})"),
            ));
        }

        for (provider, error) in &errors {
            let _ = writeln!(
                warn,
                "warning: failed to load templates from {provider}: {error}"
            );
        }

        Ok(catalog)
    }

    fn insert(&mut self, template: TemplateRef) {
        self.entries
            .entry(normalize_target_key(&template.target))
            .or_default()
            .push(template);
    }

    fn matches(&self, query: &str) -> &[TemplateRef] {
        self.entries
            .get(&normalize_target_key(query))
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub(crate) fn search(&self, queries: &[String]) -> Vec<TemplateRef> {
        let mut results: Vec<TemplateRef> = self
            .entries
            .values()
            .flat_map(|templates| templates.iter().cloned())
            .collect();
        results.sort_by(|left, right| {
            normalize_target_key(&left.target)
                .cmp(&normalize_target_key(&right.target))
                .then(provider_label(left.provider).cmp(provider_label(right.provider)))
                .then(left.target.cmp(&right.target))
        });

        if queries.is_empty() {
            return results;
        }

        let normalized_queries: Vec<String> = queries
            .iter()
            .map(|query| normalize_target_key(query))
            .collect();
        results
            .into_iter()
            .filter(|template| {
                let key = normalize_target_key(&template.target);
                normalized_queries.iter().any(|query| key.contains(query))
            })
            .collect()
    }
}

pub(crate) fn provider_label(provider: Provider) -> &'static str {
    match provider {
        Provider::Gibo => "gibo",
        Provider::Gi => "gi",
    }
}

pub(crate) fn add_templates(
    script: &mut GitIgnoreIn,
    catalog: &Catalog,
    requested: &[String],
) -> std::io::Result<Vec<TemplateRef>> {
    let mut added = Vec::new();

    for query in requested {
        if contains_template(script, query) {
            continue;
        }

        let template = resolve_template(script, catalog, query)?;
        push_template(script, &template);
        added.push(template);
    }

    Ok(added)
}

pub(crate) fn remove_templates(
    script: &mut GitIgnoreIn,
    requested: &[String],
) -> std::io::Result<Vec<TemplateRef>> {
    let requested_keys: Vec<String> = requested
        .iter()
        .map(|query| normalize_target_key(query))
        .collect();
    let missing: Vec<String> = requested
        .iter()
        .filter(|query| !contains_template(script, query))
        .cloned()
        .collect();
    if !missing.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "Template(s) not found in .gitignore.in: {}",
                missing.join(", ")
            ),
        ));
    }
    let mut removed = Vec::new();

    script.content.retain(|statement| {
        if let Some(template) = template_from_statement(statement) {
            if requested_keys
                .iter()
                .any(|query| *query == normalize_target_key(&template.target))
            {
                removed.push(template);
                return false;
            }
        }
        true
    });

    Ok(removed)
}

pub(crate) fn render(script: &GitIgnoreIn) -> String {
    if script.content.is_empty() {
        return String::new();
    }

    let lines: Vec<String> = script
        .content
        .iter()
        .map(|statement| match statement {
            GitIgnoreStatement::Comment(Comment::Content(content)) => content.clone(),
            GitIgnoreStatement::Meaningless(Meaningless::Content(content)) => content.clone(),
            GitIgnoreStatement::Gibo(Gibo::Target(target)) => {
                format!("gibo dump {}", shell_word(target))
            }
            GitIgnoreStatement::Gi(Gi::Target(target)) => format!("gi {}", shell_word(target)),
            GitIgnoreStatement::Echo(Echo::Content(content)) => {
                format!("echo {}", shell_word(content))
            }
            GitIgnoreStatement::Invalid(Invalid::Line { content, .. }) => content.clone(),
        })
        .collect();

    lines.join("\n") + "\n"
}

fn contains_template(script: &GitIgnoreIn, query: &str) -> bool {
    let query = normalize_target_key(query);

    script.content.iter().any(|statement| {
        template_from_statement(statement)
            .map(|template| normalize_target_key(&template.target) == query)
            .unwrap_or(false)
    })
}

fn resolve_template(
    script: &GitIgnoreIn,
    catalog: &Catalog,
    query: &str,
) -> std::io::Result<TemplateRef> {
    let matches = catalog.matches(query);
    if matches.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Template `{query}` was not found in gibo or gitignore.io"),
        ));
    }

    if let Some(existing_provider) = preferred_provider(script) {
        if let Some(template) = matches
            .iter()
            .find(|template| template.provider == existing_provider)
        {
            return Ok(template.clone());
        }
    }

    if let Some(template) = matches
        .iter()
        .find(|template| template.provider == Provider::Gibo)
    {
        return Ok(template.clone());
    }

    Ok(matches[0].clone())
}

fn preferred_provider(script: &GitIgnoreIn) -> Option<Provider> {
    let mut gibo_count = 0usize;
    let mut gi_count = 0usize;

    for statement in &script.content {
        match statement {
            GitIgnoreStatement::Gibo(_) => gibo_count += 1,
            GitIgnoreStatement::Gi(_) => gi_count += 1,
            _ => {}
        }
    }

    match gibo_count.cmp(&gi_count) {
        std::cmp::Ordering::Greater => Some(Provider::Gibo),
        std::cmp::Ordering::Less => Some(Provider::Gi),
        std::cmp::Ordering::Equal => None,
    }
}

fn push_template(script: &mut GitIgnoreIn, template: &TemplateRef) {
    let statement = match template.provider {
        Provider::Gibo => GitIgnoreStatement::Gibo(Gibo::Target(template.target.clone())),
        Provider::Gi => GitIgnoreStatement::Gi(Gi::Target(template.target.clone())),
    };
    script.content.push(statement);
}

fn template_from_statement(statement: &GitIgnoreStatement) -> Option<TemplateRef> {
    match statement {
        GitIgnoreStatement::Gibo(Gibo::Target(target)) => Some(TemplateRef {
            provider: Provider::Gibo,
            target: target.clone(),
        }),
        GitIgnoreStatement::Gi(Gi::Target(target)) => Some(TemplateRef {
            provider: Provider::Gi,
            target: target.clone(),
        }),
        _ => None,
    }
}

fn normalize_target_key(text: &str) -> String {
    text.trim().to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog(entries: &[(Provider, &str)]) -> Catalog {
        let mut catalog = Catalog::default();
        for (provider, target) in entries {
            catalog.insert(TemplateRef {
                provider: *provider,
                target: (*target).to_string(),
            });
        }
        catalog
    }

    #[test]
    fn add_templates_prefers_existing_provider() {
        let catalog = catalog(&[
            (Provider::Gibo, "Rust"),
            (Provider::Gi, "Rust"),
            (Provider::Gi, "node"),
        ]);
        let mut script = GitIgnoreIn {
            content: vec![GitIgnoreStatement::Gi(Gi::Target("TypeScript".to_string()))],
        };

        let added = add_templates(
            &mut script,
            &catalog,
            &["rust".to_string(), "NODE".to_string()],
        )
        .expect("failed to add templates");

        assert_eq!(
            added,
            vec![
                TemplateRef {
                    provider: Provider::Gi,
                    target: "Rust".to_string(),
                },
                TemplateRef {
                    provider: Provider::Gi,
                    target: "node".to_string(),
                }
            ]
        );
    }

    #[test]
    fn add_templates_skips_existing_target_case_insensitively() {
        let catalog = catalog(&[(Provider::Gibo, "Rust")]);
        let mut script = GitIgnoreIn {
            content: vec![GitIgnoreStatement::Gi(Gi::Target("rust".to_string()))],
        };

        let added = add_templates(&mut script, &catalog, &["Rust".to_string()])
            .expect("failed to add templates");

        assert!(added.is_empty());
        assert_eq!(script.content.len(), 1);
    }

    #[test]
    fn remove_templates_drops_matching_targets_from_both_providers() {
        let mut script = GitIgnoreIn {
            content: vec![
                GitIgnoreStatement::Gibo(Gibo::Target("Rust".to_string())),
                GitIgnoreStatement::Gi(Gi::Target("node".to_string())),
                GitIgnoreStatement::Echo(Echo::Content(".env".to_string())),
            ],
        };

        let removed = remove_templates(&mut script, &["RUST".to_string(), "Node".to_string()])
            .expect("failed to remove templates");

        assert_eq!(
            removed,
            vec![
                TemplateRef {
                    provider: Provider::Gibo,
                    target: "Rust".to_string(),
                },
                TemplateRef {
                    provider: Provider::Gi,
                    target: "node".to_string(),
                }
            ]
        );
        assert_eq!(
            script,
            GitIgnoreIn {
                content: vec![GitIgnoreStatement::Echo(Echo::Content(".env".to_string()))]
            }
        );
    }

    #[test]
    fn render_preserves_comments_and_quotes() {
        let script = GitIgnoreIn {
            content: vec![
                GitIgnoreStatement::Comment(Comment::Content("# comment".to_string())),
                GitIgnoreStatement::Meaningless(Meaningless::Content("".to_string())),
                GitIgnoreStatement::Gibo(Gibo::Target("macOS".to_string())),
                GitIgnoreStatement::Echo(Echo::Content("!.env".to_string())),
            ],
        };

        assert_eq!(
            render(&script),
            "# comment\n\ngibo dump macOS\necho '!.env'\n"
        );
    }

    #[test]
    fn add_templates_errors_for_unknown_target() {
        let catalog = catalog(&[(Provider::Gibo, "Rust")]);
        let mut script = GitIgnoreIn { content: vec![] };

        let error = add_templates(&mut script, &catalog, &["unknown".to_string()])
            .expect_err("expected unknown template error");

        assert!(error.to_string().contains("unknown"));
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    }

    #[test]
    fn remove_templates_errors_for_missing_target() {
        let mut script = GitIgnoreIn {
            content: vec![GitIgnoreStatement::Gibo(Gibo::Target("Rust".to_string()))],
        };

        let error = remove_templates(&mut script, &["node".to_string()])
            .expect_err("expected missing template error");

        assert!(error.to_string().contains("node"));
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    }

    #[test]
    fn search_matches_case_insensitive_substrings() {
        let catalog = catalog(&[
            (Provider::Gibo, "Rust"),
            (Provider::Gi, "Node"),
            (Provider::Gibo, "macOS"),
        ]);

        let results = catalog.search(&["os".to_string(), "rust".to_string()]);

        assert_eq!(
            results,
            vec![
                TemplateRef {
                    provider: Provider::Gibo,
                    target: "macOS".to_string(),
                },
                TemplateRef {
                    provider: Provider::Gibo,
                    target: "Rust".to_string(),
                }
            ]
        );
    }

    #[test]
    fn search_without_query_returns_all_templates() {
        let catalog = catalog(&[(Provider::Gi, "Node"), (Provider::Gibo, "Rust")]);

        let results = catalog.search(&[]);

        assert_eq!(
            results,
            vec![
                TemplateRef {
                    provider: Provider::Gi,
                    target: "Node".to_string(),
                },
                TemplateRef {
                    provider: Provider::Gibo,
                    target: "Rust".to_string(),
                }
            ]
        );
    }

    #[test]
    fn load_from_keeps_gibo_entries_when_gi_list_fails() {
        let mut warn = Vec::new();
        let catalog = Catalog::load_from(
            || Ok(vec!["Rust".to_string()]),
            || {
                Err(std::io::Error::new(
                    std::io::ErrorKind::NotConnected,
                    "offline",
                ))
            },
            &mut warn,
        )
        .expect("gibo catalog should be usable when gitignore.io is unavailable");

        assert_eq!(
            catalog.search(&[]),
            vec![TemplateRef {
                provider: Provider::Gibo,
                target: "Rust".to_string(),
            }]
        );
    }

    #[test]
    fn load_from_warns_on_gi_failure_when_gibo_succeeds() {
        let mut warn = Vec::new();
        Catalog::load_from(
            || Ok(vec!["Rust".to_string()]),
            || {
                Err(std::io::Error::new(
                    std::io::ErrorKind::NotConnected,
                    "offline",
                ))
            },
            &mut warn,
        )
        .expect("should succeed");

        let output = String::from_utf8(warn).unwrap();
        assert!(
            output.contains("gitignore.io"),
            "warning should name the failed provider"
        );
        assert!(
            output.contains("offline"),
            "warning should include the error message"
        );
    }

    #[test]
    fn load_from_keeps_gi_entries_when_gibo_list_fails() {
        let mut warn = Vec::new();
        let catalog = Catalog::load_from(
            || {
                Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "missing gibo",
                ))
            },
            || Ok(vec!["Node".to_string()]),
            &mut warn,
        )
        .expect("gitignore.io catalog should be usable when gibo is unavailable");

        assert_eq!(
            catalog.search(&[]),
            vec![TemplateRef {
                provider: Provider::Gi,
                target: "Node".to_string(),
            }]
        );
    }

    #[test]
    fn load_from_warns_on_gibo_failure_when_gi_succeeds() {
        let mut warn = Vec::new();
        Catalog::load_from(
            || {
                Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "missing gibo",
                ))
            },
            || Ok(vec!["Node".to_string()]),
            &mut warn,
        )
        .expect("should succeed");

        let output = String::from_utf8(warn).unwrap();
        assert!(
            output.contains("gibo"),
            "warning should name the failed provider"
        );
        assert!(
            output.contains("missing gibo"),
            "warning should include the error message"
        );
    }

    #[test]
    fn load_from_no_warning_when_both_providers_succeed() {
        let mut warn = Vec::new();
        Catalog::load_from(
            || Ok(vec!["Rust".to_string()]),
            || Ok(vec!["Node".to_string()]),
            &mut warn,
        )
        .expect("should succeed");

        assert!(
            warn.is_empty(),
            "no warning should be emitted when both providers succeed"
        );
    }

    #[test]
    fn load_from_errors_when_all_providers_fail() {
        let mut warn = Vec::new();
        let error = Catalog::load_from(
            || {
                Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "missing gibo",
                ))
            },
            || {
                Err(std::io::Error::new(
                    std::io::ErrorKind::NotConnected,
                    "offline",
                ))
            },
            &mut warn,
        )
        .expect_err("catalog load should fail when no provider can list templates");

        assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
        assert!(error.to_string().contains("gibo: missing gibo"));
        assert!(error.to_string().contains("gitignore.io: offline"));
        assert!(
            warn.is_empty(),
            "no warning should be emitted when all providers fail (error is returned instead)"
        );
    }
}
