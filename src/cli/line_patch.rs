use std::fs;
use std::path::PathBuf;

use serde::Serialize;

use crate::error::IdenteditError;
use crate::hashline::{
    HashlineApplyError, HashlineApplyMode, HashlineCheckError, HashlineCheckResult, HashlineCheckSummary,
    HashlineEdit, HashlineMismatch, HashlineMismatchStatus, apply_hashline_edits_with_mode,
    check_hashline_edits,
};
use crate::patch::engine::run_resolve_verify_apply;

#[derive(Debug, Serialize)]
pub struct HashlineCheckPayload {
    pub ok: bool,
    pub summary: HashlineCheckSummary,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mismatches: Option<Vec<HashlineMismatch>>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HashlineModeResponse {
    Strict,
    Repair,
}

#[derive(Debug, Serialize)]
pub struct HashlinePatchResponse {
    pub file: PathBuf,
    pub auto_repair: bool,
    pub strict_check: HashlineCheckPayload,
    pub applied_mode: HashlineModeResponse,
    pub changed: bool,
    pub operations_total: usize,
    pub operations_applied: usize,
}

pub(crate) fn execute_hashline_patch(
    file: PathBuf,
    edits: Vec<HashlineEdit>,
    auto_repair: bool,
) -> Result<HashlinePatchResponse, IdenteditError> {
    run_resolve_verify_apply(
        || resolve_hashline_patch_request(file, edits, auto_repair),
        verify_hashline_patch_request,
        apply_hashline_patch_request,
    )
}

#[derive(Debug)]
struct ResolvedHashlinePatch {
    file: PathBuf,
    source: String,
    edits: Vec<HashlineEdit>,
    auto_repair: bool,
}

#[derive(Debug)]
struct VerifiedHashlinePatch {
    file: PathBuf,
    source: String,
    edits: Vec<HashlineEdit>,
    auto_repair: bool,
    strict_check_result: HashlineCheckResult,
    applied_mode: HashlineApplyMode,
}

fn resolve_hashline_patch_request(
    file: PathBuf,
    edits: Vec<HashlineEdit>,
    auto_repair: bool,
) -> Result<ResolvedHashlinePatch, IdenteditError> {
    let source = fs::read_to_string(&file).map_err(|error| IdenteditError::io(&file, error))?;
    Ok(ResolvedHashlinePatch {
        file,
        source,
        edits,
        auto_repair,
    })
}

fn verify_hashline_patch_request(
    resolved: ResolvedHashlinePatch,
) -> Result<VerifiedHashlinePatch, IdenteditError> {
    let strict_check_result = check_hashline_edits(&resolved.source, &resolved.edits)
        .map_err(map_hashline_check_error)?;

    let applied_mode = if strict_check_result.ok {
        HashlineApplyMode::Strict
    } else if resolved.auto_repair && can_retry_with_repair(&strict_check_result) {
        HashlineApplyMode::Repair
    } else {
        return Err(hashline_precondition_failed_error(strict_check_result));
    };

    Ok(VerifiedHashlinePatch {
        file: resolved.file,
        source: resolved.source,
        edits: resolved.edits,
        auto_repair: resolved.auto_repair,
        strict_check_result,
        applied_mode,
    })
}

fn apply_hashline_patch_request(
    verified: VerifiedHashlinePatch,
) -> Result<HashlinePatchResponse, IdenteditError> {
    let strict_check = build_hashline_check_payload(
        verified.strict_check_result.clone(),
        verified.auto_repair || !verified.strict_check_result.ok,
    );
    let applied =
        apply_hashline_edits_with_mode(&verified.source, &verified.edits, verified.applied_mode)
            .map_err(map_hashline_apply_error)?;
    let changed = verified.source != applied.content;

    if changed {
        fs::write(&verified.file, applied.content.as_bytes())
            .map_err(|error| IdenteditError::io(&verified.file, error))?;
    }

    Ok(HashlinePatchResponse {
        file: verified.file,
        auto_repair: verified.auto_repair,
        strict_check,
        applied_mode: match verified.applied_mode {
            HashlineApplyMode::Strict => HashlineModeResponse::Strict,
            HashlineApplyMode::Repair => HashlineModeResponse::Repair,
        },
        changed,
        operations_total: applied.operations_total,
        operations_applied: applied.operations_applied,
    })
}

fn build_hashline_check_payload(check: HashlineCheckResult, verbose: bool) -> HashlineCheckPayload {
    let HashlineCheckResult {
        ok,
        summary,
        mismatches,
    } = check;
    let include_mismatches = verbose || !ok;
    HashlineCheckPayload {
        ok,
        summary,
        mismatches: include_mismatches.then_some(mismatches),
    }
}

fn map_hashline_check_error(error: HashlineCheckError) -> IdenteditError {
    IdenteditError::InvalidRequest {
        message: error.to_string(),
    }
}

fn map_hashline_apply_error(error: HashlineApplyError) -> IdenteditError {
    match error {
        HashlineApplyError::Check(check_error) => map_hashline_check_error(check_error),
        HashlineApplyError::Overlap { .. } => IdenteditError::InvalidRequest {
            message: error.to_string(),
        },
        HashlineApplyError::PreconditionFailed { check } => {
            hashline_precondition_failed_error(check)
        }
    }
}

fn hashline_precondition_failed_error(check: HashlineCheckResult) -> IdenteditError {
    let diagnostic_check = canonicalize_check_for_diagnostics(check);
    let serialized_check = serde_json::to_string_pretty(&diagnostic_check)
        .unwrap_or_else(|_| "{\"ok\":false}".to_string());
    IdenteditError::InvalidRequest {
        message: format!(
            "Hashline preconditions failed; refresh anchors with 'identedit read --mode line --json <file>' and retry.\n{serialized_check}"
        ),
    }
}

fn can_retry_with_repair(check: &HashlineCheckResult) -> bool {
    check.summary.remappable > 0
        && check.summary.ambiguous == 0
        && check.summary.mismatched == check.summary.remappable
}

fn canonicalize_check_for_diagnostics(mut check: HashlineCheckResult) -> HashlineCheckResult {
    for mismatch in &mut check.mismatches {
        mismatch.remaps.sort_by(|left, right| {
            left.line
                .cmp(&right.line)
                .then_with(|| left.hash.cmp(&right.hash))
        });
    }

    check.mismatches.sort_by(|left, right| {
        left.edit_index
            .cmp(&right.edit_index)
            .then_with(|| left.line.cmp(&right.line))
            .then_with(|| {
                mismatch_status_rank(&left.status).cmp(&mismatch_status_rank(&right.status))
            })
            .then_with(|| left.anchor.cmp(&right.anchor))
            .then_with(|| left.expected_hash.cmp(&right.expected_hash))
            .then_with(|| left.actual_hash.cmp(&right.actual_hash))
            .then_with(|| left.remaps.len().cmp(&right.remaps.len()))
    });

    check
}

fn mismatch_status_rank(status: &HashlineMismatchStatus) -> u8 {
    match status {
        HashlineMismatchStatus::Mismatch => 0,
        HashlineMismatchStatus::Remappable => 1,
        HashlineMismatchStatus::Ambiguous => 2,
    }
}
