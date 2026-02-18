use std::path::Path;

use crate::changeset::{ChangeOp, ChangePreview, FileChange, OpKind, TransformTarget, hash_text};
use crate::error::IdenteditError;
use crate::execution_context::ExecutionContext;
use crate::handle::SelectionHandle;

use super::conflict::{reject_move_operation, validate_change_conflicts};
use super::parse::{parse_handles_for_file_with_context, parse_handles_for_source_with_context};
use super::resolve::{HandleIndex, ResolvedOperationView, resolve_operation_view};
use super::{MatchedChange, TransformInstruction};

pub(super) fn build_replace_changeset(
    file: &Path,
    identity: &str,
    replacement: String,
) -> Result<FileChange, IdenteditError> {
    build_single_identity_changeset(
        file,
        identity,
        OpKind::Replace {
            new_text: replacement,
        },
    )
}

pub(super) fn build_delete_changeset(
    file: &Path,
    identity: &str,
) -> Result<FileChange, IdenteditError> {
    build_single_identity_changeset(file, identity, OpKind::Delete)
}

pub(super) fn build_insert_before_changeset(
    file: &Path,
    identity: &str,
    new_text: String,
) -> Result<FileChange, IdenteditError> {
    build_single_identity_changeset(file, identity, OpKind::InsertBefore { new_text })
}

pub(super) fn build_insert_after_changeset(
    file: &Path,
    identity: &str,
    new_text: String,
) -> Result<FileChange, IdenteditError> {
    build_single_identity_changeset(file, identity, OpKind::InsertAfter { new_text })
}

fn build_single_identity_changeset(
    file: &Path,
    identity: &str,
    op: OpKind,
) -> Result<FileChange, IdenteditError> {
    let context = ExecutionContext::new();
    let handles = parse_handles_for_file_with_context(file, &context)?;
    let matched_handle = resolve_unique_identity_handle(file, &handles, identity)?;

    let target = TransformTarget::node(
        matched_handle.identity.clone(),
        matched_handle.kind.clone(),
        Some(matched_handle.span),
        hash_text(&matched_handle.text),
    );

    let source_text = context.read_file_utf8(file)?;
    build_changeset_with_handles(
        file,
        &source_text,
        &handles,
        vec![TransformInstruction { target, op }],
    )
}

fn resolve_unique_identity_handle<'a>(
    file: &Path,
    handles: &'a [SelectionHandle],
    identity: &str,
) -> Result<&'a SelectionHandle, IdenteditError> {
    let matching_handles: Vec<&SelectionHandle> = handles
        .iter()
        .filter(|handle| handle.identity == identity)
        .collect();

    match matching_handles.as_slice() {
        [] => Err(IdenteditError::TargetMissing {
            identity: identity.to_string(),
            file: file.display().to_string(),
        }),
        [single] => Ok(*single),
        candidates => Err(IdenteditError::AmbiguousTarget {
            identity: identity.to_string(),
            file: file.display().to_string(),
            candidates: candidates.len(),
        }),
    }
}

pub(super) fn build_changeset(
    file: &Path,
    instructions: Vec<TransformInstruction>,
) -> Result<FileChange, IdenteditError> {
    let context = ExecutionContext::new();
    let source_text = context.read_file_utf8(file)?;
    let requires_structure_parse = instructions
        .iter()
        .any(|instruction| instruction.target.requires_node_resolution());
    let handles = if requires_structure_parse {
        parse_handles_for_source_with_context(file, source_text.as_bytes(), &context)?
    } else {
        Vec::new()
    };
    build_changeset_with_handles(file, &source_text, &handles, instructions)
}

fn build_changeset_with_handles(
    file: &Path,
    source_text: &str,
    handles: &[SelectionHandle],
    instructions: Vec<TransformInstruction>,
) -> Result<FileChange, IdenteditError> {
    let handle_index = HandleIndex::new(handles);
    let mut operations = Vec::new();
    let mut matched_changes = Vec::new();

    for (index, instruction) in instructions.into_iter().enumerate() {
        reject_move_operation(&instruction.op, index)?;
        let preview_new_text = op_new_text(&instruction.op).to_string();
        let resolved = resolve_operation_view(
            file,
            source_text,
            &handle_index,
            &instruction.target,
            &instruction.op,
            index,
        )?;
        let canonical_target = canonicalize_operation_target(&instruction.target, &resolved);

        matched_changes.push(MatchedChange {
            index,
            target: canonical_target.clone(),
            op: instruction.op.clone(),
            expected_hash: resolved.expected_hash.clone(),
            old_text: resolved.old_text.clone(),
            matched_span: resolved.matched_span,
            move_insert_at: resolved.move_insert_at,
            anchor_kind: resolved.anchor_kind.clone(),
            anchor_span: resolved.anchor_span,
        });

        operations.push(ChangeOp {
            target: canonical_target,
            op: instruction.op,
            preview: ChangePreview {
                old_text: Some(resolved.old_text),
                old_hash: None,
                old_len: None,
                new_text: preview_new_text,
                matched_span: resolved.matched_span,
                move_preview: None,
            },
        });
    }

    validate_change_conflicts(&matched_changes)?;

    Ok(FileChange {
        file: file.to_path_buf(),
        operations,
    })
}

fn canonicalize_operation_target(
    target: &TransformTarget,
    resolved: &ResolvedOperationView,
) -> TransformTarget {
    match target {
        TransformTarget::Node { identity, .. } => TransformTarget::node(
            resolved
                .anchor_identity
                .clone()
                .unwrap_or_else(|| identity.clone()),
            resolved.anchor_kind.clone(),
            Some(resolved.anchor_span),
            resolved.expected_hash.clone(),
        ),
        TransformTarget::FileStart { expected_file_hash } => TransformTarget::FileStart {
            expected_file_hash: expected_file_hash.clone(),
        },
        TransformTarget::FileEnd { expected_file_hash } => TransformTarget::FileEnd {
            expected_file_hash: expected_file_hash.clone(),
        },
        TransformTarget::Line { anchor, end_anchor } => TransformTarget::Line {
            anchor: anchor.clone(),
            end_anchor: end_anchor.clone(),
        },
    }
}

pub(super) fn resolve_changeset_targets(
    changeset: &FileChange,
) -> Result<Vec<MatchedChange>, IdenteditError> {
    let context = ExecutionContext::new();
    let source_text = context.read_file_utf8(&changeset.file)?;
    let handles = if changeset
        .operations
        .iter()
        .any(|operation| operation.target.requires_node_resolution())
    {
        parse_handles_for_source_with_context(&changeset.file, source_text.as_bytes(), &context)?
    } else {
        Vec::new()
    };
    resolve_changeset_targets_in_handles(changeset, &source_text, &handles)
}

pub(super) fn resolve_changeset_targets_in_handles(
    changeset: &FileChange,
    source_text: &str,
    handles: &[SelectionHandle],
) -> Result<Vec<MatchedChange>, IdenteditError> {
    let handle_index = HandleIndex::new(handles);
    let mut matched = Vec::new();

    for (index, operation) in changeset.operations.iter().enumerate() {
        reject_move_operation(&operation.op, index)?;
        let resolved = resolve_operation_view(
            &changeset.file,
            source_text,
            &handle_index,
            &operation.target,
            &operation.op,
            index,
        )?;

        matched.push(MatchedChange {
            index,
            target: operation.target.clone(),
            op: operation.op.clone(),
            expected_hash: resolved.expected_hash,
            old_text: resolved.old_text,
            matched_span: resolved.matched_span,
            move_insert_at: resolved.move_insert_at,
            anchor_kind: resolved.anchor_kind,
            anchor_span: resolved.anchor_span,
        });
    }

    Ok(matched)
}

fn op_new_text(op: &OpKind) -> &str {
    match op {
        OpKind::Replace { new_text } => new_text,
        OpKind::Delete => "",
        OpKind::InsertBefore { new_text } => new_text,
        OpKind::InsertAfter { new_text } => new_text,
        OpKind::Insert { new_text } => new_text,
        OpKind::MoveBefore { .. } | OpKind::MoveAfter { .. } => "",
        OpKind::Move { .. } => "",
    }
}
