use std::io::IsTerminal;
use std::path::Path;

use clap::ValueEnum;
use serde::Serialize;
use similar::{Algorithm, ChangeTag, DiffOp, capture_diff, group_diff_ops};

use crate::apply::{CommittedChanges, FileMoveChange};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "snake_case")]
pub(super) enum ColorMode {
    Auto,
    Always,
    Never,
}

#[derive(Debug, Serialize)]
pub(crate) struct DiffResponse {
    text: String,
    file_moves: Vec<FileMoveChange>,
}

impl DiffResponse {
    pub(super) fn from_changes(changes: CommittedChanges) -> Self {
        Self {
            text: changes
                .files
                .iter()
                .map(|file| {
                    render_file_diff(&file.file, &file.before, &file.after, ColorMode::Never)
                })
                .collect(),
            file_moves: changes.file_moves,
        }
    }

    pub(super) fn from_text(file: &Path, before: &str, after: &str) -> Self {
        Self {
            text: render_file_diff(file, before, after, ColorMode::Never),
            file_moves: Vec::new(),
        }
    }
}

const RESET: &str = "\x1b[0m";
const RED: &str = "\x1b[31m";
const GREEN: &str = "\x1b[32m";
const CYAN: &str = "\x1b[36m";
const CONTEXT_LINES: usize = 3;
const MAX_ALIGNMENT_LINES: usize = 20_000;
const MAX_ALIGNMENT_BYTES: usize = 8 * 1024 * 1024;

pub(super) fn render_file_diff(file: &Path, before: &str, after: &str, color: ColorMode) -> String {
    if before == after {
        return String::new();
    }

    // Unified patch lines are LF-delimited. CR is source data, even at EOF.
    let old: Vec<_> = before.split_inclusive('\n').collect();
    let new: Vec<_> = after.split_inclusive('\n').collect();
    let ops = aligned_ops(&old, &new);
    let use_color = match color {
        ColorMode::Auto => std::io::stdout().is_terminal(),
        ColorMode::Always => true,
        ColorMode::Never => false,
    };

    let mut rendered = String::new();
    // A tab terminates the filename field, including names containing spaces.
    rendered.push_str(&format!(
        "--- {}\t\n+++ {}\t\n",
        file.display(),
        file.display()
    ));
    for group in group_diff_ops(ops, CONTEXT_LINES) {
        let first = group.first().expect("a diff group is nonempty");
        let last = group.last().expect("a diff group is nonempty");
        let old_range = first.old_range().start..last.old_range().end;
        let new_range = first.new_range().start..last.new_range().end;
        let old_start = old_range.start + usize::from(!old_range.is_empty());
        let new_start = new_range.start + usize::from(!new_range.is_empty());
        let header = format!(
            "@@ -{old_start},{} +{new_start},{} @@\n",
            old_range.len(),
            new_range.len()
        );
        render_colored(&mut rendered, &header, CYAN, use_color);

        for op in group {
            for change in op.iter_changes(&old, &new) {
                let (prefix, color) = match change.tag() {
                    ChangeTag::Equal => (' ', ""),
                    ChangeTag::Delete => ('-', RED),
                    ChangeTag::Insert => ('+', GREEN),
                };
                let line = change.value();
                render_colored(&mut rendered, &format!("{prefix}{line}"), color, use_color);
                if !line.ends_with('\n') {
                    rendered.push_str("\n\\ No newline at end of file\n");
                }
            }
        }
    }
    rendered
}

fn aligned_ops(old: &[&str], new: &[&str]) -> Vec<DiffOp> {
    let prefix = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    let suffix = old[prefix..]
        .iter()
        .rev()
        .zip(new[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let old_end = old.len() - suffix;
    let new_end = new.len() - suffix;
    let mut ops = Vec::new();
    if prefix > 0 {
        ops.push(DiffOp::Equal {
            old_index: 0,
            new_index: 0,
            len: prefix,
        });
    }

    if alignment_within_budget(&old[prefix..old_end], &new[prefix..new_end]) {
        ops.extend(capture_diff(
            Algorithm::Myers,
            old,
            prefix..old_end,
            new,
            prefix..new_end,
        ));
    } else {
        // Exact but not necessarily minimal; no machine-speed-dependent timeout.
        ops.push(DiffOp::Replace {
            old_index: prefix,
            old_len: old_end - prefix,
            new_index: prefix,
            new_len: new_end - prefix,
        });
    }

    if suffix > 0 {
        ops.push(DiffOp::Equal {
            old_index: old_end,
            new_index: new_end,
            len: suffix,
        });
    }
    ops
}

fn alignment_within_budget(old: &[&str], new: &[&str]) -> bool {
    old.len().saturating_add(new.len()) <= MAX_ALIGNMENT_LINES
        && old
            .iter()
            .chain(new)
            .fold(0usize, |total, line| total.saturating_add(line.len()))
            <= MAX_ALIGNMENT_BYTES
}

fn render_colored(output: &mut String, text: &str, color: &str, enabled: bool) {
    if enabled && !color.is_empty() {
        let (content, terminator) = if let Some(content) = text.strip_suffix("\r\n") {
            (content, "\r\n")
        } else if let Some(content) = text.strip_suffix('\n') {
            (content, "\n")
        } else {
            (text, "")
        };
        output.push_str(color);
        output.push_str(content);
        output.push_str(RESET);
        output.push_str(terminator);
    } else {
        output.push_str(text);
    }
}

#[cfg(test)]
mod tests;
