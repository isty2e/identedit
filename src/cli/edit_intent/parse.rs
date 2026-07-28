use std::path::PathBuf;

use crate::changeset::{EditOperation, OpKind, TransformTarget};
use crate::error::IdenteditError;
use crate::hash::hash_bytes;
use crate::hashline::{HashlineEdit, InsertAfterEdit, LineAnchor, ReplaceLinesEdit, SetLineEdit};
use crate::patch::config_path::{
    ConfigPathOperation, MissingPathPolicy, resolve_config_path_operation,
};

use super::args::EditIntentArgs;
use super::model::{
    CanonicalEditIntent, LineEditIntent, NodeEditIntent, PreparedEditIntent,
    PreparedNodeEditOperation,
};
use super::target::{EditTargetIngress, NodeTargetSelector, resolve_edit_target_ingress};
use super::text::{
    reject_unused_text_source, resolve_text_payload, resolve_text_source, text_arg_present,
};

const NODE_MODE_OPERATIONS: &str = "--replace, --delete, --insert-before, --insert-after, or --scoped-regex with --scoped-replacement";
const LINE_MODE_OPERATIONS: &str = "--set-line, --replace-range, or --insert-after-line";
const CONFIG_MODE_OPERATIONS: &str = "--set-value, --append-value, or --delete";

fn node_mode_guidance() -> String {
    format!(
        "Node target mode supports {NODE_MODE_OPERATIONS}. For line edits use --at <line:hash>; for file insertion use --at file-start|file-end --insert; for config edits use --config-path with {CONFIG_MODE_OPERATIONS}."
    )
}

fn line_mode_guidance() -> String {
    format!(
        "Line target mode supports {LINE_MODE_OPERATIONS}. For node edits use --at <hex16>, --symbol, or --kind with --name."
    )
}

fn file_mode_guidance() -> String {
    "File target mode supports only --insert. Use --at file-start or --at file-end with --insert <text>."
        .to_string()
}

fn config_mode_guidance() -> String {
    format!(
        "Config path mode supports {CONFIG_MODE_OPERATIONS}. Use --create-missing only with --set-value; use --document-index <N> only for YAML multi-document streams."
    )
}

pub(crate) fn parse_flag_edit_intent(
    args: &EditIntentArgs,
) -> Result<PreparedEditIntent, IdenteditError> {
    let file = args
        .file
        .clone()
        .ok_or_else(|| IdenteditError::InvalidRequest {
            message: "FILE is required unless --json mode is enabled".to_string(),
        })?;

    match resolve_edit_target_ingress(args)? {
        EditTargetIngress::NodeIdentity(identity) => {
            parse_node_flag_edit_intent(file, NodeTargetSelector::Identity(identity), args)
        }
        EditTargetIngress::NodeSelector { kind, name_pattern } => parse_node_flag_edit_intent(
            file,
            NodeTargetSelector::Selector { kind, name_pattern },
            args,
        ),
        EditTargetIngress::NodeSymbol(symbol) => {
            parse_node_flag_edit_intent(file, NodeTargetSelector::Symbol(symbol), args)
        }
        EditTargetIngress::LineAnchor(anchor) => parse_line_flag_edit_intent(file, anchor, args),
        EditTargetIngress::FileStart => parse_file_flag_edit_intent(file, true, args),
        EditTargetIngress::FileEnd => parse_file_flag_edit_intent(file, false, args),
        EditTargetIngress::ConfigPath(path) => parse_config_flag_edit_intent(file, path, args),
    }
}

fn parse_node_flag_edit_intent(
    file: PathBuf,
    selector: NodeTargetSelector,
    args: &EditIntentArgs,
) -> Result<PreparedEditIntent, IdenteditError> {
    let operation = prepare_node_edit_operation(args)?;
    Ok(PreparedEditIntent::Node(NodeEditIntent {
        file,
        selector,
        operation,
    }))
}

fn prepare_node_edit_operation(
    args: &EditIntentArgs,
) -> Result<PreparedNodeEditOperation, IdenteditError> {
    let text_source = resolve_text_source(args)?;

    if args.end_anchor.is_some()
        || args.insert.is_some()
        || args.set_value.is_some()
        || args.append_value.is_some()
        || args.create_missing
        || args.document_index.is_some()
        || args.set_line.is_some()
        || args.replace_range.is_some()
        || args.insert_after_line.is_some()
    {
        return Err(IdenteditError::InvalidRequest {
            message: node_mode_guidance(),
        });
    }

    let scoped_regex_present = args.scoped_regex.is_some() || args.scoped_replacement.is_some();
    if scoped_regex_present && (args.scoped_regex.is_none() || args.scoped_replacement.is_none()) {
        return Err(IdenteditError::InvalidRequest {
            message: "--scoped-regex and --scoped-replacement must be provided together"
                .to_string(),
        });
    }

    let operation_count = usize::from(text_arg_present(&args.replace))
        + usize::from(args.delete)
        + usize::from(text_arg_present(&args.insert_before))
        + usize::from(text_arg_present(&args.insert_after))
        + usize::from(scoped_regex_present);
    if operation_count != 1 {
        return Err(IdenteditError::InvalidRequest {
            message: format!(
                "Choose exactly one node operation. Node target mode supports {NODE_MODE_OPERATIONS}."
            ),
        });
    }

    if let Some(pattern) = args.scoped_regex.clone() {
        let replacement = resolve_text_payload(
            "--scoped-replacement",
            args.scoped_replacement.clone(),
            text_source,
        )?
        .ok_or_else(|| IdenteditError::InvalidRequest {
            message: "missing payload for --scoped-replacement".to_string(),
        })?;
        return Ok(PreparedNodeEditOperation::ScopedRegex {
            pattern,
            replacement,
        });
    }

    if let Some(new_text) =
        resolve_text_payload("--replace", args.replace.clone(), text_source.clone())?
    {
        return Ok(PreparedNodeEditOperation::Standard(OpKind::Replace {
            new_text,
        }));
    }

    if args.delete {
        reject_unused_text_source(text_source, NODE_MODE_OPERATIONS)?;
        return Ok(PreparedNodeEditOperation::Standard(OpKind::Delete));
    }

    if let Some(new_text) = resolve_text_payload(
        "--insert-before",
        args.insert_before.clone(),
        text_source.clone(),
    )? {
        return Ok(PreparedNodeEditOperation::Standard(OpKind::InsertBefore {
            new_text,
        }));
    }

    let new_text = resolve_text_payload("--insert-after", args.insert_after.clone(), text_source)?
        .ok_or_else(|| IdenteditError::InvalidRequest {
            message: "missing operation payload for --insert-after".to_string(),
        })?;
    Ok(PreparedNodeEditOperation::Standard(OpKind::InsertAfter {
        new_text,
    }))
}

fn parse_file_flag_edit_intent(
    file: PathBuf,
    at_file_start: bool,
    args: &EditIntentArgs,
) -> Result<PreparedEditIntent, IdenteditError> {
    let text_source = resolve_text_source(args)?;

    if args.replace.is_some()
        || args.set_value.is_some()
        || args.append_value.is_some()
        || args.scoped_regex.is_some()
        || args.scoped_replacement.is_some()
        || args.delete
        || args.insert_before.is_some()
        || args.insert_after.is_some()
        || args.create_missing
        || args.document_index.is_some()
        || args.set_line.is_some()
        || args.replace_range.is_some()
        || args.insert_after_line.is_some()
    {
        return Err(IdenteditError::InvalidRequest {
            message: file_mode_guidance(),
        });
    }

    let insert_text = resolve_text_payload("--insert", args.insert.clone(), text_source)?
        .ok_or_else(|| IdenteditError::InvalidRequest {
            message: file_mode_guidance(),
        })?;

    let source = std::fs::read(&file).map_err(|error| IdenteditError::io(&file, error))?;
    let expected_file_hash = hash_bytes(&source);
    let target = if at_file_start {
        TransformTarget::FileStart { expected_file_hash }
    } else {
        TransformTarget::FileEnd { expected_file_hash }
    };

    Ok(PreparedEditIntent::Canonical(CanonicalEditIntent {
        file,
        operation: EditOperation::try_new(
            target,
            OpKind::Insert {
                new_text: insert_text,
            },
        )?,
    }))
}

fn parse_line_flag_edit_intent(
    file: PathBuf,
    anchor: LineAnchor,
    args: &EditIntentArgs,
) -> Result<PreparedEditIntent, IdenteditError> {
    let text_source = resolve_text_source(args)?;

    if args.replace.is_some()
        || args.insert.is_some()
        || args.set_value.is_some()
        || args.append_value.is_some()
        || args.scoped_regex.is_some()
        || args.scoped_replacement.is_some()
        || args.delete
        || args.insert_before.is_some()
        || args.insert_after.is_some()
        || args.create_missing
        || args.document_index.is_some()
    {
        return Err(IdenteditError::InvalidRequest {
            message: line_mode_guidance(),
        });
    }
    let line_operation_count = usize::from(text_arg_present(&args.set_line))
        + usize::from(text_arg_present(&args.replace_range))
        + usize::from(text_arg_present(&args.insert_after_line));
    if line_operation_count != 1 {
        return Err(IdenteditError::InvalidRequest {
            message: format!(
                "Choose exactly one line operation. Line target mode supports {LINE_MODE_OPERATIONS}."
            ),
        });
    }

    let edit = if let Some(new_text) =
        resolve_text_payload("--set-line", args.set_line.clone(), text_source.clone())?
    {
        if args.end_anchor.is_some() {
            return Err(IdenteditError::InvalidRequest {
                message: "Use --end-anchor only with --replace-range in line target mode."
                    .to_string(),
            });
        }
        HashlineEdit::SetLine {
            set_line: SetLineEdit { anchor, new_text },
        }
    } else if let Some(new_text) = resolve_text_payload(
        "--replace-range",
        args.replace_range.clone(),
        text_source.clone(),
    )? {
        HashlineEdit::ReplaceLines {
            replace_lines: ReplaceLinesEdit {
                start_anchor: anchor,
                end_anchor: args
                    .end_anchor
                    .as_deref()
                    .map(LineAnchor::parse)
                    .transpose()
                    .map_err(|error| IdenteditError::InvalidRequest {
                        message: error.to_string(),
                    })?,
                new_text,
            },
        }
    } else {
        if args.end_anchor.is_some() {
            return Err(IdenteditError::InvalidRequest {
                message: "Use --end-anchor only with --replace-range in line target mode."
                    .to_string(),
            });
        }
        let text = resolve_text_payload(
            "--insert-after-line",
            args.insert_after_line.clone(),
            text_source,
        )?
        .ok_or_else(|| IdenteditError::InvalidRequest {
            message: "missing operation payload for --insert-after-line".to_string(),
        })?;
        HashlineEdit::InsertAfter {
            insert_after: InsertAfterEdit { anchor, text },
        }
    };

    Ok(PreparedEditIntent::Line(LineEditIntent { file, edit }))
}

fn parse_config_flag_edit_intent(
    file: PathBuf,
    path: String,
    args: &EditIntentArgs,
) -> Result<PreparedEditIntent, IdenteditError> {
    let text_source = resolve_text_source(args)?;

    if args.at.is_some()
        || args.end_anchor.is_some()
        || args.replace.is_some()
        || args.insert.is_some()
        || args.scoped_regex.is_some()
        || args.scoped_replacement.is_some()
        || args.insert_before.is_some()
        || args.insert_after.is_some()
        || args.set_line.is_some()
        || args.replace_range.is_some()
        || args.insert_after_line.is_some()
    {
        return Err(IdenteditError::InvalidRequest {
            message: config_mode_guidance(),
        });
    }

    if args.create_missing && (args.delete || args.append_value.is_some()) {
        return Err(IdenteditError::InvalidRequest {
            message: config_mode_guidance(),
        });
    }

    let operation_count = usize::from(text_arg_present(&args.set_value))
        + usize::from(text_arg_present(&args.append_value))
        + usize::from(args.delete);
    if operation_count != 1 {
        return Err(IdenteditError::InvalidRequest {
            message: format!(
                "Choose exactly one config path operation. Config path mode supports {CONFIG_MODE_OPERATIONS}."
            ),
        });
    }

    let canonical = if let Some(new_text) =
        resolve_text_payload("--set-value", args.set_value.clone(), text_source.clone())?
    {
        resolve_config_path_operation(
            file.as_path(),
            &path,
            None,
            args.document_index,
            ConfigPathOperation::Set {
                new_text,
                missing_path: MissingPathPolicy::from_create_missing(args.create_missing),
            },
        )?
    } else if let Some(new_text) = resolve_text_payload(
        "--append-value",
        args.append_value.clone(),
        text_source.clone(),
    )? {
        resolve_config_path_operation(
            file.as_path(),
            &path,
            None,
            args.document_index,
            ConfigPathOperation::Append { new_text },
        )?
    } else {
        reject_unused_text_source(text_source, CONFIG_MODE_OPERATIONS)?;
        resolve_config_path_operation(
            file.as_path(),
            &path,
            None,
            args.document_index,
            ConfigPathOperation::Delete,
        )?
    };

    Ok(PreparedEditIntent::Canonical(CanonicalEditIntent {
        file,
        operation: canonical,
    }))
}
