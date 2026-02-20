use std::collections::HashMap;
use std::path::Path;

use tree_sitter::{Node, Parser, Tree};

use crate::changeset::{OpKind, TransformTarget};
use crate::error::IdenteditError;
use crate::handle::{SelectionHandle, Span};
use crate::hash::hash_bytes;
use crate::provider::node_text;
use crate::transform::parse_handles_for_source;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigPathOperation {
    Set {
        new_text: String,
        create_missing: bool,
    },
    Append {
        new_text: String,
    },
    Delete,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedConfigPatch {
    pub target: TransformTarget,
    pub op: OpKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum PathToken {
    Key(String),
    Index(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResolvedContainerEdit {
    container_span: Span,
    container_kind: String,
    replace_span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ConfigFormat {
    Json,
    Yaml,
    Toml,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TomlCandidate {
    path: Vec<PathToken>,
    container_span: Span,
    container_kind: String,
    set_span: Span,
    set_kind: String,
    delete_entry_span: Span,
}

struct CreateMissingSetRequest<'a> {
    format: &'a ConfigFormat,
    tree: &'a Tree,
    source: &'a [u8],
    source_text: &'a str,
    path_tokens: &'a [PathToken],
    raw_path: &'a str,
    new_text: &'a str,
}

pub fn resolve_config_path_operation(
    file: &Path,
    raw_path: &str,
    expected_file_hash: Option<&str>,
    operation: ConfigPathOperation,
) -> Result<ResolvedConfigPatch, IdenteditError> {
    let source = std::fs::read(file).map_err(|error| IdenteditError::io(file, error))?;
    let source_text = std::str::from_utf8(&source).map_err(|_| IdenteditError::InvalidRequest {
        message: format!(
            "Config path operations require UTF-8 source; file '{}' is not UTF-8",
            file.display()
        ),
    })?;

    if let Some(expected_hash) = expected_file_hash {
        let actual_hash = hash_bytes(&source);
        if actual_hash != expected_hash {
            return Err(IdenteditError::PreconditionFailed {
                expected_hash: expected_hash.to_string(),
                actual_hash,
            });
        }
    }

    let format = detect_config_format(file)?;
    let path_tokens = parse_config_path(raw_path)?;

    if let ConfigPathOperation::Set {
        new_text,
        create_missing: true,
    } = &operation
        && matches!(format, ConfigFormat::Json)
        && source.is_empty()
    {
        let updated = render_json_with_create_missing("", &path_tokens, raw_path, new_text)?;
        return Ok(ResolvedConfigPatch {
            target: TransformTarget::FileStart {
                expected_file_hash: hash_bytes(&source),
            },
            op: OpKind::Insert { new_text: updated },
        });
    }

    let tree = parse_tree_for_format(&format, &source)?;
    if let ConfigPathOperation::Set {
        new_text,
        create_missing: true,
    } = &operation
    {
        let strict_probe = ConfigPathOperation::Set {
            new_text: String::new(),
            create_missing: false,
        };
        let strict_resolved = match format {
            ConfigFormat::Json => {
                resolve_json_path(&tree, &source, &path_tokens, &strict_probe, raw_path)
            }
            ConfigFormat::Yaml => {
                resolve_yaml_path(&tree, &source, &path_tokens, &strict_probe, raw_path)
            }
            ConfigFormat::Toml => {
                resolve_toml_path(&tree, &source, &path_tokens, &strict_probe, raw_path)
            }
        };
        match strict_resolved {
            Ok(resolved) => {
                return build_resolved_patch_from_container_edit(
                    file, &source, source_text, resolved, new_text,
                );
            }
            Err(error) if !is_missing_config_path_error(&error) => return Err(error),
            Err(_) => {}
        }

        return resolve_config_path_set_with_create_missing(
            file,
            CreateMissingSetRequest {
                format: &format,
                tree: &tree,
                source: &source,
                source_text,
                path_tokens: &path_tokens,
                raw_path,
                new_text,
            },
        );
    }

    let resolved = match format {
        ConfigFormat::Json => {
            resolve_json_path(&tree, &source, &path_tokens, &operation, raw_path)?
        }
        ConfigFormat::Yaml => {
            resolve_yaml_path(&tree, &source, &path_tokens, &operation, raw_path)?
        }
        ConfigFormat::Toml => {
            resolve_toml_path(&tree, &source, &path_tokens, &operation, raw_path)?
        }
    };

    let replacement = match &operation {
        ConfigPathOperation::Set { new_text, .. } => new_text.clone(),
        ConfigPathOperation::Append { new_text } => render_append_array_replacement(
            source_text,
            resolved.container_span,
            &resolved.container_kind,
            new_text,
            raw_path,
        )?,
        ConfigPathOperation::Delete => String::new(),
    };
    build_resolved_patch_from_container_edit(file, &source, source_text, resolved, &replacement)
}

fn build_resolved_patch_from_container_edit(
    file: &Path,
    source: &[u8],
    source_text: &str,
    resolved: ResolvedContainerEdit,
    replacement: &str,
) -> Result<ResolvedConfigPatch, IdenteditError> {
    let handles = parse_handles_for_source(file, source)?;
    let container_handle = find_handle_for_span(
        file,
        &handles,
        resolved.container_span,
        &resolved.container_kind,
    )?;
    let updated_container_text = rewrite_container_text(
        source_text,
        resolved.container_span,
        resolved.replace_span,
        replacement,
    )?;

    let target = TransformTarget::node(
        container_handle.identity,
        container_handle.kind,
        Some(container_handle.span),
        container_handle.expected_old_hash,
    );

    Ok(ResolvedConfigPatch {
        target,
        op: OpKind::Replace {
            new_text: updated_container_text,
        },
    })
}

fn resolve_config_path_set_with_create_missing(
    file: &Path,
    request: CreateMissingSetRequest<'_>,
) -> Result<ResolvedConfigPatch, IdenteditError> {
    if matches!(request.format, ConfigFormat::Yaml) {
        validate_yaml_create_missing_safety(request.tree, request.source_text)?;
    }
    if matches!(request.format, ConfigFormat::Toml)
        && has_toml_comments(request.tree.root_node())
    {
        return Err(IdenteditError::InvalidRequest {
            message: "Config path create-missing does not support TOML comments yet".to_string(),
        });
    }

    let updated_root_text = match request.format {
        ConfigFormat::Json => render_json_with_create_missing(
            request.source_text,
            request.path_tokens,
            request.raw_path,
            request.new_text,
        )?,
        ConfigFormat::Yaml => render_yaml_with_create_missing(
            request.source_text,
            request.path_tokens,
            request.raw_path,
            request.new_text,
        )?,
        ConfigFormat::Toml => render_toml_with_create_missing(
            request.source_text,
            request.path_tokens,
            request.raw_path,
            request.new_text,
        )?,
    };

    if matches!(request.format, ConfigFormat::Json) && request.source.is_empty() {
        return Ok(ResolvedConfigPatch {
            target: TransformTarget::FileStart {
                expected_file_hash: hash_bytes(request.source),
            },
            op: OpKind::Insert {
                new_text: updated_root_text,
            },
        });
    }

    let root_node = match request.format {
        ConfigFormat::Json => json_root_value(request.tree.root_node()).ok_or_else(|| {
            IdenteditError::InvalidRequest {
                message: "JSON document has no root value".to_string(),
            }
        })?,
        ConfigFormat::Yaml => yaml_root_value(request.tree.root_node()).ok_or_else(|| {
            IdenteditError::InvalidRequest {
                message: "YAML document has no root value".to_string(),
            }
        })?,
        ConfigFormat::Toml => request.tree.root_node(),
    };

    let root_span = span_from_node(root_node);
    let root_kind = root_node.kind().to_string();
    let handles = parse_handles_for_source(file, request.source)?;
    let container_handle = find_handle_for_span(file, &handles, root_span, &root_kind)?;
    let target = TransformTarget::node(
        container_handle.identity,
        container_handle.kind,
        Some(container_handle.span),
        container_handle.expected_old_hash,
    );

    Ok(ResolvedConfigPatch {
        target,
        op: OpKind::Replace {
            new_text: updated_root_text,
        },
    })
}

fn render_json_with_create_missing(
    source_text: &str,
    path_tokens: &[PathToken],
    raw_path: &str,
    new_text: &str,
) -> Result<String, IdenteditError> {
    let mut root: serde_json::Value = if source_text.trim().is_empty() {
        serde_json::Value::Object(serde_json::Map::new())
    } else {
        serde_json::from_str(source_text).map_err(|error| IdenteditError::InvalidRequest {
            message: format!("Config path create-missing could not parse JSON document: {error}"),
        })?
    };
    let parsed_new_value: serde_json::Value =
        serde_json::from_str(new_text).map_err(|error| IdenteditError::InvalidRequest {
            message: format!("Config path set value is not valid JSON: {error}"),
        })?;
    apply_json_set_create_missing(&mut root, path_tokens, raw_path, &parsed_new_value)?;

    let rendered =
        serde_json::to_string_pretty(&root).map_err(|error| IdenteditError::InvalidRequest {
            message: format!(
                "Config path create-missing could not serialize JSON document: {error}"
            ),
        })?;
    Ok(apply_source_line_ending_style(&rendered, source_text))
}

fn render_yaml_with_create_missing(
    source_text: &str,
    path_tokens: &[PathToken],
    raw_path: &str,
    new_text: &str,
) -> Result<String, IdenteditError> {
    let mut root: serde_yaml::Value =
        serde_yaml::from_str(source_text).map_err(|error| IdenteditError::InvalidRequest {
            message: format!("Config path create-missing could not parse YAML document: {error}"),
        })?;
    let parsed_new_value: serde_yaml::Value =
        serde_yaml::from_str(new_text).map_err(|error| IdenteditError::InvalidRequest {
            message: format!("Config path set value is not valid YAML: {error}"),
        })?;
    apply_yaml_set_create_missing(&mut root, path_tokens, raw_path, &parsed_new_value)?;

    let rendered =
        serde_yaml::to_string(&root).map_err(|error| IdenteditError::InvalidRequest {
            message: format!(
                "Config path create-missing could not serialize YAML document: {error}"
            ),
        })?;
    let normalized = rendered
        .strip_prefix("---\n")
        .unwrap_or(&rendered)
        .to_string();
    Ok(apply_source_line_ending_style(&normalized, source_text))
}

fn render_toml_with_create_missing(
    source_text: &str,
    path_tokens: &[PathToken],
    raw_path: &str,
    new_text: &str,
) -> Result<String, IdenteditError> {
    let parse_input = if source_text.contains('\r') && !source_text.contains('\n') {
        source_text.replace('\r', "\n")
    } else {
        source_text.to_string()
    };
    let mut root: toml::Value =
        toml::from_str(&parse_input).map_err(|error| IdenteditError::InvalidRequest {
            message: format!("Config path create-missing could not parse TOML document: {error}"),
        })?;
    let parsed_new_value = parse_toml_value_fragment(new_text)?;
    apply_toml_set_create_missing(&mut root, path_tokens, raw_path, &parsed_new_value)?;

    let rendered =
        toml::to_string_pretty(&root).map_err(|error| IdenteditError::InvalidRequest {
            message: format!(
                "Config path create-missing could not serialize TOML document: {error}"
            ),
        })?;
    Ok(apply_source_line_ending_style(&rendered, source_text))
}

fn parse_toml_value_fragment(fragment: &str) -> Result<toml::Value, IdenteditError> {
    let wrapped = format!("__identedit_tmp__ = {fragment}");
    let mut table: toml::Table =
        toml::from_str(&wrapped).map_err(|error| IdenteditError::InvalidRequest {
            message: format!("Config path set value is not valid TOML value text: {error}"),
        })?;
    table
        .remove("__identedit_tmp__")
        .ok_or_else(|| IdenteditError::InvalidRequest {
            message: "Config path set value parsing produced no value".to_string(),
        })
}

fn apply_json_set_create_missing(
    current: &mut serde_json::Value,
    path_tokens: &[PathToken],
    raw_path: &str,
    new_value: &serde_json::Value,
) -> Result<(), IdenteditError> {
    let Some((head, tail)) = path_tokens.split_first() else {
        *current = new_value.clone();
        return Ok(());
    };

    match head {
        PathToken::Key(key) => {
            let object = match current {
                serde_json::Value::Object(object) => object,
                _ => {
                    return Err(expected_path_container_error(
                        raw_path,
                        head,
                        json_value_kind_name(current),
                    ));
                }
            };
            if tail.is_empty() {
                object.insert(key.clone(), new_value.clone());
                return Ok(());
            }
            if !object.contains_key(key) {
                object.insert(key.clone(), empty_json_container_for_token(&tail[0]));
            }
            let child = object
                .get_mut(key)
                .ok_or_else(|| IdenteditError::InvalidRequest {
                    message: format!("Config path '{raw_path}' segment '{key}' was not found"),
                })?;
            apply_json_set_create_missing(child, tail, raw_path, new_value)
        }
        PathToken::Index(index) => {
            let array = match current {
                serde_json::Value::Array(array) => array,
                _ => {
                    return Err(expected_path_container_error(
                        raw_path,
                        head,
                        json_value_kind_name(current),
                    ));
                }
            };
            if *index >= array.len() {
                return Err(array_index_out_of_bounds_error(
                    raw_path,
                    *index,
                    array.len(),
                ));
            }
            if tail.is_empty() {
                array[*index] = new_value.clone();
                return Ok(());
            }
            apply_json_set_create_missing(&mut array[*index], tail, raw_path, new_value)
        }
    }
}

fn apply_yaml_set_create_missing(
    current: &mut serde_yaml::Value,
    path_tokens: &[PathToken],
    raw_path: &str,
    new_value: &serde_yaml::Value,
) -> Result<(), IdenteditError> {
    let Some((head, tail)) = path_tokens.split_first() else {
        *current = new_value.clone();
        return Ok(());
    };

    match head {
        PathToken::Key(key) => {
            let mapping = match current {
                serde_yaml::Value::Mapping(mapping) => mapping,
                _ => {
                    return Err(expected_path_container_error(
                        raw_path,
                        head,
                        yaml_value_kind_name(current),
                    ));
                }
            };
            let key_value = serde_yaml::Value::String(key.clone());
            if tail.is_empty() {
                mapping.insert(key_value, new_value.clone());
                return Ok(());
            }
            if !mapping.contains_key(&key_value) {
                mapping.insert(key_value.clone(), empty_yaml_container_for_token(&tail[0]));
            }
            let child =
                mapping
                    .get_mut(&key_value)
                    .ok_or_else(|| IdenteditError::InvalidRequest {
                        message: format!("Config path '{raw_path}' segment '{key}' was not found"),
                    })?;
            apply_yaml_set_create_missing(child, tail, raw_path, new_value)
        }
        PathToken::Index(index) => {
            let sequence = match current {
                serde_yaml::Value::Sequence(sequence) => sequence,
                _ => {
                    return Err(expected_path_container_error(
                        raw_path,
                        head,
                        yaml_value_kind_name(current),
                    ));
                }
            };
            if *index >= sequence.len() {
                return Err(array_index_out_of_bounds_error(
                    raw_path,
                    *index,
                    sequence.len(),
                ));
            }
            if tail.is_empty() {
                sequence[*index] = new_value.clone();
                return Ok(());
            }
            apply_yaml_set_create_missing(&mut sequence[*index], tail, raw_path, new_value)
        }
    }
}

fn apply_toml_set_create_missing(
    current: &mut toml::Value,
    path_tokens: &[PathToken],
    raw_path: &str,
    new_value: &toml::Value,
) -> Result<(), IdenteditError> {
    let Some((head, tail)) = path_tokens.split_first() else {
        *current = new_value.clone();
        return Ok(());
    };

    match head {
        PathToken::Key(key) => {
            let table = match current {
                toml::Value::Table(table) => table,
                _ => {
                    return Err(expected_path_container_error(
                        raw_path,
                        head,
                        toml_value_kind_name(current),
                    ));
                }
            };
            if tail.is_empty() {
                table.insert(key.clone(), new_value.clone());
                return Ok(());
            }
            if !table.contains_key(key) {
                table.insert(key.clone(), empty_toml_container_for_token(&tail[0]));
            }
            let child = table
                .get_mut(key)
                .ok_or_else(|| IdenteditError::InvalidRequest {
                    message: format!("Config path '{raw_path}' segment '{key}' was not found"),
                })?;
            apply_toml_set_create_missing(child, tail, raw_path, new_value)
        }
        PathToken::Index(index) => {
            let array = match current {
                toml::Value::Array(array) => array,
                _ => {
                    return Err(expected_path_container_error(
                        raw_path,
                        head,
                        toml_value_kind_name(current),
                    ));
                }
            };
            if *index >= array.len() {
                return Err(array_index_out_of_bounds_error(
                    raw_path,
                    *index,
                    array.len(),
                ));
            }
            if tail.is_empty() {
                array[*index] = new_value.clone();
                return Ok(());
            }
            apply_toml_set_create_missing(&mut array[*index], tail, raw_path, new_value)
        }
    }
}

fn empty_json_container_for_token(next: &PathToken) -> serde_json::Value {
    match next {
        PathToken::Key(_) => serde_json::Value::Object(serde_json::Map::new()),
        PathToken::Index(_) => serde_json::Value::Array(Vec::new()),
    }
}

fn empty_yaml_container_for_token(next: &PathToken) -> serde_yaml::Value {
    match next {
        PathToken::Key(_) => serde_yaml::Value::Mapping(serde_yaml::Mapping::new()),
        PathToken::Index(_) => serde_yaml::Value::Sequence(Vec::new()),
    }
}

fn empty_toml_container_for_token(next: &PathToken) -> toml::Value {
    match next {
        PathToken::Key(_) => toml::Value::Table(toml::Table::new()),
        PathToken::Index(_) => toml::Value::Array(Vec::new()),
    }
}

fn json_value_kind_name(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "boolean",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}

fn yaml_value_kind_name(value: &serde_yaml::Value) -> &'static str {
    match value {
        serde_yaml::Value::Null => "null",
        serde_yaml::Value::Bool(_) => "boolean",
        serde_yaml::Value::Number(_) => "number",
        serde_yaml::Value::String(_) => "string",
        serde_yaml::Value::Sequence(_) => "sequence",
        serde_yaml::Value::Mapping(_) => "mapping",
        serde_yaml::Value::Tagged(_) => "tagged",
    }
}

fn toml_value_kind_name(value: &toml::Value) -> &'static str {
    match value {
        toml::Value::String(_) => "string",
        toml::Value::Integer(_) => "integer",
        toml::Value::Float(_) => "float",
        toml::Value::Boolean(_) => "boolean",
        toml::Value::Datetime(_) => "datetime",
        toml::Value::Array(_) => "array",
        toml::Value::Table(_) => "table",
    }
}

fn array_index_out_of_bounds_error(
    raw_path: &str,
    expected_index: usize,
    len: usize,
) -> IdenteditError {
    IdenteditError::InvalidRequest {
        message: format!(
            "Config path '{raw_path}' index [{expected_index}] is out of range (len={len}). Array index out-of-bounds is always an error; use a dedicated append operation if needed."
        ),
    }
}

fn render_append_array_replacement(
    source_text: &str,
    container_span: Span,
    container_kind: &str,
    new_text: &str,
    raw_path: &str,
) -> Result<String, IdenteditError> {
    let array_text = source_text.get(container_span.start..container_span.end).ok_or_else(|| {
        IdenteditError::InvalidRequest {
            message: format!(
                "Invalid append span [{}, {}) while resolving config path '{raw_path}'",
                container_span.start, container_span.end
            ),
        }
    })?;

    match container_kind {
        "array" | "flow_sequence" => {
            append_to_comma_delimited_array_text(array_text, new_text, raw_path)
        }
        "block_sequence" => append_to_block_sequence_text(
            array_text,
            new_text,
            raw_path,
            &indentation_before_offset(source_text, container_span.start),
        ),
        _ => Err(append_requires_array_error(raw_path, container_kind)),
    }
}

fn append_to_comma_delimited_array_text(
    array_text: &str,
    new_text: &str,
    raw_path: &str,
) -> Result<String, IdenteditError> {
    let open = array_text
        .find('[')
        .ok_or_else(|| append_requires_array_error(raw_path, "unknown"))?;
    let close = array_text
        .rfind(']')
        .ok_or_else(|| append_requires_array_error(raw_path, "unknown"))?;
    if open >= close {
        return Err(append_requires_array_error(raw_path, "unknown"));
    }

    let inner = &array_text[open + 1..close];
    let mut result = array_text.to_string();

    if inner.trim().is_empty() {
        result.replace_range(open + 1..close, new_text);
        return Ok(result);
    }

    let mut insert_at = close;
    while insert_at > open + 1 {
        let byte = result.as_bytes()[insert_at - 1];
        if byte == b' ' || byte == b'\t' || byte == b'\n' || byte == b'\r' {
            insert_at -= 1;
        } else {
            break;
        }
    }

    let insertion = if inner.contains('\n') || inner.contains('\r') {
        let line_ending = line_ending_literal(array_text);
        let indent = indentation_of_last_value_line(array_text, insert_at);
        format!(",{line_ending}{indent}{new_text}")
    } else {
        format!(", {new_text}")
    };
    result.insert_str(insert_at, &insertion);
    Ok(result)
}

fn append_to_block_sequence_text(
    sequence_text: &str,
    new_text: &str,
    raw_path: &str,
    base_indent: &str,
) -> Result<String, IdenteditError> {
    let indent = first_block_sequence_item_indent(sequence_text)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| base_indent.to_string());
    if indent.is_empty() {
        return Err(append_requires_array_error(raw_path, "block_sequence"));
    }
    let separator = if sequence_text.ends_with('\n') || sequence_text.ends_with('\r') {
        ""
    } else {
        line_ending_literal(sequence_text)
    };
    Ok(format!("{sequence_text}{separator}{indent}- {new_text}"))
}

fn first_block_sequence_item_indent(sequence_text: &str) -> Option<String> {
    let bytes = sequence_text.as_bytes();
    let mut start = 0usize;

    while start < bytes.len() {
        let mut end = start;
        while end < bytes.len() && bytes[end] != b'\n' && bytes[end] != b'\r' {
            end += 1;
        }
        let line = &sequence_text[start..end];
        let trimmed = line.trim_start_matches([' ', '\t']);
        if trimmed.starts_with('-') {
            let indent_len = line.len() - trimmed.len();
            return Some(line[..indent_len].to_string());
        }

        if end >= bytes.len() {
            break;
        }
        if bytes[end] == b'\r' && end + 1 < bytes.len() && bytes[end + 1] == b'\n' {
            start = end + 2;
        } else {
            start = end + 1;
        }
    }

    None
}

fn indentation_of_last_value_line(text: &str, end: usize) -> String {
    let prefix = &text[..end];
    let line_start = prefix
        .rfind('\n')
        .map(|index| index + 1)
        .or_else(|| prefix.rfind('\r').map(|index| index + 1))
        .unwrap_or(0);
    prefix[line_start..]
        .chars()
        .take_while(|character| *character == ' ' || *character == '\t')
        .collect()
}

fn indentation_before_offset(source_text: &str, offset: usize) -> String {
    let prefix = &source_text[..offset];
    let line_start = prefix
        .rfind('\n')
        .map(|index| index + 1)
        .or_else(|| prefix.rfind('\r').map(|index| index + 1))
        .unwrap_or(0);
    source_text[line_start..offset]
        .chars()
        .take_while(|character| *character == ' ' || *character == '\t')
        .collect()
}

fn line_ending_literal(source_text: &str) -> &'static str {
    match detect_line_ending_style(source_text) {
        LineEndingStyle::Lf => "\n",
        LineEndingStyle::Crlf => "\r\n",
        LineEndingStyle::Cr => "\r",
    }
}

fn append_requires_array_error(raw_path: &str, actual_kind: &str) -> IdenteditError {
    IdenteditError::InvalidRequest {
        message: format!(
            "Config path '{raw_path}' append requires an array/sequence target, found node kind '{actual_kind}'"
        ),
    }
}

fn is_missing_config_path_error(error: &IdenteditError) -> bool {
    matches!(
        error,
        IdenteditError::InvalidRequest { message } if message.contains("was not found")
    )
}

fn validate_yaml_create_missing_safety(
    tree: &Tree,
    source_text: &str,
) -> Result<(), IdenteditError> {
    let document_count = count_nodes_by_kind(tree.root_node(), "document");
    if document_count > 1 {
        return Err(IdenteditError::InvalidRequest {
            message: "Config path create-missing does not support multiple YAML documents in one file".to_string(),
        });
    }

    if has_yaml_anchor_or_alias(tree.root_node(), source_text) {
        return Err(IdenteditError::InvalidRequest {
            message: "Config path create-missing does not support YAML anchor/alias documents".to_string(),
        });
    }

    if has_yaml_comments(tree.root_node()) {
        return Err(IdenteditError::InvalidRequest {
            message: "Config path create-missing does not support YAML comments yet".to_string(),
        });
    }

    Ok(())
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

fn has_yaml_comments(root: Node<'_>) -> bool {
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

fn has_toml_comments(root: Node<'_>) -> bool {
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LineEndingStyle {
    Lf,
    Crlf,
    Cr,
}

fn apply_source_line_ending_style(rendered: &str, source_text: &str) -> String {
    let style = detect_line_ending_style(source_text);
    let had_trailing_newline = source_text.ends_with('\n') || source_text.ends_with('\r');

    let mut normalized = rendered.replace("\r\n", "\n").replace('\r', "\n");
    if !had_trailing_newline {
        while normalized.ends_with('\n') {
            normalized.pop();
        }
    }

    match style {
        LineEndingStyle::Lf => normalized,
        LineEndingStyle::Crlf => normalized.replace('\n', "\r\n"),
        LineEndingStyle::Cr => normalized.replace('\n', "\r"),
    }
}

fn detect_line_ending_style(source_text: &str) -> LineEndingStyle {
    let bytes = source_text.as_bytes();
    let mut index = 0usize;
    let mut has_crlf = false;
    let mut has_lf = false;
    let mut has_cr = false;

    while index < bytes.len() {
        match bytes[index] {
            b'\r' => {
                if index + 1 < bytes.len() && bytes[index + 1] == b'\n' {
                    has_crlf = true;
                    index += 2;
                } else {
                    has_cr = true;
                    index += 1;
                }
            }
            b'\n' => {
                has_lf = true;
                index += 1;
            }
            _ => {
                index += 1;
            }
        }
    }

    if has_crlf && !has_lf && !has_cr {
        LineEndingStyle::Crlf
    } else if has_cr && !has_crlf && !has_lf {
        LineEndingStyle::Cr
    } else {
        LineEndingStyle::Lf
    }
}

fn detect_config_format(file: &Path) -> Result<ConfigFormat, IdenteditError> {
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

fn parse_tree_for_format(format: &ConfigFormat, source: &[u8]) -> Result<Tree, IdenteditError> {
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
    let parse_source: &[u8] = if matches!(format, ConfigFormat::Toml)
        && has_cr_only_newlines(source)
    {
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

fn parse_config_path(raw_path: &str) -> Result<Vec<PathToken>, IdenteditError> {
    let path = raw_path.trim();
    if path.is_empty() {
        return Err(IdenteditError::InvalidRequest {
            message: "Config path cannot be empty".to_string(),
        });
    }

    let bytes = path.as_bytes();
    let mut index = 0usize;
    let mut tokens = Vec::new();

    while index < bytes.len() {
        match bytes[index] {
            b'[' => {
                let (value, consumed) = parse_index_segment(path, index)?;
                tokens.push(PathToken::Index(value));
                index = consumed;
            }
            b'.' => {
                return Err(IdenteditError::InvalidRequest {
                    message: format!(
                        "Invalid config path '{path}': unexpected '.' at byte offset {index}"
                    ),
                });
            }
            _ => {
                let start = index;
                while index < bytes.len() && is_key_char(bytes[index]) {
                    index += 1;
                }
                if start == index {
                    return Err(IdenteditError::InvalidRequest {
                        message: format!(
                            "Invalid config path '{path}': unsupported character '{}' at byte offset {index}",
                            bytes[index] as char
                        ),
                    });
                }
                tokens.push(PathToken::Key(path[start..index].to_string()));
            }
        }

        while index < bytes.len() && bytes[index] == b'[' {
            let (value, consumed) = parse_index_segment(path, index)?;
            tokens.push(PathToken::Index(value));
            index = consumed;
        }

        if index < bytes.len() {
            if bytes[index] != b'.' {
                return Err(IdenteditError::InvalidRequest {
                    message: format!(
                        "Invalid config path '{path}': expected '.' or '[' at byte offset {index}"
                    ),
                });
            }
            index += 1;
            if index >= bytes.len() {
                return Err(IdenteditError::InvalidRequest {
                    message: format!("Invalid config path '{path}': trailing '.' is not allowed"),
                });
            }
        }
    }

    if tokens.is_empty() {
        return Err(IdenteditError::InvalidRequest {
            message: "Config path cannot be empty".to_string(),
        });
    }

    Ok(tokens)
}

fn parse_index_segment(path: &str, start: usize) -> Result<(usize, usize), IdenteditError> {
    let bytes = path.as_bytes();
    let mut cursor = start + 1;
    let digit_start = cursor;
    while cursor < bytes.len() && bytes[cursor].is_ascii_digit() {
        cursor += 1;
    }

    if digit_start == cursor {
        return Err(IdenteditError::InvalidRequest {
            message: format!(
                "Invalid config path '{path}': expected digits after '[' at byte offset {start}"
            ),
        });
    }

    if cursor >= bytes.len() || bytes[cursor] != b']' {
        return Err(IdenteditError::InvalidRequest {
            message: format!(
                "Invalid config path '{path}': missing closing ']' for index starting at byte offset {start}"
            ),
        });
    }

    let value =
        path[digit_start..cursor]
            .parse::<usize>()
            .map_err(|_| IdenteditError::InvalidRequest {
                message: format!(
                    "Invalid config path '{path}': index '{}' is out of range",
                    &path[digit_start..cursor]
                ),
            })?;

    Ok((value, cursor + 1))
}

fn is_key_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-'
}

fn resolve_json_path(
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
                                return Err(append_requires_array_error(raw_path, value_node.kind()));
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

fn resolve_yaml_path(
    tree: &Tree,
    source: &[u8],
    path_tokens: &[PathToken],
    operation: &ConfigPathOperation,
    raw_path: &str,
) -> Result<ResolvedContainerEdit, IdenteditError> {
    let mut current =
        yaml_root_value(tree.root_node()).ok_or_else(|| IdenteditError::InvalidRequest {
            message: "YAML document has no root value".to_string(),
        })?;

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
                                return Err(append_requires_array_error(raw_path, value_node.kind()));
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
                    let items = named_children(current)
                        .into_iter()
                        .filter(|child| child.kind() == "block_sequence_item")
                        .collect::<Vec<_>>();
                    let item = items.get(*expected_index).ok_or_else(|| {
                        array_index_out_of_bounds_error(raw_path, *expected_index, items.len())
                    })?;
                    let value_node = item.child(0).and_then(yaml_unwrap_node).ok_or_else(|| {
                        IdenteditError::InvalidRequest {
                            message: format!(
                                "Config path '{raw_path}' points at empty YAML sequence item"
                            ),
                        }
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

fn resolve_toml_path(
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
            (selected.set_span, selected.set_kind.clone(), selected.set_span)
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

    let mut full_path = prefix;
    full_path.extend(key_segments.into_iter().map(PathToken::Key));

    out.push(TomlCandidate {
        path: full_path.clone(),
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
            .map(|value| decode_quoted_string(&value))
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

fn json_root_value(root: Node<'_>) -> Option<Node<'_>> {
    let node = root;
    if node.kind() == "document" {
        if let Some(value) = node.child_by_field_name("value") {
            return Some(value);
        }
        return first_named_child(node);
    }
    first_named_child(node).or(Some(node))
}

fn yaml_root_value(root: Node<'_>) -> Option<Node<'_>> {
    let mut node = root;
    if node.kind() == "stream" {
        node = first_named_child(node)?;
    }
    if node.kind() == "document" {
        node = first_named_child(node)?;
    }
    yaml_unwrap_node(node)
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

fn expected_path_container_error(
    raw_path: &str,
    token: &PathToken,
    actual_kind: &str,
) -> IdenteditError {
    let expected = match token {
        PathToken::Key(_) => "mapping/object",
        PathToken::Index(_) => "sequence/array",
    };

    IdenteditError::InvalidRequest {
        message: format!(
            "Config path '{raw_path}' expected {expected} at segment {}, found node kind '{actual_kind}'",
            token_display(token)
        ),
    }
}

fn token_display(token: &PathToken) -> String {
    match token {
        PathToken::Key(key) => format!("'{key}'"),
        PathToken::Index(index) => format!("[{index}]"),
    }
}

fn path_tokens_display(path: &[PathToken]) -> String {
    let mut output = String::new();
    for token in path {
        match token {
            PathToken::Key(key) => {
                if !output.is_empty() {
                    output.push('.');
                }
                output.push_str(key);
            }
            PathToken::Index(index) => {
                output.push_str(&format!("[{index}]"));
            }
        }
    }
    output
}

fn find_handle_for_span(
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

fn rewrite_container_text(
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

fn span_from_node(node: Node<'_>) -> Span {
    Span {
        start: node.start_byte(),
        end: node.end_byte(),
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{ConfigPathOperation, detect_config_format, parse_config_path};
    use crate::patch::config_path::{PathToken, path_tokens_display};

    #[test]
    fn parse_config_path_supports_dot_and_index_tokens() {
        let parsed = parse_config_path("service.targets[1].name").expect("path should parse");
        assert_eq!(
            parsed,
            vec![
                PathToken::Key("service".to_string()),
                PathToken::Key("targets".to_string()),
                PathToken::Index(1),
                PathToken::Key("name".to_string())
            ]
        );
    }

    #[test]
    fn parse_config_path_rejects_invalid_sequences() {
        let error = parse_config_path("service..name").expect_err("double dot must fail");
        assert!(
            matches!(error, crate::error::IdenteditError::InvalidRequest { .. }),
            "expected invalid request for malformed path"
        );

        let error = parse_config_path("service[abc]").expect_err("non-numeric index must fail");
        assert!(
            matches!(error, crate::error::IdenteditError::InvalidRequest { .. }),
            "expected invalid request for malformed index"
        );
    }

    #[test]
    fn detect_config_format_accepts_supported_extensions() {
        assert_eq!(
            detect_config_format(Path::new("fixture.json")).expect("json should be accepted"),
            super::ConfigFormat::Json
        );
        assert_eq!(
            detect_config_format(Path::new("fixture.yaml")).expect("yaml should be accepted"),
            super::ConfigFormat::Yaml
        );
        assert_eq!(
            detect_config_format(Path::new("fixture.toml")).expect("toml should be accepted"),
            super::ConfigFormat::Toml
        );
    }

    #[test]
    fn path_tokens_display_round_trips_tokens() {
        let tokens = vec![
            PathToken::Key("a".to_string()),
            PathToken::Key("b".to_string()),
            PathToken::Index(3),
        ];
        assert_eq!(path_tokens_display(&tokens), "a.b[3]");
    }

    #[test]
    fn config_path_operation_set_and_delete_are_distinct() {
        let set = ConfigPathOperation::Set {
            new_text: "42".to_string(),
            create_missing: false,
        };
        let append = ConfigPathOperation::Append {
            new_text: "42".to_string(),
        };
        let delete = ConfigPathOperation::Delete;
        assert_ne!(set, delete);
        assert_ne!(set, append);
        assert_ne!(append, delete);
    }
}
