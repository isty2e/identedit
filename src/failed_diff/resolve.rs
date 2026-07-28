use crate::changeset::TransformTarget;
use crate::hash::hash_bytes;
use crate::hashline::{HashedLine, LineAnchor, show_hashed_lines};

use super::model::{
    FailedDiffAnalysis, FailedDiffCandidate, FailedDiffCandidatePreview, FailedDiffChange,
    FailedDiffError, FailedDiffMatchSet, FailedDiffOperation, FailedDiffPreviewLine,
    FailedDiffStatus, FailedDiffSummary, ParsedFailedDiff,
};

const PREVIEW_CONTEXT_LINES: usize = 2;
const PREVIEW_MATCHED_LINES: usize = 4;

pub(crate) fn analyze_failed_diff(
    source: &str,
    parsed: ParsedFailedDiff,
) -> Result<FailedDiffAnalysis, FailedDiffError> {
    let source_lines = show_hashed_lines(source);
    let expected_file_hash = hash_bytes(source.as_bytes());
    let mut changes = Vec::with_capacity(parsed.changes.len());
    let mut summary = FailedDiffSummary {
        source_hunks: parsed.source_hunk_count,
        changes: parsed.changes.len(),
        ..FailedDiffSummary::default()
    };

    for (change_index, change) in parsed.changes.into_iter().enumerate() {
        let candidates = if change.old_lines.is_empty() {
            resolve_insertion_candidates(&source_lines, &change, &expected_file_hash)?
        } else {
            resolve_replacement_candidates(&source_lines, &change)?
        };
        let status = match candidates.len() {
            0 => {
                summary.missing += 1;
                FailedDiffStatus::Missing
            }
            1 => {
                summary.unique += 1;
                FailedDiffStatus::Unique
            }
            _ => {
                summary.ambiguous += 1;
                FailedDiffStatus::Ambiguous
            }
        };
        summary.candidates += candidates.len();
        changes.push(FailedDiffMatchSet {
            change_index,
            source_hunk_index: change.source_hunk_index,
            block_index: change.block_index,
            status,
            old_line_count: change.old_lines.len(),
            new_line_count: change.new_lines.len(),
            candidates,
        });
    }

    Ok(FailedDiffAnalysis { changes, summary })
}

fn resolve_replacement_candidates(
    source_lines: &[HashedLine],
    change: &FailedDiffChange,
) -> Result<Vec<FailedDiffCandidate>, FailedDiffError> {
    let source_contents = source_lines
        .iter()
        .map(|line| line.content.as_str())
        .collect::<Vec<_>>();
    let old_lines = change
        .old_lines
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    let starts = find_overlapping_sequences(&source_contents, &old_lines);
    let new_text = change.new_lines.join("\n");

    starts
        .into_iter()
        .enumerate()
        .map(|(candidate_index, start)| {
            let end = start + change.old_lines.len();
            let start_anchor = anchor_for(&source_lines[start])?;
            let end_anchor = if end - start > 1 {
                Some(anchor_for(&source_lines[end - 1])?)
            } else {
                None
            };
            Ok(FailedDiffCandidate {
                candidate_index,
                target: TransformTarget::Line {
                    anchor: start_anchor,
                    end_anchor,
                },
                op: FailedDiffOperation::ReplaceLines {
                    new_text: new_text.clone(),
                },
                preview: build_candidate_preview(source_lines, start, end),
            })
        })
        .collect()
}

fn resolve_insertion_candidates(
    source_lines: &[HashedLine],
    change: &FailedDiffChange,
    expected_file_hash: &crate::hash::ContentHash,
) -> Result<Vec<FailedDiffCandidate>, FailedDiffError> {
    let source_contents = source_lines
        .iter()
        .map(|line| line.content.as_str())
        .collect::<Vec<_>>();
    let before = change
        .before_context
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    let after = change
        .after_context
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();

    let boundaries = insertion_boundaries(&source_contents, &before, &after);
    let new_text = change.new_lines.join("\n");

    boundaries
        .into_iter()
        .enumerate()
        .map(|(candidate_index, boundary)| {
            let (target, op) = if boundary == 0 {
                (
                    TransformTarget::FileStart {
                        expected_file_hash: expected_file_hash.clone(),
                    },
                    FailedDiffOperation::Insert {
                        new_text: new_text.clone(),
                    },
                )
            } else {
                (
                    TransformTarget::Line {
                        anchor: anchor_for(&source_lines[boundary - 1])?,
                        end_anchor: None,
                    },
                    FailedDiffOperation::InsertAfter {
                        text: new_text.clone(),
                    },
                )
            };
            Ok(FailedDiffCandidate {
                candidate_index,
                target,
                op,
                preview: build_candidate_preview(source_lines, boundary, boundary),
            })
        })
        .collect()
}

fn anchor_for(line: &HashedLine) -> Result<LineAnchor, FailedDiffError> {
    LineAnchor::try_new(line.line, line.hash.clone())
        .map_err(|error| FailedDiffError::new(error.to_string()))
}

fn insertion_boundaries(source: &[&str], before: &[&str], after: &[&str]) -> Vec<usize> {
    if source.is_empty() && before.is_empty() && after.is_empty() {
        return vec![0];
    }
    if before.is_empty() && after.is_empty() {
        return Vec::new();
    }

    let mut matches_before = vec![before.is_empty(); source.len() + 1];
    if !before.is_empty() {
        for start in find_overlapping_sequences(source, before) {
            matches_before[start + before.len()] = true;
        }
    }

    let mut matches_after = vec![after.is_empty(); source.len() + 1];
    if !after.is_empty() {
        for start in find_overlapping_sequences(source, after) {
            matches_after[start] = true;
        }
    }

    matches_before
        .into_iter()
        .zip(matches_after)
        .enumerate()
        .filter_map(|(boundary, (before_matches, after_matches))| {
            (before_matches && after_matches).then_some(boundary)
        })
        .collect()
}

fn find_overlapping_sequences(source: &[&str], pattern: &[&str]) -> Vec<usize> {
    if pattern.is_empty() || source.len() < pattern.len() {
        return Vec::new();
    }

    let prefix = build_prefix_table(pattern);
    let mut matches = Vec::new();
    let mut matched = 0usize;

    for (index, line) in source.iter().enumerate() {
        while matched > 0 && pattern[matched] != *line {
            matched = prefix[matched - 1];
        }
        if pattern[matched] == *line {
            matched += 1;
        }
        if matched == pattern.len() {
            matches.push(index + 1 - pattern.len());
            matched = prefix[matched - 1];
        }
    }

    matches
}

fn build_prefix_table(pattern: &[&str]) -> Vec<usize> {
    let mut prefix = vec![0usize; pattern.len()];
    let mut matched = 0usize;

    for index in 1..pattern.len() {
        while matched > 0 && pattern[index] != pattern[matched] {
            matched = prefix[matched - 1];
        }
        if pattern[index] == pattern[matched] {
            matched += 1;
            prefix[index] = matched;
        }
    }

    prefix
}

fn build_candidate_preview(
    source_lines: &[HashedLine],
    start: usize,
    end: usize,
) -> FailedDiffCandidatePreview {
    let before_start = start.saturating_sub(PREVIEW_CONTEXT_LINES);
    let after_end = (end + PREVIEW_CONTEXT_LINES).min(source_lines.len());
    let matched_end = (start + PREVIEW_MATCHED_LINES).min(end);

    FailedDiffCandidatePreview {
        before: preview_lines(&source_lines[before_start..start]),
        matched: preview_lines(&source_lines[start..matched_end]),
        matched_lines_omitted: end.saturating_sub(matched_end),
        after: preview_lines(&source_lines[end..after_end]),
    }
}

fn preview_lines(lines: &[HashedLine]) -> Vec<FailedDiffPreviewLine> {
    lines
        .iter()
        .map(|line| FailedDiffPreviewLine {
            line: line.line,
            content: line.content.clone(),
        })
        .collect()
}
