use tree_sitter::{Node, Tree};

use crate::error::IdenteditError;
use crate::handle::Span;
use crate::provider::node_text;

use super::super::ConfigPathOperation;
use super::super::render::{append_requires_array_error, array_index_out_of_bounds_error};
use super::super::safety::validate_yaml_create_missing_reference_safety;
use super::super::syntax::{PathToken, expected_path_container_error, token_display};
use super::placement::{
    SiblingEntry, ends_with_line_ending, group_aware_insertion_offset, leading_comment_block_start,
    line_end_after_offset, line_end_with_ending_after_offset, line_ending_literal,
    line_start_before_offset, previous_line_bounds, starts_with_line_ending,
};
use super::{
    ResolvedContainerEdit, adjusted_delete_span_for_container, decode_quoted_string,
    first_non_comment_named_child, named_children, span_from_node, unique_match,
};

pub(in crate::patch::config_path) fn resolve_yaml_path(
    tree: &Tree,
    source: &[u8],
    path_tokens: &[PathToken],
    operation: &ConfigPathOperation,
    raw_path: &str,
    document_index: Option<usize>,
) -> Result<ResolvedContainerEdit, IdenteditError> {
    if let Some(index) = document_index {
        let root = yaml_document_root_value_at(tree.root_node(), index)?;
        return resolve_yaml_path_from_root(root, source, path_tokens, operation, raw_path);
    }

    let mut matches = Vec::new();
    let mut first_error = None;
    for root in yaml_document_root_values(tree.root_node()) {
        match resolve_yaml_path_from_root(root, source, path_tokens, operation, raw_path) {
            Ok(resolved) => matches.push(resolved),
            Err(error) => {
                first_error.get_or_insert(error);
            }
        };
    }

    match matches.as_slice() {
        [single] => Ok(single.clone()),
        [] => Err(
            first_error.unwrap_or_else(|| IdenteditError::InvalidRequest {
                message: "YAML document has no root value".to_string(),
            }),
        ),
        _ => Err(IdenteditError::InvalidRequest {
            message: format!("Config path '{raw_path}' is ambiguous across YAML documents"),
        }),
    }
}

fn resolve_yaml_path_from_root(
    mut current: Node<'_>,
    source: &[u8],
    path_tokens: &[PathToken],
    operation: &ConfigPathOperation,
    raw_path: &str,
) -> Result<ResolvedContainerEdit, IdenteditError> {
    for (index, token) in path_tokens.iter().enumerate() {
        let last = index + 1 == path_tokens.len();
        match token {
            PathToken::Key(expected_key) => {
                let pair_kind = match current.kind() {
                    "block_mapping" => "block_mapping_pair",
                    "flow_mapping" => "flow_pair",
                    _ => {
                        return Err(expected_path_container_error(
                            raw_path,
                            token,
                            current.kind(),
                        ));
                    }
                };

                let mut matches = Vec::new();
                for pair in named_children(current) {
                    if pair.kind() != pair_kind {
                        continue;
                    }
                    let Some(key_node) = pair.child_by_field_name("key") else {
                        continue;
                    };
                    let Some(key_text) = yaml_key_text(key_node, source) else {
                        continue;
                    };
                    if key_text == *expected_key {
                        matches.push(pair);
                    }
                }

                let matched_pair = unique_match(raw_path, token, matches)?;
                let value_node = matched_pair
                    .child_by_field_name("value")
                    .and_then(yaml_unwrap_node)
                    .ok_or_else(|| IdenteditError::InvalidRequest {
                        message: format!(
                            "Config path '{raw_path}' matched key '{expected_key}' without a value node"
                        ),
                    })?;

                if last {
                    return Ok(match operation {
                        ConfigPathOperation::Set { .. } => ResolvedContainerEdit {
                            container_span: span_from_node(current),
                            container_kind: current.kind().to_string(),
                            replace_span: span_from_node(value_node),
                        },
                        ConfigPathOperation::Append { .. } => {
                            if value_node.kind() != "block_sequence"
                                && value_node.kind() != "flow_sequence"
                            {
                                return Err(append_requires_array_error(
                                    raw_path,
                                    value_node.kind(),
                                ));
                            }
                            ResolvedContainerEdit {
                                container_span: span_from_node(value_node),
                                container_kind: value_node.kind().to_string(),
                                replace_span: span_from_node(value_node),
                            }
                        }
                        ConfigPathOperation::Delete => ResolvedContainerEdit {
                            container_span: span_from_node(current),
                            container_kind: current.kind().to_string(),
                            replace_span: adjusted_delete_span_for_container(
                                source,
                                span_from_node(current),
                                current.kind(),
                                span_from_node(matched_pair),
                            ),
                        },
                    });
                }

                current = value_node;
            }
            PathToken::Index(expected_index) => match current.kind() {
                "block_sequence" => {
                    let items = named_children(current);
                    let item = items.get(*expected_index).ok_or_else(|| {
                        array_index_out_of_bounds_error(raw_path, *expected_index, items.len())
                    })?;
                    let value_node = yaml_unwrap_node(*item).ok_or_else(|| IdenteditError::InvalidRequest {
                        message: format!(
                            "Config path '{raw_path}' index [{expected_index}] has no YAML value node"
                        ),
                    })?;
                    if last {
                        return Ok(match operation {
                            ConfigPathOperation::Set { .. } => ResolvedContainerEdit {
                                container_span: span_from_node(current),
                                container_kind: current.kind().to_string(),
                                replace_span: span_from_node(value_node),
                            },
                            ConfigPathOperation::Append { .. } => {
                                if value_node.kind() != "block_sequence"
                                    && value_node.kind() != "flow_sequence"
                                {
                                    return Err(append_requires_array_error(
                                        raw_path,
                                        value_node.kind(),
                                    ));
                                }
                                ResolvedContainerEdit {
                                    container_span: span_from_node(value_node),
                                    container_kind: value_node.kind().to_string(),
                                    replace_span: span_from_node(value_node),
                                }
                            }
                            ConfigPathOperation::Delete => ResolvedContainerEdit {
                                container_span: span_from_node(current),
                                container_kind: current.kind().to_string(),
                                replace_span: adjusted_delete_span_for_container(
                                    source,
                                    span_from_node(current),
                                    current.kind(),
                                    span_from_node(*item),
                                ),
                            },
                        });
                    }
                    current = value_node;
                }
                "flow_sequence" => {
                    let items = named_children(current);
                    let item = items.get(*expected_index).ok_or_else(|| {
                        array_index_out_of_bounds_error(raw_path, *expected_index, items.len())
                    })?;
                    let next = yaml_unwrap_node(*item).unwrap_or(*item);
                    if last {
                        return Ok(match operation {
                            ConfigPathOperation::Set { .. } => ResolvedContainerEdit {
                                container_span: span_from_node(current),
                                container_kind: current.kind().to_string(),
                                replace_span: span_from_node(next),
                            },
                            ConfigPathOperation::Append { .. } => {
                                if next.kind() != "block_sequence" && next.kind() != "flow_sequence"
                                {
                                    return Err(append_requires_array_error(raw_path, next.kind()));
                                }
                                ResolvedContainerEdit {
                                    container_span: span_from_node(next),
                                    container_kind: next.kind().to_string(),
                                    replace_span: span_from_node(next),
                                }
                            }
                            ConfigPathOperation::Delete => ResolvedContainerEdit {
                                container_span: span_from_node(current),
                                container_kind: current.kind().to_string(),
                                replace_span: adjusted_delete_span_for_container(
                                    source,
                                    span_from_node(current),
                                    current.kind(),
                                    span_from_node(*item),
                                ),
                            },
                        });
                    }
                    current = next;
                }
                _ => {
                    return Err(expected_path_container_error(
                        raw_path,
                        token,
                        current.kind(),
                    ));
                }
            },
        }
    }

    Err(IdenteditError::InvalidRequest {
        message: format!("Config path '{raw_path}' did not resolve to an editable value"),
    })
}

pub(in crate::patch::config_path) fn rewrite_yaml_with_comment_preserving_create_missing(
    tree: &Tree,
    source_text: &str,
    path_tokens: &[PathToken],
    raw_path: &str,
    new_text: &str,
    document_index: Option<usize>,
) -> Result<String, IdenteditError> {
    parse_yaml_value_fragment(new_text)?;

    let root = if let Some(index) = document_index {
        yaml_document_root_value_at(tree.root_node(), index)?
    } else {
        yaml_root_value(tree.root_node()).ok_or_else(|| IdenteditError::InvalidRequest {
            message: "YAML document has no root value".to_string(),
        })?
    };
    let root_span = span_from_node(root);
    let (parent_mapping, create_tokens) = find_yaml_create_missing_insertion_parent(
        root,
        source_text.as_bytes(),
        path_tokens,
        raw_path,
    )?;
    validate_yaml_create_missing_reference_safety(
        tree,
        source_text,
        document_index,
        parent_mapping,
        raw_path,
    )?;

    let indent = yaml_child_indent(source_text, parent_mapping);
    let leaf_key = yaml_create_missing_leaf_key(create_tokens, raw_path)?;
    let insertion_offset =
        yaml_create_missing_insertion_offset(source_text, parent_mapping, indent.len(), leaf_key);
    if insertion_offset < root_span.start || insertion_offset > root_span.end {
        return Err(IdenteditError::InvalidRequest {
            message: format!(
                "Config path create-missing for YAML comments produced insertion offset {insertion_offset} outside root span [{}, {})",
                root_span.start, root_span.end
            ),
        });
    }

    let line_ending = line_ending_literal(source_text);
    let entry =
        yaml_create_missing_entry_text(create_tokens, &indent, new_text, raw_path, line_ending)?;
    insert_yaml_entry_line(
        &source_text[root_span.start..root_span.end],
        insertion_offset - root_span.start,
        &entry,
        line_ending,
        &source_text[root_span.end..],
    )
}

pub(in crate::patch::config_path) fn render_yaml_comment_only_create_missing_insertion(
    source_text: &str,
    path_tokens: &[PathToken],
    raw_path: &str,
    new_text: &str,
) -> Result<String, IdenteditError> {
    parse_yaml_value_fragment(new_text)?;
    if path_tokens
        .iter()
        .any(|token| matches!(token, PathToken::Index(_)))
    {
        return Err(IdenteditError::InvalidRequest {
            message: format!(
                "Config path create-missing for YAML comments supports only dotted key paths; array indexes are not auto-created for '{raw_path}'. Use a dedicated append operation if needed."
            ),
        });
    }

    let line_ending = line_ending_literal(source_text);
    let entry = yaml_create_missing_entry_text(path_tokens, "", new_text, raw_path, line_ending)?;
    let mut insertion = String::new();
    if !source_text.is_empty() && !ends_with_line_ending(source_text) {
        insertion.push_str(line_ending);
    }
    insertion.push_str(&entry);
    insertion.push_str(line_ending);
    Ok(insertion)
}

pub(in crate::patch::config_path) fn yaml_root_value(root: Node<'_>) -> Option<Node<'_>> {
    let mut node = root;
    if node.kind() == "stream" {
        node = first_non_comment_named_child(node)?;
    }
    if node.kind() == "document" {
        node = first_non_comment_named_child(node)?;
    }
    yaml_unwrap_node(node)
}

fn yaml_document_root_values(root: Node<'_>) -> Vec<Node<'_>> {
    if root.kind() != "stream" {
        return yaml_root_value(root).into_iter().collect();
    }

    let mut roots = Vec::new();
    for child in named_children(root) {
        if child.kind() != "document" {
            continue;
        }
        if let Some(value) = yaml_root_value(child) {
            roots.push(value);
        }
    }

    if roots.is_empty() {
        yaml_root_value(root).into_iter().collect()
    } else {
        roots
    }
}

pub(in crate::patch::config_path) fn yaml_document_root_value_at(
    root: Node<'_>,
    document_index: usize,
) -> Result<Node<'_>, IdenteditError> {
    if root.kind() != "stream" {
        if document_index == 0 {
            return yaml_root_value(root).ok_or_else(|| IdenteditError::InvalidRequest {
                message: "YAML document_index 0 has no root value".to_string(),
            });
        }
        return Err(IdenteditError::InvalidRequest {
            message: format!(
                "YAML document_index {document_index} is out of range; file has 1 document"
            ),
        });
    }

    let documents = named_children(root)
        .into_iter()
        .filter(|child| child.kind() == "document")
        .collect::<Vec<_>>();
    if documents.is_empty() {
        if document_index == 0 {
            return yaml_root_value(root).ok_or_else(|| IdenteditError::InvalidRequest {
                message: "YAML document_index 0 has no root value".to_string(),
            });
        }
        return Err(IdenteditError::InvalidRequest {
            message: format!(
                "YAML document_index {document_index} is out of range; file has 1 document"
            ),
        });
    }

    let Some(document) = documents.get(document_index) else {
        return Err(IdenteditError::InvalidRequest {
            message: format!(
                "YAML document_index {document_index} is out of range; file has {} documents",
                documents.len()
            ),
        });
    };

    yaml_root_value(*document).ok_or_else(|| IdenteditError::InvalidRequest {
        message: format!("YAML document_index {document_index} has no root value"),
    })
}

pub(in crate::patch::config_path) fn yaml_set_value_replacement_text(
    source_text: &str,
    replace_span: Span,
    new_text: &str,
    raw_path: &str,
) -> Result<String, IdenteditError> {
    reject_yaml_non_ascii_line_separators(new_text, raw_path)?;
    if !new_text.contains('\n') && !new_text.contains('\r') {
        reject_yaml_implicit_null_single_line_value(new_text, raw_path)?;
        return Ok(new_text.to_string());
    }
    if let Some(single_line_value) = yaml_single_line_value_with_trailing_line_endings(new_text) {
        reject_yaml_implicit_null_single_line_value(single_line_value, raw_path)?;
        return Ok(single_line_value.to_string());
    }

    reject_yaml_block_scalar_before_trailing_line_content(source_text, replace_span, raw_path)?;

    let line_start = line_start_before_offset(source_text, replace_span.start);
    let line_prefix = &source_text[line_start..replace_span.start];
    let value_indent = line_prefix
        .chars()
        .take_while(|character| *character == ' ')
        .collect::<String>();
    let line_ending = line_ending_literal(source_text);
    let mut replacement =
        yaml_multiline_value_text(new_text, &value_indent, raw_path, line_ending)?;
    if yaml_existing_replacement_needs_block_scalar_terminator(new_text) {
        replacement.push_str(line_ending);
    }
    Ok(replacement)
}

pub(in crate::patch::config_path) fn yaml_set_value_replace_span(
    source_text: &str,
    replace_span: Span,
    replacement: &str,
    raw_path: &str,
) -> Result<Span, IdenteditError> {
    if !replacement.contains('\n') && !replacement.contains('\r') {
        return Ok(replace_span);
    }

    let line_end = line_end_after_offset(source_text, replace_span.end);
    let trailing = &source_text[replace_span.end..line_end];
    if !trailing.trim().is_empty() {
        return Err(IdenteditError::InvalidRequest {
            message: format!(
                "Config path '{raw_path}' cannot replace a YAML value with a block scalar while trailing content remains on the same line; use line mode to rewrite the full line"
            ),
        });
    }

    Ok(Span {
        start: replace_span.start,
        end: line_end,
    })
}

pub(in crate::patch::config_path) fn reject_yaml_implicit_null_single_line_value(
    text: &str,
    raw_path: &str,
) -> Result<(), IdenteditError> {
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return Err(IdenteditError::InvalidRequest {
            message: format!(
                "Config path '{raw_path}' YAML set value must be an explicit YAML value; quote empty or comment-like strings"
            ),
        });
    }

    Ok(())
}

pub(in crate::patch::config_path) fn yaml_single_line_value_with_trailing_line_endings(
    text: &str,
) -> Option<&str> {
    let trimmed = text
        .strip_suffix("\r\n")
        .or_else(|| text.strip_suffix('\n'))
        .or_else(|| text.strip_suffix('\r'))?;
    if trimmed.contains('\n') || trimmed.contains('\r') {
        return None;
    }
    if matches!(
        trimmed.trim_end_matches(' ').chars().next(),
        Some('|' | '>')
    ) {
        return None;
    }
    Some(trimmed)
}

fn yaml_existing_replacement_needs_block_scalar_terminator(text: &str) -> bool {
    let normalized = normalize_yaml_fragment_line_endings(text);
    if !normalized.ends_with('\n') {
        return false;
    }
    let Some((header_line, _)) = normalized.split_once('\n') else {
        return false;
    };
    let header = header_line.trim_end_matches(' ');
    matches!(header, "|" | ">")
}

fn reject_yaml_block_scalar_before_trailing_line_content(
    source_text: &str,
    replace_span: Span,
    raw_path: &str,
) -> Result<(), IdenteditError> {
    let line_end = line_end_after_offset(source_text, replace_span.end);
    let trailing = &source_text[replace_span.end..line_end];
    if trailing.trim().is_empty() {
        return Ok(());
    }

    Err(IdenteditError::InvalidRequest {
        message: format!(
            "Config path '{raw_path}' cannot replace a YAML value with a block scalar while trailing content remains on the same line; use line mode to rewrite the full line"
        ),
    })
}

fn parse_yaml_value_fragment(fragment: &str) -> Result<serde_yaml::Value, IdenteditError> {
    serde_yaml::from_str(fragment).map_err(|error| IdenteditError::InvalidRequest {
        message: format!("Config path set value is not valid YAML value text: {error}"),
    })
}

fn find_yaml_create_missing_insertion_parent<'a>(
    root: Node<'a>,
    source: &[u8],
    path_tokens: &'a [PathToken],
    raw_path: &str,
) -> Result<(Node<'a>, &'a [PathToken]), IdenteditError> {
    if path_tokens.is_empty() {
        return Err(IdenteditError::InvalidRequest {
            message: format!("Config path '{raw_path}' did not resolve to a YAML key"),
        });
    }

    let mut current = root;
    let mut consumed = 0usize;

    while consumed + 1 < path_tokens.len() {
        let token = &path_tokens[consumed];
        match token {
            PathToken::Key(expected_key) => {
                let mapping = require_yaml_block_mapping(current, raw_path)?;
                let mut matches = Vec::new();
                for pair in named_children(mapping) {
                    if pair.kind() != "block_mapping_pair" {
                        continue;
                    }
                    let Some(key_node) = pair.child_by_field_name("key") else {
                        continue;
                    };
                    let Some(key_text) = yaml_key_text(key_node, source) else {
                        continue;
                    };
                    if key_text == *expected_key {
                        matches.push(pair);
                    }
                }

                let matched_pair = match matches.as_slice() {
                    [] => {
                        reject_yaml_create_missing_sequence_auto_create(
                            &path_tokens[consumed..],
                            raw_path,
                        )?;
                        return Ok((mapping, &path_tokens[consumed..]));
                    }
                    [single] => *single,
                    many => {
                        return Err(IdenteditError::InvalidRequest {
                            message: format!(
                                "Config path '{raw_path}' segment {} is ambiguous ({})",
                                token_display(token),
                                many.len()
                            ),
                        });
                    }
                };
                current = matched_pair
                    .child_by_field_name("value")
                    .and_then(yaml_unwrap_node)
                    .ok_or_else(|| IdenteditError::InvalidRequest {
                        message: format!(
                            "Config path '{raw_path}' matched key '{expected_key}' without a value node"
                        ),
                    })?;
            }
            PathToken::Index(expected_index) => {
                let sequence = require_yaml_block_sequence(current, raw_path)?;
                let items = named_children(sequence);
                let item = items.get(*expected_index).ok_or_else(|| {
                    array_index_out_of_bounds_error(raw_path, *expected_index, items.len())
                })?;
                current = yaml_unwrap_node(*item).ok_or_else(|| IdenteditError::InvalidRequest {
                    message: format!(
                        "Config path '{raw_path}' index [{expected_index}] has no YAML value node"
                    ),
                })?;
            }
        }
        consumed += 1;
    }

    let parent_mapping = require_yaml_block_mapping(current, raw_path)?;
    reject_yaml_create_missing_sequence_auto_create(&path_tokens[consumed..], raw_path)?;
    Ok((parent_mapping, &path_tokens[consumed..]))
}

fn yaml_create_missing_entry_text(
    create_tokens: &[PathToken],
    base_indent: &str,
    new_text: &str,
    raw_path: &str,
    line_ending: &str,
) -> Result<String, IdenteditError> {
    if create_tokens.is_empty() {
        return Err(IdenteditError::InvalidRequest {
            message: format!("Config path '{raw_path}' was not found in YAML document"),
        });
    }

    let mut entry = String::new();
    for (index, token) in create_tokens.iter().enumerate() {
        let PathToken::Key(key) = token else {
            unreachable!("index tokens were rejected before YAML entry rendering");
        };
        let entry_indent = format!("{base_indent}{}", "  ".repeat(index));
        entry.push_str(&entry_indent);
        entry.push_str(&yaml_render_key_segment(key));
        entry.push(':');
        if index + 1 == create_tokens.len() {
            let value_text =
                yaml_create_missing_value_text(new_text, &entry_indent, raw_path, line_ending)?;
            entry.push_str(&value_text);
        }
        if index + 1 < create_tokens.len() {
            entry.push_str(line_ending);
        }
    }
    Ok(entry)
}

fn yaml_create_missing_leaf_key<'a>(
    create_tokens: &'a [PathToken],
    raw_path: &str,
) -> Result<&'a str, IdenteditError> {
    let Some(PathToken::Key(key)) = create_tokens.last() else {
        return Err(IdenteditError::InvalidRequest {
            message: format!("Config path '{raw_path}' did not resolve to a YAML key"),
        });
    };
    Ok(key)
}

fn yaml_render_key_segment(key: &str) -> String {
    if is_yaml_plain_key_safe(key) {
        key.to_string()
    } else {
        serde_json::to_string(key)
            .unwrap_or_else(|_| format!("\"{}\"", key.replace('\\', "\\\\").replace('"', "\\\"")))
    }
}

fn is_yaml_plain_key_safe(key: &str) -> bool {
    if key.is_empty() || key.trim() != key {
        return false;
    }
    if key == "<<" {
        return false;
    }

    let mut chars = key.chars();
    if let Some(first) = chars.next()
        && (first.is_ascii_digit()
            || matches!(
                first,
                '-' | '?'
                    | ':'
                    | '{'
                    | '}'
                    | '['
                    | ']'
                    | ','
                    | '&'
                    | '*'
                    | '#'
                    | '!'
                    | '|'
                    | '>'
                    | '\''
                    | '"'
                    | '%'
                    | '@'
                    | '`'
            ))
    {
        return false;
    }

    !key.chars()
        .any(|character| character.is_control() || character == '\t')
        && !key.contains(": ")
        && !key.contains(" #")
        && plain_yaml_scalar_round_trips_as_string(key)
}

fn plain_yaml_scalar_round_trips_as_string(key: &str) -> bool {
    matches!(
        serde_yaml::from_str::<serde_yaml::Value>(key),
        Ok(serde_yaml::Value::String(value)) if value == key
    )
}

fn yaml_create_missing_value_text(
    new_text: &str,
    leaf_indent: &str,
    raw_path: &str,
    line_ending: &str,
) -> Result<String, IdenteditError> {
    reject_yaml_non_ascii_line_separators(new_text, raw_path)?;
    if !new_text.contains('\n') && !new_text.contains('\r') {
        reject_yaml_implicit_null_single_line_value(new_text, raw_path)?;
        return Ok(format!(" {new_text}"));
    }
    if let Some(single_line_value) = yaml_single_line_value_with_trailing_line_endings(new_text) {
        reject_yaml_implicit_null_single_line_value(single_line_value, raw_path)?;
        return Ok(format!(" {single_line_value}"));
    }

    Ok(format!(
        " {}",
        yaml_multiline_value_text(new_text, leaf_indent, raw_path, line_ending)?
    ))
}

fn yaml_multiline_value_text(
    new_text: &str,
    leaf_indent: &str,
    raw_path: &str,
    line_ending: &str,
) -> Result<String, IdenteditError> {
    let normalized = normalize_yaml_fragment_line_endings(new_text);
    let Some((header_line, body_text)) = normalized.split_once('\n') else {
        return Err(yaml_multiline_value_policy_error(raw_path));
    };
    let header = yaml_block_scalar_header(header_line, raw_path)?;
    let body_lines = yaml_block_scalar_body_lines(body_text);
    let strip_indent = yaml_block_scalar_body_indent(&body_lines, raw_path)?;
    let body_indent = format!("{leaf_indent}  ");

    let mut rendered = header.to_string();
    for line in body_lines {
        rendered.push_str(line_ending);
        rendered.push_str(&body_indent);
        if !line.is_empty() {
            rendered.push_str(
                line.get(strip_indent..)
                    .ok_or_else(|| yaml_block_scalar_indent_error(raw_path))?,
            );
        }
    }
    Ok(rendered)
}

fn normalize_yaml_fragment_line_endings(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

fn yaml_block_scalar_header<'a>(
    header_line: &'a str,
    raw_path: &str,
) -> Result<&'a str, IdenteditError> {
    if header_line.trim_start() != header_line {
        return Err(yaml_multiline_value_policy_error(raw_path));
    }

    let header = header_line.trim_end_matches(' ');
    let mut chars = header.chars();
    let Some(style @ ('|' | '>')) = chars.next() else {
        return Err(yaml_multiline_value_policy_error(raw_path));
    };
    let suffix = chars.as_str();
    if matches!(suffix, "" | "-" | "+") {
        return Ok(header);
    }
    if suffix.chars().any(|character| character.is_ascii_digit()) {
        return Err(IdenteditError::InvalidRequest {
            message: format!(
                "Config path create-missing for YAML comments does not support explicit block scalar indent indicators for '{raw_path}'; use {style}, {style}- or {style}+ without a numeric indent"
            ),
        });
    }
    Err(yaml_multiline_value_policy_error(raw_path))
}

fn yaml_block_scalar_body_lines(body_text: &str) -> Vec<&str> {
    let mut lines = body_text.split('\n').collect::<Vec<_>>();
    if body_text.ends_with('\n') {
        lines.pop();
    }
    lines
}

fn yaml_block_scalar_body_indent(lines: &[&str], raw_path: &str) -> Result<usize, IdenteditError> {
    let mut min_indent: Option<usize> = None;
    for line in lines.iter().copied().filter(|line| !line.is_empty()) {
        let indent = line.bytes().take_while(|byte| *byte == b' ').count();
        if indent == 0 {
            return Err(yaml_block_scalar_indent_error(raw_path));
        }
        min_indent = Some(min_indent.map_or(indent, |current| current.min(indent)));
    }
    Ok(min_indent.unwrap_or(0))
}

fn yaml_multiline_value_policy_error(raw_path: &str) -> IdenteditError {
    IdenteditError::InvalidRequest {
        message: format!(
            "Config path create-missing for YAML comments supports multiline values only as explicit block scalar leaf values for '{raw_path}' (|, |-, |+, >, >-, or >+); use line mode for multiline mappings or sequences"
        ),
    }
}

fn reject_yaml_non_ascii_line_separators(text: &str, raw_path: &str) -> Result<(), IdenteditError> {
    if text.contains('\0') {
        return Err(IdenteditError::InvalidRequest {
            message: format!(
                "Config path '{raw_path}' YAML set value must not contain raw NUL characters"
            ),
        });
    }
    if text.contains('\u{2028}') || text.contains('\u{2029}') {
        return Err(IdenteditError::InvalidRequest {
            message: format!(
                "Config path '{raw_path}' YAML set value must not contain raw Unicode line separator characters; use an escaped quoted scalar if the character is intentional"
            ),
        });
    }
    Ok(())
}

fn yaml_block_scalar_indent_error(raw_path: &str) -> IdenteditError {
    IdenteditError::InvalidRequest {
        message: format!(
            "Config path create-missing for YAML comments cannot safely reindent block scalar content for '{raw_path}'; indent every non-empty scalar content line in the value fragment"
        ),
    }
}

fn require_yaml_block_mapping<'a>(
    node: Node<'a>,
    raw_path: &str,
) -> Result<Node<'a>, IdenteditError> {
    if node.kind() == "block_mapping" {
        return Ok(node);
    }

    Err(IdenteditError::InvalidRequest {
        message: format!(
            "Config path create-missing for YAML comments supports only existing block mappings; path '{raw_path}' resolved through node kind '{}'",
            node.kind()
        ),
    })
}

fn require_yaml_block_sequence<'a>(
    node: Node<'a>,
    raw_path: &str,
) -> Result<Node<'a>, IdenteditError> {
    if node.kind() == "block_sequence" {
        return Ok(node);
    }

    Err(IdenteditError::InvalidRequest {
        message: format!(
            "Config path create-missing for YAML comments supports only existing block sequences when traversing array indexes; path '{raw_path}' resolved through node kind '{}'",
            node.kind()
        ),
    })
}

fn reject_yaml_create_missing_sequence_auto_create(
    create_tokens: &[PathToken],
    raw_path: &str,
) -> Result<(), IdenteditError> {
    if create_tokens
        .iter()
        .any(|token| matches!(token, PathToken::Index(_)))
    {
        return Err(IdenteditError::InvalidRequest {
            message: format!(
                "Config path create-missing for YAML comments creates only mapping keys; array indexes are not auto-created for '{raw_path}'. Use a dedicated append operation if needed."
            ),
        });
    }

    Ok(())
}

fn yaml_child_indent(source_text: &str, mapping: Node<'_>) -> String {
    let line_start = line_start_before_offset(source_text, mapping.start_byte());
    let prefix = &source_text[line_start..mapping.start_byte()];
    let trimmed = prefix.trim_start();
    if trimmed.starts_with('-') && trimmed[1..].trim().is_empty() {
        return prefix.replacen('-', " ", 1);
    }
    prefix.to_string()
}

fn yaml_create_missing_insertion_offset(
    source_text: &str,
    parent_mapping: Node<'_>,
    child_indent_len: usize,
    leaf_key: &str,
) -> usize {
    let fallback = yaml_create_missing_fallback_insertion_offset(
        source_text,
        parent_mapping.end_byte(),
        child_indent_len,
    );
    let Some(entries) = yaml_sibling_entries(source_text, parent_mapping, child_indent_len) else {
        return fallback;
    };
    group_aware_insertion_offset(source_text, entries, fallback, leaf_key)
}

fn yaml_create_missing_fallback_insertion_offset(
    source_text: &str,
    initial_offset: usize,
    child_indent_len: usize,
) -> usize {
    let mut cursor = initial_offset.min(source_text.len());
    let mut insertion_offset = cursor;
    while let Some((line_start, line_end)) = previous_line_bounds(source_text, cursor) {
        let line = &source_text[line_start..line_end];
        let trimmed = line.trim_start_matches(' ');
        if line.trim().is_empty()
            || (line.len() - trimmed.len() < child_indent_len && trimmed.starts_with('#'))
        {
            insertion_offset = line_start;
            cursor = line_start;
            continue;
        }
        break;
    }
    insertion_offset
}

fn yaml_sibling_entries(
    source_text: &str,
    mapping: Node<'_>,
    child_indent_len: usize,
) -> Option<Vec<SiblingEntry>> {
    let source = source_text.as_bytes();
    let mut entries = Vec::new();
    for pair in named_children(mapping) {
        if pair.kind() != "block_mapping_pair" {
            continue;
        }
        let key_node = pair.child_by_field_name("key")?;
        let key = yaml_key_text(key_node, source)?;
        let key_line_start = line_start_before_offset(source_text, pair.start_byte());
        entries.push(SiblingEntry {
            key,
            insertion_start: leading_comment_block_start(
                source_text,
                key_line_start,
                child_indent_len,
            ),
            end: line_end_with_ending_after_offset(source_text, pair.end_byte()),
        });
    }
    Some(entries)
}

fn insert_yaml_entry_line(
    root_text: &str,
    offset: usize,
    entry: &str,
    line_ending: &str,
    following_source_text: &str,
) -> Result<String, IdenteditError> {
    if offset > root_text.len() {
        return Err(IdenteditError::InvalidRequest {
            message: format!(
                "Invalid YAML insertion offset {offset} for root length {}",
                root_text.len()
            ),
        });
    }

    let before = &root_text[..offset];
    let after = &root_text[offset..];
    let needs_prefix = !before.is_empty() && !ends_with_line_ending(before);
    let needs_suffix = if after.is_empty() {
        !starts_with_line_ending(following_source_text)
    } else if starts_with_line_ending(after) {
        ends_with_line_ending(before)
    } else {
        true
    };

    let mut updated = root_text.to_string();
    let mut text = String::new();
    if needs_prefix {
        text.push_str(line_ending);
    }
    text.push_str(entry);
    if needs_suffix {
        text.push_str(line_ending);
    }
    updated.insert_str(offset, &text);
    Ok(updated)
}

fn yaml_unwrap_node(mut node: Node<'_>) -> Option<Node<'_>> {
    loop {
        match node.kind() {
            "block_node" | "flow_node" | "block_sequence_item" => {
                node = first_yaml_content_child(node)?;
            }
            _ => return Some(node),
        }
    }
}

fn first_yaml_content_child(node: Node<'_>) -> Option<Node<'_>> {
    for index in 0..node.named_child_count() {
        let child = node.named_child(index as u32)?;
        if matches!(child.kind(), "anchor" | "tag") {
            continue;
        }
        return Some(child);
    }
    None
}

fn yaml_key_text(key_node: Node<'_>, source: &[u8]) -> Option<String> {
    let node = yaml_unwrap_node(key_node)?;
    let raw = node_text(node, source)?;
    Some(match node.kind() {
        "double_quote_scalar" => decode_quoted_string(&raw),
        "single_quote_scalar" => raw
            .strip_prefix('\'')
            .and_then(|value| value.strip_suffix('\''))
            .map(|value| value.replace("''", "'"))
            .unwrap_or(raw),
        _ => raw.trim().to_string(),
    })
}
