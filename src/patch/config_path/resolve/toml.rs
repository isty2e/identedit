use std::collections::HashMap;

use tree_sitter::{Node, Tree};

use crate::error::IdenteditError;
use crate::handle::Span;
use crate::provider::node_text;

use super::super::ConfigPathOperation;
use super::super::render::{append_requires_array_error, parse_toml_value_fragment};
use super::super::syntax::{PathToken, path_tokens_display};
use super::placement::{
    SiblingEntry, ends_with_blank_line, ends_with_line_ending, group_aware_insertion_offset,
    leading_comment_block_start, line_end_with_ending_after_offset, line_ending_literal,
    line_start_before_offset, previous_line_bounds, starts_with_line_ending,
};
use super::{
    ResolvedContainerEdit, adjusted_delete_span_for_container, decode_quoted_string,
    named_children, span_from_node,
};

struct TomlCandidate {
    path: Vec<PathToken>,
    container_path: Vec<PathToken>,
    container_span: Span,
    container_kind: String,
    set_span: Span,
    set_kind: String,
    delete_entry_span: Span,
}

pub(in crate::patch::config_path) fn resolve_toml_path(
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

pub(in crate::patch::config_path) fn rewrite_toml_with_comment_preserving_create_missing(
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
            offset: toml_root_leaf_insertion_offset(tree.root_node(), source_text, leaf_key),
            preserve_following_separator: true,
        }
    } else {
        if let Some(table) =
            find_toml_table_for_path(tree.root_node(), source_text.as_bytes(), parent_path)
        {
            let offset = toml_table_leaf_insertion_offset(table, source_text, leaf_key);
            return insert_toml_entry_line(
                source_text,
                TomlInsertion {
                    offset,
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
            line_ending_literal(source_text),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TomlInsertion {
    offset: usize,
    preserve_following_separator: bool,
}

fn toml_root_leaf_insertion_offset(root: Node<'_>, source_text: &str, leaf_key: &str) -> usize {
    let fallback = root_toml_insertion_offset(root, source_text);
    let Some(entries) = toml_root_sibling_entries(root, source_text) else {
        return fallback;
    };
    group_aware_insertion_offset(source_text, entries, fallback, leaf_key)
}

fn toml_table_leaf_insertion_offset(table: Node<'_>, source_text: &str, leaf_key: &str) -> usize {
    let fallback = move_offset_before_preceding_blank_lines(source_text, table.end_byte());
    let Some(entries) = toml_table_sibling_entries(table, source_text) else {
        return fallback;
    };
    group_aware_insertion_offset(source_text, entries, fallback, leaf_key)
}

fn toml_root_sibling_entries(root: Node<'_>, source_text: &str) -> Option<Vec<SiblingEntry>> {
    let source = source_text.as_bytes();
    let mut entries = Vec::new();
    for child in named_children(root) {
        match child.kind() {
            "pair" => entries.push(toml_sibling_entry(child, source_text, source)?),
            "table" | "table_array_element" => break,
            _ => {}
        }
    }
    Some(entries)
}

fn toml_table_sibling_entries(table: Node<'_>, source_text: &str) -> Option<Vec<SiblingEntry>> {
    let source = source_text.as_bytes();
    let mut entries = Vec::new();
    for child in named_children(table) {
        if child.kind() == "pair" {
            entries.push(toml_sibling_entry(child, source_text, source)?);
        }
    }
    Some(entries)
}

fn toml_sibling_entry(pair: Node<'_>, source_text: &str, source: &[u8]) -> Option<SiblingEntry> {
    let (key_segments, _) = toml_pair_key_and_value(pair, source)?;
    let [key] = key_segments.as_slice() else {
        return None;
    };
    let key_line_start = line_start_before_offset(source_text, pair.start_byte());
    Some(SiblingEntry {
        key: key.clone(),
        insertion_start: leading_comment_block_start(source_text, key_line_start, 0),
        end: line_end_with_ending_after_offset(source_text, pair.end_byte()),
    })
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

    let line_ending = line_ending_literal(source_text);
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

    let line_ending = line_ending_literal(source_text);
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
