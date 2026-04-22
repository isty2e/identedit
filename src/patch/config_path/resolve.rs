use std::collections::HashMap;
use std::path::Path;

use tree_sitter::{Node, Tree};

use crate::error::IdenteditError;
use crate::handle::{SelectionHandle, Span};
use crate::provider::node_text;

use super::ConfigPathOperation;
use super::render::{
    append_requires_array_error, array_index_out_of_bounds_error, parse_toml_value_fragment,
};
use super::syntax::{PathToken, expected_path_container_error, path_tokens_display, token_display};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ResolvedContainerEdit {
    pub(super) container_span: Span,
    pub(super) container_kind: String,
    pub(super) replace_span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TomlCandidate {
    path: Vec<PathToken>,
    container_path: Vec<PathToken>,
    container_span: Span,
    container_kind: String,
    set_span: Span,
    set_kind: String,
    delete_entry_span: Span,
}

pub(super) fn resolve_json_path(
    tree: &Tree,
    source: &[u8],
    path_tokens: &[PathToken],
    operation: &ConfigPathOperation,
    raw_path: &str,
) -> Result<ResolvedContainerEdit, IdenteditError> {
    let mut current =
        json_root_value(tree.root_node()).ok_or_else(|| IdenteditError::InvalidRequest {
            message: "JSON document has no root value".to_string(),
        })?;

    for (index, token) in path_tokens.iter().enumerate() {
        let last = index + 1 == path_tokens.len();
        match token {
            PathToken::Key(expected_key) => {
                if current.kind() != "object" {
                    return Err(expected_path_container_error(
                        raw_path,
                        token,
                        current.kind(),
                    ));
                }

                let mut matches = Vec::new();
                for child in named_children(current) {
                    if child.kind() != "pair" {
                        continue;
                    }
                    let Some(key_node) = child.child_by_field_name("key") else {
                        continue;
                    };
                    let Some(raw_key) = node_text(key_node, source) else {
                        continue;
                    };
                    let decoded = decode_json_string(&raw_key)
                        .unwrap_or_else(|| raw_key.trim_matches('"').to_string());
                    if decoded == *expected_key {
                        matches.push(child);
                    }
                }

                let matched_pair = unique_match(raw_path, token, matches)?;
                let value_node = matched_pair
                    .child_by_field_name("value")
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
                            if value_node.kind() != "array" {
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
            PathToken::Index(expected_index) => {
                if current.kind() != "array" {
                    return Err(expected_path_container_error(
                        raw_path,
                        token,
                        current.kind(),
                    ));
                }

                let elements = named_children(current);
                let entry = elements.get(*expected_index).ok_or_else(|| {
                    array_index_out_of_bounds_error(raw_path, *expected_index, elements.len())
                })?;

                if last {
                    return Ok(match operation {
                        ConfigPathOperation::Set { .. } => ResolvedContainerEdit {
                            container_span: span_from_node(current),
                            container_kind: current.kind().to_string(),
                            replace_span: span_from_node(*entry),
                        },
                        ConfigPathOperation::Append { .. } => {
                            if entry.kind() != "array" {
                                return Err(append_requires_array_error(raw_path, entry.kind()));
                            }
                            ResolvedContainerEdit {
                                container_span: span_from_node(*entry),
                                container_kind: entry.kind().to_string(),
                                replace_span: span_from_node(*entry),
                            }
                        }
                        ConfigPathOperation::Delete => ResolvedContainerEdit {
                            container_span: span_from_node(current),
                            container_kind: current.kind().to_string(),
                            replace_span: adjusted_delete_span_for_container(
                                source,
                                span_from_node(current),
                                current.kind(),
                                span_from_node(*entry),
                            ),
                        },
                    });
                }

                current = *entry;
            }
        }
    }

    Err(IdenteditError::InvalidRequest {
        message: format!("Config path '{raw_path}' did not resolve to an editable value"),
    })
}

pub(super) fn resolve_yaml_path(
    tree: &Tree,
    source: &[u8],
    path_tokens: &[PathToken],
    operation: &ConfigPathOperation,
    raw_path: &str,
) -> Result<ResolvedContainerEdit, IdenteditError> {
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

pub(super) fn resolve_toml_path(
    tree: &Tree,
    source: &[u8],
    path_tokens: &[PathToken],
    operation: &ConfigPathOperation,
    raw_path: &str,
) -> Result<ResolvedContainerEdit, IdenteditError> {
    let root = tree.root_node();
    let mut candidates = Vec::new();
    collect_toml_candidates(root, source, &mut candidates);

    let matched = candidates
        .iter()
        .filter(|candidate| candidate.path == path_tokens)
        .collect::<Vec<_>>();

    let selected = match matched.as_slice() {
        [] => {
            return Err(IdenteditError::InvalidRequest {
                message: format!("Config path '{raw_path}' was not found in TOML document"),
            });
        }
        [single] => *single,
        many => {
            return Err(IdenteditError::InvalidRequest {
                message: format!(
                    "Config path '{raw_path}' is ambiguous in TOML document ({})",
                    many.len()
                ),
            });
        }
    };

    let (container_span, container_kind, replace_span) = match operation {
        ConfigPathOperation::Set { .. } => (
            selected.container_span,
            selected.container_kind.clone(),
            selected.set_span,
        ),
        ConfigPathOperation::Append { .. } => {
            if selected.set_kind != "array" {
                return Err(append_requires_array_error(raw_path, &selected.set_kind));
            }
            (
                selected.set_span,
                selected.set_kind.clone(),
                selected.set_span,
            )
        }
        ConfigPathOperation::Delete => (
            selected.container_span,
            selected.container_kind.clone(),
            adjusted_delete_span_for_container(
                source,
                selected.container_span,
                &selected.container_kind,
                selected.delete_entry_span,
            ),
        ),
    };

    Ok(ResolvedContainerEdit {
        container_span,
        container_kind,
        replace_span,
    })
}

pub(super) fn rewrite_toml_with_comment_preserving_create_missing(
    tree: &Tree,
    source_text: &str,
    path_tokens: &[PathToken],
    raw_path: &str,
    new_text: &str,
) -> Result<String, IdenteditError> {
    parse_toml_value_fragment(new_text)?;

    let (leaf_key, parent_path) = toml_create_missing_leaf_and_parent(path_tokens, raw_path)?;
    let entry = format!("{} = {new_text}", toml_render_key_segment(leaf_key));

    let insertion = if parent_path.is_empty() {
        TomlInsertion {
            offset: root_toml_insertion_offset(tree.root_node(), source_text),
            preserve_following_separator: true,
        }
    } else {
        if let Some(table) =
            find_toml_table_for_path(tree.root_node(), source_text.as_bytes(), parent_path)
        {
            return insert_toml_entry_line(
                source_text,
                TomlInsertion {
                    offset: move_offset_before_preceding_blank_lines(source_text, table.end_byte()),
                    preserve_following_separator: true,
                },
                &entry,
            );
        }

        reject_toml_missing_table_conflict(
            tree.root_node(),
            source_text.as_bytes(),
            parent_path,
            raw_path,
        )?;
        let block = toml_missing_table_block(
            parent_path,
            raw_path,
            &entry,
            toml_line_ending_literal(source_text),
        )?;
        let insertion = find_toml_missing_table_insertion(
            tree.root_node(),
            source_text.as_bytes(),
            source_text,
            parent_path,
        );
        return insert_toml_table_block(source_text, insertion, &block);
    };

    insert_toml_entry_line(source_text, insertion, &entry)
}

pub(super) fn rewrite_yaml_with_comment_preserving_create_missing(
    tree: &Tree,
    source_text: &str,
    path_tokens: &[PathToken],
    raw_path: &str,
    new_text: &str,
) -> Result<String, IdenteditError> {
    parse_yaml_value_fragment(new_text)?;

    let root = yaml_root_value(tree.root_node()).ok_or_else(|| IdenteditError::InvalidRequest {
        message: "YAML document has no root value".to_string(),
    })?;
    let root_span = span_from_node(root);
    let (parent_mapping, create_tokens) = find_yaml_create_missing_insertion_parent(
        root,
        source_text.as_bytes(),
        path_tokens,
        raw_path,
    )?;

    let insertion_offset = parent_mapping.end_byte();
    if insertion_offset < root_span.start || insertion_offset > root_span.end {
        return Err(IdenteditError::InvalidRequest {
            message: format!(
                "Config path create-missing for YAML comments produced insertion offset {insertion_offset} outside root span [{}, {})",
                root_span.start, root_span.end
            ),
        });
    }

    let indent = yaml_child_indent(source_text, parent_mapping);
    let line_ending = yaml_line_ending_literal(source_text);
    let entry =
        yaml_create_missing_entry_text(create_tokens, &indent, new_text, raw_path, line_ending)?;
    insert_yaml_entry_line(
        &source_text[root_span.start..root_span.end],
        insertion_offset - root_span.start,
        &entry,
        line_ending,
    )
}

pub(super) fn render_yaml_comment_only_create_missing_insertion(
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

    let line_ending = yaml_line_ending_literal(source_text);
    let entry = yaml_create_missing_entry_text(path_tokens, "", new_text, raw_path, line_ending)?;
    let mut insertion = String::new();
    if !source_text.is_empty() && !ends_with_line_ending(source_text) {
        insertion.push_str(line_ending);
    }
    insertion.push_str(&entry);
    insertion.push_str(line_ending);
    Ok(insertion)
}

pub(super) fn json_root_value(root: Node<'_>) -> Option<Node<'_>> {
    let node = root;
    if node.kind() == "document" {
        if let Some(value) = node.child_by_field_name("value") {
            return Some(value);
        }
        return first_named_child(node);
    }
    first_named_child(node).or(Some(node))
}

pub(super) fn yaml_root_value(root: Node<'_>) -> Option<Node<'_>> {
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

pub(super) fn yaml_set_value_replacement_text(
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
    let line_ending = yaml_line_ending_literal(source_text);
    let mut replacement =
        yaml_multiline_value_text(new_text, &value_indent, raw_path, line_ending)?;
    if yaml_existing_replacement_needs_block_scalar_terminator(new_text) {
        replacement.push_str(line_ending);
    }
    Ok(replacement)
}

pub(super) fn yaml_set_value_replace_span(
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

pub(super) fn reject_yaml_implicit_null_single_line_value(
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

pub(super) fn yaml_single_line_value_with_trailing_line_endings(text: &str) -> Option<&str> {
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

fn collect_toml_candidates(root: Node<'_>, source: &[u8], out: &mut Vec<TomlCandidate>) {
    let mut array_table_counts: HashMap<String, usize> = HashMap::new();
    for child in named_children(root) {
        match child.kind() {
            "pair" => collect_toml_pair_candidates(child, source, Vec::new(), root, out),
            "table" => {
                let prefix = toml_table_prefix(child, source);
                for pair in named_children(child) {
                    if pair.kind() == "pair" {
                        collect_toml_pair_candidates(pair, source, prefix.clone(), child, out);
                    }
                }
            }
            "table_array_element" => {
                let prefix = toml_table_prefix(child, source);
                let counter_key = path_tokens_display(&prefix);
                let index = array_table_counts
                    .entry(counter_key)
                    .and_modify(|value| *value += 1)
                    .or_insert(0);
                let mut indexed_prefix = prefix;
                indexed_prefix.push(PathToken::Index(*index));
                for pair in named_children(child) {
                    if pair.kind() == "pair" {
                        collect_toml_pair_candidates(
                            pair,
                            source,
                            indexed_prefix.clone(),
                            child,
                            out,
                        );
                    }
                }
            }
            _ => {}
        }
    }
}

fn toml_create_missing_leaf_and_parent<'a>(
    path_tokens: &'a [PathToken],
    raw_path: &str,
) -> Result<(&'a str, &'a [PathToken]), IdenteditError> {
    let Some((last, parent_path)) = path_tokens.split_last() else {
        return Err(IdenteditError::InvalidRequest {
            message: format!("Config path '{raw_path}' did not resolve to a TOML key"),
        });
    };

    if path_tokens
        .iter()
        .any(|token| matches!(token, PathToken::Index(_)))
    {
        return Err(IdenteditError::InvalidRequest {
            message: format!(
                "Config path create-missing for TOML comments supports only dotted key paths; array indexes are not auto-created for '{raw_path}'. Use a dedicated append operation if needed."
            ),
        });
    }

    let PathToken::Key(leaf_key) = last else {
        unreachable!("index tokens were rejected above");
    };
    Ok((leaf_key, parent_path))
}

fn find_toml_table_for_path<'a>(
    root: Node<'a>,
    source: &[u8],
    parent_path: &[PathToken],
) -> Option<Node<'a>> {
    named_children(root)
        .into_iter()
        .find(|child| child.kind() == "table" && toml_table_prefix(*child, source) == parent_path)
}

fn reject_toml_missing_table_conflict(
    root: Node<'_>,
    source: &[u8],
    table_path: &[PathToken],
    raw_path: &str,
) -> Result<(), IdenteditError> {
    let mut candidates = Vec::new();
    collect_toml_candidates(root, source, &mut candidates);

    for candidate in candidates {
        if candidate.path == table_path
            || path_tokens_start_with(table_path, &candidate.path)
            || (path_tokens_start_with(&candidate.path, table_path)
                && !path_tokens_start_with(&candidate.container_path, table_path))
        {
            return Err(IdenteditError::InvalidRequest {
                message: format!(
                    "Config path create-missing for TOML comments cannot create table '{}' for '{raw_path}' because '{}' already resolves to a TOML value; use line mode for this rewrite",
                    path_tokens_display(table_path),
                    path_tokens_display(&candidate.path)
                ),
            });
        }
    }

    for child in named_children(root) {
        if child.kind() != "table_array_element" {
            continue;
        }
        let prefix = toml_table_prefix(child, source);
        if prefix == table_path || path_tokens_start_with(table_path, &prefix) {
            return Err(IdenteditError::InvalidRequest {
                message: format!(
                    "Config path create-missing for TOML comments cannot create standard table '{}' for '{raw_path}' because an array of tables '{}' already exists; use append or line mode instead",
                    path_tokens_display(table_path),
                    path_tokens_display(&prefix)
                ),
            });
        }
    }

    Ok(())
}

fn toml_missing_table_block(
    table_path: &[PathToken],
    raw_path: &str,
    entry: &str,
    line_ending: &str,
) -> Result<String, IdenteditError> {
    let table_header = toml_table_header_for_path(table_path, raw_path)?;
    Ok(format!("{table_header}{line_ending}{entry}"))
}

fn toml_table_header_for_path(
    table_path: &[PathToken],
    raw_path: &str,
) -> Result<String, IdenteditError> {
    if table_path.is_empty() {
        return Err(IdenteditError::InvalidRequest {
            message: format!("Config path '{raw_path}' did not resolve to a TOML table path"),
        });
    }

    let mut segments = Vec::with_capacity(table_path.len());
    for token in table_path {
        let PathToken::Key(key) = token else {
            return Err(IdenteditError::InvalidRequest {
                message: format!(
                    "Config path create-missing for TOML comments supports only dotted key paths; array indexes are not auto-created for '{raw_path}'. Use a dedicated append operation if needed."
                ),
            });
        };
        segments.push(toml_render_key_segment(key));
    }

    Ok(format!("[{}]", segments.join(".")))
}

fn toml_render_key_segment(key: &str) -> String {
    if is_toml_bare_key(key) {
        key.to_string()
    } else {
        serde_json::to_string(key)
            .unwrap_or_else(|_| format!("\"{}\"", key.replace('\\', "\\\\").replace('"', "\\\"")))
    }
}

fn is_toml_bare_key(key: &str) -> bool {
    !key.is_empty()
        && key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

fn find_toml_missing_table_insertion(
    root: Node<'_>,
    source: &[u8],
    source_text: &str,
    table_path: &[PathToken],
) -> TomlInsertion {
    let mut nearest_prefix: Option<(usize, Node<'_>)> = None;
    let mut first_descendant: Option<Node<'_>> = None;

    for child in named_children(root) {
        if child.kind() != "table" && child.kind() != "table_array_element" {
            continue;
        }

        let prefix = toml_table_prefix(child, source);
        if child.kind() == "table"
            && prefix.len() < table_path.len()
            && path_tokens_start_with(table_path, &prefix)
        {
            if nearest_prefix
                .as_ref()
                .is_none_or(|(length, _)| prefix.len() > *length)
            {
                nearest_prefix = Some((prefix.len(), child));
            }
        } else if table_path.len() < prefix.len()
            && path_tokens_start_with(&prefix, table_path)
            && first_descendant
                .as_ref()
                .is_none_or(|current| child.start_byte() < current.start_byte())
        {
            first_descendant = Some(child);
        }
    }

    if let Some((_, table)) = nearest_prefix {
        return TomlInsertion {
            offset: move_offset_before_preceding_blank_lines(source_text, table.end_byte()),
            preserve_following_separator: true,
        };
    }

    if let Some(table) = first_descendant {
        return TomlInsertion {
            offset: move_offset_before_table_leading_comment_block(source_text, table.start_byte()),
            preserve_following_separator: true,
        };
    }

    TomlInsertion {
        offset: source_text.len(),
        preserve_following_separator: false,
    }
}

fn path_tokens_start_with(path: &[PathToken], prefix: &[PathToken]) -> bool {
    prefix.len() <= path.len() && path[..prefix.len()] == *prefix
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

fn line_start_before_offset(source_text: &str, offset: usize) -> usize {
    let bytes = source_text.as_bytes();
    let mut cursor = offset.min(bytes.len());
    while cursor > 0 && bytes[cursor - 1] != b'\n' && bytes[cursor - 1] != b'\r' {
        cursor -= 1;
    }
    cursor
}

fn line_end_after_offset(source_text: &str, offset: usize) -> usize {
    let bytes = source_text.as_bytes();
    let mut cursor = offset.min(bytes.len());
    while cursor < bytes.len() && bytes[cursor] != b'\n' && bytes[cursor] != b'\r' {
        cursor += 1;
    }
    cursor
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TomlInsertion {
    offset: usize,
    preserve_following_separator: bool,
}

fn root_toml_insertion_offset(root: Node<'_>, source_text: &str) -> usize {
    let first_table_start = named_children(root)
        .into_iter()
        .filter(|child| child.kind() == "table" || child.kind() == "table_array_element")
        .map(|child| child.start_byte())
        .min()
        .unwrap_or(source_text.len());
    move_offset_before_preceding_blank_lines(source_text, first_table_start)
}

fn move_offset_before_preceding_blank_lines(source_text: &str, offset: usize) -> usize {
    let mut cursor = offset;
    loop {
        let Some((line_start, line_end)) = previous_line_bounds(source_text, cursor) else {
            return cursor;
        };
        if source_text[line_start..line_end].trim().is_empty() {
            cursor = line_start;
        } else {
            return cursor;
        }
    }
}

fn move_offset_before_table_leading_comment_block(source_text: &str, offset: usize) -> usize {
    let mut cursor = offset;
    let mut comment_start = offset;
    let mut saw_comment = false;

    while let Some((line_start, line_end)) = previous_line_bounds(source_text, cursor) {
        let line = &source_text[line_start..line_end];
        if line.trim_start().starts_with('#') {
            saw_comment = true;
            comment_start = line_start;
            cursor = line_start;
            continue;
        }
        break;
    }

    if saw_comment {
        let before_comment_block =
            move_offset_before_preceding_blank_lines(source_text, comment_start);
        if before_comment_block < comment_start
            && !source_text[..before_comment_block].trim().is_empty()
        {
            return before_comment_block;
        }
    }

    move_offset_before_preceding_blank_lines(source_text, offset)
}

fn previous_line_bounds(source_text: &str, cursor: usize) -> Option<(usize, usize)> {
    if cursor == 0 {
        return None;
    }

    let bytes = source_text.as_bytes();
    let mut end = cursor;
    if end > 0 && bytes[end - 1] == b'\n' {
        end -= 1;
        if end > 0 && bytes[end - 1] == b'\r' {
            end -= 1;
        }
    } else if end > 0 && bytes[end - 1] == b'\r' {
        end -= 1;
    }

    let mut start = end;
    while start > 0 && bytes[start - 1] != b'\n' && bytes[start - 1] != b'\r' {
        start -= 1;
    }
    Some((start, end))
}

fn insert_toml_entry_line(
    source_text: &str,
    insertion: TomlInsertion,
    entry: &str,
) -> Result<String, IdenteditError> {
    if insertion.offset > source_text.len() {
        return Err(IdenteditError::InvalidRequest {
            message: format!(
                "Invalid TOML insertion offset {} for source length {}",
                insertion.offset,
                source_text.len()
            ),
        });
    }

    let line_ending = toml_line_ending_literal(source_text);
    let before = &source_text[..insertion.offset];
    let after = &source_text[insertion.offset..];
    let needs_prefix = !before.is_empty() && !ends_with_line_ending(before);
    let needs_suffix = after.is_empty()
        || !starts_with_line_ending(after)
        || insertion.preserve_following_separator;

    let mut updated = source_text.to_string();
    let mut text = String::new();
    if needs_prefix {
        text.push_str(line_ending);
    }
    text.push_str(entry);
    if needs_suffix {
        text.push_str(line_ending);
    }
    updated.insert_str(insertion.offset, &text);
    Ok(updated)
}

fn insert_toml_table_block(
    source_text: &str,
    insertion: TomlInsertion,
    block: &str,
) -> Result<String, IdenteditError> {
    if insertion.offset > source_text.len() {
        return Err(IdenteditError::InvalidRequest {
            message: format!(
                "Invalid TOML insertion offset {} for source length {}",
                insertion.offset,
                source_text.len()
            ),
        });
    }

    let line_ending = toml_line_ending_literal(source_text);
    let before = &source_text[..insertion.offset];
    let after = &source_text[insertion.offset..];
    let needs_prefix = !before.is_empty() && !ends_with_line_ending(before);
    let needs_leading_separator = !before.trim().is_empty() && !ends_with_blank_line(before);
    let needs_suffix = after.is_empty()
        || !starts_with_line_ending(after)
        || insertion.preserve_following_separator;

    let mut updated = source_text.to_string();
    let mut text = String::new();
    if needs_prefix {
        text.push_str(line_ending);
    }
    if needs_leading_separator {
        text.push_str(line_ending);
    }
    text.push_str(block);
    if needs_suffix {
        text.push_str(line_ending);
    }
    if insertion.preserve_following_separator
        && !after.is_empty()
        && !starts_with_line_ending(after)
    {
        text.push_str(line_ending);
    }
    updated.insert_str(insertion.offset, &text);
    Ok(updated)
}

fn insert_yaml_entry_line(
    root_text: &str,
    offset: usize,
    entry: &str,
    line_ending: &str,
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
    let needs_suffix = after.is_empty() || !starts_with_line_ending(after);

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

fn starts_with_line_ending(value: &str) -> bool {
    value.starts_with('\n') || value.starts_with('\r')
}

fn ends_with_line_ending(value: &str) -> bool {
    value.ends_with('\n') || value.ends_with('\r')
}

fn ends_with_blank_line(value: &str) -> bool {
    value.ends_with("\n\n") || value.ends_with("\r\n\r\n") || value.ends_with("\r\r")
}

fn toml_line_ending_literal(source_text: &str) -> &'static str {
    yaml_line_ending_literal(source_text)
}

fn yaml_line_ending_literal(source_text: &str) -> &'static str {
    let bytes = source_text.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            b'\r' if index + 1 < bytes.len() && bytes[index + 1] == b'\n' => return "\r\n",
            b'\r' => return "\r",
            b'\n' => return "\n",
            _ => index += 1,
        }
    }
    "\n"
}

fn collect_toml_pair_candidates(
    pair: Node<'_>,
    source: &[u8],
    prefix: Vec<PathToken>,
    container: Node<'_>,
    out: &mut Vec<TomlCandidate>,
) {
    let Some((key_segments, value_node)) = toml_pair_key_and_value(pair, source) else {
        return;
    };

    let container_path = prefix.clone();
    let mut full_path = prefix;
    full_path.extend(key_segments.into_iter().map(PathToken::Key));

    out.push(TomlCandidate {
        path: full_path.clone(),
        container_path: container_path.clone(),
        container_span: span_from_node(container),
        container_kind: container.kind().to_string(),
        set_span: span_from_node(value_node),
        set_kind: value_node.kind().to_string(),
        delete_entry_span: span_from_node(pair),
    });

    collect_toml_nested_value_candidates(value_node, source, full_path, out);
}

fn collect_toml_nested_value_candidates(
    value: Node<'_>,
    source: &[u8],
    prefix: Vec<PathToken>,
    out: &mut Vec<TomlCandidate>,
) {
    match value.kind() {
        "inline_table" => {
            for child in named_children(value) {
                if child.kind() == "pair" {
                    collect_toml_pair_candidates(child, source, prefix.clone(), value, out);
                }
            }
        }
        "array" => {
            let elements = named_children(value);
            for (index, element) in elements.into_iter().enumerate() {
                let mut indexed_path = prefix.clone();
                indexed_path.push(PathToken::Index(index));

                out.push(TomlCandidate {
                    path: indexed_path.clone(),
                    container_path: prefix.clone(),
                    container_span: span_from_node(value),
                    container_kind: value.kind().to_string(),
                    set_span: span_from_node(element),
                    set_kind: element.kind().to_string(),
                    delete_entry_span: span_from_node(element),
                });

                collect_toml_nested_value_candidates(element, source, indexed_path, out);
            }
        }
        _ => {}
    }
}

fn toml_table_prefix(table: Node<'_>, source: &[u8]) -> Vec<PathToken> {
    let mut prefix = Vec::new();
    for child in named_children(table) {
        if child.kind() == "pair" {
            break;
        }
        for segment in toml_key_segments(child, source) {
            prefix.push(PathToken::Key(segment));
        }
    }
    prefix
}

fn toml_pair_key_and_value<'a>(pair: Node<'a>, source: &[u8]) -> Option<(Vec<String>, Node<'a>)> {
    let children = named_children(pair);
    if children.len() < 2 {
        return None;
    }

    let key_node = pair
        .child_by_field_name("key")
        .or_else(|| children.first().copied())?;
    let value_node = pair.child_by_field_name("value").or_else(|| {
        children
            .iter()
            .rev()
            .find(|node| node.kind() != "comment")
            .copied()
    })?;
    let key_segments = toml_key_segments(key_node, source);
    if key_segments.is_empty() {
        return None;
    }

    Some((key_segments, value_node))
}

fn toml_key_segments(key_node: Node<'_>, source: &[u8]) -> Vec<String> {
    match key_node.kind() {
        "bare_key" => node_text(key_node, source)
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .into_iter()
            .collect(),
        "quoted_key" => node_text(key_node, source)
            .map(|value| decode_toml_quoted_key(&value))
            .into_iter()
            .collect(),
        "dotted_key" => {
            let mut segments = Vec::new();
            for child in named_children(key_node) {
                segments.extend(toml_key_segments(child, source));
            }
            segments
        }
        _ => Vec::new(),
    }
}

fn decode_toml_quoted_key(raw: &str) -> String {
    if raw.starts_with('\'') && raw.ends_with('\'') && raw.len() >= 2 {
        raw[1..raw.len() - 1].to_string()
    } else {
        decode_toml_basic_string(raw).unwrap_or_else(|| decode_quoted_string(raw))
    }
}

fn decode_toml_basic_string(raw: &str) -> Option<String> {
    if !raw.starts_with('"') || !raw.ends_with('"') {
        return None;
    }

    let document = format!("key = {raw}");
    let value = toml::from_str::<toml::Value>(&document).ok()?;
    value.get("key")?.as_str().map(ToString::to_string)
}

fn yaml_unwrap_node(mut node: Node<'_>) -> Option<Node<'_>> {
    loop {
        match node.kind() {
            "block_node" | "flow_node" | "block_sequence_item" => {
                node = first_named_child(node)?;
            }
            _ => return Some(node),
        }
    }
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

fn decode_quoted_string(raw: &str) -> String {
    if raw.starts_with('"') && raw.ends_with('"') && raw.len() >= 2 {
        serde_json::from_str::<String>(raw).unwrap_or_else(|_| raw.trim_matches('"').to_string())
    } else {
        raw.to_string()
    }
}

fn decode_json_string(text: &str) -> Option<String> {
    serde_json::from_str::<String>(text).ok()
}

fn unique_match<'a>(
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

fn adjusted_delete_span_for_container(
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

fn named_children(node: Node<'_>) -> Vec<Node<'_>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).collect()
}

fn first_named_child(node: Node<'_>) -> Option<Node<'_>> {
    named_children(node).into_iter().next()
}

fn first_non_comment_named_child(node: Node<'_>) -> Option<Node<'_>> {
    named_children(node)
        .into_iter()
        .find(|child| child.kind() != "comment")
}
