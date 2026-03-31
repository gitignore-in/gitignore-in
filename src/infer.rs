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

#[derive(Debug, Clone)]
pub(crate) struct InferOptions {
    pub(crate) gibo_targets: Vec<String>,
    pub(crate) gi_targets: Vec<String>,
    pub(crate) min_overlap: usize,
}

impl Default for InferOptions {
    fn default() -> Self {
        Self {
            gibo_targets: Vec::new(),
            gi_targets: Vec::new(),
            min_overlap: 2,
        }
    }
}

pub(crate) fn infer_with_options(text: &str, options: &InferOptions) -> std::io::Result<String> {
    if restore::looks_generated(text) {
        return Ok(restore::restore(text));
    }

    let candidates = load_candidates(options)?;
    Ok(infer_from_candidates(
        text,
        &candidates,
        options.min_overlap,
    ))
}

fn load_candidates(options: &InferOptions) -> std::io::Result<Vec<Candidate>> {
    let mut candidates = Vec::new();
    let gibo_targets = if options.gibo_targets.is_empty() && options.gi_targets.is_empty() {
        gibo_list()?
    } else {
        options.gibo_targets.clone()
    };
    let gi_targets = if options.gi_targets.is_empty() && options.gibo_targets.is_empty() {
        gi_list()?
    } else {
        options.gi_targets.clone()
    };

    for target in &gibo_targets {
        let content = gibo_command(target)?;
        candidates.push(Candidate {
            command: format!("gibo dump {target}"),
            lines: normalize_content(&content),
        });
    }

    for target in &gi_targets {
        let content = gi_command(target)?;
        candidates.push(Candidate {
            command: format!("gi {target}"),
            lines: normalize_content(&content),
        });
    }

    Ok(candidates)
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
    fn falls_back_when_no_candidate_matches() {
        let text = "# comment\ncustom.log\n";
        let inferred = infer_from_candidates(text, &[], 2);
        assert_eq!(inferred, "# comment\necho 'custom.log'\n");
    }
}
