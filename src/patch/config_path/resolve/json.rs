use tree_sitter::{Node, Tree};

use crate::error::IdenteditError;
use crate::provider::node_text;

use super::super::ConfigPathOperation;
use super::super::render::{append_requires_array_error, array_index_out_of_bounds_error};
use super::super::syntax::{PathToken, expected_path_container_error};
use super::{
    ResolvedContainerEdit, adjusted_delete_span_for_container, first_named_child, named_children,
    span_from_node, unique_match,
};

mod create_missing;

pub(in crate::patch::config_path) use create_missing::{
    render_missing_json_object, rewrite_json_with_create_missing,
};

pub(in crate::patch::config_path) fn validate_json_set_value(
    new_text: &str,
) -> Result<(), IdenteditError> {
    serde_json::from_str::<&serde_json::value::RawValue>(new_text)
        .map(|_| ())
        .map_err(|error| IdenteditError::InvalidRequest {
            message: format!("Config path set value must be a single valid JSON value: {error}"),
        })
}

pub(in crate::patch::config_path) fn resolve_json_path(
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

                let matched_pair = unique_match(
                    raw_path,
                    token,
                    matching_json_pairs(current, source, expected_key),
                )?;
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

pub(in crate::patch::config_path) fn json_root_value(root: Node<'_>) -> Option<Node<'_>> {
    let node = root;
    if node.kind() == "document" {
        if let Some(value) = node.child_by_field_name("value") {
            return Some(value);
        }
        return first_named_child(node);
    }
    first_named_child(node).or(Some(node))
}

fn decode_json_string(text: &str) -> Option<String> {
    serde_json::from_str::<String>(text).ok()
}

fn matching_json_pairs<'tree>(
    object: Node<'tree>,
    source: &[u8],
    expected_key: &str,
) -> Vec<Node<'tree>> {
    named_children(object)
        .into_iter()
        .filter(|child| {
            child.kind() == "pair"
                && child
                    .child_by_field_name("key")
                    .and_then(|key| node_text(key, source))
                    .and_then(|raw| decode_json_string(&raw))
                    .is_some_and(|key| key == expected_key)
        })
        .collect()
}
