use std::io::Read;
use std::path::PathBuf;

use serde::Deserialize;
use serde_json::Value;

use crate::changeset::{OpKind, TransformTarget};
use crate::error::IdenteditError;
use crate::handle::Span;
use crate::hash::ContentHash;
use crate::hashline::{HashlineEdit, InsertAfterEdit, LineAnchor, ReplaceLinesEdit, SetLineEdit};
use crate::patch::config_path::{
    ConfigPathOperation, MissingPathPolicy, resolve_config_path_operation,
};

use super::super::line_patch::execute_hashline_patch;
use super::execute::{
    run_patch_edit_operation_json, run_patch_node_operation_json,
    run_patch_scoped_regex_node_operation, serialize_line_patch_response,
};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StdinPatchRequest {
    command: String,
    file: PathBuf,
    target: StdinPatchTarget,
    op: Value,
    #[serde(default)]
    options: StdinPatchOptions,
}

#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct StdinPatchOptions {
    #[serde(default)]
    auto_repair: bool,
    #[serde(default)]
    dry_run: bool,
    #[serde(default)]
    verbose: bool,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum StdinPatchTarget {
    Node {
        identity: String,
        kind: String,
        #[serde(default)]
        span_hint: Option<Span>,
        expected_old_hash: ContentHash,
    },
    FileStart {
        expected_file_hash: ContentHash,
    },
    FileEnd {
        expected_file_hash: ContentHash,
    },
    Line {
        anchor: LineAnchor,
        #[serde(default)]
        end_anchor: Option<LineAnchor>,
    },
    ConfigPath {
        path: String,
        #[serde(default)]
        expected_file_hash: Option<ContentHash>,
        #[serde(default)]
        document_index: Option<usize>,
    },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum NodePatchOp {
    Replace {
        new_text: String,
    },
    ScopedRegex {
        pattern: String,
        replacement: String,
    },
    Delete,
    InsertBefore {
        new_text: String,
    },
    InsertAfter {
        new_text: String,
    },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum LinePatchOp {
    SetLine {
        new_text: String,
    },
    ReplaceLines {
        new_text: String,
    },
    #[serde(rename = "insert_after", alias = "line_insert_after")]
    InsertAfter {
        text: String,
    },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum FilePatchOp {
    Insert { new_text: String },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum ConfigPatchOp {
    Set {
        new_text: String,
        #[serde(default)]
        create_missing: bool,
    },
    Append {
        new_text: String,
    },
    Delete,
}

pub(super) fn run_patch_json_mode(cli_dry_run: bool) -> Result<Value, IdenteditError> {
    let mut request_body = String::new();
    std::io::stdin()
        .read_to_string(&mut request_body)
        .map_err(|error| IdenteditError::StdinRead { source: error })?;

    let request: StdinPatchRequest = serde_json::from_str(&request_body)
        .map_err(|source| IdenteditError::InvalidJsonRequest { source })?;

    if request.command != "patch" {
        return Err(IdenteditError::InvalidRequest {
            message: format!(
                "Unsupported command '{}' in patch JSON mode; expected 'patch'",
                request.command
            ),
        });
    }

    let dry_run = cli_dry_run || request.options.dry_run;

    match request.target {
        StdinPatchTarget::Node {
            identity,
            kind,
            span_hint,
            expected_old_hash,
        } => run_patch_json_node(
            request.file,
            TransformTarget::node(identity, kind, span_hint, expected_old_hash),
            request.op,
            dry_run,
            request.options.verbose,
        ),
        StdinPatchTarget::FileStart { expected_file_hash } => run_patch_json_file(
            request.file,
            TransformTarget::FileStart { expected_file_hash },
            request.op,
            dry_run,
            request.options.verbose,
        ),
        StdinPatchTarget::FileEnd { expected_file_hash } => run_patch_json_file(
            request.file,
            TransformTarget::FileEnd { expected_file_hash },
            request.op,
            dry_run,
            request.options.verbose,
        ),
        StdinPatchTarget::Line { anchor, end_anchor } => run_patch_json_line(
            request.file,
            anchor,
            end_anchor,
            request.op,
            request.options.auto_repair,
            dry_run,
        ),
        StdinPatchTarget::ConfigPath {
            path,
            expected_file_hash,
            document_index,
        } => run_patch_json_config(
            request.file,
            path,
            expected_file_hash,
            document_index,
            request.op,
            dry_run,
            request.options.verbose,
        ),
    }
}

fn run_patch_json_file(
    file: PathBuf,
    target: TransformTarget,
    op: Value,
    dry_run: bool,
    verbose: bool,
) -> Result<Value, IdenteditError> {
    let file_op = serde_json::from_value::<FilePatchOp>(op).map_err(|error| {
        IdenteditError::InvalidRequest {
            message: format!("Invalid file patch operation payload: {error}"),
        }
    })?;

    match file_op {
        FilePatchOp::Insert { new_text } => run_patch_node_operation_json(
            file,
            target,
            OpKind::Insert { new_text },
            dry_run,
            verbose,
            None,
        ),
    }
}

fn run_patch_json_node(
    file: PathBuf,
    target: TransformTarget,
    op: Value,
    dry_run: bool,
    verbose: bool,
) -> Result<Value, IdenteditError> {
    let node_op = serde_json::from_value::<NodePatchOp>(op).map_err(|error| {
        IdenteditError::InvalidRequest {
            message: format!("Invalid node patch operation payload: {error}"),
        }
    })?;

    match node_op {
        NodePatchOp::Replace { new_text } => run_patch_node_operation_json(
            file,
            target,
            OpKind::Replace { new_text },
            dry_run,
            verbose,
            None,
        ),
        NodePatchOp::Delete => {
            run_patch_node_operation_json(file, target, OpKind::Delete, dry_run, verbose, None)
        }
        NodePatchOp::InsertBefore { new_text } => run_patch_node_operation_json(
            file,
            target,
            OpKind::InsertBefore { new_text },
            dry_run,
            verbose,
            None,
        ),
        NodePatchOp::InsertAfter { new_text } => run_patch_node_operation_json(
            file,
            target,
            OpKind::InsertAfter { new_text },
            dry_run,
            verbose,
            None,
        ),
        NodePatchOp::ScopedRegex {
            pattern,
            replacement,
        } => run_patch_scoped_regex_node_operation(
            file,
            target,
            pattern,
            replacement,
            dry_run,
            verbose,
        ),
    }
}

fn run_patch_json_line(
    file: PathBuf,
    anchor: LineAnchor,
    end_anchor: Option<LineAnchor>,
    op: Value,
    auto_repair: bool,
    dry_run: bool,
) -> Result<Value, IdenteditError> {
    let line_op = serde_json::from_value::<LinePatchOp>(op).map_err(|error| {
        IdenteditError::InvalidRequest {
            message: format!("Invalid line patch operation payload: {error}"),
        }
    })?;
    let edit = match line_op {
        LinePatchOp::SetLine { new_text } => HashlineEdit::SetLine {
            set_line: SetLineEdit { anchor, new_text },
        },
        LinePatchOp::ReplaceLines { new_text } => HashlineEdit::ReplaceLines {
            replace_lines: ReplaceLinesEdit {
                start_anchor: anchor,
                end_anchor,
                new_text,
            },
        },
        LinePatchOp::InsertAfter { text } => HashlineEdit::InsertAfter {
            insert_after: InsertAfterEdit { anchor, text },
        },
    };
    let patch_response = execute_hashline_patch(file, vec![edit], auto_repair, dry_run)?;
    serialize_line_patch_response(patch_response)
}

fn run_patch_json_config(
    file: PathBuf,
    path: String,
    expected_file_hash: Option<ContentHash>,
    document_index: Option<usize>,
    op: Value,
    dry_run: bool,
    verbose: bool,
) -> Result<Value, IdenteditError> {
    if let Some(object) = op.as_object()
        && object
            .get("type")
            .and_then(|value| value.as_str())
            .is_some_and(|value| value == "delete")
        && object.contains_key("create_missing")
    {
        return Err(IdenteditError::InvalidRequest {
            message: "Config path delete operation does not accept create_missing".to_string(),
        });
    }

    let config_op = serde_json::from_value::<ConfigPatchOp>(op).map_err(|error| {
        IdenteditError::InvalidRequest {
            message: format!("Invalid config path operation payload: {error}"),
        }
    })?;

    let canonical = match config_op {
        ConfigPatchOp::Set {
            new_text,
            create_missing,
        } => resolve_config_path_operation(
            file.as_path(),
            &path,
            expected_file_hash.as_ref(),
            document_index,
            ConfigPathOperation::Set {
                new_text,
                missing_path: MissingPathPolicy::from_create_missing(create_missing),
            },
        )?,
        ConfigPatchOp::Append { new_text } => resolve_config_path_operation(
            file.as_path(),
            &path,
            expected_file_hash.as_ref(),
            document_index,
            ConfigPathOperation::Append { new_text },
        )?,
        ConfigPatchOp::Delete => resolve_config_path_operation(
            file.as_path(),
            &path,
            expected_file_hash.as_ref(),
            document_index,
            ConfigPathOperation::Delete,
        )?,
    };

    run_patch_edit_operation_json(file, canonical, dry_run, verbose)
}
