use serde::Serialize;
use thiserror::Error;

use crate::changeset::TransformTarget;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParsedFailedDiff {
    pub(crate) header_file: Option<String>,
    pub(crate) source_hunk_count: usize,
    pub(crate) changes: Vec<FailedDiffChange>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FailedDiffChange {
    pub(crate) source_hunk_index: usize,
    pub(crate) block_index: usize,
    pub(crate) old_lines: Vec<String>,
    pub(crate) new_lines: Vec<String>,
    pub(crate) before_context: Vec<String>,
    pub(crate) after_context: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("Invalid failed diff: {message}")]
pub(crate) struct FailedDiffError {
    pub(crate) message: String,
}

impl FailedDiffError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct FailedDiffAnalysis {
    pub(crate) changes: Vec<FailedDiffMatchSet>,
    pub(crate) summary: FailedDiffSummary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct FailedDiffMatchSet {
    pub(crate) change_index: usize,
    pub(crate) source_hunk_index: usize,
    pub(crate) block_index: usize,
    pub(crate) status: FailedDiffStatus,
    pub(crate) old_line_count: usize,
    pub(crate) new_line_count: usize,
    pub(crate) candidates: Vec<FailedDiffCandidate>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FailedDiffStatus {
    Unique,
    Ambiguous,
    Missing,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct FailedDiffCandidate {
    pub(crate) candidate_index: usize,
    pub(crate) target: TransformTarget,
    pub(crate) op: FailedDiffOperation,
    pub(crate) preview: FailedDiffCandidatePreview,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum FailedDiffOperation {
    ReplaceLines { new_text: String },
    InsertAfter { text: String },
    Insert { new_text: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct FailedDiffCandidatePreview {
    pub(crate) before: Vec<FailedDiffPreviewLine>,
    pub(crate) matched: Vec<FailedDiffPreviewLine>,
    pub(crate) matched_lines_omitted: usize,
    pub(crate) after: Vec<FailedDiffPreviewLine>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct FailedDiffPreviewLine {
    pub(crate) line: usize,
    pub(crate) content: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub(crate) struct FailedDiffSummary {
    pub(crate) source_hunks: usize,
    pub(crate) changes: usize,
    pub(crate) unique: usize,
    pub(crate) ambiguous: usize,
    pub(crate) missing: usize,
    pub(crate) candidates: usize,
}
