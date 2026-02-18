use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::changeset::{ChangeOp, FileChange, OpKind};
use crate::error::IdenteditError;

use super::io::{
    ApplyFileLock, ApplyGuardState, acquire_apply_lock, capture_apply_guard_state,
    sync_parent_directory, verify_apply_guard_state,
};
use super::{ApplyFileResult, ApplyFileStatus};

#[derive(Debug, Clone)]
struct MoveEdge {
    source: PathBuf,
    destination: PathBuf,
}

#[derive(Debug, Clone)]
pub(super) struct NormalizedMoveEdge {
    pub(super) source: PathBuf,
    pub(super) destination: PathBuf,
}

pub(super) fn validate_move_operation_constraints(
    changesets: &[FileChange],
) -> Result<Vec<NormalizedMoveEdge>, IdenteditError> {
    let mut move_edges = Vec::new();
    for changeset in changesets {
        if let Some(move_edge) = validate_file_move_operation_constraints(changeset)? {
            move_edges.push(move_edge);
        }
    }

    if move_edges.is_empty() {
        return Ok(Vec::new());
    }

    validate_move_graph(&move_edges)
}

fn validate_file_move_operation_constraints(
    changeset: &FileChange,
) -> Result<Option<MoveEdge>, IdenteditError> {
    let move_count = changeset
        .operations
        .iter()
        .filter(|operation| matches!(operation.op, OpKind::Move { .. }))
        .count();
    let has_content_edit = changeset
        .operations
        .iter()
        .any(|operation| !matches!(operation.op, OpKind::Move { .. }));

    if move_count > 1 {
        return Err(IdenteditError::InvalidRequest {
            message: format!(
                "Only one move operation is allowed per file: '{}'",
                changeset.file.display()
            ),
        });
    }

    if move_count == 0 {
        return Ok(None);
    }

    if has_content_edit {
        return Err(IdenteditError::InvalidRequest {
            message: "Move cannot be combined with content-edit operations for the same file"
                .to_string(),
        });
    }

    let move_operation = changeset
        .operations
        .iter()
        .find(|operation| matches!(operation.op, OpKind::Move { .. }))
        .ok_or_else(|| IdenteditError::InvalidRequest {
            message: format!(
                "Internal validation error: expected one move operation for '{}'",
                changeset.file.display()
            ),
        })?;
    let destination = match &move_operation.op {
        OpKind::Move { to } => to.clone(),
        _ => unreachable!("move_operation must be move"),
    };
    validate_move_preview(changeset, move_operation, &destination)?;

    Ok(Some(MoveEdge {
        source: changeset.file.clone(),
        destination,
    }))
}

fn validate_move_preview(
    changeset: &FileChange,
    operation: &ChangeOp,
    destination: &Path,
) -> Result<(), IdenteditError> {
    let Some(preview) = operation.preview.move_preview.as_ref() else {
        // Backward-compatible path: legacy move payloads may omit preview.move.
        return Ok(());
    };

    if preview.from != changeset.file || preview.to != destination {
        return Err(IdenteditError::InvalidRequest {
            message: format!(
                "Move preview mismatch for '{}': expected move.from='{}' and move.to='{}'",
                changeset.file.display(),
                changeset.file.display(),
                destination.display(),
            ),
        });
    }

    if !operation
        .preview
        .old_text
        .as_deref()
        .unwrap_or("")
        .is_empty()
        || operation.preview.old_hash.is_some()
        || operation.preview.old_len.is_some()
        || !operation.preview.new_text.is_empty()
        || operation.preview.matched_span.start != 0
        || operation.preview.matched_span.end != 0
    {
        return Err(IdenteditError::InvalidRequest {
            message: format!(
                "Move operation for '{}' must use canonical placeholder preview fields (old_text/new_text empty, no compact old_hash/old_len, matched_span [0,0))",
                changeset.file.display()
            ),
        });
    }

    Ok(())
}

fn validate_move_graph(move_edges: &[MoveEdge]) -> Result<Vec<NormalizedMoveEdge>, IdenteditError> {
    let normalized_edges = normalize_move_edges(move_edges)?;

    let mut source_to_destination = BTreeMap::new();
    let mut destination_to_source = BTreeMap::new();
    for edge in &normalized_edges {
        if edge.source == edge.destination {
            return Err(IdenteditError::InvalidRequest {
                message: format!(
                    "Move self-move is not supported: '{}' -> '{}'",
                    edge.source.display(),
                    edge.destination.display()
                ),
            });
        }

        if let Some(previous_destination) =
            source_to_destination.insert(edge.source.clone(), edge.destination.clone())
        {
            return Err(IdenteditError::InvalidRequest {
                message: format!(
                    "Duplicate move source path is not supported: '{}' maps to both '{}' and '{}'",
                    edge.source.display(),
                    previous_destination.display(),
                    edge.destination.display()
                ),
            });
        }

        if let Some(previous_source) =
            destination_to_source.insert(edge.destination.clone(), edge.source.clone())
        {
            return Err(IdenteditError::InvalidRequest {
                message: format!(
                    "Duplicate move destination path is not supported: '{}' is targeted by both '{}' and '{}'",
                    edge.destination.display(),
                    previous_source.display(),
                    edge.source.display()
                ),
            });
        }
    }

    validate_move_destination_existence(&source_to_destination)?;
    let topo_order = validate_move_graph_is_acyclic(&source_to_destination)?;
    build_move_execution_order(&normalized_edges, &topo_order)
}

fn normalize_move_edges(
    move_edges: &[MoveEdge],
) -> Result<Vec<NormalizedMoveEdge>, IdenteditError> {
    let mut normalized = Vec::with_capacity(move_edges.len());
    for edge in move_edges {
        let source = fs::canonicalize(&edge.source)
            .map_err(|error| IdenteditError::io(&edge.source, error))?;
        let destination = normalize_move_destination_path(&edge.destination)?;
        normalized.push(NormalizedMoveEdge {
            source,
            destination,
        });
    }

    Ok(normalized)
}

fn normalize_move_destination_path(path: &Path) -> Result<PathBuf, IdenteditError> {
    match fs::canonicalize(path) {
        Ok(canonical_path) => Ok(canonical_path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let absolute = if path.is_absolute() {
                path.to_path_buf()
            } else {
                std::env::current_dir()
                    .map_err(|current_dir_error| {
                        IdenteditError::io(Path::new("."), current_dir_error)
                    })?
                    .join(path)
            };
            Ok(normalize_lexical_path(&absolute))
        }
        Err(error) => Err(IdenteditError::io(path, error)),
    }
}

fn normalize_lexical_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                let _ = normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }

    if normalized.as_os_str().is_empty() {
        path.to_path_buf()
    } else {
        normalized
    }
}

fn validate_move_destination_existence(
    source_to_destination: &BTreeMap<PathBuf, PathBuf>,
) -> Result<(), IdenteditError> {
    let sources = source_to_destination
        .keys()
        .cloned()
        .collect::<BTreeSet<PathBuf>>();
    for destination in source_to_destination.values() {
        if move_path_exists(destination)? && !sources.contains(destination) {
            return Err(IdenteditError::InvalidRequest {
                message: format!(
                    "Destination path already exists: '{}'",
                    destination.display()
                ),
            });
        }
    }

    Ok(())
}

fn move_path_exists(path: &Path) -> Result<bool, IdenteditError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(IdenteditError::io(path, error)),
    }
}

fn validate_move_graph_is_acyclic(
    source_to_destination: &BTreeMap<PathBuf, PathBuf>,
) -> Result<Vec<PathBuf>, IdenteditError> {
    let mut indegrees = BTreeMap::<PathBuf, usize>::new();
    let mut outgoing = BTreeMap::<PathBuf, Vec<PathBuf>>::new();
    for (source, destination) in source_to_destination {
        indegrees.entry(source.clone()).or_insert(0);
        *indegrees.entry(destination.clone()).or_insert(0) += 1;
        outgoing
            .entry(source.clone())
            .or_default()
            .push(destination.clone());
    }

    for destinations in outgoing.values_mut() {
        destinations.sort();
    }

    let mut queue = indegrees
        .iter()
        .filter_map(|(node, indegree)| {
            if *indegree == 0 {
                Some(node.clone())
            } else {
                None
            }
        })
        .collect::<BTreeSet<PathBuf>>();

    let mut topo_order = Vec::with_capacity(indegrees.len());
    let mut visited = 0usize;
    while let Some(node) = queue.pop_first() {
        visited += 1;
        topo_order.push(node.clone());
        if let Some(destinations) = outgoing.get(&node) {
            for destination in destinations {
                if let Some(indegree) = indegrees.get_mut(destination) {
                    *indegree = indegree.saturating_sub(1);
                    if *indegree == 0 {
                        queue.insert(destination.clone());
                    }
                }
            }
        }
    }

    if visited != indegrees.len() {
        return Err(IdenteditError::InvalidRequest {
            message: "Move graph contains a cycle; move operations must form an acyclic chain"
                .to_string(),
        });
    }

    Ok(topo_order)
}

fn build_move_execution_order(
    normalized_edges: &[NormalizedMoveEdge],
    topo_order: &[PathBuf],
) -> Result<Vec<NormalizedMoveEdge>, IdenteditError> {
    let mut source_ranks = BTreeMap::new();
    for (index, node) in topo_order.iter().enumerate() {
        source_ranks.insert(node.clone(), index);
    }

    let mut execution_order = normalized_edges.to_vec();
    execution_order.sort_by(|left, right| {
        let left_rank = source_ranks.get(&left.source).copied().unwrap_or(0);
        let right_rank = source_ranks.get(&right.source).copied().unwrap_or(0);
        right_rank
            .cmp(&left_rank)
            .then(left.source.cmp(&right.source))
    });

    let source_set = normalized_edges
        .iter()
        .map(|edge| edge.source.clone())
        .collect::<BTreeSet<_>>();
    let missing_source = execution_order
        .iter()
        .find(|edge| !source_set.contains(&edge.source));
    if let Some(missing) = missing_source {
        return Err(IdenteditError::InvalidRequest {
            message: format!(
                "Internal move planning error: missing source '{}' in execution order",
                missing.source.display()
            ),
        });
    }

    Ok(execution_order)
}

#[derive(Debug)]
pub(super) struct MovePreflightPlan {
    pub(super) source: PathBuf,
    pub(super) destination: PathBuf,
    pub(super) operations_total: usize,
    pub(super) guard_state: ApplyGuardState,
    pub(super) _lock_guard: ApplyFileLock,
}

pub(super) fn preflight_move_plans(
    execution_order: &[NormalizedMoveEdge],
) -> Result<Vec<MovePreflightPlan>, IdenteditError> {
    if execution_order.is_empty() {
        return Ok(Vec::new());
    }

    let mut lock_order = execution_order.to_vec();
    lock_order.sort_by(|left, right| left.source.cmp(&right.source));

    let mut plans_by_source = BTreeMap::new();
    for edge in lock_order {
        let lock_guard = acquire_apply_lock(&edge.source)?;
        let guard_state = capture_apply_guard_state(&edge.source)?;
        plans_by_source.insert(
            edge.source.clone(),
            MovePreflightPlan {
                source: edge.source,
                destination: edge.destination,
                operations_total: 1,
                guard_state,
                _lock_guard: lock_guard,
            },
        );
    }

    let mut ordered = Vec::with_capacity(execution_order.len());
    for edge in execution_order {
        let plan =
            plans_by_source
                .remove(&edge.source)
                .ok_or_else(|| IdenteditError::InvalidRequest {
                    message: format!(
                        "Internal move preflight error: missing lock plan for '{}'",
                        edge.source.display()
                    ),
                })?;
        ordered.push(plan);
    }

    Ok(ordered)
}

pub(super) fn commit_move_plans<After>(
    plans: &[MovePreflightPlan],
    mut after_verify_hook: After,
) -> Result<Vec<ApplyFileResult>, (IdenteditError, Vec<usize>)>
where
    After: FnMut() -> Result<(), IdenteditError>,
{
    let mut applied = Vec::with_capacity(plans.len());
    let mut committed_indices = Vec::new();
    for (index, plan) in plans.iter().enumerate() {
        match commit_move_plan(plan, &mut after_verify_hook) {
            Ok(result) => {
                applied.push(result);
                committed_indices.push(index);
            }
            Err(error) => return Err((error, committed_indices)),
        }
    }

    Ok(applied)
}

fn commit_move_plan<After>(
    plan: &MovePreflightPlan,
    after_verify_hook: After,
) -> Result<ApplyFileResult, IdenteditError>
where
    After: FnMut() -> Result<(), IdenteditError>,
{
    commit_move_plan_with_after_and_rename(plan, after_verify_hook, |from, to| fs::rename(from, to))
}

#[cfg(test)]
pub(super) fn commit_move_plan_with_rename<After, R>(
    plan: &MovePreflightPlan,
    after_verify_hook: After,
    rename_file: R,
) -> Result<ApplyFileResult, IdenteditError>
where
    After: FnMut() -> Result<(), IdenteditError>,
    R: FnMut(&Path, &Path) -> std::io::Result<()>,
{
    commit_move_plan_with_after_and_rename(plan, after_verify_hook, rename_file)
}

fn commit_move_plan_with_after_and_rename<After, R>(
    plan: &MovePreflightPlan,
    mut after_verify_hook: After,
    mut rename_file: R,
) -> Result<ApplyFileResult, IdenteditError>
where
    After: FnMut() -> Result<(), IdenteditError>,
    R: FnMut(&Path, &Path) -> std::io::Result<()>,
{
    verify_apply_guard_state(&plan.source, &plan.guard_state)?;
    after_verify_hook()?;

    if move_path_exists(&plan.destination)? {
        return Err(IdenteditError::InvalidRequest {
            message: format!(
                "Destination path already exists: '{}'",
                plan.destination.display()
            ),
        });
    }

    rename_file(&plan.source, &plan.destination)
        .map_err(|error| IdenteditError::io(&plan.source, error))?;
    sync_parent_directory(&plan.source)?;
    sync_parent_directory(&plan.destination)?;

    Ok(ApplyFileResult {
        file: plan.source.display().to_string(),
        operations_applied: plan.operations_total,
        operations_total: plan.operations_total,
        status: ApplyFileStatus::Applied,
    })
}

pub(super) fn rollback_committed_moves(
    plans: &[MovePreflightPlan],
    committed_indices: &[usize],
) -> Result<(), IdenteditError> {
    for index in committed_indices.iter().rev() {
        let plan = plans
            .get(*index)
            .ok_or_else(|| IdenteditError::InvalidRequest {
                message: format!(
                    "Internal move rollback error: missing plan for committed index {index}"
                ),
            })?;

        fs::rename(&plan.destination, &plan.source)
            .map_err(|error| IdenteditError::io(&plan.destination, error))?;
        sync_parent_directory(&plan.source)?;
        sync_parent_directory(&plan.destination)?;
    }

    Ok(())
}
