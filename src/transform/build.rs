use std::path::Path;

use crate::changeset::{
    ChangeOp, ChangePreview, EditOperation, FileChange, OpKind, TransformTarget,
};
use crate::error::IdenteditError;
use crate::execution_context::ExecutionContext;
use crate::handle::SelectionHandle;
use crate::hash::ContentHash;
#[cfg(test)]
use crate::hash::hash_text;

use super::MatchedChange;
use super::conflict::{reject_move_operation, validate_change_conflicts_with_indices};
#[cfg(test)]
use super::parse::parse_handles_for_file_with_context;
use super::parse::parse_handles_for_source_with_context;
use super::resolve::{HandleIndex, ResolvedOperationView, resolve_operation_view};

#[derive(Debug)]
pub(crate) struct BuildChangesetError {
    pub(crate) error: IdenteditError,
    pub(crate) operation_indices: Vec<usize>,
}

impl BuildChangesetError {
    fn operation(index: usize, error: IdenteditError) -> Self {
        Self {
            error,
            operation_indices: vec![index],
        }
    }
}

impl From<IdenteditError> for BuildChangesetError {
    fn from(error: IdenteditError) -> Self {
        Self {
            error,
            operation_indices: vec![],
        }
    }
}

#[cfg(test)]
pub(crate) fn build_replace_changeset(
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

#[cfg(test)]
pub(crate) fn build_delete_changeset(
    file: &Path,
    identity: &str,
) -> Result<FileChange, IdenteditError> {
    build_single_identity_changeset(file, identity, OpKind::Delete)
}

#[cfg(test)]
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
    let operation = EditOperation::try_new(target, op)?;
    build_changeset_with_handles(file, &source_text, &handles, vec![operation], None)
        .map_err(|failure| failure.error)
}

#[cfg(test)]
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
            candidate_contexts: vec![],
        }),
    }
}

#[cfg(test)]
pub(crate) fn build_changeset(
    file: &Path,
    instructions: Vec<EditOperation>,
) -> Result<FileChange, IdenteditError> {
    let context = ExecutionContext::new();
    build_changeset_with_context(file, instructions, None, &context)
}

pub(crate) fn build_changeset_with_context(
    file: &Path,
    instructions: Vec<EditOperation>,
    expected_file_hash: Option<ContentHash>,
    context: &ExecutionContext,
) -> Result<FileChange, IdenteditError> {
    build_changeset_with_context_indexed(file, instructions, expected_file_hash, context)
        .map_err(|failure| failure.error)
}

pub(crate) fn build_changeset_with_context_indexed(
    file: &Path,
    instructions: Vec<EditOperation>,
    expected_file_hash: Option<ContentHash>,
    context: &ExecutionContext,
) -> Result<FileChange, BuildChangesetError> {
    let source_text = context.read_file_utf8(file)?;
    let requires_structure_parse = instructions
        .iter()
        .any(|instruction| instruction.target().requires_node_resolution());
    let observation = if requires_structure_parse {
        Some(context.observe_source(file, source_text.as_bytes())?)
    } else {
        None
    };
    let handles = observation
        .as_ref()
        .map_or(&[][..], |source| source.handles());
    build_changeset_with_handles(
        file,
        &source_text,
        handles,
        instructions,
        expected_file_hash,
    )
}

fn build_changeset_with_handles(
    file: &Path,
    source_text: &str,
    handles: &[SelectionHandle],
    instructions: Vec<EditOperation>,
    expected_file_hash: Option<ContentHash>,
) -> Result<FileChange, BuildChangesetError> {
    let handle_index = HandleIndex::new(handles);
    let mut operations = Vec::new();
    let mut matched_changes = Vec::new();

    for (index, instruction) in instructions.into_iter().enumerate() {
        reject_move_operation(instruction.op(), index)
            .map_err(|error| BuildChangesetError::operation(index, error))?;
        let resolved = resolve_operation_view(file, source_text, &handle_index, &instruction)
            .map_err(|error| BuildChangesetError::operation(index, error))?;
        let canonical_target = canonicalize_operation_target(instruction.target(), &resolved);
        let op = instruction.op().clone();
        let effective_op = resolved.effective_op.clone().unwrap_or_else(|| op.clone());
        let preview_new_text = op_new_text(&effective_op).to_string();

        matched_changes.push(MatchedChange {
            index,
            op: effective_op,
            old_text: resolved.old_text.clone(),
            matched_span: resolved.matched_span,
            move_insert_at: resolved.move_insert_at,
            anchor_kind: resolved.anchor_kind.clone(),
            anchor_span: resolved.anchor_span,
        });

        let change = ChangeOp::from_parts(
            canonical_target,
            op,
            ChangePreview::text(
                Some(resolved.old_text),
                None,
                None,
                preview_new_text,
                resolved.matched_span,
            ),
        )
        .map_err(|error| BuildChangesetError::operation(index, error.into()))?;
        operations.push(change);
    }

    validate_change_conflicts_with_indices(&matched_changes).map_err(|conflict| {
        BuildChangesetError {
            error: conflict.error,
            operation_indices: conflict.operation_indices.to_vec(),
        }
    })?;

    let changeset = FileChange {
        file: file.to_path_buf(),
        expected_file_hash,
        operations,
    };
    changeset.verify_source(source_text.as_bytes())?;
    Ok(changeset)
}

fn canonicalize_operation_target(
    target: &TransformTarget,
    resolved: &ResolvedOperationView,
) -> TransformTarget {
    match target {
        TransformTarget::Node {
            identity,
            expected_old_hash,
            ..
        } => TransformTarget::node(
            resolved
                .anchor_identity
                .clone()
                .unwrap_or_else(|| identity.clone()),
            resolved.anchor_kind.clone(),
            Some(resolved.anchor_span),
            expected_old_hash.clone(),
        ),
        TransformTarget::FileStart { expected_file_hash } => TransformTarget::FileStart {
            expected_file_hash: expected_file_hash.clone(),
        },
        TransformTarget::FileEnd { expected_file_hash } => TransformTarget::FileEnd {
            expected_file_hash: expected_file_hash.clone(),
        },
        TransformTarget::File { expected_file_hash } => TransformTarget::File {
            expected_file_hash: expected_file_hash.clone(),
        },
        TransformTarget::Line { anchor, end_anchor } => TransformTarget::Line {
            anchor: anchor.clone(),
            end_anchor: end_anchor.clone(),
        },
    }
}

pub(crate) fn resolve_changeset_targets(
    changeset: &FileChange,
) -> Result<Vec<MatchedChange>, IdenteditError> {
    let context = ExecutionContext::new();
    let source_text = context.read_file_utf8(&changeset.file)?;
    let handles = if changeset
        .operations
        .iter()
        .any(|operation| operation.target().requires_node_resolution())
    {
        parse_handles_for_source_with_context(&changeset.file, source_text.as_bytes(), &context)?
    } else {
        Vec::new()
    };
    resolve_changeset_targets_in_handles(changeset, &source_text, &handles)
}

pub(crate) fn resolve_changeset_targets_in_handles(
    changeset: &FileChange,
    source_text: &str,
    handles: &[SelectionHandle],
) -> Result<Vec<MatchedChange>, IdenteditError> {
    changeset.verify_source(source_text.as_bytes())?;
    let handle_index = HandleIndex::new(handles);
    let mut matched = Vec::new();

    for (index, operation) in changeset.operations.iter().enumerate() {
        reject_move_operation(operation.op(), index)?;
        let resolved = resolve_operation_view(
            &changeset.file,
            source_text,
            &handle_index,
            operation.operation(),
        )?;
        let effective_op = resolved
            .effective_op
            .unwrap_or_else(|| operation.op().clone());
        matched.push(MatchedChange {
            index,
            op: effective_op,
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
        OpKind::SetLine { .. }
        | OpKind::ReplaceLines { .. }
        | OpKind::BlankLines
        | OpKind::DeleteLines
        | OpKind::InsertAfterLine { .. } => {
            unreachable!("logical line operations must be resolved before preview generation")
        }
        OpKind::Delete => "",
        OpKind::InsertBefore { new_text } => new_text,
        OpKind::InsertAfter { new_text } => new_text,
        OpKind::Insert { new_text } => new_text,
        OpKind::MoveBefore { .. } | OpKind::MoveAfter { .. } => "",
        OpKind::Move { .. } => "",
    }
}

#[cfg(test)]
mod tests {
    use super::build_changeset_with_handles;
    use crate::error::IdenteditError;
    use crate::hash::hash_text;
    use std::path::Path;

    #[test]
    fn compilation_checks_the_buffer_used_for_previews_not_a_new_observation() {
        let source = "{\"value\":1}\r\n";
        let expected = hash_text(source);
        let compiled = build_changeset_with_handles(
            Path::new("not-read-from-disk.json"),
            source,
            &[],
            vec![],
            Some(expected.clone()),
        )
        .unwrap();
        assert_eq!(compiled.expected_file_hash, Some(expected.clone()));

        let changed = "{\"value\":1}\n";
        let failure = build_changeset_with_handles(
            Path::new("not-read-from-disk.json"),
            changed,
            &[],
            vec![],
            Some(expected),
        )
        .unwrap_err();
        assert!(matches!(
            failure.error,
            IdenteditError::PreconditionFailed { .. }
        ));
    }
}
