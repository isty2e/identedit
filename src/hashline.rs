use serde::{Deserialize, Serialize};
use thiserror::Error;

mod anchor;
mod apply;
mod check;
mod repair;
mod show;

pub use anchor::{LineAnchor, LineHash};

pub const HASHLINE_PUBLIC_HEX_LEN: usize = 12;
const HASHLINE_DISPLAY_MIN_HEX_LEN: usize = 8;
const HASHLINE_DISPLAY_MAX_HEX_LEN: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HashedLine {
    pub line: usize,
    pub hash: LineHash,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HashlineMismatchStatus {
    Mismatch,
    Remappable,
    Ambiguous,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HashlineRemapTarget {
    pub line: usize,
    pub hash: LineHash,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HashlineMismatch {
    pub edit_index: usize,
    pub anchor: LineAnchor,
    pub line: usize,
    pub expected_hash: LineHash,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual_hash: Option<LineHash>,
    pub status: HashlineMismatchStatus,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub remaps: Vec<HashlineRemapTarget>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct HashlineCheckSummary {
    pub total: usize,
    pub matched: usize,
    pub mismatched: usize,
    pub remappable: usize,
    pub ambiguous: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HashlineCheckResult {
    pub ok: bool,
    pub summary: HashlineCheckSummary,
    pub mismatches: Vec<HashlineMismatch>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetLineEdit {
    pub anchor: LineAnchor,
    pub new_text: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplaceLinesEdit {
    pub start_anchor: LineAnchor,
    #[serde(default)]
    pub end_anchor: Option<LineAnchor>,
    pub new_text: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InsertAfterEdit {
    pub anchor: LineAnchor,
    pub text: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, untagged)]
pub enum HashlineEdit {
    SetLine { set_line: SetLineEdit },
    ReplaceLines { replace_lines: ReplaceLinesEdit },
    InsertAfter { insert_after: InsertAfterEdit },
}

impl HashlineEdit {
    fn anchors_with_index(&self, edit_index: usize) -> Vec<AnchorCheckRequest> {
        match self {
            Self::SetLine { set_line } => vec![AnchorCheckRequest {
                edit_index,
                anchor: set_line.anchor.clone(),
            }],
            Self::ReplaceLines { replace_lines } => {
                let mut anchors = vec![AnchorCheckRequest {
                    edit_index,
                    anchor: replace_lines.start_anchor.clone(),
                }];
                if let Some(end_anchor) = &replace_lines.end_anchor {
                    anchors.push(AnchorCheckRequest {
                        edit_index,
                        anchor: end_anchor.clone(),
                    });
                }
                anchors
            }
            Self::InsertAfter { insert_after } => vec![AnchorCheckRequest {
                edit_index,
                anchor: insert_after.anchor.clone(),
            }],
        }
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum HashlineCheckError {
    #[error("Invalid hashline request: {message}")]
    InvalidRequest { message: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LineSpanKind {
    Replace,
    InsertAfter,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LineSpan {
    pub kind: LineSpanKind,
    pub start_line: usize,
    pub end_line: usize,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum HashlineApplyError {
    #[error(transparent)]
    Check(#[from] HashlineCheckError),

    #[error("Hashline preconditions failed")]
    PreconditionFailed { check: HashlineCheckResult },

    #[error(
        "Overlapping hashline edits are not allowed between edit #{first_edit_index} and edit #{second_edit_index}"
    )]
    Overlap {
        first_edit_index: usize,
        second_edit_index: usize,
        first_span: LineSpan,
        second_span: LineSpan,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HashlineApplyResult {
    pub content: String,
    pub operations_total: usize,
    pub operations_applied: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HashlineApplyMode {
    Strict,
    Repair,
}

#[derive(Debug, Clone)]
struct AnchorCheckRequest {
    edit_index: usize,
    anchor: LineAnchor,
}

#[derive(Debug, Clone)]
struct ResolvedEdit {
    edit_index: usize,
    span: LineSpan,
    operation: ResolvedOperation,
}

impl ResolvedEdit {
    fn sort_key(&self) -> usize {
        match self.operation {
            ResolvedOperation::ReplaceRange { end_line, .. } => end_line,
            ResolvedOperation::InsertAfter { anchor_line, .. } => anchor_line,
        }
    }
}

#[derive(Debug, Clone)]
enum ResolvedOperation {
    ReplaceRange {
        start_line: usize,
        end_line: usize,
        replacement_lines: Vec<String>,
    },
    InsertAfter {
        anchor_line: usize,
        insert_lines: Vec<String>,
    },
}

pub fn compute_line_hash(line: &str) -> LineHash {
    LineHash::from_content(line)
}

pub fn show_hashed_lines(source: &str) -> Vec<HashedLine> {
    show::show_hashed_lines(source)
}

pub fn check_hashline_edits(
    source: &str,
    edits: &[HashlineEdit],
) -> Result<HashlineCheckResult, HashlineCheckError> {
    let anchors = edits
        .iter()
        .enumerate()
        .flat_map(|(edit_index, edit)| edit.anchors_with_index(edit_index))
        .collect::<Vec<_>>();
    check::check_hashline_anchors(source, &anchors)
}

#[cfg(test)]
pub fn check_hashline_refs(
    source: &str,
    refs: &[String],
) -> Result<HashlineCheckResult, HashlineCheckError> {
    let anchors = refs
        .iter()
        .map(|anchor| {
            LineAnchor::parse(anchor).map_err(|error| HashlineCheckError::InvalidRequest {
                message: error.to_string(),
            })
        })
        .collect::<Result<Vec<_>, HashlineCheckError>>()?;
    check_line_anchors(source, &anchors)
}

pub(crate) fn check_line_anchors(
    source: &str,
    anchors: &[LineAnchor],
) -> Result<HashlineCheckResult, HashlineCheckError> {
    let anchors = anchors
        .iter()
        .cloned()
        .enumerate()
        .map(|(edit_index, anchor)| AnchorCheckRequest { edit_index, anchor })
        .collect::<Vec<_>>();
    check::check_hashline_anchors(source, &anchors)
}

#[cfg(test)]
pub(crate) fn apply_hashline_edits(
    source: &str,
    edits: &[HashlineEdit],
) -> Result<HashlineApplyResult, HashlineApplyError> {
    apply_hashline_edits_with_mode(source, edits, HashlineApplyMode::Strict)
}

pub fn apply_hashline_edits_with_mode(
    source: &str,
    edits: &[HashlineEdit],
    mode: HashlineApplyMode,
) -> Result<HashlineApplyResult, HashlineApplyError> {
    let prepared_edits = repair::prepare_edits_for_mode(source, edits, mode)?;
    let check = check_hashline_edits(source, &prepared_edits)?;
    if !check.ok {
        return Err(HashlineApplyError::PreconditionFailed { check });
    }

    let mut source_layout = show::split_source_lines(source);
    let mut resolved = apply::resolve_edits(source_layout.line_count(), &prepared_edits)?;
    if mode == HashlineApplyMode::Repair {
        repair::apply_repair_merge_expansion(&source_layout, &mut resolved);
    }
    apply::ensure_non_overlapping(&resolved)?;

    resolved.sort_by(|left, right| {
        right
            .sort_key()
            .cmp(&left.sort_key())
            .then_with(|| right.edit_index.cmp(&left.edit_index))
    });

    for edit in &resolved {
        match &edit.operation {
            ResolvedOperation::ReplaceRange {
                start_line,
                end_line,
                replacement_lines,
            } => {
                source_layout.replace_range(*start_line, *end_line, replacement_lines.clone());
            }
            ResolvedOperation::InsertAfter {
                anchor_line,
                insert_lines,
            } => {
                source_layout.insert_after(*anchor_line, insert_lines.clone());
            }
        }
    }

    Ok(HashlineApplyResult {
        content: source_layout.into_content(),
        operations_total: prepared_edits.len(),
        operations_applied: prepared_edits.len(),
    })
}

#[cfg(test)]
mod tests;
