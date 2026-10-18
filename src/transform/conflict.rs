use std::collections::BTreeMap;

use crate::changeset::OpKind;
use crate::error::IdenteditError;
use crate::handle::Span;

use super::MatchedChange;

#[derive(Debug)]
pub(crate) struct ChangeConflict {
    pub(crate) error: IdenteditError,
    pub(crate) operation_indices: [usize; 2],
}

pub(crate) fn reject_move_operation(op: &OpKind, index: usize) -> Result<(), IdenteditError> {
    if let OpKind::Move { .. } = op {
        return Err(IdenteditError::InvalidRequest {
            message: format!(
                "Operation {index} uses file move, but file move operations cannot be built by edit; use move_before/move_after for structural moves"
            ),
        });
    }

    Ok(())
}

pub(crate) fn validate_change_conflicts(
    matched_changes: &[MatchedChange],
) -> Result<(), IdenteditError> {
    validate_change_conflicts_with_indices(matched_changes).map_err(|conflict| conflict.error)
}

pub(crate) fn validate_change_conflicts_with_indices(
    matched_changes: &[MatchedChange],
) -> Result<(), ChangeConflict> {
    let mut anchor_groups: BTreeMap<(String, usize, usize), Vec<&MatchedChange>> = BTreeMap::new();
    for matched in matched_changes {
        anchor_groups
            .entry((
                matched.anchor_kind.clone(),
                matched.anchor_span.start,
                matched.anchor_span.end,
            ))
            .or_default()
            .push(matched);
    }

    for group in anchor_groups.into_values() {
        let anchor_rewrite = group.iter().find(|matched| {
            matches!(
                matched.op,
                OpKind::Replace { .. }
                    | OpKind::SetLine { .. }
                    | OpKind::ReplaceLines { .. }
                    | OpKind::BlankLines
                    | OpKind::DeleteLines
                    | OpKind::Delete
                    | OpKind::MoveBefore { .. }
                    | OpKind::MoveAfter { .. }
            )
        });
        let insert = group.iter().find(|matched| {
            matches!(
                matched.op,
                OpKind::InsertBefore { .. }
                    | OpKind::InsertAfter { .. }
                    | OpKind::Insert { .. }
                    | OpKind::InsertAfterLine { .. }
            )
        });
        if let (Some(rewrite), Some(insert)) = (anchor_rewrite, insert) {
            let anchor = group[0];
            return Err(ChangeConflict {
                operation_indices: [rewrite.index, insert.index],
                error: IdenteditError::InvalidRequest {
                    message: format!(
                        "Conflicting operations on the same anchor are not supported: anchor '{}' [{}, {}) mixes rewrite and insert operations",
                        anchor.anchor_kind, anchor.anchor_span.start, anchor.anchor_span.end
                    ),
                },
            });
        }
    }

    let mut effects = Vec::new();
    for matched in matched_changes {
        effects.push(EffectSpan {
            operation_index: matched.index,
            span: matched.matched_span,
        });
        if let Some(insert_at) = matched.move_insert_at {
            effects.push(EffectSpan {
                operation_index: matched.index,
                span: Span {
                    start: insert_at,
                    end: insert_at,
                },
            });
        }
    }

    effects.sort_by_key(|effect| (effect.span.start, effect.span.end, effect.operation_index));

    for window in effects.windows(2) {
        let first = &window[0];
        let second = &window[1];
        let first_insert = first.span.start == first.span.end;
        let second_insert = second.span.start == second.span.end;

        if first.span.end > second.span.start
            || (first.span.end == second.span.start && (first_insert || second_insert))
        {
            return Err(ChangeConflict {
                operation_indices: [first.operation_index, second.operation_index],
                error: IdenteditError::InvalidRequest {
                    message: format!(
                        "Overlapping operations are not supported: [{}, {}) conflicts with [{}, {})",
                        first.span.start, first.span.end, second.span.start, second.span.end,
                    ),
                },
            });
        }
    }

    Ok(())
}

#[derive(Debug, Clone, Copy)]
struct EffectSpan {
    operation_index: usize,
    span: Span,
}
