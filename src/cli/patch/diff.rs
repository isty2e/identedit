use std::io::IsTerminal;
use std::path::Path;

use crate::changeset::{FileChange, MultiFileChangeset, TextChangePreview};
use crate::error::IdenteditError;
use crate::handle::Span;

use super::ColorMode;

const RESET: &str = "\x1b[0m";
const RED: &str = "\x1b[31m";
const GREEN: &str = "\x1b[32m";
const CYAN: &str = "\x1b[36m";
const MAX_LCS_CELLS: usize = 200_000;

pub(super) fn render_changeset_diff(
    changeset: &MultiFileChangeset,
    color: ColorMode,
) -> Result<String, IdenteditError> {
    let use_color = should_color(color);
    let mut rendered = String::new();

    for file_change in &changeset.files {
        let source = std::fs::read_to_string(&file_change.file)
            .map_err(|error| IdenteditError::io(&file_change.file, error))?;
        render_file_change_diff(&mut rendered, file_change, &source, use_color)?;
    }

    Ok(rendered)
}

pub(super) fn render_file_diff(file: &Path, before: &str, after: &str, color: ColorMode) -> String {
    let use_color = should_color(color);
    let old_line_count = diff_line_count(before);
    let new_line_count = diff_line_count(after);
    let mut rendered = String::new();
    render_diff_header(&mut rendered, file);
    render_hunk_header(
        &mut rendered,
        1,
        old_line_count,
        1,
        new_line_count,
        use_color,
    );
    render_removed_lines(&mut rendered, before, use_color);
    render_added_lines(&mut rendered, after, use_color);
    rendered
}

fn render_file_change_diff(
    rendered: &mut String,
    file_change: &FileChange,
    source: &str,
    use_color: bool,
) -> Result<(), IdenteditError> {
    for operation in &file_change.operations {
        let Some(preview) = operation.text_preview() else {
            return Err(IdenteditError::InvalidRequest {
                message: "--diff currently supports text patch previews only.".to_string(),
            });
        };
        render_text_preview_diff(rendered, &file_change.file, source, preview, use_color)?;
    }
    Ok(())
}

fn render_text_preview_diff(
    rendered: &mut String,
    file: &Path,
    source: &str,
    preview: &TextChangePreview,
    use_color: bool,
) -> Result<(), IdenteditError> {
    let old_text = preview
        .old_text
        .as_deref()
        .ok_or_else(|| IdenteditError::InvalidRequest {
            message: "--diff requires full preview text; retry without compact preview output."
                .to_string(),
        })?;
    let old_start_line = line_number_at_byte(source, preview.matched_span)?;
    let hunks = minimal_diff_hunks(old_text, &preview.new_text);
    if hunks.is_empty() {
        return Ok(());
    }

    render_diff_header(rendered, file);
    for hunk in hunks {
        render_hunk_header(
            rendered,
            old_start_line + hunk.old_start_offset,
            hunk.old_lines.len(),
            old_start_line + hunk.new_start_offset,
            hunk.new_lines.len(),
            use_color,
        );
        render_removed_line_values(rendered, &hunk.old_lines, use_color);
        render_added_line_values(rendered, &hunk.new_lines, use_color);
    }
    Ok(())
}

fn render_diff_header(rendered: &mut String, file: &Path) {
    let label = file.display();
    rendered.push_str(&format!("--- {label}\n+++ {label}\n"));
}

fn render_hunk_header(
    rendered: &mut String,
    old_start: usize,
    old_count: usize,
    new_start: usize,
    new_count: usize,
    use_color: bool,
) {
    let header = format!(
        "@@ -{},{} +{},{} @@",
        old_start, old_count, new_start, new_count
    );
    push_colored_line(rendered, &header, CYAN, use_color);
}

fn render_removed_lines(rendered: &mut String, text: &str, use_color: bool) {
    render_removed_line_values(rendered, &diff_lines(text), use_color);
}

fn render_added_lines(rendered: &mut String, text: &str, use_color: bool) {
    render_added_line_values(rendered, &diff_lines(text), use_color);
}

fn render_removed_line_values(rendered: &mut String, lines: &[String], use_color: bool) {
    for line in lines {
        push_colored_line(rendered, &format!("-{line}"), RED, use_color);
    }
}

fn render_added_line_values(rendered: &mut String, lines: &[String], use_color: bool) {
    for line in lines {
        push_colored_line(rendered, &format!("+{line}"), GREEN, use_color);
    }
}

fn push_colored_line(rendered: &mut String, line: &str, color: &str, use_color: bool) {
    if use_color {
        rendered.push_str(color);
        rendered.push_str(line);
        rendered.push_str(RESET);
    } else {
        rendered.push_str(line);
    }
    rendered.push('\n');
}

fn diff_lines(text: &str) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }

    let mut lines = text.split('\n').collect::<Vec<_>>();
    if text.ends_with('\n') {
        lines.pop();
    }

    lines
        .into_iter()
        .map(|line| line.trim_end_matches('\r').to_string())
        .collect()
}

fn diff_line_count(text: &str) -> usize {
    diff_lines(text).len()
}

#[derive(Debug, PartialEq, Eq)]
struct LineDiffHunk {
    old_start_offset: usize,
    new_start_offset: usize,
    old_lines: Vec<String>,
    new_lines: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
enum LineDiffOp {
    Equal,
    Remove(String),
    Add(String),
}

fn minimal_diff_hunks(old_text: &str, new_text: &str) -> Vec<LineDiffHunk> {
    if old_text == new_text {
        return Vec::new();
    }

    let old_lines = diff_lines(old_text);
    let new_lines = diff_lines(new_text);

    if old_lines == new_lines {
        return vec![LineDiffHunk {
            old_start_offset: 0,
            new_start_offset: 0,
            old_lines,
            new_lines,
        }];
    }

    if old_lines.len().saturating_mul(new_lines.len()) > MAX_LCS_CELLS {
        return single_trimmed_hunk(old_lines, new_lines);
    }

    let ops = line_diff_ops(&old_lines, &new_lines);
    collect_line_diff_hunks(ops)
}

fn single_trimmed_hunk(old_lines: Vec<String>, new_lines: Vec<String>) -> Vec<LineDiffHunk> {
    let common_prefix = old_lines
        .iter()
        .zip(new_lines.iter())
        .take_while(|(old, new)| old == new)
        .count();

    let remaining_old = old_lines.len().saturating_sub(common_prefix);
    let remaining_new = new_lines.len().saturating_sub(common_prefix);
    let max_suffix = remaining_old.min(remaining_new);
    let common_suffix = (0..max_suffix)
        .take_while(|offset| {
            old_lines[old_lines.len() - 1 - offset] == new_lines[new_lines.len() - 1 - offset]
        })
        .count();

    let old_end = old_lines.len() - common_suffix;
    let new_end = new_lines.len() - common_suffix;
    let changed_old = old_lines[common_prefix..old_end].to_vec();
    let changed_new = new_lines[common_prefix..new_end].to_vec();

    if changed_old.is_empty() && changed_new.is_empty() {
        return Vec::new();
    }

    vec![LineDiffHunk {
        old_start_offset: common_prefix,
        new_start_offset: common_prefix,
        old_lines: changed_old,
        new_lines: changed_new,
    }]
}

fn line_diff_ops(old_lines: &[String], new_lines: &[String]) -> Vec<LineDiffOp> {
    let old_len = old_lines.len();
    let new_len = new_lines.len();
    let width = new_len + 1;
    let mut lcs = vec![0usize; (old_len + 1) * width];

    for old_index in (0..old_len).rev() {
        for new_index in (0..new_len).rev() {
            let index = old_index * width + new_index;
            lcs[index] = if old_lines[old_index] == new_lines[new_index] {
                1 + lcs[(old_index + 1) * width + new_index + 1]
            } else {
                lcs[(old_index + 1) * width + new_index].max(lcs[old_index * width + new_index + 1])
            };
        }
    }

    let mut old_index = 0usize;
    let mut new_index = 0usize;
    let mut ops = Vec::new();

    while old_index < old_len || new_index < new_len {
        if old_index < old_len
            && new_index < new_len
            && old_lines[old_index] == new_lines[new_index]
        {
            ops.push(LineDiffOp::Equal);
            old_index += 1;
            new_index += 1;
        } else if old_index < old_len
            && (new_index == new_len
                || lcs[(old_index + 1) * width + new_index]
                    >= lcs[old_index * width + new_index + 1])
        {
            ops.push(LineDiffOp::Remove(old_lines[old_index].clone()));
            old_index += 1;
        } else {
            ops.push(LineDiffOp::Add(new_lines[new_index].clone()));
            new_index += 1;
        }
    }

    ops
}

fn collect_line_diff_hunks(ops: Vec<LineDiffOp>) -> Vec<LineDiffHunk> {
    let mut old_offset = 0usize;
    let mut new_offset = 0usize;
    let mut current = None::<LineDiffHunk>;
    let mut hunks = Vec::new();

    for op in ops {
        match op {
            LineDiffOp::Equal => {
                if let Some(hunk) = current.take() {
                    hunks.push(hunk);
                }
                old_offset += 1;
                new_offset += 1;
            }
            LineDiffOp::Remove(line) => {
                let hunk = current.get_or_insert_with(|| LineDiffHunk {
                    old_start_offset: old_offset,
                    new_start_offset: new_offset,
                    old_lines: Vec::new(),
                    new_lines: Vec::new(),
                });
                hunk.old_lines.push(line);
                old_offset += 1;
            }
            LineDiffOp::Add(line) => {
                let hunk = current.get_or_insert_with(|| LineDiffHunk {
                    old_start_offset: old_offset,
                    new_start_offset: new_offset,
                    old_lines: Vec::new(),
                    new_lines: Vec::new(),
                });
                hunk.new_lines.push(line);
                new_offset += 1;
            }
        }
    }

    if let Some(hunk) = current {
        hunks.push(hunk);
    }

    hunks
}

fn line_number_at_byte(source: &str, span: Span) -> Result<usize, IdenteditError> {
    if span.start > source.len() || !source.is_char_boundary(span.start) {
        return Err(IdenteditError::InvalidRequest {
            message: format!(
                "Cannot render diff: preview span starts at invalid byte offset {}",
                span.start
            ),
        });
    }

    Ok(source[..span.start]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
        + 1)
}

fn should_color(color: ColorMode) -> bool {
    match color {
        ColorMode::Auto => std::io::stdout().is_terminal(),
        ColorMode::Always => true,
        ColorMode::Never => false,
    }
}

#[cfg(test)]
mod tests {
    use super::{LineDiffHunk, diff_line_count, diff_lines, minimal_diff_hunks};

    #[test]
    fn diff_lines_do_not_emit_extra_line_for_trailing_newline() {
        assert_eq!(diff_lines("a\nb\n"), vec!["a", "b"]);
    }

    #[test]
    fn diff_lines_include_final_non_newline_line() {
        assert_eq!(diff_lines("a\nb"), vec!["a", "b"]);
    }

    #[test]
    fn diff_line_count_treats_empty_as_zero() {
        assert_eq!(diff_line_count(""), 0);
        assert_eq!(diff_line_count("a"), 1);
        assert_eq!(diff_line_count("a\nb"), 2);
        assert_eq!(diff_line_count("a\nb\n"), 2);
    }

    #[test]
    fn minimal_diff_hunks_trim_common_prefix_and_suffix() {
        assert_eq!(
            minimal_diff_hunks("a\nold\nz\n", "a\nnew\nz\n"),
            vec![LineDiffHunk {
                old_start_offset: 1,
                new_start_offset: 1,
                old_lines: vec!["old".to_string()],
                new_lines: vec!["new".to_string()],
            }]
        );
    }

    #[test]
    fn minimal_diff_hunks_return_empty_for_identical_text() {
        assert_eq!(minimal_diff_hunks("a\nb\n", "a\nb\n"), Vec::new());
    }

    #[test]
    fn minimal_diff_hunks_do_not_hide_trailing_newline_only_changes() {
        assert_eq!(
            minimal_diff_hunks("a\n", "a"),
            vec![LineDiffHunk {
                old_start_offset: 0,
                new_start_offset: 0,
                old_lines: vec!["a".to_string()],
                new_lines: vec!["a".to_string()],
            }]
        );
    }

    #[test]
    fn minimal_diff_hunks_handle_pure_insertion_between_common_lines() {
        assert_eq!(
            minimal_diff_hunks("a\nz\n", "a\nnew\nz\n"),
            vec![LineDiffHunk {
                old_start_offset: 1,
                new_start_offset: 1,
                old_lines: Vec::new(),
                new_lines: vec!["new".to_string()],
            }]
        );
    }

    #[test]
    fn minimal_diff_hunks_split_separated_line_changes() {
        assert_eq!(
            minimal_diff_hunks("a\nold_a\nkeep\nold_b\nz\n", "a\nnew_a\nkeep\nnew_b\nz\n"),
            vec![
                LineDiffHunk {
                    old_start_offset: 1,
                    new_start_offset: 1,
                    old_lines: vec!["old_a".to_string()],
                    new_lines: vec!["new_a".to_string()],
                },
                LineDiffHunk {
                    old_start_offset: 3,
                    new_start_offset: 3,
                    old_lines: vec!["old_b".to_string()],
                    new_lines: vec!["new_b".to_string()],
                }
            ]
        );
    }

    #[test]
    fn minimal_diff_hunks_split_repeated_line_changes_without_anchor_drift() {
        assert_eq!(
            minimal_diff_hunks(
                "same\nold_a\nsame\nold_b\nsame\n",
                "same\nnew_a\nsame\nnew_b\nsame\n"
            ),
            vec![
                LineDiffHunk {
                    old_start_offset: 1,
                    new_start_offset: 1,
                    old_lines: vec!["old_a".to_string()],
                    new_lines: vec!["new_a".to_string()],
                },
                LineDiffHunk {
                    old_start_offset: 3,
                    new_start_offset: 3,
                    old_lines: vec!["old_b".to_string()],
                    new_lines: vec!["new_b".to_string()],
                }
            ]
        );
    }

    #[test]
    fn minimal_diff_hunks_split_separated_pure_deletions() {
        assert_eq!(
            minimal_diff_hunks("a\ndrop_a\nkeep\ndrop_b\nz\n", "a\nkeep\nz\n"),
            vec![
                LineDiffHunk {
                    old_start_offset: 1,
                    new_start_offset: 1,
                    old_lines: vec!["drop_a".to_string()],
                    new_lines: Vec::new(),
                },
                LineDiffHunk {
                    old_start_offset: 3,
                    new_start_offset: 2,
                    old_lines: vec!["drop_b".to_string()],
                    new_lines: Vec::new(),
                }
            ]
        );
    }

    #[test]
    fn minimal_diff_hunks_split_separated_pure_insertions() {
        assert_eq!(
            minimal_diff_hunks("a\nkeep\nz\n", "a\nadd_a\nkeep\nadd_b\nz\n"),
            vec![
                LineDiffHunk {
                    old_start_offset: 1,
                    new_start_offset: 1,
                    old_lines: Vec::new(),
                    new_lines: vec!["add_a".to_string()],
                },
                LineDiffHunk {
                    old_start_offset: 2,
                    new_start_offset: 3,
                    old_lines: Vec::new(),
                    new_lines: vec!["add_b".to_string()],
                }
            ]
        );
    }

    #[test]
    fn minimal_diff_hunks_keep_adjacent_changes_in_one_hunk() {
        assert_eq!(
            minimal_diff_hunks("a\nold_a\nold_b\nz\n", "a\nnew_a\nnew_b\nz\n"),
            vec![LineDiffHunk {
                old_start_offset: 1,
                new_start_offset: 1,
                old_lines: vec!["old_a".to_string(), "old_b".to_string()],
                new_lines: vec!["new_a".to_string(), "new_b".to_string()],
            }]
        );
    }

    #[test]
    fn minimal_diff_hunks_large_inputs_fall_back_to_single_trimmed_hunk() {
        let old_middle = (0..500)
            .map(|index| format!("old_{index}"))
            .collect::<Vec<_>>()
            .join("\n");
        let new_middle = (0..500)
            .map(|index| format!("new_{index}"))
            .collect::<Vec<_>>()
            .join("\n");
        let old_text = format!("prefix\n{old_middle}\nsuffix\n");
        let new_text = format!("prefix\n{new_middle}\nsuffix\n");

        let hunks = minimal_diff_hunks(&old_text, &new_text);

        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].old_start_offset, 1);
        assert_eq!(hunks[0].new_start_offset, 1);
        assert_eq!(hunks[0].old_lines.len(), 500);
        assert_eq!(hunks[0].new_lines.len(), 500);
    }
}
