use std::env;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use clap::Args;
use serde::{Deserialize, Serialize};

use crate::apply::{
    ApplyFailureInjection, ApplyFileResult, ApplyResponse, ApplySummary, ApplyTransaction,
    apply_multi_file_changeset, apply_multi_file_changeset_with_injection,
    dry_run_multi_file_changeset,
};
use crate::changeset::{FileChange, MultiFileChangeset, TransformTarget, hash_text};
use crate::error::IdenteditError;
use crate::hashline::{
    HashlineCheckError, HashlineCheckResult, HashlineMismatchStatus, check_hashline_refs,
    format_line_ref,
};

#[derive(Debug, Args)]
pub struct ApplyArgs {
    #[arg(long, help = "Read wrapped apply request JSON from stdin")]
    pub json: bool,
    #[arg(long, help = "Validate and preview without writing files")]
    pub dry_run: bool,
    #[arg(
        long,
        help = "Enable line-target anchor remap/repair for deterministic stale anchors"
    )]
    pub repair: bool,
    #[arg(long, help = "Include per-file apply results in output")]
    pub verbose: bool,
    #[arg(long = "inject-failure-after-writes", hide = true, value_name = "N")]
    pub inject_failure_after_writes: Option<usize>,
    #[arg(
        value_name = "PLAN",
        help = "Path to edit-plan JSON; if omitted, read raw plan JSON from stdin"
    )]
    pub input: Option<PathBuf>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StdinApplyRequest {
    command: String,
    changeset: MultiFileChangeset,
}

#[derive(Debug, Serialize)]
pub struct ApplyCliResponse {
    pub summary: ApplySummary,
    pub transaction: ApplyTransaction,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub applied: Option<Vec<ApplyFileResult>>,
}

pub fn run_apply(args: ApplyArgs) -> Result<ApplyCliResponse, IdenteditError> {
    let failure_injection = parse_failure_injection(args.inject_failure_after_writes)?;
    if args.dry_run && failure_injection.is_some() {
        return Err(IdenteditError::InvalidRequest {
            message: "--dry-run cannot be combined with --inject-failure-after-writes".to_string(),
        });
    }

    let mut changeset = if args.json {
        run_apply_json_mode()?
    } else if let Some(input_path) = args.input {
        read_changeset_from_file(&input_path)?
    } else {
        read_changeset_from_stdin()?
    };
    if args.repair {
        repair_line_targets_in_changeset(&mut changeset)?;
        refresh_line_previews_after_repair(&mut changeset)?;
    }

    let response =
        apply_changeset_with_optional_injection(&changeset, failure_injection, args.dry_run)?;

    Ok(shape_apply_response(response, args.verbose))
}

fn run_apply_json_mode() -> Result<MultiFileChangeset, IdenteditError> {
    let mut request_body = String::new();
    std::io::stdin()
        .read_to_string(&mut request_body)
        .map_err(|error| IdenteditError::StdinRead { source: error })?;

    let request: StdinApplyRequest = serde_json::from_str(&request_body)
        .map_err(|error| IdenteditError::InvalidJsonRequest { source: error })?;

    if request.command != "apply" {
        return Err(IdenteditError::InvalidRequest {
            message: format!(
                "Unsupported command '{}' in stdin JSON mode; expected 'apply'",
                request.command
            ),
        });
    }

    Ok(request.changeset)
}

fn apply_changeset_with_optional_injection(
    changeset: &MultiFileChangeset,
    failure_injection: Option<ApplyFailureInjection>,
    dry_run: bool,
) -> Result<ApplyResponse, IdenteditError> {
    if dry_run {
        return dry_run_multi_file_changeset(changeset);
    }

    if let Some(injection) = failure_injection {
        return apply_multi_file_changeset_with_injection(changeset, Some(injection));
    }

    apply_multi_file_changeset(changeset)
}

fn repair_line_targets_in_changeset(
    changeset: &mut MultiFileChangeset,
) -> Result<(), IdenteditError> {
    for file_change in &mut changeset.files {
        let mut target_refs = Vec::<(usize, bool, String)>::new();
        for (operation_index, operation) in file_change.operations.iter().enumerate() {
            if let TransformTarget::Line { anchor, end_anchor } = &operation.target {
                target_refs.push((operation_index, false, anchor.clone()));
                if let Some(end_anchor) = end_anchor {
                    target_refs.push((operation_index, true, end_anchor.clone()));
                }
            }
        }

        if target_refs.is_empty() {
            continue;
        }

        let source = fs::read_to_string(&file_change.file)
            .map_err(|error| IdenteditError::io(&file_change.file, error))?;
        let refs = target_refs
            .iter()
            .map(|(_, _, anchor)| anchor.clone())
            .collect::<Vec<_>>();
        let check = check_hashline_refs(&source, &refs).map_err(map_hashline_check_error)?;
        if check.ok {
            continue;
        }
        if !check.mismatches.iter().all(|mismatch| {
            mismatch.status == HashlineMismatchStatus::Remappable && mismatch.remaps.len() == 1
        }) {
            return Err(hashline_precondition_failed_error(check));
        }

        let remapped = check
            .mismatches
            .into_iter()
            .map(|mismatch| {
                let target = mismatch.remaps.first().expect("validated remap count");
                (
                    mismatch.edit_index,
                    format_line_ref(target.line, &target.hash),
                )
            })
            .collect::<std::collections::HashMap<usize, String>>();

        for (ref_index, (operation_index, is_end_anchor, _)) in target_refs.iter().enumerate() {
            let Some(new_anchor) = remapped.get(&ref_index) else {
                continue;
            };

            let operation = file_change
                .operations
                .get_mut(*operation_index)
                .ok_or_else(|| IdenteditError::InvalidRequest {
                    message: format!(
                        "Internal apply repair error: missing operation {} for file '{}'",
                        operation_index,
                        file_change.file.display()
                    ),
                })?;
            let TransformTarget::Line { anchor, end_anchor } = &mut operation.target else {
                return Err(IdenteditError::InvalidRequest {
                    message: format!(
                        "Internal apply repair error: expected line target at operation {}",
                        operation_index
                    ),
                });
            };

            if *is_end_anchor {
                if let Some(end_anchor) = end_anchor {
                    *end_anchor = new_anchor.clone();
                }
            } else {
                *anchor = new_anchor.clone();
            }
        }
    }

    Ok(())
}

fn refresh_line_previews_after_repair(
    changeset: &mut MultiFileChangeset,
) -> Result<(), IdenteditError> {
    for file_change in &mut changeset.files {
        refresh_line_operation_previews(file_change)?;
    }
    Ok(())
}

fn refresh_line_operation_previews(file_change: &mut FileChange) -> Result<(), IdenteditError> {
    let mut original_indices = Vec::new();
    let mut line_operations = Vec::new();
    for (index, operation) in file_change.operations.iter().enumerate() {
        if matches!(operation.target, TransformTarget::Line { .. }) {
            original_indices.push(index);
            line_operations.push(operation.clone());
        }
    }

    if line_operations.is_empty() {
        return Ok(());
    }

    let line_only_change = FileChange {
        file: file_change.file.clone(),
        operations: line_operations,
    };
    let resolved = crate::transform::resolve_changeset_targets(&line_only_change)?;

    for (resolved_index, matched_change) in resolved.into_iter().enumerate() {
        let original_index = original_indices
            .get(resolved_index)
            .copied()
            .ok_or_else(|| IdenteditError::InvalidRequest {
                message: format!(
                    "Internal apply repair error: line operation index {} is out of range",
                    resolved_index
                ),
            })?;
        let operation = file_change
            .operations
            .get_mut(original_index)
            .ok_or_else(|| IdenteditError::InvalidRequest {
                message: format!(
                    "Internal apply repair error: missing operation {} while refreshing previews",
                    original_index
                ),
            })?;

        operation.preview.matched_span = matched_change.matched_span;
        if operation.preview.old_text.is_some() {
            operation.preview.old_text = Some(matched_change.old_text);
            operation.preview.old_hash = None;
            operation.preview.old_len = None;
        } else {
            operation.preview.old_hash = Some(hash_text(&matched_change.old_text));
            operation.preview.old_len = Some(matched_change.old_text.len());
            operation.preview.old_text = None;
        }
    }

    Ok(())
}

fn map_hashline_check_error(error: HashlineCheckError) -> IdenteditError {
    IdenteditError::InvalidRequest {
        message: error.to_string(),
    }
}

fn hashline_precondition_failed_error(check: HashlineCheckResult) -> IdenteditError {
    let serialized_check =
        serde_json::to_string_pretty(&check).unwrap_or_else(|_| "{\"ok\":false}".to_string());
    IdenteditError::InvalidRequest {
        message: format!(
            "Hashline preconditions failed during apply --repair.\n{serialized_check}"
        ),
    }
}

pub(crate) fn shape_apply_response(response: ApplyResponse, verbose: bool) -> ApplyCliResponse {
    let ApplyResponse {
        applied,
        summary,
        transaction,
    } = response;

    ApplyCliResponse {
        summary,
        transaction,
        applied: verbose.then_some(applied),
    }
}

fn parse_failure_injection(
    inject_failure_after_writes: Option<usize>,
) -> Result<Option<ApplyFailureInjection>, IdenteditError> {
    let Some(after_writes) = inject_failure_after_writes else {
        return Ok(None);
    };

    if after_writes == 0 {
        return Err(IdenteditError::InvalidRequest {
            message: "--inject-failure-after-writes must be greater than 0".to_string(),
        });
    }

    if env::var("IDENTEDIT_EXPERIMENTAL").ok().as_deref() != Some("1") {
        return Err(IdenteditError::InvalidRequest {
            message:
                "The hidden --inject-failure-after-writes flag requires IDENTEDIT_EXPERIMENTAL=1"
                    .to_string(),
        });
    }

    Ok(Some(ApplyFailureInjection { after_writes }))
}

fn read_changeset_from_file(path: &Path) -> Result<MultiFileChangeset, IdenteditError> {
    let content = fs::read_to_string(path).map_err(|error| IdenteditError::io(path, error))?;
    serde_json::from_str(&content)
        .map_err(|error| IdenteditError::InvalidJsonRequest { source: error })
}

fn read_changeset_from_stdin() -> Result<MultiFileChangeset, IdenteditError> {
    let mut request_body = String::new();
    std::io::stdin()
        .read_to_string(&mut request_body)
        .map_err(|error| IdenteditError::StdinRead { source: error })?;

    serde_json::from_str(&request_body)
        .map_err(|error| IdenteditError::InvalidJsonRequest { source: error })
}
