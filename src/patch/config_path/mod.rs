use std::path::Path;

use tree_sitter::Tree;

use crate::changeset::{OpKind, TransformTarget};
use crate::error::IdenteditError;
use crate::hash::hash_bytes;
use crate::transform::parse::parse_handles_for_source;

mod render;
mod resolve;
mod safety;
mod syntax;

use render::{
    render_append_array_replacement, render_json_with_create_missing,
    render_toml_with_create_missing, render_yaml_with_create_missing,
};
use resolve::{
    ResolvedContainerEdit, find_handle_for_span, json_root_value, resolve_json_path,
    resolve_toml_path, resolve_yaml_path, rewrite_container_text, rewrite_full_source_text,
    span_from_node, yaml_root_value,
};
use safety::{
    ConfigFormat, detect_config_format, has_toml_comments, is_missing_config_path_error,
    parse_tree_for_format, validate_rendered_config_document, validate_yaml_create_missing_safety,
};
use syntax::{PathToken, parse_config_path};

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
                    &format,
                    file,
                    &source,
                    source_text,
                    resolved,
                    new_text,
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
    build_resolved_patch_from_container_edit(
        &format,
        file,
        &source,
        source_text,
        resolved,
        &replacement,
    )
}

fn build_resolved_patch_from_container_edit(
    format: &ConfigFormat,
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
    let updated_source = rewrite_full_source_text(
        source_text,
        resolved.container_span,
        &updated_container_text,
    )?;
    validate_rendered_config_document(format, &updated_source)?;

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
    if matches!(request.format, ConfigFormat::Toml) && has_toml_comments(request.tree.root_node()) {
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

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::syntax::{PathToken, parse_config_path, path_tokens_display};
    use super::{ConfigPathOperation, detect_config_format};

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
