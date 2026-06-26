use std::collections::{BTreeSet, HashMap};

use crate::{
    gi::{gi_command, gi_list},
    gibo::{gibo_command, gibo_list},
    restore,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Candidate {
    pub(crate) command: String,
    pub(crate) lines: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TemplateTargets {
    All,
    Explicit(Vec<String>),
}

#[derive(Debug, Clone)]
pub(crate) struct InferOptions {
    pub(crate) gibo_targets: TemplateTargets,
    pub(crate) gi_targets: TemplateTargets,
    pub(crate) min_overlap: usize,
}

impl Default for InferOptions {
    fn default() -> Self {
        Self {
            gibo_targets: TemplateTargets::All,
            gi_targets: TemplateTargets::All,
            min_overlap: 2,
        }
    }
}

/// Infer a `.gitignore.in` from `text` and also return the raw template
/// content fetched during inference, keyed by target name.
///
/// The returned [`crate::build::TemplateCache`] can be passed to
/// fetched during inference, keyed by target name.
///
/// The returned [`crate::build::TemplateCache`] can be passed to
/// [`crate::build::build`] as a seed so the subsequent build phase reuses
/// the already-fetched content instead of fetching each template a second time.
pub(crate) fn infer_with_cache(
    text: &str,
    options: &InferOptions,
) -> std::io::Result<(String, crate::build::TemplateCache)> {
    let has_explicit_targets = matches!(options.gibo_targets, TemplateTargets::Explicit(_))
        || matches!(options.gi_targets, TemplateTargets::Explicit(_));
    if !has_explicit_targets && restore::looks_generated(text) {
        return Ok((
            restore::restore(text),
            crate::build::TemplateCache::default(),
        ));
    }

    let (candidates, cache) = load_candidates(options)?;
    let inferred = infer_from_candidates(text, &candidates, options.min_overlap);
    Ok((inferred, cache))
}

fn load_candidates(
    options: &InferOptions,
) -> std::io::Result<(Vec<Candidate>, crate::build::TemplateCache)> {
    let mut candidates = Vec::new();
    let mut cache = crate::build::TemplateCache::default();
    let gibo_targets = match &options.gibo_targets {
        TemplateTargets::All => gibo_list()?,
        TemplateTargets::Explicit(targets) => targets.clone(),
    };
    let gi_targets = match &options.gi_targets {
        TemplateTargets::All => gi_list()?,
        TemplateTargets::Explicit(targets) => targets.clone(),
    };

    for target in &gibo_targets {
        let content = gibo_command(target)?;
        cache.gibo.insert(target.clone(), content.clone());
        candidates.push(Candidate {
            command: format!("gibo dump {}", shell_quote_target(target)),
            lines: normalize_content(&content),
        });
    }

    for target in &gi_targets {
        let content = gi_command(target)?;
        cache.gi.insert(target.clone(), content.clone());
        candidates.push(Candidate {
            command: format!("gi {}", shell_quote_target(target)),
            lines: normalize_content(&content),
        });
    }

    candidates.sort_by(|a, b| a.command.cmp(&b.command));
    Ok((candidates, cache))
}

pub(crate) fn infer_from_candidates(
    text: &str,
    candidates: &[Candidate],
    min_overlap: usize,
) -> String {
    let normalized_lines = collect_target_lines(text);
    let mut remaining: BTreeSet<String> = normalized_lines.iter().cloned().collect();
    let mut selected_commands = Vec::new();
    let mut matched_counts = HashMap::new();

    while let Some((best_index, overlap)) =
        choose_best_candidate(candidates, &remaining, min_overlap)
    {
        let candidate = &candidates[best_index];
        if overlap.is_empty() {
            break;
        }

        selected_commands.push(candidate.command.clone());
        for line in overlap {
            remaining.remove(&line);
            *matched_counts.entry(line).or_insert(0usize) += 1;
        }
    }

    let mut result = Vec::new();
    result.extend(selected_commands);
    let residual = residual_lines(text, &mut matched_counts);
    if !result.is_empty() && !residual.is_empty() {
        result.push(String::new());
    }
    result.extend(residual);
    if result.is_empty() {
        return String::new();
    }
    result.join("\n") + "\n"
}

fn choose_best_candidate(
    candidates: &[Candidate],
    remaining: &BTreeSet<String>,
    min_overlap: usize,
) -> Option<(usize, Vec<String>)> {
    let mut best: Option<(usize, Vec<String>, usize)> = None;

    for (index, candidate) in candidates.iter().enumerate() {
        if candidate.lines.is_empty() {
            continue;
        }

        let overlap: Vec<String> = candidate
            .lines
            .iter()
            .filter(|line| remaining.contains(*line))
            .cloned()
            .collect();

        if overlap.len() < min_overlap {
            continue;
        }

        if overlap.len() * 2 < candidate.lines.len() {
            continue;
        }

        match &best {
            None => best = Some((index, overlap, candidate.lines.len())),
            Some((_, best_overlap, best_size)) => {
                let overlap_score = overlap.len() * overlap.len() * *best_size;
                let best_score = best_overlap.len() * best_overlap.len() * candidate.lines.len();
                if overlap_score > best_score
                    || (overlap_score == best_score && overlap.len() > best_overlap.len())
                {
                    best = Some((index, overlap, candidate.lines.len()));
                }
            }
        }
    }

    best.map(|(index, overlap, _)| (index, overlap))
}

fn collect_target_lines(text: &str) -> Vec<String> {
    text.lines().filter_map(normalize_line).collect()
}

fn normalize_content(content: &str) -> BTreeSet<String> {
    content.lines().filter_map(normalize_line).collect()
}

fn normalize_line(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    Some(trimmed.to_string())
}

fn residual_lines(text: &str, matched_counts: &mut HashMap<String, usize>) -> Vec<String> {
    let mut result = Vec::new();

    for line in text.lines() {
        if line.is_empty() {
            result.push(String::new());
            continue;
        }

        if let Some(normalized) = normalize_line(line) {
            if let Some(count) = matched_counts.get_mut(&normalized) {
                if *count > 0 {
                    *count -= 1;
                    continue;
                }
            }

            result.push(format!("echo {}", shell_quote(line)));
            continue;
        }

        result.push(line.to_string());
    }

    while matches!(result.last(), Some(last) if last.is_empty()) {
        result.pop();
    }

    result
}

fn shell_quote_target(text: &str) -> String {
    if text.contains(|c: char| c.is_whitespace() || c == '\'') {
        shell_quote(text)
    } else {
        text.to_string()
    }
}

fn shell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', r#"'\''"#))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(command: &str, lines: &[&str]) -> Candidate {
        Candidate {
            command: command.to_string(),
            lines: lines.iter().map(|line| line.to_string()).collect(),
        }
    }

    #[test]
    fn default_options_select_all_provider_targets_explicitly() {
        let options = InferOptions::default();

        assert_eq!(options.gibo_targets, TemplateTargets::All);
        assert_eq!(options.gi_targets, TemplateTargets::All);
    }

    #[test]
    fn selects_best_covering_templates() {
        let text = "target/\nCargo.lock\nnode_modules/\ndist/\ncustom.log\n";
        let candidates = vec![
            candidate("gibo dump Rust", &["target/", "Cargo.lock", "*.rs.bk"]),
            candidate("gi node", &["node_modules/", "dist/", ".env"]),
            candidate("gibo dump Logs", &["custom.log"]),
        ];

        let inferred = infer_from_candidates(text, &candidates, 2);
        let expected = "gibo dump Rust\ngi node\n\necho 'custom.log'\n";
        assert_eq!(inferred, expected);
    }

    #[test]
    fn keeps_comments_and_unmatched_lines() {
        let text = "# existing comment\nnode_modules/\ndist/\n# keep this too\ncustom.log\n";
        let candidates = vec![candidate("gi node", &["node_modules/", "dist/", ".env"])];

        let inferred = infer_from_candidates(text, &candidates, 2);
        let expected = "gi node\n\n# existing comment\n# keep this too\necho 'custom.log'\n";
        assert_eq!(inferred, expected);
    }

    #[test]
    fn quotes_multiword_gibo_candidate_command() {
        let candidates = vec![candidate("gibo dump 'Visual Studio'", &["*.suo", "*.user"])];
        let text = "*.suo\n*.user\n";
        let inferred = infer_from_candidates(text, &candidates, 2);
        assert_eq!(inferred, "gibo dump 'Visual Studio'\n");
    }

    #[test]
    fn quotes_multiword_gi_candidate_command() {
        let candidates = vec![candidate("gi 'Visual Studio'", &["*.suo", "*.user"])];
        let text = "*.suo\n*.user\n";
        let inferred = infer_from_candidates(text, &candidates, 2);
        assert_eq!(inferred, "gi 'Visual Studio'\n");
    }

    #[test]
    fn falls_back_when_no_candidate_matches() {
        let text = "# comment\ncustom.log\n";
        let inferred = infer_from_candidates(text, &[], 2);
        assert_eq!(inferred, "# comment\necho 'custom.log'\n");
    }

    #[test]
    fn generated_file_is_restored_when_no_explicit_targets() {
        let generated = "# DO NOT EDIT THIS FILE\n\
            # Generated by gitignore.in\n\
            # See https://gitignore.in/\n\
            # Edit .gitignore.in instead of this file\n\
            # Run `gitignore.in` to build .gitignore\n\
            target/\n";
        let (result, _) = infer_with_cache(generated, &InferOptions::default()).unwrap();
        assert_eq!(result, restore::restore(generated));
    }

    #[test]
    fn fast_path_and_full_infer_produce_different_output_for_generated_files() {
        // Verifies that the fast path (restore) and the full inference produce
        // distinct output for generated files, so bypassing the fast path when
        // explicit targets are provided has a visible effect.
        let generated = "# DO NOT EDIT THIS FILE\n\
            # Generated by gitignore.in\n\
            # See https://gitignore.in/\n\
            # Edit .gitignore.in instead of this file\n\
            # Run `gitignore.in` to build .gitignore\n\
            target/\n";
        let infer_result = infer_from_candidates(generated, &[], 2);
        let restore_result = restore::restore(generated);
        assert_ne!(infer_result, restore_result);
    }
}
