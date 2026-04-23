use crate::error::IdenteditError;
use crate::handle::Span;

use super::syntax::{PathToken, expected_path_container_error};

pub(super) fn render_json_with_create_missing(
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

pub(super) fn parse_toml_value_fragment(fragment: &str) -> Result<toml::Value, IdenteditError> {
    let wrapped = format!("__identedit_tmp__ = {fragment}");
    let mut table: toml::Table =
        toml::from_str(&wrapped).map_err(|error| IdenteditError::InvalidRequest {
            message: format!("Config path set value is not valid TOML value text: {error}"),
        })?;
    let value =
        table
            .remove("__identedit_tmp__")
            .ok_or_else(|| IdenteditError::InvalidRequest {
                message: "Config path set value parsing produced no value".to_string(),
            })?;
    if !table.is_empty() {
        return Err(IdenteditError::InvalidRequest {
            message: "Config path set value must be a single TOML value, not additional TOML items"
                .to_string(),
        });
    }
    Ok(value)
}

pub(super) fn array_index_out_of_bounds_error(
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

pub(super) fn render_append_array_replacement(
    source_text: &str,
    container_span: Span,
    container_kind: &str,
    new_text: &str,
    raw_path: &str,
) -> Result<String, IdenteditError> {
    let array_text = source_text
        .get(container_span.start..container_span.end)
        .ok_or_else(|| IdenteditError::InvalidRequest {
            message: format!(
                "Invalid append span [{}, {}) while resolving config path '{raw_path}'",
                container_span.start, container_span.end
            ),
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

pub(super) fn append_requires_array_error(raw_path: &str, actual_kind: &str) -> IdenteditError {
    IdenteditError::InvalidRequest {
        message: format!(
            "Config path '{raw_path}' append requires an array/sequence target, found node kind '{actual_kind}'"
        ),
    }
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

fn empty_json_container_for_token(next: &PathToken) -> serde_json::Value {
    match next {
        PathToken::Key(_) => serde_json::Value::Object(serde_json::Map::new()),
        PathToken::Index(_) => serde_json::Value::Array(Vec::new()),
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LineEndingStyle {
    Lf,
    Crlf,
    Cr,
}
