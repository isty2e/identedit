use std::collections::{BTreeMap, BTreeSet};
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::ffi::CString;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use crate::changeset::{ChangeOp, FileChange};
use crate::error::IdenteditError;
use crate::hash::ContentHash;

use super::io::{
    ApplyFileLock, ApplyGuardState, acquire_apply_lock, capture_apply_guard_state,
    sync_parent_directory, verify_apply_guard_state,
};
use super::{ApplyFileResult, ApplyFileStatus};

#[derive(Debug, Clone)]
struct MoveEdge {
    source: PathBuf,
    destination: PathBuf,
    expected_file_hash: ContentHash,
}

#[derive(Debug, Clone)]
pub(super) struct NormalizedMoveEdge {
    pub(super) source: PathBuf,
    pub(super) destination: PathBuf,
    pub(super) expected_file_hash: ContentHash,
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
        .filter(|operation| operation.as_file_move().is_some())
        .count();
    let has_content_edit = changeset
        .operations
        .iter()
        .any(|operation| operation.as_file_move().is_none());

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
        .find_map(ChangeOp::as_file_move)
        .ok_or_else(|| IdenteditError::InvalidRequest {
            message: format!(
                "Internal validation error: expected one move operation for '{}'",
                changeset.file.display()
            ),
        })?;
    validate_move_preview(
        changeset,
        move_operation.preview,
        move_operation.destination,
    )?;

    Ok(Some(MoveEdge {
        source: changeset.file.clone(),
        destination: move_operation.destination.to_path_buf(),
        expected_file_hash: move_operation.expected_file_hash.clone(),
    }))
}

fn validate_move_preview(
    changeset: &FileChange,
    preview: &crate::changeset::MoveChangePreview,
    destination: &Path,
) -> Result<(), IdenteditError> {
    let Some(move_preview) = preview.move_preview.as_ref() else {
        // Backward-compatible payloads are normalized to an absent move preview at ingress.
        return Ok(());
    };

    if move_preview.from != changeset.file || move_preview.to != destination {
        return Err(IdenteditError::InvalidRequest {
            message: format!(
                "Move preview mismatch for '{}': expected move.from='{}' and move.to='{}'",
                changeset.file.display(),
                changeset.file.display(),
                destination.display(),
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
            expected_file_hash: edge.expected_file_hash.clone(),
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MoveCommitState {
    NotCommitted,
    Committed,
}

#[derive(Debug)]
struct MoveCommitFailure {
    error: IdenteditError,
    commit_state: MoveCommitState,
}

impl From<IdenteditError> for MoveCommitFailure {
    fn from(error: IdenteditError) -> Self {
        Self {
            error,
            commit_state: MoveCommitState::NotCommitted,
        }
    }
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
        if guard_state.source_hash != edge.expected_file_hash {
            return Err(IdenteditError::PreconditionFailed {
                expected_hash: edge.expected_file_hash.to_string(),
                actual_hash: guard_state.source_hash.to_string(),
            });
        }
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
    after_verify_hook: After,
) -> Result<Vec<ApplyFileResult>, (IdenteditError, Vec<usize>)>
where
    After: FnMut() -> Result<(), IdenteditError>,
{
    commit_move_plans_with_hooks(plans, after_verify_hook, || Ok(()), || Ok(()))
}

#[cfg(test)]
pub(super) fn commit_move_plans_with_after_rename_hook<After, Renamed>(
    plans: &[MovePreflightPlan],
    after_verify_hook: After,
    after_rename_hook: Renamed,
) -> Result<Vec<ApplyFileResult>, (IdenteditError, Vec<usize>)>
where
    After: FnMut() -> Result<(), IdenteditError>,
    Renamed: FnMut() -> Result<(), IdenteditError>,
{
    commit_move_plans_with_hooks(plans, after_verify_hook, || Ok(()), after_rename_hook)
}

fn commit_move_plans_with_hooks<After, BeforeRename, Renamed>(
    plans: &[MovePreflightPlan],
    mut after_verify_hook: After,
    mut before_rename_hook: BeforeRename,
    mut after_rename_hook: Renamed,
) -> Result<Vec<ApplyFileResult>, (IdenteditError, Vec<usize>)>
where
    After: FnMut() -> Result<(), IdenteditError>,
    BeforeRename: FnMut() -> Result<(), IdenteditError>,
    Renamed: FnMut() -> Result<(), IdenteditError>,
{
    let mut applied = Vec::with_capacity(plans.len());
    let mut committed_indices = Vec::new();
    for (index, plan) in plans.iter().enumerate() {
        match commit_move_plan_with_hooks(
            plan,
            &mut after_verify_hook,
            &mut before_rename_hook,
            &mut after_rename_hook,
            rename_file_no_replace,
        ) {
            Ok(result) => {
                applied.push(result);
                committed_indices.push(index);
            }
            Err(commit_failure) => {
                if commit_failure.commit_state == MoveCommitState::Committed {
                    committed_indices.push(index);
                }
                return Err((commit_failure.error, committed_indices));
            }
        }
    }

    Ok(applied)
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
    commit_move_plan_with_hooks(plan, after_verify_hook, || Ok(()), || Ok(()), rename_file)
        .map_err(|failure| failure.error)
}

#[cfg(test)]
pub(super) fn commit_move_plan_with_before_rename_hook<After, BeforeRename>(
    plan: &MovePreflightPlan,
    after_verify_hook: After,
    before_rename_hook: BeforeRename,
) -> Result<ApplyFileResult, IdenteditError>
where
    After: FnMut() -> Result<(), IdenteditError>,
    BeforeRename: FnMut() -> Result<(), IdenteditError>,
{
    commit_move_plan_with_hooks(
        plan,
        after_verify_hook,
        before_rename_hook,
        || Ok(()),
        rename_file_no_replace,
    )
    .map_err(|failure| failure.error)
}

fn commit_move_plan_with_hooks<After, BeforeRename, Renamed, R>(
    plan: &MovePreflightPlan,
    mut after_verify_hook: After,
    mut before_rename_hook: BeforeRename,
    mut after_rename_hook: Renamed,
    mut rename_file: R,
) -> Result<ApplyFileResult, MoveCommitFailure>
where
    After: FnMut() -> Result<(), IdenteditError>,
    BeforeRename: FnMut() -> Result<(), IdenteditError>,
    Renamed: FnMut() -> Result<(), IdenteditError>,
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
        }
        .into());
    }

    before_rename_hook()?;
    rename_file(&plan.source, &plan.destination).map_err(|error| {
        let error = if error.kind() == io::ErrorKind::AlreadyExists {
            IdenteditError::InvalidRequest {
                message: format!(
                    "Destination path already exists: '{}'",
                    plan.destination.display()
                ),
            }
        } else {
            IdenteditError::io(&plan.source, error)
        };
        MoveCommitFailure::from(error)
    })?;
    let committed_failure = |error| MoveCommitFailure {
        error,
        commit_state: MoveCommitState::Committed,
    };
    after_rename_hook().map_err(committed_failure)?;
    sync_parent_directory(&plan.source).map_err(committed_failure)?;
    sync_parent_directory(&plan.destination).map_err(committed_failure)?;

    Ok(ApplyFileResult {
        file: plan.source.display().to_string(),
        operations_applied: plan.operations_total,
        operations_total: plan.operations_total,
        status: ApplyFileStatus::Applied,
    })
}

#[cfg(target_os = "linux")]
fn rename_file_no_replace(source: &Path, destination: &Path) -> io::Result<()> {
    let source = path_to_c_string(source)?;
    let destination = path_to_c_string(destination)?;
    // SAFETY: both pointers reference live, NUL-terminated path buffers for the duration
    // of the call, and renameat2 does not retain either pointer.
    let result = unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            source.as_ptr(),
            libc::AT_FDCWD,
            destination.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(target_os = "macos")]
fn rename_file_no_replace(source: &Path, destination: &Path) -> io::Result<()> {
    let source = path_to_c_string(source)?;
    let destination = path_to_c_string(destination)?;
    // SAFETY: both pointers reference live, NUL-terminated path buffers for the duration
    // of the call, and renamex_np does not retain either pointer.
    let result =
        unsafe { libc::renamex_np(source.as_ptr(), destination.as_ptr(), libc::RENAME_EXCL) };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn path_to_c_string(path: &Path) -> io::Result<CString> {
    use std::os::unix::ffi::OsStrExt;

    CString::new(path.as_os_str().as_bytes()).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Path contains an interior NUL byte: '{}'", path.display()),
        )
    })
}

#[cfg(windows)]
fn rename_file_no_replace(source: &Path, destination: &Path) -> io::Result<()> {
    use windows_sys::Win32::Storage::FileSystem::{MOVEFILE_WRITE_THROUGH, MoveFileExW};

    let source = path_to_windows_wide(source)?;
    let destination = path_to_windows_wide(destination)?;
    // Omitting MOVEFILE_REPLACE_EXISTING makes destination creation an atomic failure.
    // SAFETY: both pointers reference live, NUL-terminated UTF-16 buffers for the call.
    let result = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_WRITE_THROUGH,
        )
    };
    if result != 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(windows)]
fn path_to_windows_wide(path: &Path) -> io::Result<Vec<u16>> {
    use std::os::windows::ffi::OsStrExt;

    const LEGACY_MAX_PATH: usize = 248;
    const VERBATIM_PREFIX: &[u16] = &[b'\\' as u16, b'\\' as u16, b'?' as u16, b'\\' as u16];
    const NT_PREFIX: &[u16] = &[b'\\' as u16, b'?' as u16, b'?' as u16, b'\\' as u16];
    const DEVICE_PREFIX: &[u16] = &[b'\\' as u16, b'\\' as u16, b'.' as u16, b'\\' as u16];
    const UNC_PREFIX: &[u16] = &[
        b'\\' as u16,
        b'\\' as u16,
        b'?' as u16,
        b'\\' as u16,
        b'U' as u16,
        b'N' as u16,
        b'C' as u16,
        b'\\' as u16,
    ];
    const UNC_PATH_PREFIX: &[u16] = &[b'\\' as u16, b'\\' as u16];

    let absolute = std::path::absolute(path)?;
    let mut encoded = absolute.as_os_str().encode_wide().collect::<Vec<_>>();
    if encoded.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Path contains an interior NUL byte: '{}'", path.display()),
        ));
    }

    if encoded.len() + 1 >= LEGACY_MAX_PATH
        && !encoded.starts_with(VERBATIM_PREFIX)
        && !encoded.starts_with(NT_PREFIX)
    {
        if encoded.starts_with(DEVICE_PREFIX) {
            encoded.splice(..DEVICE_PREFIX.len(), VERBATIM_PREFIX.iter().copied());
        } else if encoded.starts_with(UNC_PATH_PREFIX) {
            encoded.splice(..UNC_PATH_PREFIX.len(), UNC_PREFIX.iter().copied());
        } else {
            encoded.splice(..0, VERBATIM_PREFIX.iter().copied());
        }
    }

    encoded.push(0);
    Ok(encoded)
}

#[cfg(all(test, windows))]
mod windows_tests {
    use std::fs;

    use tempfile::tempdir;

    use super::rename_file_no_replace;

    #[test]
    fn rename_file_no_replace_moves_to_missing_destination() {
        let workspace = tempdir().expect("tempdir should be created");
        let source = workspace.path().join("source.txt");
        let destination = workspace.path().join("destination.txt");
        fs::write(&source, "source").expect("source should be written");

        rename_file_no_replace(&source, &destination).expect("move should succeed");

        assert!(!source.exists());
        assert_eq!(
            fs::read_to_string(&destination).expect("destination should be readable"),
            "source"
        );
    }

    #[test]
    fn rename_file_no_replace_preserves_existing_destination() {
        let workspace = tempdir().expect("tempdir should be created");
        let source = workspace.path().join("source.txt");
        let destination = workspace.path().join("destination.txt");
        fs::write(&source, "source").expect("source should be written");
        fs::write(&destination, "destination").expect("destination should be written");

        rename_file_no_replace(&source, &destination)
            .expect_err("existing destination must make the move fail");

        assert_eq!(
            fs::read_to_string(&source).expect("source should remain readable"),
            "source"
        );
        assert_eq!(
            fs::read_to_string(&destination).expect("destination should remain readable"),
            "destination"
        );
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn rename_file_no_replace(_source: &Path, _destination: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "atomic no-replace file moves are unsupported on this platform",
    ))
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

        verify_apply_guard_state(&plan.destination, &plan.guard_state)?;
        if move_path_exists(&plan.source)? {
            return Err(IdenteditError::InvalidRequest {
                message: format!(
                    "Move rollback source path already exists: '{}'",
                    plan.source.display()
                ),
            });
        }
        rename_file_no_replace(&plan.destination, &plan.source).map_err(|error| {
            if error.kind() == io::ErrorKind::AlreadyExists {
                IdenteditError::InvalidRequest {
                    message: format!(
                        "Move rollback source path already exists: '{}'",
                        plan.source.display()
                    ),
                }
            } else {
                IdenteditError::io(&plan.destination, error)
            }
        })?;
        sync_parent_directory(&plan.source)?;
        sync_parent_directory(&plan.destination)?;
    }

    Ok(())
}
