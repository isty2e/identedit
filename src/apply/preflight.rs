use std::collections::HashMap;
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use crate::changeset::{FileChange, hash_bytes};
use crate::error::IdenteditError;
use crate::execution_context::ExecutionContext;
use crate::provider::ProviderRegistry;
use crate::transform::build::resolve_changeset_targets_in_handles;
use crate::transform::conflict::validate_change_conflicts;
use crate::transform::parse::parse_handles_for_source_with_registry;

use super::io::{
    ApplyFileLock, ApplyGuardState, AtomicWriteFailure, acquire_apply_lock,
    capture_apply_guard_state, verify_apply_guard_state, write_text_atomically,
    write_text_atomically_tracked,
};
#[cfg(test)]
use super::io::{AtomicWritePhase, write_text_atomically_tracked_with_hook};
use super::replacements::{
    apply_replacements_to_text, matched_changes_to_replacements, validate_preview_consistency,
};
use super::{ApplyFileResult, ApplyFileStatus};

#[derive(Debug)]
pub(super) struct PreflightFilePlan {
    pub(super) file: PathBuf,
    pub(super) operations_total: usize,
    original_text: String,
    original_permissions: std::fs::Permissions,
    pub(super) updated_text: String,
    guard_state: ApplyGuardState,
    _lock_guard: ApplyFileLock,
}

pub(super) fn preflight_changesets_in_order(
    changesets: &[FileChange],
    registry: &ProviderRegistry,
) -> Result<Vec<PreflightFilePlan>, IdenteditError> {
    let ordered_changesets = order_changesets_for_preflight(changesets)?;
    let context = ExecutionContext::new();
    let mut plans = Vec::with_capacity(changesets.len());
    for changeset in ordered_changesets {
        let plan = preflight_changeset(changeset, registry, &context)?;
        plans.push(plan);
    }

    Ok(plans)
}

pub(super) fn validate_unique_changeset_files(
    changesets: &[FileChange],
) -> Result<(), IdenteditError> {
    order_changesets_for_preflight(changesets).map(|_| ())
}

pub(super) fn preflight_resolved_text_update(
    file: &Path,
    expected_source: &str,
    updated_text: String,
    operations_total: usize,
) -> Result<PreflightFilePlan, IdenteditError> {
    let lock_guard = acquire_apply_lock(file)?;
    let guard_state = capture_apply_guard_state(file)?;
    let source_text = fs::read_to_string(file).map_err(|error| IdenteditError::io(file, error))?;
    if source_text != expected_source {
        return Err(IdenteditError::PreconditionFailed {
            expected_hash: hash_bytes(expected_source.as_bytes()),
            actual_hash: hash_bytes(source_text.as_bytes()),
        });
    }
    let original_permissions = fs::metadata(file)
        .map_err(|error| IdenteditError::io(file, error))?
        .permissions();

    Ok(PreflightFilePlan {
        file: file.to_path_buf(),
        operations_total,
        original_text: source_text,
        original_permissions,
        updated_text,
        guard_state,
        _lock_guard: lock_guard,
    })
}

#[derive(Debug)]
struct OrderedChangeset<'a> {
    canonical_path: PathBuf,
    duplicate_key: ChangesetDuplicateKey,
    sort_key: String,
    original_index: usize,
    changeset: &'a FileChange,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum ChangesetDuplicateKey {
    #[cfg(unix)]
    UnixInode { device: u64, inode: u64 },
    #[cfg(not(unix))]
    CanonicalPath(PathBuf),
}

fn duplicate_key_for_canonical_path(path: &Path) -> Result<ChangesetDuplicateKey, IdenteditError> {
    #[cfg(unix)]
    {
        let metadata = fs::metadata(path).map_err(|error| IdenteditError::io(path, error))?;
        Ok(ChangesetDuplicateKey::UnixInode {
            device: metadata.dev(),
            inode: metadata.ino(),
        })
    }

    #[cfg(not(unix))]
    {
        Ok(ChangesetDuplicateKey::CanonicalPath(path.to_path_buf()))
    }
}

fn order_changesets_for_preflight(
    changesets: &[FileChange],
) -> Result<Vec<&FileChange>, IdenteditError> {
    let mut ordered = Vec::with_capacity(changesets.len());
    for (index, changeset) in changesets.iter().enumerate() {
        let canonical_path = std::fs::canonicalize(&changeset.file)
            .map_err(|error| IdenteditError::io(&changeset.file, error))?;
        let duplicate_key = duplicate_key_for_canonical_path(&canonical_path)?;
        let sort_key = canonical_path.to_string_lossy().into_owned();
        ordered.push(OrderedChangeset {
            canonical_path,
            duplicate_key,
            sort_key,
            original_index: index,
            changeset,
        });
    }

    ordered.sort_by(|left, right| {
        left.sort_key
            .cmp(&right.sort_key)
            .then(left.original_index.cmp(&right.original_index))
    });

    let mut seen_duplicate_keys = HashMap::with_capacity(ordered.len());
    for entry in &ordered {
        if let Some(first_seen_path) =
            seen_duplicate_keys.insert(entry.duplicate_key.clone(), entry.canonical_path.clone())
        {
            return Err(IdenteditError::InvalidRequest {
                message: format!(
                    "Duplicate file entry in changeset.files is not supported: '{}' appears more than once",
                    first_seen_path.display()
                ),
            });
        }
    }

    Ok(ordered.into_iter().map(|entry| entry.changeset).collect())
}

fn preflight_changeset(
    changeset: &FileChange,
    registry: &ProviderRegistry,
    context: &ExecutionContext,
) -> Result<PreflightFilePlan, IdenteditError> {
    let lock_guard = acquire_apply_lock(&changeset.file)?;
    let guard_state = capture_apply_guard_state(&changeset.file)?;
    let source_text = context.read_file_utf8(&changeset.file)?;
    let original_permissions = fs::metadata(&changeset.file)
        .map_err(|error| IdenteditError::io(&changeset.file, error))?
        .permissions();
    let requires_structure_parse = changeset.operations.is_empty()
        || changeset
            .operations
            .iter()
            .any(|operation| operation.target.requires_node_resolution());
    let handles = if requires_structure_parse {
        parse_handles_for_source_with_registry(&changeset.file, source_text.as_bytes(), registry)?
    } else {
        Vec::new()
    };
    let matched_changes = resolve_changeset_targets_in_handles(changeset, &source_text, &handles)?;
    validate_change_conflicts(&matched_changes)?;
    validate_preview_consistency(changeset, &matched_changes)?;
    let replacements = matched_changes_to_replacements(matched_changes)?;
    let original_text = source_text.clone();
    let updated_text = apply_replacements_to_text(&changeset.file, source_text, replacements)?;

    Ok(PreflightFilePlan {
        file: changeset.file.clone(),
        operations_total: changeset.operations.len(),
        original_text,
        original_permissions,
        updated_text,
        guard_state,
        _lock_guard: lock_guard,
    })
}

#[derive(Debug, Clone)]
pub(super) struct FileRollbackSnapshot {
    pub(super) file: PathBuf,
    pub(super) original_text: String,
    pub(super) original_permissions: std::fs::Permissions,
}

#[derive(Debug)]
pub(super) struct CommitBatch {
    pub(super) preflight_plans: Vec<PreflightFilePlan>,
    pub(super) rollback_snapshots: Vec<FileRollbackSnapshot>,
}

#[derive(Debug)]
pub(super) struct CommittedEditBatch {
    pub(super) applied: Vec<ApplyFileResult>,
    pub(super) rollback_snapshots: Vec<FileRollbackSnapshot>,
    pub(super) committed_files: Vec<CommittedFileRollback>,
}

#[derive(Debug, Clone)]
pub(super) struct CommittedFileRollback {
    pub(super) snapshot_index: usize,
    pub(super) expected_guard: ApplyGuardState,
}

pub(super) fn prepare_commit_batch(preflight_plans: Vec<PreflightFilePlan>) -> CommitBatch {
    let rollback_snapshots = preflight_plans
        .iter()
        .map(|plan| FileRollbackSnapshot {
            file: plan.file.clone(),
            original_text: plan.original_text.clone(),
            original_permissions: plan.original_permissions.clone(),
        })
        .collect();

    CommitBatch {
        preflight_plans,
        rollback_snapshots,
    }
}

fn validate_commit_batch_invariants(batch: &CommitBatch) -> Result<(), IdenteditError> {
    if batch.preflight_plans.len() != batch.rollback_snapshots.len() {
        return Err(IdenteditError::InvalidRequest {
            message: "Internal commit planning error: rollback snapshot count mismatch".to_string(),
        });
    }

    for (index, (plan, snapshot)) in batch
        .preflight_plans
        .iter()
        .zip(batch.rollback_snapshots.iter())
        .enumerate()
    {
        if snapshot.file != plan.file {
            return Err(IdenteditError::InvalidRequest {
                message: format!(
                    "Internal commit planning error at index {index}: snapshot file '{}' does not match plan file '{}'",
                    snapshot.file.display(),
                    plan.file.display()
                ),
            });
        }

        let snapshot_hash = hash_bytes(snapshot.original_text.as_bytes());
        if snapshot_hash != plan.guard_state.source_hash {
            return Err(IdenteditError::InvalidRequest {
                message: format!(
                    "Internal commit planning error at index {index}: snapshot hash does not match preflight source hash for '{}'",
                    plan.file.display()
                ),
            });
        }

        let _snapshot_readonly = snapshot.original_permissions.readonly();
    }

    Ok(())
}

pub(super) fn commit_preflight_batch<Before, After>(
    batch: CommitBatch,
    before_write_hook: Before,
    after_verify_hook: After,
) -> Result<CommittedEditBatch, IdenteditError>
where
    Before: FnMut() -> Result<(), IdenteditError>,
    After: FnMut() -> Result<(), IdenteditError>,
{
    commit_preflight_batch_with_writer(batch, before_write_hook, after_verify_hook, |plan| {
        write_text_atomically_tracked(&plan.file, &plan.updated_text, Some(&plan.guard_state))
    })
}

#[cfg(test)]
pub(super) fn commit_preflight_batch_with_write_hook<Before, After, Phase>(
    batch: CommitBatch,
    before_write_hook: Before,
    after_verify_hook: After,
    mut phase_hook: Phase,
) -> Result<CommittedEditBatch, IdenteditError>
where
    Before: FnMut() -> Result<(), IdenteditError>,
    After: FnMut() -> Result<(), IdenteditError>,
    Phase: FnMut(AtomicWritePhase) -> std::io::Result<()>,
{
    commit_preflight_batch_with_writer(batch, before_write_hook, after_verify_hook, |plan| {
        write_text_atomically_tracked_with_hook(
            &plan.file,
            &plan.updated_text,
            Some(&plan.guard_state),
            &mut phase_hook,
        )
    })
}

fn commit_preflight_batch_with_writer<Before, After, Write>(
    batch: CommitBatch,
    mut before_write_hook: Before,
    mut after_verify_hook: After,
    mut write_plan: Write,
) -> Result<CommittedEditBatch, IdenteditError>
where
    Before: FnMut() -> Result<(), IdenteditError>,
    After: FnMut() -> Result<(), IdenteditError>,
    Write: FnMut(&PreflightFilePlan) -> Result<ApplyGuardState, AtomicWriteFailure>,
{
    validate_commit_batch_invariants(&batch)?;
    before_write_hook()?;

    let rollback_snapshots = batch.rollback_snapshots;
    let mut applied = Vec::with_capacity(batch.preflight_plans.len());
    let mut committed_files = Vec::with_capacity(batch.preflight_plans.len());
    for (index, plan) in batch.preflight_plans.into_iter().enumerate() {
        match commit_preflight_plan(plan, &mut after_verify_hook, &mut write_plan) {
            Ok((applied_result, expected_guard)) => {
                applied.push(applied_result);
                committed_files.push(CommittedFileRollback {
                    snapshot_index: index,
                    expected_guard,
                });
            }
            Err(commit_failure) => {
                let (commit_error, committed_guard) = commit_failure.into_parts();
                if let Some(expected_guard) = committed_guard {
                    committed_files.push(CommittedFileRollback {
                        snapshot_index: index,
                        expected_guard,
                    });
                }
                let rollback_result =
                    rollback_committed_files(&rollback_snapshots, &committed_files);
                match rollback_result {
                    Ok(()) => return Err(commit_error),
                    Err(rollback_error) => {
                        return Err(IdenteditError::RollbackFailed {
                            message: format!(
                                "Commit failed ({}); rollback failed ({rollback_error})",
                                commit_error
                            ),
                        });
                    }
                }
            }
        }
    }

    Ok(CommittedEditBatch {
        applied,
        rollback_snapshots,
        committed_files,
    })
}

pub(super) fn rollback_committed_files(
    rollback_snapshots: &[FileRollbackSnapshot],
    committed_files: &[CommittedFileRollback],
) -> Result<(), IdenteditError> {
    for committed_file in committed_files.iter().rev() {
        let snapshot = rollback_snapshots
            .get(committed_file.snapshot_index)
            .ok_or_else(|| IdenteditError::InvalidRequest {
                message: format!(
                    "Internal rollback error: missing snapshot for committed index {}",
                    committed_file.snapshot_index
                ),
            })?;
        verify_apply_guard_state(&snapshot.file, &committed_file.expected_guard)?;
    }

    for committed_file in committed_files.iter().rev() {
        let snapshot = rollback_snapshots
            .get(committed_file.snapshot_index)
            .ok_or_else(|| IdenteditError::InvalidRequest {
                message: format!(
                    "Internal rollback error: missing snapshot for committed index {}",
                    committed_file.snapshot_index
                ),
            })?;
        write_text_atomically(
            &snapshot.file,
            &snapshot.original_text,
            Some(&committed_file.expected_guard),
        )?;
        fs::set_permissions(&snapshot.file, snapshot.original_permissions.clone())
            .map_err(|error| IdenteditError::io(&snapshot.file, error))?;
    }

    Ok(())
}

fn commit_preflight_plan<After, Write>(
    plan: PreflightFilePlan,
    mut after_verify_hook: After,
    mut write_plan: Write,
) -> Result<(ApplyFileResult, ApplyGuardState), AtomicWriteFailure>
where
    After: FnMut() -> Result<(), IdenteditError>,
    Write: FnMut(&PreflightFilePlan) -> Result<ApplyGuardState, AtomicWriteFailure>,
{
    verify_apply_guard_state(&plan.file, &plan.guard_state).map_err(AtomicWriteFailure::from)?;
    after_verify_hook().map_err(AtomicWriteFailure::from)?;
    let expected_guard = write_plan(&plan)?;

    Ok((
        ApplyFileResult {
            file: plan.file.display().to_string(),
            operations_applied: plan.operations_total,
            operations_total: plan.operations_total,
            status: ApplyFileStatus::Applied,
        },
        expected_guard,
    ))
}
