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
        let Some(preview) = operation.preview.as_text() else {
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
    let old_line_count = diff_line_count(old_text);
    let new_line_count = diff_line_count(&preview.new_text);

    render_diff_header(rendered, file);
    render_hunk_header(
        rendered,
        old_start_line,
        old_line_count,
        old_start_line,
        new_line_count,
        use_color,
    );
    render_removed_lines(rendered, old_text, use_color);
    render_added_lines(rendered, &preview.new_text, use_color);
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
    for line in diff_lines(text) {
        push_colored_line(rendered, &format!("-{line}"), RED, use_color);
    }
}

fn render_added_lines(rendered: &mut String, text: &str, use_color: bool) {
    for line in diff_lines(text) {
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
    use super::{diff_line_count, diff_lines};

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
}
