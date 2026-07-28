use std::path::Path;

use tree_sitter::Node;

use crate::error::IdenteditError;
use crate::handle::{SelectionHandle, Span};

use super::syntax::{PathToken, token_display};

mod json;
mod placement;
mod toml;
mod yaml;

pub(super) use json::{json_root_value, resolve_json_path};
pub(super) use toml::{resolve_toml_path, rewrite_toml_with_comment_preserving_create_missing};
pub(super) use yaml::{
    reject_yaml_implicit_null_single_line_value, render_yaml_comment_only_create_missing_insertion,
    resolve_yaml_path, rewrite_yaml_with_comment_preserving_create_missing,
    yaml_document_root_value_at, yaml_root_value, yaml_set_value_replace_span,
    yaml_set_value_replacement_text, yaml_single_line_value_with_trailing_line_endings,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ResolvedContainerEdit {
    pub(super) container_span: Span,
    pub(super) container_kind: String,
    pub(super) replace_span: Span,
}

pub(super) fn span_from_node(node: Node<'_>) -> Span {
    Span {
        start: node.start_byte(),
        end: node.end_byte(),
    }
}

pub(super) fn find_handle_for_span(
    file: &Path,
    handles: &[SelectionHandle],
    span: Span,
    expected_kind: &str,
) -> Result<SelectionHandle, IdenteditError> {
    let matches_by_kind = handles
        .iter()
        .filter(|handle| handle.span == span && handle.kind == expected_kind)
        .cloned()
        .collect::<Vec<_>>();

    if let [single] = matches_by_kind.as_slice() {
        return Ok(single.clone());
    }

    let matches_by_span = handles
        .iter()
        .filter(|handle| handle.span == span)
        .cloned()
        .collect::<Vec<_>>();

    match matches_by_span.as_slice() {
        [] => Err(IdenteditError::InvalidRequest {
            message: format!(
                "Config path resolver produced span [{}, {}) without a matching structural handle in '{}'",
                span.start,
                span.end,
                file.display()
            ),
        }),
        [single] => Ok(single.clone()),
        many => Err(IdenteditError::InvalidRequest {
            message: format!(
                "Config path resolver produced ambiguous span [{}, {}) kind '{}' in '{}' ({} handles)",
                span.start,
                span.end,
                expected_kind,
                file.display(),
                many.len()
            ),
        }),
    }
}

pub(super) fn rewrite_container_text(
    source_text: &str,
    container_span: Span,
    replace_span: Span,
    replacement: &str,
) -> Result<String, IdenteditError> {
    if container_span.start > container_span.end || container_span.end > source_text.len() {
        return Err(IdenteditError::InvalidRequest {
            message: format!(
                "Invalid container span [{}, {}) during config path rewrite",
                container_span.start, container_span.end
            ),
        });
    }
    if replace_span.start > replace_span.end
        || replace_span.start < container_span.start
        || replace_span.end > container_span.end
    {
        return Err(IdenteditError::InvalidRequest {
            message: format!(
                "Invalid replace span [{}, {}) inside container [{}, {}) during config path rewrite",
                replace_span.start, replace_span.end, container_span.start, container_span.end
            ),
        });
    }

    let mut container_text = source_text[container_span.start..container_span.end].to_string();
    let relative_start = replace_span.start - container_span.start;
    let relative_end = replace_span.end - container_span.start;
    container_text.replace_range(relative_start..relative_end, replacement);
    Ok(container_text)
}

pub(super) fn rewrite_full_source_text(
    source_text: &str,
    target_span: Span,
    replacement: &str,
) -> Result<String, IdenteditError> {
    if target_span.start > target_span.end || target_span.end > source_text.len() {
        return Err(IdenteditError::InvalidRequest {
            message: format!(
                "Invalid full-source replace span [{}, {}) during config path rewrite",
                target_span.start, target_span.end
            ),
        });
    }

    let mut updated = source_text.to_string();
    updated.replace_range(target_span.start..target_span.end, replacement);
    Ok(updated)
}

pub(super) fn decode_quoted_string(raw: &str) -> String {
    if raw.starts_with('"') && raw.ends_with('"') && raw.len() >= 2 {
        serde_json::from_str::<String>(raw).unwrap_or_else(|_| raw.trim_matches('"').to_string())
    } else {
        raw.to_string()
    }
}

pub(super) fn unique_match<'a>(
    raw_path: &str,
    token: &PathToken,
    matches: Vec<Node<'a>>,
) -> Result<Node<'a>, IdenteditError> {
    match matches.as_slice() {
        [] => Err(IdenteditError::InvalidRequest {
            message: format!(
                "Config path '{raw_path}' segment {} was not found",
                token_display(token)
            ),
        }),
        [single] => Ok(*single),
        many => Err(IdenteditError::InvalidRequest {
            message: format!(
                "Config path '{raw_path}' segment {} is ambiguous ({})",
                token_display(token),
                many.len()
            ),
        }),
    }
}

pub(super) fn adjusted_delete_span_for_container(
    source: &[u8],
    container_span: Span,
    container_kind: &str,
    entry_span: Span,
) -> Span {
    if is_comma_delimited_container(container_kind) {
        return adjusted_comma_delimited_delete_span(source, container_span, entry_span);
    }

    adjusted_line_delimited_delete_span(source, container_span, entry_span)
}

fn is_comma_delimited_container(kind: &str) -> bool {
    matches!(
        kind,
        "object" | "array" | "flow_mapping" | "flow_sequence" | "inline_table"
    )
}

fn adjusted_comma_delimited_delete_span(
    source: &[u8],
    container_span: Span,
    entry_span: Span,
) -> Span {
    let mut start = entry_span.start;
    let mut end = entry_span.end;

    let mut next_significant = end;
    while next_significant < container_span.end && source[next_significant].is_ascii_whitespace() {
        next_significant += 1;
    }
    if next_significant < container_span.end && source[next_significant] == b',' {
        end = next_significant + 1;
        while end < container_span.end && (source[end] == b' ' || source[end] == b'\t') {
            end += 1;
        }
        return Span { start, end };
    }

    let mut previous_significant = start;
    while previous_significant > container_span.start
        && source[previous_significant - 1].is_ascii_whitespace()
    {
        previous_significant -= 1;
    }
    if previous_significant > container_span.start && source[previous_significant - 1] == b',' {
        start = previous_significant - 1;
    }

    Span { start, end }
}

fn adjusted_line_delimited_delete_span(
    source: &[u8],
    container_span: Span,
    entry_span: Span,
) -> Span {
    let mut start = entry_span.start;
    let mut end = entry_span.end;

    let mut line_start = start;
    while line_start > container_span.start
        && source[line_start - 1] != b'\n'
        && source[line_start - 1] != b'\r'
    {
        line_start -= 1;
    }
    if source[line_start..start]
        .iter()
        .all(|byte| *byte == b' ' || *byte == b'\t')
    {
        start = line_start;
    }

    if end < container_span.end {
        if source[end] == b'\r' {
            if end + 1 < container_span.end && source[end + 1] == b'\n' {
                end += 2;
            } else {
                end += 1;
            }
        } else if source[end] == b'\n' {
            end += 1;
        }
    } else if start > container_span.start && source[start - 1] == b'\n' {
        start -= 1;
        if start > container_span.start && source[start - 1] == b'\r' {
            start -= 1;
        }
    }

    Span { start, end }
}

pub(super) fn named_children(node: Node<'_>) -> Vec<Node<'_>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).collect()
}

pub(super) fn first_named_child(node: Node<'_>) -> Option<Node<'_>> {
    named_children(node).into_iter().next()
}

pub(super) fn first_non_comment_named_child(node: Node<'_>) -> Option<Node<'_>> {
    named_children(node)
        .into_iter()
        .find(|child| child.kind() != "comment")
}
