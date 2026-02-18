use serde::{Deserialize, Serialize};
use thiserror::Error;

mod apply;
mod check;
mod repair;
mod show;

pub const HASHLINE_PUBLIC_HEX_LEN: usize = 12;
pub const HASHLINE_MIN_HEX_LEN: usize = HASHLINE_PUBLIC_HEX_LEN;
pub const HASHLINE_MAX_HEX_LEN: usize = HASHLINE_PUBLIC_HEX_LEN;
pub const HASHLINE_DEFAULT_HEX_LEN: usize = HASHLINE_PUBLIC_HEX_LEN;
const HASHLINE_DISPLAY_MIN_HEX_LEN: usize = 8;
const HASHLINE_DISPLAY_MAX_HEX_LEN: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HashedLine {
    pub line: usize,
    pub hash: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineRef {
    pub line: usize,
    pub hash: String,
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
    pub hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HashlineMismatch {
    pub edit_index: usize,
    pub anchor: String,
    pub line: usize,
    pub expected_hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual_hash: Option<String>,
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
    pub anchor: String,
    pub new_text: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplaceLinesEdit {
    pub start_anchor: String,
    #[serde(default)]
    pub end_anchor: Option<String>,
    pub new_text: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InsertAfterEdit {
    pub anchor: String,
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
    anchor: String,
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

pub fn compute_line_hash(line: &str) -> String {
    let full_hex = compute_line_hash_full(line);
    full_hex[..HASHLINE_DEFAULT_HEX_LEN].to_string()
}

pub fn format_line_ref(line: usize, hash: &str) -> String {
    format!("{line}:{hash}")
}

pub fn parse_line_ref(value: &str) -> Result<LineRef, HashlineCheckError> {
    let raw = value.trim();
    let without_display_suffix = raw.split_once('|').map_or(raw, |(prefix, _)| prefix).trim();
    let (line_raw, hash_raw) = without_display_suffix.split_once(':').ok_or_else(|| {
        HashlineCheckError::InvalidRequest {
            message: format!(
                "Invalid hashline anchor '{}': expected format '<line>:<hex-hash>'",
                value
            ),
        }
    })?;

    let line =
        line_raw
            .trim()
            .parse::<usize>()
            .map_err(|_| HashlineCheckError::InvalidRequest {
                message: format!(
                    "Invalid hashline anchor '{}': line number must be a positive integer",
                    value
                ),
            })?;

    if line == 0 {
        return Err(HashlineCheckError::InvalidRequest {
            message: format!(
                "Invalid hashline anchor '{}': line number must be >= 1",
                value
            ),
        });
    }

    let normalized_hash = hash_raw.trim().to_ascii_lowercase();
    validate_hash_segment(value, &normalized_hash)?;

    Ok(LineRef {
        line,
        hash: normalized_hash,
    })
}

pub fn show_hashed_lines(source: &str) -> Vec<HashedLine> {
    show::show_hashed_lines(source)
}

pub fn format_hashed_lines(source: &str) -> String {
    show::format_hashed_lines(source)
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

pub fn check_hashline_refs(
    source: &str,
    refs: &[String],
) -> Result<HashlineCheckResult, HashlineCheckError> {
    let anchors = refs
        .iter()
        .enumerate()
        .map(|(edit_index, anchor)| AnchorCheckRequest {
            edit_index,
            anchor: anchor.clone(),
        })
        .collect::<Vec<_>>();
    check::check_hashline_anchors(source, &anchors)
}

pub fn apply_hashline_edits(
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

    let source_layout = show::split_source_lines(source);
    let show::SourceLayout {
        mut lines,
        had_trailing_newline,
        newline,
    } = source_layout;
    let mut resolved = apply::resolve_edits(&lines, &prepared_edits)?;
    if mode == HashlineApplyMode::Repair {
        repair::apply_repair_merge_expansion(&lines, &mut resolved);
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
                let start_index = start_line - 1;
                let end_index = *end_line;
                lines.splice(start_index..end_index, replacement_lines.clone());
            }
            ResolvedOperation::InsertAfter {
                anchor_line,
                insert_lines,
            } => {
                let insert_index = *anchor_line;
                lines.splice(insert_index..insert_index, insert_lines.clone());
            }
        }
    }

    Ok(HashlineApplyResult {
        content: show::join_source_lines(&lines, had_trailing_newline, newline),
        operations_total: prepared_edits.len(),
        operations_applied: prepared_edits.len(),
    })
}

fn compute_line_hash_full(line: &str) -> String {
    blake3::hash(line.as_bytes()).to_hex().to_string()
}

fn validate_hash_segment(anchor: &str, hash: &str) -> Result<(), HashlineCheckError> {
    if HASHLINE_MIN_HEX_LEN == HASHLINE_MAX_HEX_LEN {
        if hash.len() != HASHLINE_MIN_HEX_LEN {
            return Err(HashlineCheckError::InvalidRequest {
                message: format!(
                    "Invalid hashline anchor '{}': hash must be exactly {} hex chars",
                    anchor, HASHLINE_MIN_HEX_LEN
                ),
            });
        }
    } else {
        if hash.len() < HASHLINE_MIN_HEX_LEN {
            return Err(HashlineCheckError::InvalidRequest {
                message: format!(
                    "Invalid hashline anchor '{}': hash must be at least {} hex chars",
                    anchor, HASHLINE_MIN_HEX_LEN
                ),
            });
        }

        if hash.len() > HASHLINE_MAX_HEX_LEN {
            return Err(HashlineCheckError::InvalidRequest {
                message: format!(
                    "Invalid hashline anchor '{}': hash must be at most {} hex chars",
                    anchor, HASHLINE_MAX_HEX_LEN
                ),
            });
        }
    }

    if !hash.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(HashlineCheckError::InvalidRequest {
            message: format!(
                "Invalid hashline anchor '{}': hash must contain only hex characters",
                anchor
            ),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests;
