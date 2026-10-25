use std::path::PathBuf;

use serde_json::Value;

use crate::apply::{
    ApplyResponse, apply_multi_file_changeset_with_context,
    preview_multi_file_changeset_with_context,
};
use crate::changeset::{EditOperation, FileChange, MultiFileChangeset, OpKind, TransformTarget};
use crate::cli::apply::shape_apply_response;
use crate::cli::edit_intent::{NodeEditIntent, PreparedEditIntent};
use crate::error::IdenteditError;
use crate::execution_context::ExecutionContext;
use crate::patch::engine::run_resolve_verify_apply;
use crate::patch::scoped_regex::rewrite_node_target_with_scoped_regex;
use crate::transform::build::build_changeset_with_context;

use super::super::line_patch::{HashlinePatchExecution, execute_hashline_patch_with_preview};
use super::{ColorMode, PatchArgs, PatchCommandOutput};
use crate::cli::diff::{DiffResponse, render_file_diff};

#[derive(Debug, Clone, Copy)]
pub(super) struct PatchExecution {
    apply_backed: ApplyBackedExecution,
    line: LineExecution,
}

impl PatchExecution {
    pub(super) fn from_args(args: &PatchArgs) -> Self {
        Self {
            apply_backed: ApplyBackedExecution::from_args(args),
            line: LineExecution::from_args(args),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ApplyBackedExecution {
    pub dry_run: bool,
    pub verbose: bool,
    output: PatchOutputMode,
    pub(super) include_diff: bool,
}

impl ApplyBackedExecution {
    pub(super) fn from_args(args: &PatchArgs) -> Self {
        Self {
            dry_run: args.dry_run,
            verbose: args.verbose,
            output: PatchOutputMode::from_args(args),
            include_diff: !args.no_diff,
        }
    }

    pub(super) fn json(dry_run: bool, verbose: bool, include_diff: bool) -> Self {
        Self {
            dry_run,
            verbose,
            output: PatchOutputMode::Json,
            include_diff,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineExecution {
    pub dry_run: bool,
    pub auto_repair: bool,
    output: PatchOutputMode,
    include_diff: bool,
}

impl LineExecution {
    pub(super) fn from_args(args: &PatchArgs) -> Self {
        Self {
            dry_run: args.dry_run,
            auto_repair: args.auto_repair,
            output: PatchOutputMode::from_args(args),
            include_diff: !args.no_diff,
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

pub(super) fn execute_flag_patch_request(
    intent: PreparedEditIntent,
    execution: PatchExecution,
) -> Result<PatchCommandOutput, IdenteditError> {
    match intent {
        PreparedEditIntent::Node(intent) => execute_node_flag_patch_request(
            intent,
            execution.apply_backed,
            &ExecutionContext::new(),
        ),
        PreparedEditIntent::Canonical(intent) => run_patch_edit_operation(
            intent.file,
            intent.operation,
            intent.expected_file_hash,
            execution.apply_backed,
            None,
            &ExecutionContext::new(),
        ),
        PreparedEditIntent::Line(intent) => match execution.line.output {
            PatchOutputMode::Json => {
                let response = execute_hashline_patch_with_preview(
                    intent.file,
                    vec![intent.edit],
                    execution.line.auto_repair,
                    execution.line.dry_run,
                )?;
                serialize_line_patch_response(response, execution.line.include_diff)
                    .map(PatchCommandOutput::Json)
            }
            PatchOutputMode::Diff { color } => {
                let execution = execute_hashline_patch_with_preview(
                    intent.file,
                    vec![intent.edit],
                    execution.line.auto_repair,
                    execution.line.dry_run,
                )?;
                Ok(PatchCommandOutput::Text(render_line_patch_diff(
                    execution, color,
                )))
            }
        },
    }
}

fn execute_node_flag_patch_request(
    intent: NodeEditIntent,
    execution: ApplyBackedExecution,
    context: &ExecutionContext,
) -> Result<PatchCommandOutput, IdenteditError> {
    let resolved = intent.resolve_with_context(context)?;
    run_patch_edit_operation(
        resolved.file,
        resolved.operation,
        None,
        execution,
        resolved.regex_replacements,
        context,
    )
}

pub(super) fn run_patch_node_operation(
    file: PathBuf,
    target: TransformTarget,
    op: OpKind,
    execution: ApplyBackedExecution,
    regex_replacements: Option<usize>,
    context: &ExecutionContext,
) -> Result<PatchCommandOutput, IdenteditError> {
    let operation = EditOperation::try_new(target, op)?;
    run_patch_edit_operation(
        file,
        operation,
        None,
        execution,
        regex_replacements,
        context,
    )
}

fn run_patch_edit_operation(
    file: PathBuf,
    operation: EditOperation,
    expected_file_hash: Option<crate::hash::ContentHash>,
    execution: ApplyBackedExecution,
    regex_replacements: Option<usize>,
    context: &ExecutionContext,
) -> Result<PatchCommandOutput, IdenteditError> {
    run_resolve_verify_apply(
        || {
            let file_change =
                build_changeset_with_context(&file, vec![operation], expected_file_hash, context)?;
            Ok(wrap_single_file(file_change))
        },
        verify_prepared_changeset,
        |changeset| match execution.output {
            PatchOutputMode::Diff { color } => {
                let preview = preview_multi_file_changeset_with_context(&changeset, context)?;
                Ok(PatchCommandOutput::Text(
                    preview
                        .files
                        .iter()
                        .map(|file| render_file_diff(&file.file, &file.before, &file.after, color))
                        .collect(),
                ))
            }
            PatchOutputMode::Json => {
                let response = if execution.dry_run {
                    preview_multi_file_changeset_with_context(&changeset, context)
                        .map(|preview| preview.response)
                } else {
                    apply_multi_file_changeset_with_context(&changeset, context)
                }?;
                serialize_node_patch_response(
                    response,
                    execution.verbose,
                    execution.include_diff,
                    regex_replacements,
                )
                .map(PatchCommandOutput::Json)
            }
        },
    )
}

pub(super) fn run_patch_edit_operation_json(
    file: PathBuf,
    operation: EditOperation,
    expected_file_hash: Option<crate::hash::ContentHash>,
    execution: ApplyBackedExecution,
) -> Result<Value, IdenteditError> {
    let output = run_patch_edit_operation(
        file,
        operation,
        expected_file_hash,
        execution,
        None,
        &ExecutionContext::new(),
    )?;
    json_patch_output(output)
}

fn json_patch_output(output: PatchCommandOutput) -> Result<Value, IdenteditError> {
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
    execution: ApplyBackedExecution,
    regex_replacements: Option<usize>,
) -> Result<Value, IdenteditError> {
    let output = run_patch_node_operation(
        file,
        target,
        op,
        execution,
        regex_replacements,
        &ExecutionContext::new(),
    )?;
    json_patch_output(output)
}

pub(super) fn run_patch_scoped_regex_node_operation(
    file: PathBuf,
    target: TransformTarget,
    pattern: String,
    replacement: String,
    execution: ApplyBackedExecution,
) -> Result<Value, IdenteditError> {
    let context = ExecutionContext::new();
    let rewritten =
        rewrite_node_target_with_scoped_regex(&file, &target, &pattern, &replacement, &context)?;
    let output = run_patch_node_operation(
        file,
        target,
        OpKind::Replace {
            new_text: rewritten.new_text,
        },
        execution,
        Some(rewritten.replacements),
        &context,
    )?;
    json_patch_output(output)
}

fn serialize_node_patch_response(
    response: ApplyResponse,
    verbose: bool,
    include_diff: bool,
    regex_replacements: Option<usize>,
) -> Result<Value, IdenteditError> {
    let mut value = serde_json::to_value(shape_apply_response(response, verbose, include_diff))
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
    execution: HashlinePatchExecution,
    include_diff: bool,
) -> Result<Value, IdenteditError> {
    let diff = if include_diff && !execution.response.dry_run {
        Some(DiffResponse::from_text(
            &execution.response.file,
            &execution.source,
            &execution.applied_content,
        ))
    } else {
        None
    };
    let mut value = serde_json::to_value(execution.response)
        .map_err(|source| IdenteditError::ResponseSerialization { source })?;
    if let Some(diff) = diff {
        value["diff"] = serde_json::to_value(diff)
            .map_err(|source| IdenteditError::ResponseSerialization { source })?;
    }
    Ok(value)
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

#[cfg(test)]
mod tests;
