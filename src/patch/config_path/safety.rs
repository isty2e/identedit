use std::path::Path;

use tree_sitter::{Node, Parser, Tree};

use crate::error::IdenteditError;

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
    source_text: &str,
) -> Result<(), IdenteditError> {
    let document_count = count_nodes_by_kind(tree.root_node(), "document");
    if document_count > 1 {
        return Err(IdenteditError::InvalidRequest {
            message:
                "Config path create-missing does not support multiple YAML documents in one file"
                    .to_string(),
        });
    }

    if has_yaml_anchor_or_alias(tree.root_node(), source_text) {
        return Err(IdenteditError::InvalidRequest {
            message: "Config path create-missing does not support YAML anchor/alias documents"
                .to_string(),
        });
    }

    Ok(())
}

pub(super) fn has_toml_comments(root: Node<'_>) -> bool {
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

pub(super) fn validate_rendered_config_document(
    format: &ConfigFormat,
    updated_source: &str,
) -> Result<(), IdenteditError> {
    parse_tree_for_format(format, updated_source.as_bytes()).map_err(|error| match error {
        IdenteditError::ParseFailure { .. } => IdenteditError::InvalidRequest {
            message: format!(
                "Config path edit produced invalid {} syntax",
                config_format_name(format)
            ),
        },
        other => other,
    })?;
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

fn has_yaml_anchor_or_alias(root: Node<'_>, source_text: &str) -> bool {
    if source_text.contains("<<: *") {
        return true;
    }

    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        let kind = node.kind();
        if kind.contains("anchor") || kind.contains("alias") {
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
