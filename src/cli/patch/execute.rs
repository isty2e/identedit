use std::path::PathBuf;

use serde_json::Value;

use crate::apply::{ApplyResponse, apply_multi_file_changeset, dry_run_multi_file_changeset};
use crate::changeset::{EditOperation, FileChange, MultiFileChangeset, OpKind, TransformTarget};
use crate::cli::apply::shape_apply_response;
use crate::error::IdenteditError;
use crate::handle::SelectionHandle;
use crate::patch::engine::run_resolve_verify_apply;
use crate::patch::scoped_regex::rewrite_node_target_with_scoped_regex;
use crate::transform::build::build_changeset;

use super::super::line_patch::{
    HashlinePatchExecution, HashlinePatchResponse, execute_hashline_patch,
    execute_hashline_patch_with_preview,
};
use super::diff::{render_changeset_diff, render_file_diff};
use super::target::NodeTargetSelector;
use super::{ColorMode, PatchArgs, PatchCommandOutput};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ApplyBackedExecution {
    pub dry_run: bool,
    pub verbose: bool,
    output: PatchOutputMode,
}

impl ApplyBackedExecution {
    pub(super) fn from_args(args: &PatchArgs) -> Self {
        Self {
            dry_run: args.dry_run,
            verbose: args.verbose,
            output: PatchOutputMode::from_args(args),
        }
    }

    pub(super) fn json(dry_run: bool, verbose: bool) -> Self {
        Self {
            dry_run,
            verbose,
            output: PatchOutputMode::Json,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineExecution {
    pub dry_run: bool,
    pub auto_repair: bool,
    output: PatchOutputMode,
}

impl LineExecution {
    pub(super) fn from_args(args: &PatchArgs) -> Self {
        Self {
            dry_run: args.dry_run,
            auto_repair: args.auto_repair,
            output: PatchOutputMode::from_args(args),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PatchOutputMode {
    Json,
    Diff { color: ColorMode },
}

impl PatchOutputMode {
    fn from_args(args: &PatchArgs) -> Self {
        if args.diff {
            Self::Diff {
                color: args.color.unwrap_or(ColorMode::Auto),
            }
        } else {
            Self::Json
        }
    }
}

#[derive(Debug, Clone)]
pub struct NodeFlagPatchRequest {
    pub file: PathBuf,
    pub selector: NodeTargetSelector,
    pub operation: PreparedNodePatchOperation,
    pub execution: ApplyBackedExecution,
}

#[derive(Debug, Clone)]
pub(super) struct CanonicalFlagPatchRequest {
    pub file: PathBuf,
    pub operation: EditOperation,
    pub execution: ApplyBackedExecution,
}

#[derive(Debug, Clone)]
pub(super) struct LineFlagPatchRequest {
    pub file: PathBuf,
    pub edit: crate::hashline::HashlineEdit,
    pub execution: LineExecution,
}

#[derive(Debug, Clone)]
pub enum FlagPatchRequest {
    Node(NodeFlagPatchRequest),
    Canonical(CanonicalFlagPatchRequest),
    Line(LineFlagPatchRequest),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreparedNodePatchOperation {
    Standard(OpKind),
    ScopedRegex {
        pattern: String,
        replacement: String,
    },
}

pub(super) fn execute_flag_patch_request(
    request: FlagPatchRequest,
) -> Result<PatchCommandOutput, IdenteditError> {
    match request {
        FlagPatchRequest::Node(request) => execute_node_flag_patch_request(request),
        FlagPatchRequest::Canonical(request) => {
            run_patch_edit_operation(request.file, request.operation, request.execution, None)
        }
        FlagPatchRequest::Line(request) => match request.execution.output {
            PatchOutputMode::Json => {
                let response = execute_hashline_patch(
                    request.file,
                    vec![request.edit],
                    request.execution.auto_repair,
                    request.execution.dry_run,
                )?;
                serialize_line_patch_response(response).map(PatchCommandOutput::Json)
            }
            PatchOutputMode::Diff { color } => {
                let execution = execute_hashline_patch_with_preview(
                    request.file,
                    vec![request.edit],
                    request.execution.auto_repair,
                    request.execution.dry_run,
                )?;
                Ok(PatchCommandOutput::Text(render_line_patch_diff(
                    execution, color,
                )))
            }
        },
    }
}

pub(super) fn execute_node_flag_patch_request(
    request: NodeFlagPatchRequest,
) -> Result<PatchCommandOutput, IdenteditError> {
    let handle = request.selector.resolve(&request.file)?;
    execute_patch_flag_node_operation(request.file, handle, request.operation, request.execution)
}

fn execute_patch_flag_node_operation(
    file: PathBuf,
    handle: SelectionHandle,
    operation: PreparedNodePatchOperation,
    execution: ApplyBackedExecution,
) -> Result<PatchCommandOutput, IdenteditError> {
    let target = TransformTarget::node(
        handle.identity,
        handle.kind,
        Some(handle.span),
        handle.expected_old_hash,
    );

    match operation {
        PreparedNodePatchOperation::Standard(op) => {
            run_patch_node_operation(file, target, op, execution, None)
        }
        PreparedNodePatchOperation::ScopedRegex {
            pattern,
            replacement,
        } => {
            let rewritten =
                rewrite_node_target_with_scoped_regex(&file, &target, &pattern, &replacement)?;
            run_patch_node_operation(
                file,
                target,
                OpKind::Replace {
                    new_text: rewritten.new_text,
                },
                execution,
                Some(rewritten.replacements),
            )
        }
    }
}

pub(super) fn run_patch_node_operation(
    file: PathBuf,
    target: TransformTarget,
    op: OpKind,
    execution: ApplyBackedExecution,
    regex_replacements: Option<usize>,
) -> Result<PatchCommandOutput, IdenteditError> {
    let operation = EditOperation::try_new(target, op)?;
    run_patch_edit_operation(file, operation, execution, regex_replacements)
}

fn run_patch_edit_operation(
    file: PathBuf,
    operation: EditOperation,
    execution: ApplyBackedExecution,
    regex_replacements: Option<usize>,
) -> Result<PatchCommandOutput, IdenteditError> {
    let outcome = run_resolve_verify_apply(
        || {
            let file_change = build_changeset(&file, vec![operation])?;
            Ok(wrap_single_file(file_change))
        },
        verify_prepared_changeset,
        |changeset| {
            let response = if execution.dry_run {
                dry_run_multi_file_changeset(&changeset)
            } else {
                apply_multi_file_changeset(&changeset)
            }?;
            Ok::<_, IdenteditError>(PatchApplyOutcome {
                changeset,
                response,
            })
        },
    )?;

    match execution.output {
        PatchOutputMode::Json => {
            serialize_node_patch_response(outcome.response, execution.verbose, regex_replacements)
                .map(PatchCommandOutput::Json)
        }
        PatchOutputMode::Diff { color } => Ok(PatchCommandOutput::Text(render_changeset_diff(
            &outcome.changeset,
            color,
        )?)),
    }
}

pub(super) fn run_patch_edit_operation_json(
    file: PathBuf,
    operation: EditOperation,
    dry_run: bool,
    verbose: bool,
) -> Result<Value, IdenteditError> {
    let output = run_patch_edit_operation(
        file,
        operation,
        ApplyBackedExecution::json(dry_run, verbose),
        None,
    )?;
    match output {
        PatchCommandOutput::Json(value) => Ok(value),
        PatchCommandOutput::Text(_) => Err(IdenteditError::InvalidRequest {
            message: "Internal patch error: JSON mode unexpectedly produced text output"
                .to_string(),
        }),
    }
}

pub(super) fn run_patch_node_operation_json(
    file: PathBuf,
    target: TransformTarget,
    op: OpKind,
    dry_run: bool,
    verbose: bool,
    regex_replacements: Option<usize>,
) -> Result<Value, IdenteditError> {
    let output = run_patch_node_operation(
        file,
        target,
        op,
        ApplyBackedExecution::json(dry_run, verbose),
        regex_replacements,
    )?;
    match output {
        PatchCommandOutput::Json(value) => Ok(value),
        PatchCommandOutput::Text(_) => Err(IdenteditError::InvalidRequest {
            message: "Internal patch error: JSON mode unexpectedly produced text output"
                .to_string(),
        }),
    }
}

pub(super) fn run_patch_scoped_regex_node_operation(
    file: PathBuf,
    target: TransformTarget,
    pattern: String,
    replacement: String,
    dry_run: bool,
    verbose: bool,
) -> Result<Value, IdenteditError> {
    let rewritten = rewrite_node_target_with_scoped_regex(&file, &target, &pattern, &replacement)?;
    run_patch_node_operation_json(
        file,
        target,
        OpKind::Replace {
            new_text: rewritten.new_text,
        },
        dry_run,
        verbose,
        Some(rewritten.replacements),
    )
}

fn serialize_node_patch_response(
    response: ApplyResponse,
    verbose: bool,
    regex_replacements: Option<usize>,
) -> Result<Value, IdenteditError> {
    let mut value = serde_json::to_value(shape_apply_response(response, verbose))
        .map_err(|source| IdenteditError::ResponseSerialization { source })?;
    if let Some(replacements) = regex_replacements
        && let Some(object) = value.as_object_mut()
    {
        object.insert(
            "regex_replacements".to_string(),
            Value::Number(serde_json::Number::from(replacements)),
        );
    }
    Ok(value)
}

pub(super) fn serialize_line_patch_response(
    response: HashlinePatchResponse,
) -> Result<Value, IdenteditError> {
    serde_json::to_value(response)
        .map_err(|source| IdenteditError::ResponseSerialization { source })
}

struct PatchApplyOutcome {
    changeset: MultiFileChangeset,
    response: ApplyResponse,
}

fn render_line_patch_diff(execution: HashlinePatchExecution, color: ColorMode) -> String {
    render_file_diff(
        &execution.response.file,
        &execution.source,
        &execution.applied_content,
        color,
    )
}

fn wrap_single_file(file_change: FileChange) -> MultiFileChangeset {
    MultiFileChangeset {
        files: vec![file_change],
        transaction: Default::default(),
    }
}

fn verify_prepared_changeset(
    changeset: MultiFileChangeset,
) -> Result<MultiFileChangeset, IdenteditError> {
    Ok(changeset)
}
