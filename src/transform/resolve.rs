use std::collections::HashMap;
use std::path::Path;

use crate::changeset::{EditOperation, OpKind, TransformTarget};
use crate::error::IdenteditError;
use crate::handle::{SelectionHandle, Span};
use crate::hash::{ContentHash, hash_text};
use crate::hashline::{LineAnchor, LineHash, compute_line_hash};

pub(super) struct ResolvedOperationView {
    pub(super) expected_hash: ContentHash,
    pub(super) old_text: String,
    pub(super) matched_span: Span,
    pub(super) move_insert_at: Option<usize>,
    pub(super) anchor_identity: Option<String>,
    pub(super) anchor_kind: String,
    pub(super) anchor_span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct KindSpanKey<'a> {
    kind: &'a str,
    start: usize,
    end: usize,
}

pub(super) struct HandleIndex<'a> {
    handles: &'a [SelectionHandle],
    hashes: Vec<ContentHash>,
    by_identity: HashMap<&'a str, Vec<usize>>,
    by_kind_span: HashMap<KindSpanKey<'a>, Vec<usize>>,
}

impl<'a> HandleIndex<'a> {
    pub(super) fn new(handles: &'a [SelectionHandle]) -> Self {
        let mut hashes = Vec::with_capacity(handles.len());
        let mut by_identity: HashMap<&str, Vec<usize>> = HashMap::new();
        let mut by_kind_span: HashMap<KindSpanKey<'a>, Vec<usize>> = HashMap::new();

        for (index, handle) in handles.iter().enumerate() {
            hashes.push(hash_text(&handle.text));
            by_identity
                .entry(handle.identity.as_str())
                .or_default()
                .push(index);
            by_kind_span
                .entry(KindSpanKey {
                    kind: handle.kind.as_str(),
                    start: handle.span.start,
                    end: handle.span.end,
                })
                .or_default()
                .push(index);
        }

        Self {
            handles,
            hashes,
            by_identity,
            by_kind_span,
        }
    }

    fn handles_by_identity(&self, identity: &str) -> Vec<&'a SelectionHandle> {
        let Some(indices) = self.by_identity.get(identity) else {
            return Vec::new();
        };
        indices.iter().map(|idx| &self.handles[*idx]).collect()
    }

    fn handles_by_kind_and_span(&self, kind: &str, span: Span) -> Vec<&'a SelectionHandle> {
        let Some(indices) = self.by_kind_span.get(&KindSpanKey {
            kind,
            start: span.start,
            end: span.end,
        }) else {
            return Vec::new();
        };
        indices.iter().map(|idx| &self.handles[*idx]).collect()
    }

    fn handles_by_kind_and_hash(
        &self,
        kind: &str,
        expected_hash: &ContentHash,
    ) -> Vec<&'a SelectionHandle> {
        self.handles
            .iter()
            .zip(self.hashes.iter())
            .filter_map(|(handle, actual_hash)| {
                (handle.kind == kind && actual_hash == expected_hash).then_some(handle)
            })
            .collect()
    }
}

pub(super) fn resolve_operation_view(
    file: &Path,
    source_text: &str,
    handle_index: &HandleIndex<'_>,
    operation: &EditOperation,
) -> Result<ResolvedOperationView, IdenteditError> {
    let target = operation.target();
    let op = operation.op();
    match target {
        TransformTarget::Node {
            expected_old_hash, ..
        } => {
            if let OpKind::MoveBefore { destination } | OpKind::MoveAfter { destination } = op {
                return resolve_same_file_move_view(
                    file,
                    source_text,
                    handle_index,
                    target,
                    destination,
                    matches!(op, OpKind::MoveBefore { .. }),
                );
            }
            let anchor = resolve_target_in_handles_with_index(file, handle_index, target)?;
            let (old_text, matched_span) = edit_view_for_node_operation(op, &anchor);
            Ok(ResolvedOperationView {
                expected_hash: expected_old_hash.clone(),
                old_text,
                matched_span,
                move_insert_at: None,
                anchor_identity: Some(anchor.identity.clone()),
                anchor_kind: anchor.kind.clone(),
                anchor_span: anchor.span,
            })
        }
        TransformTarget::FileStart { expected_file_hash } => {
            verify_file_target_precondition(source_text, expected_file_hash)?;
            let start = file_content_start_offset(source_text);
            Ok(ResolvedOperationView {
                expected_hash: expected_file_hash.clone(),
                old_text: String::new(),
                matched_span: Span { start, end: start },
                move_insert_at: None,
                anchor_identity: None,
                anchor_kind: "file".to_string(),
                anchor_span: Span { start, end: start },
            })
        }
        TransformTarget::FileEnd { expected_file_hash } => {
            verify_file_target_precondition(source_text, expected_file_hash)?;
            let end = source_text.len();
            Ok(ResolvedOperationView {
                expected_hash: expected_file_hash.clone(),
                old_text: String::new(),
                matched_span: Span { start: end, end },
                move_insert_at: None,
                anchor_identity: None,
                anchor_kind: "file".to_string(),
                anchor_span: Span { start: end, end },
            })
        }
        TransformTarget::File { .. } => Err(IdenteditError::InvalidRequest {
            message: "File target is only valid for whole-file move operations".to_string(),
        }),
        TransformTarget::Line { anchor, end_anchor } => {
            resolve_line_operation_view(source_text, anchor, end_anchor.as_ref(), op)
        }
    }
}

fn resolve_line_operation_view(
    source_text: &str,
    anchor: &LineAnchor,
    end_anchor: Option<&LineAnchor>,
    op: &OpKind,
) -> Result<ResolvedOperationView, IdenteditError> {
    let ranges = compute_line_ranges(source_text);
    let start_line = resolve_line_anchor(anchor, &ranges)?;
    let end_line = match end_anchor {
        Some(raw_end_anchor) => resolve_line_anchor(raw_end_anchor, &ranges)?,
        None => start_line.clone(),
    };

    if end_line.line < start_line.line {
        return Err(IdenteditError::InvalidRequest {
            message: format!(
                "Invalid line target: end line {} must be >= start line {}",
                end_line.line, start_line.line
            ),
        });
    }

    match op {
        OpKind::Replace { .. } => {
            let matched_span = Span {
                start: start_line.full_start,
                end: end_line.full_end,
            };
            let old_text = source_text[matched_span.start..matched_span.end].to_string();
            Ok(ResolvedOperationView {
                expected_hash: hash_text(&old_text),
                old_text,
                matched_span,
                move_insert_at: None,
                anchor_identity: None,
                anchor_kind: "line".to_string(),
                anchor_span: Span {
                    start: start_line.full_start,
                    end: start_line.full_end,
                },
            })
        }
        OpKind::InsertAfter { .. } => {
            assert!(
                end_anchor.is_none(),
                "EditOperation guarantees line insert_after targets omit end_anchor"
            );
            let insert_at = start_line.full_end;
            Ok(ResolvedOperationView {
                expected_hash: hash_text(""),
                old_text: String::new(),
                matched_span: Span {
                    start: insert_at,
                    end: insert_at,
                },
                move_insert_at: None,
                anchor_identity: None,
                anchor_kind: "line".to_string(),
                anchor_span: Span {
                    start: start_line.full_start,
                    end: start_line.full_end,
                },
            })
        }
        _ => unreachable!("EditOperation guarantees line target compatibility"),
    }
}

fn resolve_same_file_move_view(
    file: &Path,
    source_text: &str,
    handle_index: &HandleIndex<'_>,
    source_target: &TransformTarget,
    destination_target: &TransformTarget,
    insert_before: bool,
) -> Result<ResolvedOperationView, IdenteditError> {
    let source_anchor = resolve_target_in_handles_with_index(file, handle_index, source_target)?;
    let destination_offset = resolve_destination_offset(
        file,
        source_text,
        handle_index,
        destination_target,
        insert_before,
    )?;

    if destination_offset >= source_anchor.span.start
        && destination_offset <= source_anchor.span.end
    {
        return Err(IdenteditError::InvalidRequest {
            message: format!(
                "Same-file move destination overlaps source span [{}, {})",
                source_anchor.span.start, source_anchor.span.end
            ),
        });
    }

    let expected_hash = match source_target {
        TransformTarget::Node {
            expected_old_hash, ..
        } => expected_old_hash.clone(),
        _ => unreachable!("same-file move sources are always node targets"),
    };

    Ok(ResolvedOperationView {
        expected_hash,
        old_text: source_anchor.text.clone(),
        matched_span: source_anchor.span,
        move_insert_at: Some(destination_offset),
        anchor_identity: Some(source_anchor.identity),
        anchor_kind: source_anchor.kind,
        anchor_span: source_anchor.span,
    })
}

fn resolve_destination_offset(
    file: &Path,
    source_text: &str,
    handle_index: &HandleIndex<'_>,
    destination_target: &TransformTarget,
    insert_before: bool,
) -> Result<usize, IdenteditError> {
    match destination_target {
        TransformTarget::Node { .. } => {
            let destination_anchor =
                resolve_target_in_handles_with_index(file, handle_index, destination_target)?;
            Ok(if insert_before {
                destination_anchor.span.start
            } else {
                destination_anchor.span.end
            })
        }
        TransformTarget::FileStart { expected_file_hash } => {
            verify_file_target_precondition(source_text, expected_file_hash)?;
            Ok(file_content_start_offset(source_text))
        }
        TransformTarget::FileEnd { expected_file_hash } => {
            verify_file_target_precondition(source_text, expected_file_hash)?;
            Ok(source_text.len())
        }
        TransformTarget::File { .. } => {
            unreachable!("EditOperation rejects whole-file move destinations")
        }
        TransformTarget::Line { anchor, end_anchor } => {
            assert!(
                end_anchor.is_none(),
                "EditOperation guarantees line move destinations omit end_anchor"
            );
            let ranges = compute_line_ranges(source_text);
            let destination_line = resolve_line_anchor(anchor, &ranges)?;
            Ok(if insert_before {
                destination_line.full_start
            } else {
                destination_line.full_end
            })
        }
    }
}

fn edit_view_for_node_operation(op: &OpKind, anchor: &SelectionHandle) -> (String, Span) {
    match op {
        OpKind::Replace { .. } => (anchor.text.clone(), anchor.span),
        OpKind::Delete => (anchor.text.clone(), anchor.span),
        OpKind::InsertBefore { .. } => (
            String::new(),
            Span {
                start: anchor.span.start,
                end: anchor.span.start,
            },
        ),
        OpKind::InsertAfter { .. } => (
            String::new(),
            Span {
                start: anchor.span.end,
                end: anchor.span.end,
            },
        ),
        OpKind::Insert { .. } => {
            unreachable!("EditOperation rejects node target with insert operation")
        }
        OpKind::MoveBefore { .. } | OpKind::MoveAfter { .. } => (anchor.text.clone(), anchor.span),
        OpKind::Move { .. } => {
            unreachable!("EditOperation rejects node target with whole-file move")
        }
    }
}

fn verify_file_target_precondition(
    source_text: &str,
    expected_file_hash: &ContentHash,
) -> Result<(), IdenteditError> {
    let actual_hash = hash_text(source_text);
    if &actual_hash != expected_file_hash {
        return Err(IdenteditError::PreconditionFailed {
            expected_hash: expected_file_hash.to_string(),
            actual_hash: actual_hash.to_string(),
        });
    }

    Ok(())
}

fn file_content_start_offset(source_text: &str) -> usize {
    if source_text.as_bytes().starts_with(&[0xEF, 0xBB, 0xBF]) {
        3
    } else {
        0
    }
}

pub(crate) fn resolve_target_in_handles(
    file: &Path,
    handles: &[SelectionHandle],
    target: &TransformTarget,
) -> Result<SelectionHandle, IdenteditError> {
    let handle_index = HandleIndex::new(handles);
    resolve_target_in_handles_with_index(file, &handle_index, target)
}

fn resolve_target_in_handles_with_index(
    file: &Path,
    handle_index: &HandleIndex<'_>,
    target: &TransformTarget,
) -> Result<SelectionHandle, IdenteditError> {
    let (identity, kind, span_hint, expected_old_hash) = match target {
        TransformTarget::Node {
            identity,
            kind,
            span_hint,
            expected_old_hash,
        } => (identity, kind, *span_hint, expected_old_hash),
        TransformTarget::FileStart { .. }
        | TransformTarget::FileEnd { .. }
        | TransformTarget::File { .. } => {
            return Err(IdenteditError::InvalidRequest {
                message: format!(
                    "Target type '{}' is not resolvable against syntax handles",
                    target.kind_name()
                ),
            });
        }
        TransformTarget::Line { .. } => {
            return Err(IdenteditError::InvalidRequest {
                message: format!(
                    "Target type '{}' is not resolvable against syntax handles",
                    target.kind_name()
                ),
            });
        }
    };

    if let Some(span_hint) = span_hint {
        validate_span_hint(span_hint, identity)?;
    }

    let by_identity = handle_index.handles_by_identity(identity);

    if by_identity.is_empty() {
        if let Some(fallback) = resolve_unique_kind_hash_candidate(
            file,
            handle_index,
            identity,
            kind,
            expected_old_hash,
        )? {
            return Ok(fallback);
        }

        if let Some(span_hint) = span_hint {
            let stale_candidates = handle_index.handles_by_kind_and_span(kind, span_hint);

            if stale_candidates.len() == 1 {
                return Err(IdenteditError::PreconditionFailed {
                    expected_hash: expected_old_hash.to_string(),
                    actual_hash: hash_text(&stale_candidates[0].text).to_string(),
                });
            }

            if stale_candidates.len() > 1 {
                return Err(IdenteditError::AmbiguousTarget {
                    identity: identity.clone(),
                    file: file.display().to_string(),
                    candidates: stale_candidates.len(),
                    candidate_contexts: vec![],
                });
            }
        }

        return Err(IdenteditError::TargetMissing {
            identity: identity.clone(),
            file: file.display().to_string(),
        });
    }

    let by_kind: Vec<&SelectionHandle> = by_identity
        .into_iter()
        .filter(|handle| handle.kind == *kind)
        .collect();

    if by_kind.is_empty() {
        return Err(IdenteditError::TargetMissing {
            identity: identity.clone(),
            file: file.display().to_string(),
        });
    }

    if by_kind.len() == 1 {
        if let Some(span_hint) = span_hint
            && by_kind[0].span != span_hint
            && let Some(fallback) = resolve_unique_kind_hash_candidate(
                file,
                handle_index,
                identity,
                kind,
                expected_old_hash,
            )?
        {
            return Ok(fallback);
        }
        validate_span_hint_match(by_kind[0], span_hint, identity)?;
        return verify_precondition(by_kind[0], expected_old_hash);
    }

    let narrowed_by_span = if let Some(span_hint) = span_hint {
        by_kind
            .iter()
            .copied()
            .filter(|handle| {
                handle.span.start == span_hint.start && handle.span.end == span_hint.end
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };

    if narrowed_by_span.is_empty() {
        if let Some(fallback) = resolve_unique_kind_hash_candidate(
            file,
            handle_index,
            identity,
            kind,
            expected_old_hash,
        )? {
            return Ok(fallback);
        }

        return Err(IdenteditError::AmbiguousTarget {
            identity: identity.clone(),
            file: file.display().to_string(),
            candidates: by_kind.len(),
            candidate_contexts: vec![],
        });
    }

    if narrowed_by_span.len() > 1 {
        if let Some(fallback) = resolve_unique_kind_hash_candidate(
            file,
            handle_index,
            identity,
            kind,
            expected_old_hash,
        )? {
            return Ok(fallback);
        }

        return Err(IdenteditError::AmbiguousTarget {
            identity: identity.clone(),
            file: file.display().to_string(),
            candidates: narrowed_by_span.len(),
            candidate_contexts: vec![],
        });
    }

    verify_precondition(narrowed_by_span[0], expected_old_hash)
}

fn resolve_unique_kind_hash_candidate(
    file: &Path,
    handle_index: &HandleIndex<'_>,
    identity: &str,
    kind: &str,
    expected_old_hash: &ContentHash,
) -> Result<Option<SelectionHandle>, IdenteditError> {
    let candidates = handle_index.handles_by_kind_and_hash(kind, expected_old_hash);

    if candidates.len() == 1 {
        return Ok(Some(candidates[0].clone()));
    }

    if candidates.len() > 1 {
        return Err(IdenteditError::AmbiguousTarget {
            identity: identity.to_string(),
            file: file.display().to_string(),
            candidates: candidates.len(),
            candidate_contexts: vec![],
        });
    }

    Ok(None)
}

fn validate_span_hint(span_hint: Span, identity: &str) -> Result<(), IdenteditError> {
    if span_hint.start > span_hint.end {
        return Err(IdenteditError::InvalidRequest {
            message: format!(
                "Invalid span_hint [{}, {}) for target '{}': start must be <= end",
                span_hint.start, span_hint.end, identity
            ),
        });
    }

    if span_hint.start == span_hint.end {
        return Err(IdenteditError::InvalidRequest {
            message: format!(
                "Invalid span_hint [{}, {}) for target '{}': zero-length spans are not supported",
                span_hint.start, span_hint.end, identity
            ),
        });
    }

    Ok(())
}

fn validate_span_hint_match(
    matched_handle: &SelectionHandle,
    span_hint: Option<Span>,
    identity: &str,
) -> Result<(), IdenteditError> {
    if let Some(span_hint) = span_hint
        && matched_handle.span != span_hint
    {
        return Err(IdenteditError::InvalidRequest {
            message: format!(
                "Provided span_hint [{}, {}) does not match resolved target span [{}, {}) for '{}'",
                span_hint.start,
                span_hint.end,
                matched_handle.span.start,
                matched_handle.span.end,
                identity
            ),
        });
    }

    Ok(())
}

fn verify_precondition(
    matched_handle: &SelectionHandle,
    expected_old_hash: &ContentHash,
) -> Result<SelectionHandle, IdenteditError> {
    let actual_hash = hash_text(&matched_handle.text);
    if &actual_hash != expected_old_hash {
        return Err(IdenteditError::PreconditionFailed {
            expected_hash: expected_old_hash.to_string(),
            actual_hash: actual_hash.to_string(),
        });
    }

    Ok(matched_handle.clone())
}

#[derive(Debug, Clone)]
struct LineRange {
    line: usize,
    full_start: usize,
    full_end: usize,
    expected_hash: LineHash,
}

fn compute_line_ranges(source_text: &str) -> Vec<LineRange> {
    if source_text.is_empty() {
        return Vec::new();
    }

    let bytes = source_text.as_bytes();
    let mut ranges = Vec::new();
    let mut start = 0usize;
    let mut index = 0usize;
    let mut line = 1usize;

    while index < bytes.len() {
        match bytes[index] {
            b'\n' => {
                let content = &source_text[start..index];
                ranges.push(LineRange {
                    line,
                    full_start: start,
                    full_end: index + 1,
                    expected_hash: compute_line_hash(content),
                });
                index += 1;
                start = index;
                line += 1;
            }
            b'\r' => {
                let delimiter_len = if index + 1 < bytes.len() && bytes[index + 1] == b'\n' {
                    2
                } else {
                    1
                };
                let content = &source_text[start..index];
                ranges.push(LineRange {
                    line,
                    full_start: start,
                    full_end: index + delimiter_len,
                    expected_hash: compute_line_hash(content),
                });
                index += delimiter_len;
                start = index;
                line += 1;
            }
            _ => {
                index += 1;
            }
        }
    }

    if start < source_text.len() {
        let content = &source_text[start..];
        ranges.push(LineRange {
            line,
            full_start: start,
            full_end: source_text.len(),
            expected_hash: compute_line_hash(content),
        });
    }

    ranges
}

fn resolve_line_anchor(
    anchor: &LineAnchor,
    ranges: &[LineRange],
) -> Result<LineRange, IdenteditError> {
    if anchor.line() > ranges.len() {
        return Err(IdenteditError::InvalidRequest {
            message: format!(
                "Invalid hashline anchor '{}': line {} is out of range for current file (1..={})",
                anchor,
                anchor.line(),
                ranges.len()
            ),
        });
    }

    let range = ranges[anchor.line() - 1].clone();
    if &range.expected_hash != anchor.hash() {
        return Err(IdenteditError::PreconditionFailed {
            expected_hash: anchor.hash().to_string(),
            actual_hash: range.expected_hash.to_string(),
        });
    }

    Ok(range)
}
