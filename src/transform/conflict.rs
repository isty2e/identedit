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
        if write_spans_conflict(first.span, second.span) {
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

pub(crate) fn write_spans_conflict(first: Span, second: Span) -> bool {
    let (first, second) = if (first.start, first.end) <= (second.start, second.end) {
        (first, second)
    } else {
        (second, first)
    };
    let first_insert = first.start == first.end;
    let second_insert = second.start == second.end;

    first.end > second.start || (first.end == second.start && (first_insert || second_insert))
}

#[derive(Debug, Clone, Copy)]
struct EffectSpan {
    operation_index: usize,
    span: Span,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_conflicts_preserve_point_and_endpoint_semantics_in_both_orders() {
        for (first, second, conflict) in [
            ((0, 2), (2, 4), false),
            ((0, 2), (1, 4), true),
            ((0, 2), (0, 0), true),
            ((0, 2), (1, 1), true),
            ((0, 2), (2, 2), true),
            ((0, 2), (3, 3), false),
            ((2, 2), (2, 2), true),
        ] {
            let first = Span {
                start: first.0,
                end: first.1,
            };
            let second = Span {
                start: second.0,
                end: second.1,
            };
            assert_eq!(write_spans_conflict(first, second), conflict);
            assert_eq!(write_spans_conflict(second, first), conflict);
        }
    }
}
