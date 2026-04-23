use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use tree_sitter::{Node, Parser, Tree};

use crate::error::IdenteditError;
use crate::handle::Span;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ConfigFormat {
    Json,
    Yaml,
    Toml,
}

pub(super) fn is_missing_config_path_error(error: &IdenteditError) -> bool {
    matches!(
        error,
        IdenteditError::InvalidRequest { message }
            if message.contains("was not found") || message.contains("has no root value")
    )
}

pub(super) fn validate_yaml_create_missing_safety(
    tree: &Tree,
    _source_text: &str,
    document_index: Option<usize>,
) -> Result<(), IdenteditError> {
    let parsed_document_count = count_nodes_by_kind(tree.root_node(), "document");
    let document_count = parsed_document_count.max(1);
    if parsed_document_count > 1 && document_index.is_none() {
        return Err(IdenteditError::InvalidRequest {
            message:
                "Config path create-missing on multiple YAML documents requires document_index"
                    .to_string(),
        });
    }
    if let Some(index) = document_index
        && index >= document_count
    {
        return Err(IdenteditError::InvalidRequest {
            message: format!(
                "YAML document_index {index} is out of range; file has {document_count} documents"
            ),
        });
    }

    let scope = selected_yaml_document_scope(tree.root_node(), document_index)?;
    if has_yaml_tag_syntax(scope) {
        return Err(IdenteditError::InvalidRequest {
            message: "Config path create-missing does not support YAML tag documents".to_string(),
        });
    }

    Ok(())
}

pub(super) fn validate_yaml_create_missing_reference_safety(
    tree: &Tree,
    source_text: &str,
    document_index: Option<usize>,
    insertion_parent: Node<'_>,
    raw_path: &str,
) -> Result<(), IdenteditError> {
    validate_yaml_config_path_reference_safety(
        tree,
        source_text,
        document_index,
        span_from_node(insertion_parent),
        raw_path,
    )
}

pub(super) fn validate_yaml_config_path_reference_safety(
    tree: &Tree,
    source_text: &str,
    document_index: Option<usize>,
    target_span: Span,
    raw_path: &str,
) -> Result<(), IdenteditError> {
    let scope = yaml_document_scope_for_span(tree.root_node(), document_index, target_span)?;
    if yaml_merge_key_mapping_contains_span(scope, source_text, target_span) {
        return Err(yaml_non_local_reference_error(raw_path));
    }

    let aliases = yaml_alias_names(scope, source_text);
    if aliases.is_empty() {
        return Ok(());
    }

    for anchor in yaml_anchors(scope, source_text) {
        if aliases.contains(&anchor.name) && span_contains(anchor.owner_span, target_span) {
            return Err(yaml_non_local_reference_error(raw_path));
        }
    }

    Ok(())
}

pub(super) fn validate_rendered_config_document(
    format: &ConfigFormat,
    original_source: &str,
    updated_source: &str,
) -> Result<(), IdenteditError> {
    let tree =
        parse_tree_for_format(format, updated_source.as_bytes()).map_err(|error| match error {
            IdenteditError::ParseFailure { .. } => IdenteditError::InvalidRequest {
                message: format!(
                    "Config path edit produced invalid {} syntax",
                    config_format_name(format)
                ),
            },
            other => other,
        })?;
    if matches!(format, ConfigFormat::Yaml) {
        let original_tree = parse_tree_for_format(format, original_source.as_bytes()).map_err(
            |error| match error {
                IdenteditError::ParseFailure { .. } => IdenteditError::InvalidRequest {
                    message: "Config path edit could not validate the original YAML document"
                        .to_string(),
                },
                other => other,
            },
        )?;
        let original_document_count = count_nodes_by_kind(original_tree.root_node(), "document");
        let updated_document_count = count_nodes_by_kind(tree.root_node(), "document");
        if updated_document_count > original_document_count {
            return Err(IdenteditError::InvalidRequest {
                message:
                    "Config path edit introduced additional YAML documents, which is not supported"
                        .to_string(),
            });
        }
        let original_references =
            yaml_reference_or_tag_fingerprints(original_tree.root_node(), original_source);
        let updated_references =
            yaml_reference_or_tag_fingerprints(tree.root_node(), updated_source);
        if original_references != updated_references {
            return Err(IdenteditError::InvalidRequest {
                message:
                    "Config path edit introduced or changed YAML anchor/alias/tag/merge syntax, which is not supported"
                        .to_string(),
            });
        }
    }
    Ok(())
}

pub(super) fn detect_config_format(file: &Path) -> Result<ConfigFormat, IdenteditError> {
    let extension = file
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .ok_or_else(|| IdenteditError::InvalidRequest {
            message: format!(
                "Config path operations require a file extension; '{}' has none",
                file.display()
            ),
        })?;

    match extension.as_str() {
        "json" => Ok(ConfigFormat::Json),
        "yaml" | "yml" => Ok(ConfigFormat::Yaml),
        "toml" => Ok(ConfigFormat::Toml),
        _ => Err(IdenteditError::InvalidRequest {
            message: format!(
                "Config path operations support only .json, .yaml/.yml, and .toml files (got .{extension})"
            ),
        }),
    }
}

pub(super) fn parse_tree_for_format(
    format: &ConfigFormat,
    source: &[u8],
) -> Result<Tree, IdenteditError> {
    let mut parser = Parser::new();
    let language: tree_sitter::Language = match format {
        ConfigFormat::Json => tree_sitter_json::LANGUAGE.into(),
        ConfigFormat::Yaml => tree_sitter_yaml::LANGUAGE.into(),
        ConfigFormat::Toml => tree_sitter_toml::LANGUAGE.into(),
    };

    parser
        .set_language(&language)
        .map_err(|error| IdenteditError::LanguageSetup {
            message: error.to_string(),
        })?;

    let parse_buffer;
    let parse_source: &[u8] =
        if matches!(format, ConfigFormat::Toml) && has_cr_only_newlines(source) {
            parse_buffer = source
                .iter()
                .map(|byte| if *byte == b'\r' { b'\n' } else { *byte })
                .collect::<Vec<_>>();
            &parse_buffer
        } else {
            source
        };

    let tree = parser
        .parse(parse_source, None)
        .ok_or_else(|| IdenteditError::ParseFailure {
            provider: provider_name(format),
            message: "Tree-sitter returned no syntax tree".to_string(),
        })?;

    if tree.root_node().has_error() {
        return Err(IdenteditError::ParseFailure {
            provider: provider_name(format),
            message: "Syntax errors detected while resolving config path".to_string(),
        });
    }

    Ok(tree)
}

fn has_yaml_tag_syntax(root: Node<'_>) -> bool {
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        let kind = node.kind();
        if kind == "tag" || kind == "tag_directive" {
            return true;
        }
        for index in 0..node.child_count() {
            if let Some(child) = node.child(index as u32) {
                stack.push(child);
            }
        }
    }
    false
}

fn selected_yaml_document_scope(
    root: Node<'_>,
    document_index: Option<usize>,
) -> Result<Node<'_>, IdenteditError> {
    let documents = yaml_document_nodes(root);
    if let Some(index) = document_index {
        if let Some(document) = documents.get(index) {
            return Ok(*document);
        }
        let document_count = documents.len().max(1);
        return Err(IdenteditError::InvalidRequest {
            message: format!(
                "YAML document_index {index} is out of range; file has {document_count} documents"
            ),
        });
    }
    Ok(documents.first().copied().unwrap_or(root))
}

fn yaml_document_scope_for_span(
    root: Node<'_>,
    document_index: Option<usize>,
    target_span: Span,
) -> Result<Node<'_>, IdenteditError> {
    if document_index.is_some() {
        return selected_yaml_document_scope(root, document_index);
    }

    let documents = yaml_document_nodes(root);
    if documents.is_empty() {
        return Ok(root);
    }
    for document in documents {
        if span_contains(span_from_node(document), target_span) {
            return Ok(document);
        }
    }
    Ok(root)
}

fn yaml_document_nodes(root: Node<'_>) -> Vec<Node<'_>> {
    let mut documents = Vec::new();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if node.kind() == "document" {
            documents.push(node);
            continue;
        }
        for index in (0..node.child_count()).rev() {
            if let Some(child) = node.child(index as u32) {
                stack.push(child);
            }
        }
    }
    documents.sort_by_key(Node::start_byte);
    documents
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct YamlAnchor {
    name: String,
    owner_span: Span,
}

fn yaml_anchors(root: Node<'_>, source_text: &str) -> Vec<YamlAnchor> {
    let mut anchors = Vec::new();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if node.kind() == "anchor"
            && let Some(name) = yaml_anchor_or_alias_name(node, source_text, "anchor_name", '&')
        {
            anchors.push(YamlAnchor {
                name,
                owner_span: yaml_anchor_owner_span(node),
            });
        }
        for index in 0..node.child_count() {
            if let Some(child) = node.child(index as u32) {
                stack.push(child);
            }
        }
    }
    anchors
}

fn yaml_alias_names(root: Node<'_>, source_text: &str) -> BTreeSet<String> {
    let mut aliases = BTreeSet::new();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if node.kind() == "alias"
            && let Some(name) = yaml_anchor_or_alias_name(node, source_text, "alias_name", '*')
        {
            aliases.insert(name);
        }
        for index in 0..node.child_count() {
            if let Some(child) = node.child(index as u32) {
                stack.push(child);
            }
        }
    }
    aliases
}

fn yaml_anchor_or_alias_name(
    node: Node<'_>,
    source_text: &str,
    child_kind: &str,
    sigil: char,
) -> Option<String> {
    for index in 0..node.named_child_count() {
        let child = node.named_child(index as u32)?;
        if child.kind() == child_kind {
            return node_text(child, source_text).map(|value| value.to_string());
        }
    }
    node_text(node, source_text).map(|value| value.trim_start_matches(sigil).to_string())
}

fn yaml_anchor_owner_span(anchor: Node<'_>) -> Span {
    let mut current = anchor;
    while let Some(parent) = current.parent() {
        if matches!(parent.kind(), "block_node" | "flow_node") {
            return span_from_node(parent);
        }
        current = parent;
    }
    span_from_node(anchor)
}

fn yaml_merge_key_mapping_contains_span(
    scope: Node<'_>,
    source_text: &str,
    target_span: Span,
) -> bool {
    let mut stack = vec![scope];
    while let Some(node) = stack.pop() {
        if matches!(node.kind(), "block_mapping" | "flow_mapping")
            && yaml_mapping_has_plain_merge_key(node, source_text)
            && span_contains(span_from_node(node), target_span)
        {
            return true;
        }
        for index in 0..node.child_count() {
            if let Some(child) = node.child(index as u32) {
                stack.push(child);
            }
        }
    }
    false
}

fn yaml_mapping_has_plain_merge_key(mapping: Node<'_>, source_text: &str) -> bool {
    let pair_kind = match mapping.kind() {
        "block_mapping" => "block_mapping_pair",
        "flow_mapping" => "flow_pair",
        _ => return false,
    };
    for index in 0..mapping.named_child_count() {
        let Some(pair) = mapping.named_child(index as u32) else {
            continue;
        };
        if pair.kind() != pair_kind {
            continue;
        }
        let Some(key_node) = pair.child_by_field_name("key") else {
            continue;
        };
        let Some(unwrapped) = yaml_unwrap_node(key_node) else {
            continue;
        };
        if unwrapped.kind() == "plain_scalar" && node_text(unwrapped, source_text) == Some("<<") {
            return true;
        }
    }
    false
}

fn yaml_reference_or_tag_fingerprints(
    root: Node<'_>,
    source_text: &str,
) -> BTreeMap<String, usize> {
    let mut fingerprints = BTreeMap::new();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if matches!(node.kind(), "anchor" | "alias" | "tag" | "tag_directive") {
            let text = node_text(node, source_text).unwrap_or("");
            *fingerprints
                .entry(format!("{}:{text}", node.kind()))
                .or_insert(0) += 1;
        }
        if matches!(node.kind(), "block_mapping" | "flow_mapping")
            && yaml_mapping_has_plain_merge_key(node, source_text)
        {
            *fingerprints.entry("merge:<<".to_string()).or_insert(0) += 1;
        }
        for index in 0..node.child_count() {
            if let Some(child) = node.child(index as u32) {
                stack.push(child);
            }
        }
    }
    fingerprints
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

fn first_named_child(node: Node<'_>) -> Option<Node<'_>> {
    for index in 0..node.named_child_count() {
        if let Some(child) = node.named_child(index as u32) {
            return Some(child);
        }
    }
    None
}

fn node_text<'a>(node: Node<'_>, source_text: &'a str) -> Option<&'a str> {
    source_text.get(node.start_byte()..node.end_byte())
}

fn span_from_node(node: Node<'_>) -> Span {
    Span {
        start: node.start_byte(),
        end: node.end_byte(),
    }
}

fn span_contains(outer: Span, inner: Span) -> bool {
    outer.start <= inner.start && inner.end <= outer.end
}

fn yaml_non_local_reference_error(raw_path: &str) -> IdenteditError {
    IdenteditError::InvalidRequest {
        message: format!(
            "Config path edit for '{raw_path}' would modify YAML anchor/alias/merge semantics; target a concrete mapping without aliases or merge keys"
        ),
    }
}

fn count_nodes_by_kind(root: Node<'_>, expected_kind: &str) -> usize {
    let mut count = 0usize;
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if node.kind() == expected_kind {
            count += 1;
        }
        for index in 0..node.child_count() {
            if let Some(child) = node.child(index as u32) {
                stack.push(child);
            }
        }
    }
    count
}

pub(super) fn has_yaml_comments(root: Node<'_>) -> bool {
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if node.kind().contains("comment") {
            return true;
        }
        for index in 0..node.child_count() {
            if let Some(child) = node.child(index as u32) {
                stack.push(child);
            }
        }
    }
    false
}

fn config_format_name(format: &ConfigFormat) -> &'static str {
    match format {
        ConfigFormat::Json => "JSON",
        ConfigFormat::Yaml => "YAML",
        ConfigFormat::Toml => "TOML",
    }
}

fn has_cr_only_newlines(source: &[u8]) -> bool {
    let mut has_cr = false;
    let mut has_lf = false;
    let mut index = 0usize;

    while index < source.len() {
        match source[index] {
            b'\r' => {
                has_cr = true;
                if index + 1 < source.len() && source[index + 1] == b'\n' {
                    return false;
                }
            }
            b'\n' => {
                has_lf = true;
            }
            _ => {}
        }
        index += 1;
    }

    has_cr && !has_lf
}

fn provider_name(format: &ConfigFormat) -> &'static str {
    match format {
        ConfigFormat::Json => "json",
        ConfigFormat::Yaml => "tree-sitter-yaml",
        ConfigFormat::Toml => "tree-sitter-toml",
    }
}
