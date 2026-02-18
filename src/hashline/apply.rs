use super::{
    HashlineApplyError, HashlineCheckError, HashlineEdit, LineSpan, LineSpanKind, ResolvedEdit,
    ResolvedOperation, parse_line_ref,
};

pub(super) fn resolve_edits(
    lines: &[String],
    edits: &[HashlineEdit],
) -> Result<Vec<ResolvedEdit>, HashlineApplyError> {
    let line_count = lines.len();
    let mut resolved = Vec::with_capacity(edits.len());

    for (edit_index, edit) in edits.iter().enumerate() {
        match edit {
            HashlineEdit::SetLine { set_line } => {
                let anchor = parse_line_ref(&set_line.anchor)?;
                ensure_line_exists(anchor.line, line_count, &set_line.anchor)?;
                let replacement_lines = super::show::split_set_line_text(&set_line.new_text);
                resolved.push(ResolvedEdit {
                    edit_index,
                    span: LineSpan {
                        kind: LineSpanKind::Replace,
                        start_line: anchor.line,
                        end_line: anchor.line,
                    },
                    operation: ResolvedOperation::ReplaceRange {
                        start_line: anchor.line,
                        end_line: anchor.line,
                        replacement_lines,
                    },
                });
            }
            HashlineEdit::ReplaceLines { replace_lines } => {
                let start = parse_line_ref(&replace_lines.start_anchor)?;
                let end = if let Some(end_anchor) = &replace_lines.end_anchor {
                    parse_line_ref(end_anchor)?
                } else {
                    start.clone()
                };

                ensure_line_exists(start.line, line_count, &replace_lines.start_anchor)?;
                ensure_line_exists(
                    end.line,
                    line_count,
                    replace_lines
                        .end_anchor
                        .as_deref()
                        .unwrap_or(&replace_lines.start_anchor),
                )?;

                if end.line < start.line {
                    return Err(HashlineCheckError::InvalidRequest {
                        message: format!(
                            "Invalid replace_lines edit #{}: end line {} must be >= start line {}",
                            edit_index, end.line, start.line
                        ),
                    }
                    .into());
                }

                let replacement_lines =
                    super::show::split_replace_lines_text(&replace_lines.new_text);
                resolved.push(ResolvedEdit {
                    edit_index,
                    span: LineSpan {
                        kind: LineSpanKind::Replace,
                        start_line: start.line,
                        end_line: end.line,
                    },
                    operation: ResolvedOperation::ReplaceRange {
                        start_line: start.line,
                        end_line: end.line,
                        replacement_lines,
                    },
                });
            }
            HashlineEdit::InsertAfter { insert_after } => {
                let anchor = parse_line_ref(&insert_after.anchor)?;
                ensure_line_exists(anchor.line, line_count, &insert_after.anchor)?;
                if insert_after.text.is_empty() {
                    return Err(HashlineCheckError::InvalidRequest {
                        message: format!(
                            "Invalid insert_after edit #{}: text must not be empty",
                            edit_index
                        ),
                    }
                    .into());
                }

                let insert_lines = super::show::split_multiline_text(&insert_after.text);
                resolved.push(ResolvedEdit {
                    edit_index,
                    span: LineSpan {
                        kind: LineSpanKind::InsertAfter,
                        start_line: anchor.line,
                        end_line: anchor.line,
                    },
                    operation: ResolvedOperation::InsertAfter {
                        anchor_line: anchor.line,
                        insert_lines,
                    },
                });
            }
        }
    }

    Ok(resolved)
}

pub(super) fn ensure_non_overlapping(resolved: &[ResolvedEdit]) -> Result<(), HashlineApplyError> {
    for left_index in 0..resolved.len() {
        for right_index in (left_index + 1)..resolved.len() {
            let left = &resolved[left_index];
            let right = &resolved[right_index];
            if edits_conflict(left, right) {
                return Err(HashlineApplyError::Overlap {
                    first_edit_index: left.edit_index,
                    second_edit_index: right.edit_index,
                    first_span: left.span.clone(),
                    second_span: right.span.clone(),
                });
            }
        }
    }

    Ok(())
}

fn edits_conflict(left: &ResolvedEdit, right: &ResolvedEdit) -> bool {
    match (&left.operation, &right.operation) {
        (
            ResolvedOperation::ReplaceRange {
                start_line: left_start,
                end_line: left_end,
                ..
            },
            ResolvedOperation::ReplaceRange {
                start_line: right_start,
                end_line: right_end,
                ..
            },
        ) => left_start <= right_end && right_start <= left_end,
        (
            ResolvedOperation::InsertAfter {
                anchor_line: left_anchor,
                ..
            },
            ResolvedOperation::InsertAfter {
                anchor_line: right_anchor,
                ..
            },
        ) => left_anchor == right_anchor,
        (
            ResolvedOperation::InsertAfter {
                anchor_line: insert_anchor,
                ..
            },
            ResolvedOperation::ReplaceRange {
                start_line,
                end_line,
                ..
            },
        )
        | (
            ResolvedOperation::ReplaceRange {
                start_line,
                end_line,
                ..
            },
            ResolvedOperation::InsertAfter {
                anchor_line: insert_anchor,
                ..
            },
        ) => {
            let boundary_start = start_line.saturating_sub(1);
            insert_anchor >= &boundary_start && insert_anchor <= end_line
        }
    }
}

fn ensure_line_exists(
    line: usize,
    line_count: usize,
    anchor: &str,
) -> Result<(), HashlineCheckError> {
    if line == 0 || line > line_count {
        return Err(HashlineCheckError::InvalidRequest {
            message: format!(
                "Invalid hashline anchor '{}': line {} is out of range for current file (1..={})",
                anchor, line, line_count
            ),
        });
    }
    Ok(())
}
