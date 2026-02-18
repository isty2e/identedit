use super::{HashedLine, compute_line_hash};

pub(super) fn show_hashed_lines(source: &str) -> Vec<HashedLine> {
    split_source_lines(source)
        .lines
        .into_iter()
        .enumerate()
        .map(|(index, content)| HashedLine {
            line: index + 1,
            hash: compute_line_hash(&content),
            content,
        })
        .collect()
}

pub(super) fn format_hashed_lines(source: &str) -> String {
    show_hashed_lines(source)
        .into_iter()
        .map(|line| format!("{}:{}|{}", line.line, line.hash, line.content))
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn split_set_line_text(text: &str) -> Vec<String> {
    split_multiline_text(text)
}

pub(super) fn split_replace_lines_text(text: &str) -> Vec<String> {
    if text.is_empty() {
        Vec::new()
    } else {
        split_multiline_text(text)
    }
}

pub(super) fn split_multiline_text(text: &str) -> Vec<String> {
    text.replace("\r\n", "\n")
        .split('\n')
        .map(ToString::to_string)
        .collect()
}

#[derive(Debug, Clone)]
pub(super) struct SourceLayout {
    pub(super) lines: Vec<String>,
    pub(super) had_trailing_newline: bool,
    pub(super) newline: &'static str,
}

pub(super) fn split_source_lines(source: &str) -> SourceLayout {
    let lines = split_line_contents(source);
    SourceLayout {
        lines,
        had_trailing_newline: source.ends_with('\n') || source.ends_with('\r'),
        newline: detect_newline_style(source),
    }
}

fn split_line_contents(source: &str) -> Vec<String> {
    if source.is_empty() {
        return Vec::new();
    }

    let bytes = source.as_bytes();
    let mut lines = Vec::new();
    let mut start = 0usize;
    let mut index = 0usize;

    while index < bytes.len() {
        match bytes[index] {
            b'\n' => {
                lines.push(source[start..index].to_string());
                index += 1;
                start = index;
            }
            b'\r' => {
                lines.push(source[start..index].to_string());
                if index + 1 < bytes.len() && bytes[index + 1] == b'\n' {
                    index += 2;
                } else {
                    index += 1;
                }
                start = index;
            }
            _ => {
                index += 1;
            }
        }
    }

    if start < source.len() {
        lines.push(source[start..].to_string());
    }

    lines
}

pub(super) fn join_source_lines(
    lines: &[String],
    had_trailing_newline: bool,
    newline: &str,
) -> String {
    if lines.is_empty() {
        return String::new();
    }

    let mut content = lines.join(newline);
    if had_trailing_newline {
        content.push_str(newline);
    }
    content
}

fn detect_newline_style(source: &str) -> &'static str {
    if source.contains("\r\n") && !contains_lone_lf(source) && !contains_lone_cr(source) {
        "\r\n"
    } else if source.contains('\r') && !source.contains('\n') {
        "\r"
    } else {
        "\n"
    }
}

fn contains_lone_lf(source: &str) -> bool {
    let bytes = source.as_bytes();
    for (index, byte) in bytes.iter().enumerate() {
        if *byte == b'\n' && (index == 0 || bytes[index - 1] != b'\r') {
            return true;
        }
    }
    false
}

fn contains_lone_cr(source: &str) -> bool {
    let bytes = source.as_bytes();
    for (index, byte) in bytes.iter().enumerate() {
        if *byte == b'\r' && (index + 1 >= bytes.len() || bytes[index + 1] != b'\n') {
            return true;
        }
    }
    false
}
