use serde::Serialize;

use crate::changeset::{FileChange, MultiFileChangeset, OpKind, TransactionMode};
use crate::error::IdenteditError;
use crate::execution_context::ExecutionContext;

mod io;
mod move_ops;
mod preflight;
mod replacements;

use move_ops::{
    commit_move_plans, preflight_move_plans, rollback_committed_moves,
    validate_move_operation_constraints,
};
use preflight::{
    commit_preflight_batch, preflight_changesets_in_order, prepare_commit_batch,
    rollback_committed_files,
};

#[cfg(test)]
use io::{
    ApplyGuardState, AtomicWritePhase, acquire_apply_lock, capture_path_fingerprint,
    verify_apply_guard_state, write_text_atomically_with_hook,
    write_text_atomically_with_hook_and_rename,
};
#[cfg(test)]
use move_ops::commit_move_plan_with_rename;
#[cfg(test)]
use preflight::FileRollbackSnapshot;
#[cfg(test)]
use replacements::{ResolvedReplacement, apply_replacements_to_text, ensure_non_overlapping};

#[derive(Debug, Clone, Serialize)]
pub struct ApplyResponse {
    pub applied: Vec<ApplyFileResult>,
    pub summary: ApplySummary,
    pub transaction: ApplyTransaction,
}

#[derive(Debug, Clone, Serialize)]
pub struct ApplyFileResult {
    pub file: String,
    pub operations_applied: usize,
    pub operations_total: usize,
    pub status: ApplyFileStatus,
}

#[derive(Debug, Clone, Serialize)]
pub struct ApplySummary {
    pub files_modified: usize,
    pub operations_applied: usize,
    pub operations_failed: usize,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ApplyFileStatus {
    Applied,
    RolledBack,
    RollbackFailed,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TransactionStatus {
    Committed,
    DryRun,
    RolledBack,
    RollbackFailed,
}

#[derive(Debug, Clone, Serialize)]
pub struct ApplyTransaction {
    pub mode: TransactionMode,
    pub status: TransactionStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ApplyFailureInjection {
    pub(crate) after_writes: usize,
}

fn summarize_apply_results(applied: &[ApplyFileResult]) -> ApplySummary {
    let files_modified = applied
        .iter()
        .filter(|result| result.operations_applied > 0)
        .count();
    let operations_applied = applied.iter().map(|result| result.operations_applied).sum();
    let operations_failed = applied
        .iter()
        .map(|result| {
            result
                .operations_total
                .saturating_sub(result.operations_applied)
        })
        .sum();

    ApplySummary {
        files_modified,
        operations_applied,
        operations_failed,
    }
}

pub fn apply_changeset(changeset: &FileChange) -> Result<ApplyResponse, IdenteditError> {
    apply_changeset_with_hooks(changeset, || Ok(()), || Ok(()))
}

pub fn apply_multi_file_changeset(
    changeset: &MultiFileChangeset,
) -> Result<ApplyResponse, IdenteditError> {
    apply_multi_file_changeset_with_injection(changeset, None)
}

pub fn dry_run_multi_file_changeset(
    changeset: &MultiFileChangeset,
) -> Result<ApplyResponse, IdenteditError> {
    if changeset.files.is_empty() {
        return Err(IdenteditError::InvalidRequest {
            message: "changeset.files must contain at least one file".to_string(),
        });
    }

    let move_execution_order = validate_move_operation_constraints(&changeset.files)?;
    let edit_changesets = changeset
        .files
        .iter()
        .filter(|changeset| !changeset_has_move(changeset))
        .cloned()
        .collect::<Vec<_>>();

    let context = ExecutionContext::new();
    let preflight_plans = preflight_changesets_in_order(&edit_changesets, context.registry())?;
    let move_plans = preflight_move_plans(&move_execution_order)?;

    let mut applied = Vec::with_capacity(preflight_plans.len() + move_plans.len());
    for plan in preflight_plans {
        applied.push(ApplyFileResult {
            file: plan.file.display().to_string(),
            operations_applied: plan.operations_total,
            operations_total: plan.operations_total,
            status: ApplyFileStatus::Applied,
        });
    }
    for plan in move_plans {
        applied.push(ApplyFileResult {
            file: plan.source.display().to_string(),
            operations_applied: plan.operations_total,
            operations_total: plan.operations_total,
            status: ApplyFileStatus::Applied,
        });
    }

    let summary = summarize_apply_results(&applied);
    let transaction = ApplyTransaction {
        mode: TransactionMode::AllOrNothing,
        status: TransactionStatus::DryRun,
    };

    Ok(ApplyResponse {
        applied,
        summary,
        transaction,
    })
}

pub(crate) fn apply_multi_file_changeset_with_injection(
    changeset: &MultiFileChangeset,
    failure_injection: Option<ApplyFailureInjection>,
) -> Result<ApplyResponse, IdenteditError> {
    if changeset.files.is_empty() {
        return Err(IdenteditError::InvalidRequest {
            message: "changeset.files must contain at least one file".to_string(),
        });
    }

    let mut committed_writes = 0usize;
    apply_changesets_with_hooks(
        &changeset.files,
        || Ok(()),
        || {
            if let Some(injection) = failure_injection {
                if committed_writes == injection.after_writes {
                    return Err(IdenteditError::InvalidRequest {
                        message: format!(
                            "Injected apply failure for rollback rehearsal after {} committed writes (blocked write #{})",
                            injection.after_writes,
                            injection.after_writes + 1
                        ),
                    });
                }
                committed_writes += 1;
            }
            Ok(())
        },
    )
}

#[cfg(test)]
fn apply_changeset_with_hook<F>(
    changeset: &FileChange,
    mut before_write_hook: F,
) -> Result<ApplyResponse, IdenteditError>
where
    F: FnMut() -> Result<(), IdenteditError>,
{
    apply_changeset_with_hooks(changeset, &mut before_write_hook, || Ok(()))
}

fn apply_changeset_with_hooks<Before, After>(
    changeset: &FileChange,
    before_write_hook: Before,
    after_verify_hook: After,
) -> Result<ApplyResponse, IdenteditError>
where
    Before: FnMut() -> Result<(), IdenteditError>,
    After: FnMut() -> Result<(), IdenteditError>,
{
    apply_changesets_with_hooks(
        std::slice::from_ref(changeset),
        before_write_hook,
        after_verify_hook,
    )
}

fn apply_changesets_with_hooks<Before, After>(
    changesets: &[FileChange],
    mut before_write_hook: Before,
    mut after_verify_hook: After,
) -> Result<ApplyResponse, IdenteditError>
where
    Before: FnMut() -> Result<(), IdenteditError>,
    After: FnMut() -> Result<(), IdenteditError>,
{
    let move_execution_order = validate_move_operation_constraints(changesets)?;
    let edit_changesets = changesets
        .iter()
        .filter(|changeset| !changeset_has_move(changeset))
        .cloned()
        .collect::<Vec<_>>();

    let context = ExecutionContext::new();
    let preflight_plans = preflight_changesets_in_order(&edit_changesets, context.registry())?;
    let commit_batch = prepare_commit_batch(preflight_plans);
    let edit_rollback_snapshots = commit_batch.rollback_snapshots.clone();
    let move_plans = preflight_move_plans(&move_execution_order)?;

    let mut applied = if commit_batch.preflight_plans.is_empty() {
        if !move_plans.is_empty() {
            before_write_hook()?;
        }
        Vec::new()
    } else {
        commit_preflight_batch(commit_batch, &mut before_write_hook, &mut after_verify_hook)?
    };

    if !move_plans.is_empty() {
        match commit_move_plans(&move_plans, &mut after_verify_hook) {
            Ok(mut move_applied) => applied.append(&mut move_applied),
            Err((commit_error, committed_move_indices)) => {
                let move_rollback_error =
                    rollback_committed_moves(&move_plans, &committed_move_indices).err();
                let committed_edit_indices = (0..edit_rollback_snapshots.len()).collect::<Vec<_>>();
                let edit_rollback_error =
                    rollback_committed_files(&edit_rollback_snapshots, &committed_edit_indices)
                        .err();

                if move_rollback_error.is_none() && edit_rollback_error.is_none() {
                    return Err(commit_error);
                }

                let mut rollback_details = vec![format!("Commit failed ({commit_error})")];
                if let Some(move_error) = move_rollback_error {
                    rollback_details.push(format!("move rollback failed ({move_error})"));
                }
                if let Some(edit_error) = edit_rollback_error {
                    rollback_details.push(format!("content rollback failed ({edit_error})"));
                }
                return Err(IdenteditError::RollbackFailed {
                    message: rollback_details.join("; "),
                });
            }
        }
    }

    let summary = summarize_apply_results(&applied);
    let transaction = ApplyTransaction {
        mode: TransactionMode::AllOrNothing,
        status: TransactionStatus::Committed,
    };

    Ok(ApplyResponse {
        applied,
        summary,
        transaction,
    })
}

fn changeset_has_move(changeset: &FileChange) -> bool {
    changeset
        .operations
        .iter()
        .any(|operation| matches!(operation.op, OpKind::Move { .. }))
}

#[cfg(test)]
mod tests;
