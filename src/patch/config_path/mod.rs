use std::path::Path;

use tree_sitter::Tree;

use crate::changeset::{EditOperation, OpKind, TransformTarget};
use crate::error::IdenteditError;
use crate::hash::{ContentHash, hash_bytes};
use crate::transform::parse::parse_handles_for_source;

mod composition;
mod render;
mod resolve;
mod safety;
mod syntax;

use composition::{ConfigBoundary, ConfigEffect, ConfigGuard, ConfigWrite, compose_config_effects};
use render::render_append_array_write;
use resolve::{
    ResolvedContainerEdit, find_handle_for_span, json_root_value,
    reject_yaml_implicit_null_single_line_value, render_missing_json_object,
    render_yaml_comment_only_create_missing_insertion, resolve_json_path, resolve_toml_path,
    resolve_yaml_path, rewrite_container_text, rewrite_full_source_text,
    rewrite_json_with_create_missing, rewrite_toml_with_comment_preserving_create_missing,
    rewrite_yaml_with_comment_preserving_create_missing, span_from_node, validate_json_set_value,
    yaml_document_root_value_at, yaml_root_value, yaml_set_value_replace_span,
    yaml_set_value_replacement_text, yaml_single_line_value_with_trailing_line_endings,
};
use safety::{
    ConfigFormat, detect_config_format, has_yaml_comments, is_missing_config_path_error,
    parse_tree_for_format, validate_rendered_config_document,
    validate_yaml_config_path_reference_safety, validate_yaml_create_missing_safety,
};
use syntax::{PathToken, parse_config_path};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigPathOperation {
    Set {
        new_text: String,
        missing_path: MissingPathPolicy,
    },
    Append {
        new_text: String,
    },
    Delete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MissingPathPolicy {
    Reject,
    Create,
}

impl MissingPathPolicy {
    pub fn from_create_missing(value: bool) -> Self {
        if value { Self::Create } else { Self::Reject }
    }
}

struct CreateMissingSetRequest<'a> {
    format: &'a ConfigFormat,
    tree: &'a Tree,
    source: &'a [u8],
    source_text: &'a str,
    path_tokens: &'a [PathToken],
    raw_path: &'a str,
    new_text: &'a str,
    document_index: Option<usize>,
}

pub struct ResolvedConfigPathOperation {
    pub operation: EditOperation,
    pub expected_file_hash: ContentHash,
}

#[derive(Debug)]
pub(crate) struct ConfigPathRequest {
    pub(crate) path: String,
    pub(crate) expected_file_hash: Option<ContentHash>,
    pub(crate) document_index: Option<usize>,
    pub(crate) operation: ConfigPathOperation,
}

pub(crate) struct CompiledConfigPathOperation {
    pub(crate) operation: EditOperation,
    pub(crate) first_request_index: usize,
    pub(crate) guard_request_index: usize,
}

pub(crate) struct ResolvedConfigPathBatch {
    pub(crate) operations: Vec<CompiledConfigPathOperation>,
    pub(crate) expected_file_hash: ContentHash,
}

#[derive(Debug)]
pub(crate) struct ConfigPathBatchError {
    pub(crate) error: IdenteditError,
    pub(crate) request_indices: Vec<usize>,
}

impl ConfigPathBatchError {
    fn for_request(error: IdenteditError, index: usize) -> Self {
        Self {
            error,
            request_indices: vec![index],
        }
    }
}

impl From<IdenteditError> for ConfigPathBatchError {
    fn from(error: IdenteditError) -> Self {
        Self {
            error,
            request_indices: Vec::new(),
        }
    }
}

pub fn resolve_config_path_operation(
    file: &Path,
    raw_path: &str,
    expected_file_hash: Option<&ContentHash>,
    document_index: Option<usize>,
    operation: ConfigPathOperation,
) -> Result<ResolvedConfigPathOperation, IdenteditError> {
    let request = ConfigPathRequest {
        path: raw_path.to_string(),
        expected_file_hash: expected_file_hash.cloned(),
        document_index,
        operation,
    };
    let batch =
        resolve_config_path_operations(file, &[request]).map_err(|failure| failure.error)?;
    let compiled = batch
        .operations
        .into_iter()
        .next()
        .expect("a config request produces one operation");
    Ok(ResolvedConfigPathOperation {
        operation: compiled.operation,
        expected_file_hash: batch.expected_file_hash,
    })
}

pub(crate) fn resolve_config_path_operations(
    file: &Path,
    requests: &[ConfigPathRequest],
) -> Result<ResolvedConfigPathBatch, ConfigPathBatchError> {
    if requests.is_empty() {
        return Err(IdenteditError::InvalidRequest {
            message: "Config path compilation requires at least one request".to_string(),
        }
        .into());
    }
    let source = std::fs::read(file).map_err(|error| IdenteditError::io(file, error))?;
    let actual_hash = hash_bytes(&source);
    for (index, request) in requests.iter().enumerate() {
        if let Some(expected_hash) = &request.expected_file_hash
            && &actual_hash != expected_hash
        {
            return Err(ConfigPathBatchError::for_request(
                IdenteditError::FilePreconditionFailed {
                    path: file.display().to_string(),
                    expected_hash: expected_hash.to_string(),
                    actual_hash: actual_hash.to_string(),
                },
                index,
            ));
        }
    }

    let source_text = std::str::from_utf8(&source).map_err(|_| IdenteditError::InvalidRequest {
        message: format!(
            "Config path operations require UTF-8 source; file '{}' is not UTF-8",
            file.display()
        ),
    })?;

    let format = detect_config_format(file)?;
    let effects = requests
        .iter()
        .enumerate()
        .map(|(index, request)| {
            resolve_config_path_in_source(
                &format,
                &request.path,
                request.document_index,
                &request.operation,
                source_text,
            )
            .map_err(|error| ConfigPathBatchError::for_request(error, index))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let composed =
        compose_config_effects(&format, source_text, effects).map_err(|mut failure| {
            if let [first, second] = failure.request_indices.as_slice()
                && let IdenteditError::InvalidRequest { message } = &mut failure.error
            {
                *message = format!(
                    "Config paths '{}' and '{}': {message}",
                    requests[*first].path, requests[*second].path,
                );
            }
            failure
        })?;
    let handles = if composed
        .iter()
        .any(|effect| matches!(effect.guard, ConfigGuard::Syntax { .. }))
    {
        parse_handles_for_source(file, &source)?
    } else {
        Vec::new()
    };
    let mut operations = Vec::with_capacity(composed.len());
    for effect in composed {
        let (target, op) = match effect.guard {
            ConfigGuard::Syntax { span, kind, .. } => {
                let handle =
                    find_handle_for_span(file, &handles, span, &kind).map_err(|error| {
                        ConfigPathBatchError::for_request(error, effect.guard_request_index)
                    })?;
                (
                    TransformTarget::node(
                        handle.identity,
                        handle.kind,
                        Some(handle.span),
                        handle.expected_old_hash,
                    ),
                    OpKind::Replace {
                        new_text: effect.new_text,
                    },
                )
            }
            ConfigGuard::Boundary { side, .. } => {
                let target = match side {
                    ConfigBoundary::Start => TransformTarget::FileStart {
                        expected_file_hash: actual_hash.clone(),
                    },
                    ConfigBoundary::End => TransformTarget::FileEnd {
                        expected_file_hash: actual_hash.clone(),
                    },
                };
                (
                    target,
                    OpKind::Insert {
                        new_text: effect.new_text,
                    },
                )
            }
        };
        let operation = EditOperation::try_new(target, op).map_err(|error| {
            ConfigPathBatchError::for_request(error.into(), effect.guard_request_index)
        })?;
        operations.push(CompiledConfigPathOperation {
            operation,
            first_request_index: effect.first_request_index,
            guard_request_index: effect.guard_request_index,
        });
    }
    Ok(ResolvedConfigPathBatch {
        operations,
        expected_file_hash: actual_hash,
    })
}

fn resolve_config_path_in_source(
    format: &ConfigFormat,
    raw_path: &str,
    document_index: Option<usize>,
    operation: &ConfigPathOperation,
    source_text: &str,
) -> Result<ConfigEffect, IdenteditError> {
    let source = source_text.as_bytes();
    if document_index.is_some() && !matches!(format, ConfigFormat::Yaml) {
        return Err(IdenteditError::InvalidRequest {
            message: "Config path document_index is supported only for YAML files".to_string(),
        });
    }
    let path_tokens = parse_config_path(raw_path)?;

    if let ConfigPathOperation::Set { new_text, .. } = operation
        && matches!(format, ConfigFormat::Json)
    {
        validate_json_set_value(new_text)?;
    }

    if let ConfigPathOperation::Set {
        new_text,
        missing_path: MissingPathPolicy::Create,
    } = operation
        && matches!(format, ConfigFormat::Json)
        && json_effectively_empty_source(source_text)
    {
        let updated = render_missing_json_object(&path_tokens, raw_path, new_text)?;
        validate_rendered_config_document(format, "", &updated)?;
        let side = if source.is_empty() {
            ConfigBoundary::Start
        } else {
            ConfigBoundary::End
        };
        return Ok(ConfigEffect::boundary(side, source.len(), updated));
    }

    if let ConfigPathOperation::Set {
        new_text,
        missing_path: MissingPathPolicy::Create,
    } = operation
        && matches!(format, ConfigFormat::Yaml)
    {
        reject_yaml_create_missing_value_text(new_text, raw_path)?;
    }

    if let ConfigPathOperation::Set {
        new_text,
        missing_path: MissingPathPolicy::Create,
    } = operation
        && matches!(format, ConfigFormat::Yaml)
        && yaml_effectively_empty_source(source_text)
    {
        let updated = render_yaml_comment_only_create_missing_insertion(
            source_text,
            &path_tokens,
            raw_path,
            new_text,
        )?;
        let side = if source.is_empty() {
            ConfigBoundary::Start
        } else {
            ConfigBoundary::End
        };
        return Ok(ConfigEffect::boundary(side, source.len(), updated));
    }

    let tree = parse_tree_for_format(format, source)?;
    if let ConfigPathOperation::Set {
        new_text,
        missing_path: MissingPathPolicy::Create,
    } = operation
    {
        let strict_probe = ConfigPathOperation::Set {
            new_text: String::new(),
            missing_path: MissingPathPolicy::Reject,
        };
        let strict_resolved = match format {
            ConfigFormat::Json => {
                resolve_json_path(&tree, source, &path_tokens, &strict_probe, raw_path)
            }
            ConfigFormat::Yaml => resolve_yaml_path(
                &tree,
                source,
                &path_tokens,
                &strict_probe,
                raw_path,
                document_index,
            ),
            ConfigFormat::Toml => {
                resolve_toml_path(&tree, source, &path_tokens, &strict_probe, raw_path)
            }
        };
        match strict_resolved {
            Ok(resolved) => {
                if matches!(format, ConfigFormat::Yaml) {
                    validate_yaml_config_path_reference_safety(
                        &tree,
                        source_text,
                        document_index,
                        resolved.replace_span,
                        raw_path,
                    )?;
                }
                return prepare_container_effect(
                    format,
                    source_text,
                    resolved,
                    operation,
                    raw_path,
                );
            }
            Err(error) if !is_missing_config_path_error(&error) => return Err(error),
            Err(_) => {}
        }

        return resolve_config_path_set_with_create_missing(CreateMissingSetRequest {
            format,
            tree: &tree,
            source,
            source_text,
            path_tokens: &path_tokens,
            raw_path,
            new_text,
            document_index,
        });
    }

    let resolved = match format {
        ConfigFormat::Json => resolve_json_path(&tree, source, &path_tokens, operation, raw_path)?,
        ConfigFormat::Yaml => resolve_yaml_path(
            &tree,
            source,
            &path_tokens,
            operation,
            raw_path,
            document_index,
        )?,
        ConfigFormat::Toml => resolve_toml_path(&tree, source, &path_tokens, operation, raw_path)?,
    };
    if matches!(format, ConfigFormat::Yaml) {
        validate_yaml_config_path_reference_safety(
            &tree,
            source_text,
            document_index,
            resolved.replace_span,
            raw_path,
        )?;
    }

    prepare_container_effect(format, source_text, resolved, operation, raw_path)
}

fn reject_yaml_create_missing_value_text(
    new_text: &str,
    raw_path: &str,
) -> Result<(), IdenteditError> {
    if !new_text.contains('\n') && !new_text.contains('\r') {
        reject_yaml_implicit_null_single_line_value(new_text, raw_path)?;
    } else if let Some(single_line_value) =
        yaml_single_line_value_with_trailing_line_endings(new_text)
    {
        reject_yaml_implicit_null_single_line_value(single_line_value, raw_path)?;
    }
    Ok(())
}

fn set_replacement_text(
    format: &ConfigFormat,
    source_text: &str,
    replace_span: crate::handle::Span,
    new_text: &str,
    raw_path: &str,
) -> Result<String, IdenteditError> {
    match format {
        ConfigFormat::Yaml => {
            yaml_set_value_replacement_text(source_text, replace_span, new_text, raw_path)
        }
        ConfigFormat::Json | ConfigFormat::Toml => Ok(new_text.to_string()),
    }
}

fn prepare_container_effect(
    format: &ConfigFormat,
    source_text: &str,
    resolved: ResolvedContainerEdit<'_>,
    operation: &ConfigPathOperation,
    raw_path: &str,
) -> Result<ConfigEffect, IdenteditError> {
    let container_span = span_from_node(resolved.container);
    let container_kind = resolved.container.kind();
    let append_write = if let ConfigPathOperation::Append { new_text } = operation {
        Some(render_append_array_write(
            source_text,
            container_span,
            container_kind,
            new_text,
            raw_path,
        )?)
    } else {
        None
    };
    let replacement = match operation {
        ConfigPathOperation::Set { new_text, .. } => set_replacement_text(
            format,
            source_text,
            resolved.replace_span,
            new_text,
            raw_path,
        )?,
        ConfigPathOperation::Append { .. } => {
            let write = append_write.as_ref().expect("append has a rendered write");
            rewrite_container_text(source_text, container_span, write.span, &write.new_text)?
        }
        ConfigPathOperation::Delete => String::new(),
    };
    let replace_span = set_replace_span(
        format,
        source_text,
        resolved.replace_span,
        &replacement,
        raw_path,
    )?;
    validate_replacement_for_container(format, container_kind, &replacement, raw_path)?;
    let updated_container_text =
        rewrite_container_text(source_text, container_span, replace_span, &replacement)?;
    let updated_source =
        rewrite_full_source_text(source_text, container_span, &updated_container_text)?;
    validate_rendered_config_document(format, source_text, &updated_source)?;

    let write = append_write.unwrap_or(ConfigWrite {
        span: replace_span,
        new_text: replacement,
    });
    Ok(ConfigEffect::syntax(resolved.container, write))
}

fn set_replace_span(
    format: &ConfigFormat,
    source_text: &str,
    replace_span: crate::handle::Span,
    replacement: &str,
    raw_path: &str,
) -> Result<crate::handle::Span, IdenteditError> {
    match format {
        ConfigFormat::Yaml => {
            yaml_set_value_replace_span(source_text, replace_span, replacement, raw_path)
        }
        ConfigFormat::Json | ConfigFormat::Toml => Ok(replace_span),
    }
}

fn validate_replacement_for_container(
    format: &ConfigFormat,
    container_kind: &str,
    replacement: &str,
    raw_path: &str,
) -> Result<(), IdenteditError> {
    if !matches!(format, ConfigFormat::Yaml)
        || !matches!(container_kind, "flow_sequence" | "flow_mapping")
    {
        return Ok(());
    }

    let trimmed = replacement.trim_start();
    let starts_with_explicit_yaml_collection_or_string =
        matches!(trimmed.chars().next(), Some('"' | '\'' | '[' | '{'));
    if !starts_with_explicit_yaml_collection_or_string && replacement.contains(',') {
        return Err(IdenteditError::InvalidRequest {
            message: format!(
                "Config path '{raw_path}' YAML flow-container replacements containing ',' must be quoted or expressed as an explicit flow collection"
            ),
        });
    }
    if !starts_with_explicit_yaml_collection_or_string
        && contains_yaml_plain_mapping_indicator(replacement)
    {
        return Err(IdenteditError::InvalidRequest {
            message: format!(
                "Config path '{raw_path}' YAML flow-container replacements containing mapping-like ':' syntax must be quoted or expressed as an explicit flow mapping"
            ),
        });
    }

    Ok(())
}

fn contains_yaml_plain_mapping_indicator(text: &str) -> bool {
    let trimmed = text.trim_end_matches([' ', '\t']);
    trimmed.ends_with(':') || text.contains(": ") || text.contains(":\t")
}

fn resolve_config_path_set_with_create_missing(
    request: CreateMissingSetRequest<'_>,
) -> Result<ConfigEffect, IdenteditError> {
    if matches!(request.format, ConfigFormat::Yaml) {
        validate_yaml_create_missing_safety(
            request.tree,
            request.source_text,
            request.document_index,
        )?;
        if has_yaml_comments(request.tree.root_node())
            && yaml_root_value(request.tree.root_node()).is_none()
        {
            let new_text = render_yaml_comment_only_create_missing_insertion(
                request.source_text,
                request.path_tokens,
                request.raw_path,
                request.new_text,
            )?;
            return Ok(ConfigEffect::boundary(
                ConfigBoundary::End,
                request.source.len(),
                new_text,
            ));
        }
    }
    let updated_root_text = match request.format {
        ConfigFormat::Json => rewrite_json_with_create_missing(
            request.tree,
            request.source_text,
            request.path_tokens,
            request.raw_path,
            request.new_text,
        )?,
        ConfigFormat::Yaml => rewrite_yaml_with_comment_preserving_create_missing(
            request.tree,
            request.source_text,
            request.path_tokens,
            request.raw_path,
            request.new_text,
            request.document_index,
        )?,
        ConfigFormat::Toml => rewrite_toml_with_comment_preserving_create_missing(
            request.tree,
            request.source_text,
            request.path_tokens,
            request.raw_path,
            request.new_text,
        )?,
    };
    let yaml_validation_source;
    let validation_source = if matches!(request.format, ConfigFormat::Yaml) {
        let root_node = if let Some(index) = request.document_index {
            yaml_document_root_value_at(request.tree.root_node(), index)?
        } else {
            yaml_root_value(request.tree.root_node()).ok_or_else(|| {
                IdenteditError::InvalidRequest {
                    message: "YAML document has no root value".to_string(),
                }
            })?
        };
        yaml_validation_source = rewrite_full_source_text(
            request.source_text,
            span_from_node(root_node),
            &updated_root_text,
        )?;
        &yaml_validation_source
    } else {
        &updated_root_text
    };
    validate_rendered_config_document(request.format, request.source_text, validation_source)?;

    if matches!(request.format, ConfigFormat::Toml)
        && toml_effectively_empty_source(request.source_text)
    {
        return Ok(ConfigEffect::boundary(
            ConfigBoundary::End,
            request.source.len(),
            insertion_text_after_existing_empty_source(request.source_text, updated_root_text),
        ));
    }

    let root_node = match request.format {
        ConfigFormat::Json => json_root_value(request.tree.root_node()).ok_or_else(|| {
            IdenteditError::InvalidRequest {
                message: "JSON document has no root value".to_string(),
            }
        })?,
        ConfigFormat::Yaml => {
            if let Some(index) = request.document_index {
                yaml_document_root_value_at(request.tree.root_node(), index)?
            } else {
                yaml_root_value(request.tree.root_node()).ok_or_else(|| {
                    IdenteditError::InvalidRequest {
                        message: "YAML document has no root value".to_string(),
                    }
                })?
            }
        }
        ConfigFormat::Toml => request.tree.root_node(),
    };

    let root_span = span_from_node(root_node);
    if matches!(request.format, ConfigFormat::Toml) && root_span.start == root_span.end {
        return Ok(ConfigEffect::boundary(
            ConfigBoundary::End,
            request.source.len(),
            updated_root_text,
        ));
    }

    let replacement_text = replacement_text_for_root_span(
        request.format,
        request.source_text,
        root_span,
        updated_root_text,
    )?;
    Ok(ConfigEffect::syntax(
        root_node,
        ConfigWrite {
            span: root_span,
            new_text: replacement_text,
        },
    ))
}

fn insertion_text_after_existing_empty_source(
    _source_text: &str,
    updated_source_text: String,
) -> String {
    updated_source_text
        .trim_start_matches('\u{feff}')
        .trim()
        .to_string()
}

fn toml_effectively_empty_source(source_text: &str) -> bool {
    source_text
        .strip_prefix('\u{feff}')
        .unwrap_or(source_text)
        .trim()
        .is_empty()
}

fn json_effectively_empty_source(source_text: &str) -> bool {
    source_text
        .strip_prefix('\u{feff}')
        .unwrap_or(source_text)
        .bytes()
        .all(|byte| matches!(byte, b' ' | b'\t' | b'\r' | b'\n'))
}

fn yaml_effectively_empty_source(source_text: &str) -> bool {
    source_text
        .strip_prefix('\u{feff}')
        .unwrap_or(source_text)
        .trim()
        .is_empty()
}

fn replacement_text_for_root_span(
    format: &ConfigFormat,
    source_text: &str,
    root_span: crate::handle::Span,
    updated_source_text: String,
) -> Result<String, IdenteditError> {
    if !matches!(format, ConfigFormat::Toml) || root_span.start == 0 {
        return Ok(updated_source_text);
    }

    let prefix = &source_text[..root_span.start];
    if prefix == "\u{feff}" && !updated_source_text.starts_with(prefix) {
        return Ok(updated_source_text);
    }

    if !updated_source_text.starts_with(prefix) {
        return Err(IdenteditError::InvalidRequest {
            message: "Config path create-missing produced root replacement that does not preserve the source prefix"
                .to_string(),
        });
    }

    let replacement_start = prefix.len();
    Ok(updated_source_text[replacement_start..].to_string())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::syntax::{PathToken, parse_config_path, path_tokens_display};
    use super::{ConfigPathOperation, MissingPathPolicy, detect_config_format};

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
    fn parse_config_path_supports_quoted_key_segments() {
        let parsed = parse_config_path(r#"["on"].jobs["build/test"].steps[0]["run:script"]"#)
            .expect("quoted key path should parse");
        assert_eq!(
            parsed,
            vec![
                PathToken::Key("on".to_string()),
                PathToken::Key("jobs".to_string()),
                PathToken::Key("build/test".to_string()),
                PathToken::Key("steps".to_string()),
                PathToken::Index(0),
                PathToken::Key("run:script".to_string()),
            ]
        );
    }

    #[test]
    fn parse_config_path_supports_json_string_escapes_in_quoted_key_segments() {
        let parsed = parse_config_path(r#"root["quote\"key"]["backslash\\key"]["unicode-\uD55C"]"#)
            .expect("escaped quoted key path should parse");
        assert_eq!(
            parsed,
            vec![
                PathToken::Key("root".to_string()),
                PathToken::Key("quote\"key".to_string()),
                PathToken::Key("backslash\\key".to_string()),
                PathToken::Key("unicode-한".to_string()),
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

        let error = parse_config_path(r#"service["unterminated]"#)
            .expect_err("unterminated quoted key must fail");
        assert!(
            matches!(error, crate::error::IdenteditError::InvalidRequest { .. }),
            "expected invalid request for unterminated quoted key"
        );

        let error = parse_config_path(r#"service["key"]trailing"#)
            .expect_err("trailing characters after quoted key segment must fail");
        assert!(
            matches!(error, crate::error::IdenteditError::InvalidRequest { .. }),
            "expected invalid request for trailing characters after quoted key"
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
    fn path_tokens_display_quotes_non_bare_key_segments() {
        let tokens = vec![
            PathToken::Key("x.y".to_string()),
            PathToken::Key("quote\"key".to_string()),
            PathToken::Index(2),
            PathToken::Key("run:script".to_string()),
        ];
        assert_eq!(
            path_tokens_display(&tokens),
            r#"["x.y"]["quote\"key"][2]["run:script"]"#
        );
    }

    #[test]
    fn config_path_operation_set_and_delete_are_distinct() {
        let set = ConfigPathOperation::Set {
            new_text: "42".to_string(),
            missing_path: MissingPathPolicy::Reject,
        };
        let create_missing_set = ConfigPathOperation::Set {
            new_text: "42".to_string(),
            missing_path: MissingPathPolicy::Create,
        };
        let append = ConfigPathOperation::Append {
            new_text: "42".to_string(),
        };
        let delete = ConfigPathOperation::Delete;
        assert_ne!(set, delete);
        assert_ne!(set, create_missing_set);
        assert_ne!(set, append);
        assert_ne!(append, delete);
    }

    #[test]
    fn missing_path_policy_normalizes_legacy_create_missing_bool() {
        assert_eq!(
            MissingPathPolicy::from_create_missing(false),
            MissingPathPolicy::Reject
        );
        assert_eq!(
            MissingPathPolicy::from_create_missing(true),
            MissingPathPolicy::Create
        );
    }
}
