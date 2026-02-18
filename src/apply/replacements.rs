use std::path::Path;

use crate::changeset::{FileChange, OpKind, TransformTarget, hash_text};
use crate::error::IdenteditError;
use crate::handle::Span;
use crate::transform::MatchedChange;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ResolvedReplacement {
    pub(super) index: usize,
    pub(super) expected_hash: String,
    pub(super) old_text: String,
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) new_text: String,
}

pub(super) fn matched_changes_to_replacements(
    matched_changes: Vec<MatchedChange>,
) -> Result<Vec<ResolvedReplacement>, IdenteditError> {
    let mut replacements = Vec::with_capacity(matched_changes.len());
    let empty_text_hash = hash_text("");

    for matched in matched_changes {
        match matched.op {
            OpKind::Replace { new_text } => replacements.push(ResolvedReplacement {
                index: matched.index,
                expected_hash: matched.expected_hash,
                old_text: matched.old_text,
                start: matched.matched_span.start,
                end: matched.matched_span.end,
                new_text,
            }),
            OpKind::Delete => replacements.push(ResolvedReplacement {
                index: matched.index,
                expected_hash: matched.expected_hash,
                old_text: matched.old_text,
                start: matched.matched_span.start,
                end: matched.matched_span.end,
                new_text: String::new(),
            }),
            OpKind::InsertBefore { new_text }
            | OpKind::InsertAfter { new_text }
            | OpKind::Insert { new_text } => replacements.push(ResolvedReplacement {
                index: matched.index,
                expected_hash: matched.expected_hash,
                old_text: matched.old_text,
                start: matched.matched_span.start,
                end: matched.matched_span.end,
                new_text,
            }),
            OpKind::MoveBefore { .. } | OpKind::MoveAfter { .. } => {
                let insert_at = matched.move_insert_at.ok_or_else(|| IdenteditError::InvalidRequest {
                    message: format!(
                        "Operation {} uses same-file move, but destination offset was not resolved",
                        matched.index
                    ),
                })?;
                let moved_text = matched.old_text.clone();

                replacements.push(ResolvedReplacement {
                    index: matched.index,
                    expected_hash: matched.expected_hash,
                    old_text: matched.old_text,
                    start: matched.matched_span.start,
                    end: matched.matched_span.end,
                    new_text: String::new(),
                });
                replacements.push(ResolvedReplacement {
                    index: matched.index,
                    expected_hash: empty_text_hash.clone(),
                    old_text: String::new(),
                    start: insert_at,
                    end: insert_at,
                    new_text: moved_text,
                });
            }
            OpKind::Move { .. } => {
                return Err(IdenteditError::InvalidRequest {
                    message: format!(
                        "Operation {} uses move, but move execution is not supported yet",
                        matched.index
                    ),
                });
            }
        }
    }

    Ok(replacements)
}

pub(super) fn apply_replacements_to_text(
    file: &Path,
    mut source_text: String,
    mut replacements: Vec<ResolvedReplacement>,
) -> Result<String, IdenteditError> {
    replacements.sort_by_key(|replacement| (replacement.start, replacement.end, replacement.index));
    ensure_non_overlapping(&replacements)?;

    for replacement in replacements.iter().rev() {
        let span = replacement.start..replacement.end;
        let current_text = source_text
            .get(span.clone())
            .ok_or_else(|| IdenteditError::InvalidRequest {
                message: format!(
                    "Operation {} matched span [{}, {}) is not a valid UTF-8 boundary range for file '{}'",
                    replacement.index,
                    replacement.start,
                    replacement.end,
                    file.display(),
                ),
            })?;

        if current_text != replacement.old_text {
            let actual_hash = hash_text(current_text);
            return Err(IdenteditError::PreconditionFailed {
                expected_hash: replacement.expected_hash.clone(),
                actual_hash,
            });
        }

        source_text.replace_range(span, &replacement.new_text);
    }

    Ok(source_text)
}

pub(super) fn validate_preview_consistency(
    changeset: &FileChange,
    matched_changes: &[MatchedChange],
) -> Result<(), IdenteditError> {
    for matched in matched_changes {
        let operation = changeset.operations.get(matched.index).ok_or_else(|| {
            IdenteditError::InvalidRequest {
                message: format!(
                    "Resolved operation index {} is out of range for changeset",
                    matched.index
                ),
            }
        })?;

        validate_target_preview_span_consistency(matched.index, operation)?;
        validate_preview_old_state(matched.index, operation, matched)?;

        if operation.preview.matched_span != matched.matched_span
            && !allow_stale_preview_span(operation)
        {
            return Err(IdenteditError::InvalidRequest {
                message: format!(
                    "Operation {} preview.matched_span does not match resolved target span; span_hint may be stale",
                    matched.index
                ),
            });
        }

        if operation.preview.move_preview.is_some() {
            return Err(IdenteditError::InvalidRequest {
                message: format!(
                    "Operation {} preview.move is only allowed for move operations",
                    matched.index
                ),
            });
        }

        let op_new_text = match &operation.op {
            OpKind::Replace { new_text } => new_text,
            OpKind::Delete => "",
            OpKind::InsertBefore { new_text } => new_text,
            OpKind::InsertAfter { new_text } => new_text,
            OpKind::Insert { new_text } => new_text,
            OpKind::MoveBefore { .. } | OpKind::MoveAfter { .. } => "",
            OpKind::Move { .. } => {
                return Err(IdenteditError::InvalidRequest {
                    message: format!(
                        "Operation {} uses move, but move execution is not supported yet",
                        matched.index
                    ),
                });
            }
        };
        if operation.preview.new_text != *op_new_text {
            return Err(IdenteditError::InvalidRequest {
                message: format!(
                    "Operation {} preview.new_text does not match op payload",
                    matched.index
                ),
            });
        }
    }

    Ok(())
}

fn validate_target_preview_span_consistency(
    index: usize,
    operation: &crate::changeset::ChangeOp,
) -> Result<(), IdenteditError> {
    let TransformTarget::Node {
        span_hint: Some(span_hint),
        ..
    } = &operation.target
    else {
        return Ok(());
    };

    let expected_preview_span = match operation.op {
        OpKind::Replace { .. }
        | OpKind::Delete
        | OpKind::MoveBefore { .. }
        | OpKind::MoveAfter { .. } => *span_hint,
        OpKind::InsertBefore { .. } => Span {
            start: span_hint.start,
            end: span_hint.start,
        },
        OpKind::InsertAfter { .. } => Span {
            start: span_hint.end,
            end: span_hint.end,
        },
        OpKind::Insert { .. } | OpKind::Move { .. } => operation.preview.matched_span,
    };

    if operation.preview.matched_span != expected_preview_span {
        return Err(IdenteditError::InvalidRequest {
            message: format!(
                "Operation {index} preview.matched_span must be consistent with target span_hint",
            ),
        });
    }

    Ok(())
}

fn allow_stale_preview_span(operation: &crate::changeset::ChangeOp) -> bool {
    if !matches!(
        operation.op,
        OpKind::Replace { .. }
            | OpKind::Delete
            | OpKind::MoveBefore { .. }
            | OpKind::MoveAfter { .. }
    ) {
        return false;
    }

    let TransformTarget::Node {
        span_hint: Some(span_hint),
        ..
    } = &operation.target
    else {
        return false;
    };

    operation.preview.matched_span == *span_hint
}

fn validate_preview_old_state(
    index: usize,
    operation: &crate::changeset::ChangeOp,
    matched: &MatchedChange,
) -> Result<(), IdenteditError> {
    let has_old_text = operation.preview.old_text.is_some();
    let has_compact = operation.preview.old_hash.is_some() || operation.preview.old_len.is_some();

    if has_old_text && has_compact {
        return Err(IdenteditError::InvalidRequest {
            message: format!(
                "Operation {} preview cannot include both full old_text and compact old_hash/old_len fields",
                index
            ),
        });
    }

    if has_old_text {
        if operation.preview.old_text.as_deref() != Some(matched.old_text.as_str()) {
            return Err(IdenteditError::InvalidRequest {
                message: format!(
                    "Operation {} preview.old_text does not match resolved target text",
                    index
                ),
            });
        }
        return Ok(());
    }

    let preview_old_hash =
        operation
            .preview
            .old_hash
            .as_ref()
            .ok_or_else(|| IdenteditError::InvalidRequest {
                message: format!(
                    "Operation {} preview must include either old_text or compact old_hash/old_len fields",
                    index
                ),
            })?;
    let preview_old_len = operation.preview.old_len.ok_or_else(|| {
        IdenteditError::InvalidRequest {
            message: format!(
                "Operation {} preview must include either old_text or compact old_hash/old_len fields",
                index
            ),
        }
    })?;
    let expected_hash = hash_text(&matched.old_text);
    let expected_len = matched.old_text.len();

    if *preview_old_hash != expected_hash {
        return Err(IdenteditError::InvalidRequest {
            message: format!(
                "Operation {} preview.old_hash does not match resolved target text hash",
                index
            ),
        });
    }

    if preview_old_len != expected_len {
        return Err(IdenteditError::InvalidRequest {
            message: format!(
                "Operation {} preview.old_len does not match resolved target text length",
                index
            ),
        });
    }

    Ok(())
}

pub(super) fn ensure_non_overlapping(
    replacements: &[ResolvedReplacement],
) -> Result<(), IdenteditError> {
    for window in replacements.windows(2) {
        let first = &window[0];
        let second = &window[1];
        let first_insert = first.start == first.end;
        let second_insert = second.start == second.end;

        if first.end > second.start
            || (first.end == second.start && (first_insert || second_insert))
        {
            return Err(IdenteditError::InvalidRequest {
                message: format!(
                    "Overlapping operations are not supported: [{}, {}) conflicts with [{}, {})",
                    first.start, first.end, second.start, second.end,
                ),
            });
        }
    }

    Ok(())
}
