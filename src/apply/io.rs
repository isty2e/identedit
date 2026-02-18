use std::fs::{self, File, OpenOptions};
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use fs2::FileExt;

use crate::changeset::hash_bytes;
use crate::error::IdenteditError;

static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
pub(super) struct ApplyFileLock {
    _file: File,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AtomicWritePhase {
    TempWritten,
    TempSynced,
    Renamed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ApplyGuardState {
    pub(super) path_fingerprint: PathFingerprint,
    pub(super) source_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PathFingerprint {
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
    length: u64,
    modified_nanos: Option<u128>,
}

pub(super) fn acquire_apply_lock(path: &Path) -> Result<ApplyFileLock, IdenteditError> {
    let file = OpenOptions::new()
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)
        .map_err(|error| IdenteditError::io(path, error))?;

    file.try_lock_exclusive().map_err(|error| {
        if error.kind() == std::io::ErrorKind::WouldBlock {
            IdenteditError::ResourceBusy {
                path: path.display().to_string(),
            }
        } else {
            IdenteditError::io(path, error)
        }
    })?;

    Ok(ApplyFileLock { _file: file })
}

pub(super) fn capture_path_fingerprint(path: &Path) -> Result<PathFingerprint, IdenteditError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| IdenteditError::io(path, error))?;

    if metadata.file_type().is_symlink() {
        return Err(IdenteditError::InvalidRequest {
            message: format!(
                "Refusing to apply changes through symbolic link '{}'",
                path.display()
            ),
        });
    }

    let modified_nanos = metadata
        .modified()
        .ok()
        .and_then(|timestamp| timestamp.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos());

    Ok(PathFingerprint {
        #[cfg(unix)]
        device: metadata.dev(),
        #[cfg(unix)]
        inode: metadata.ino(),
        length: metadata.len(),
        modified_nanos,
    })
}

pub(super) fn capture_apply_guard_state(path: &Path) -> Result<ApplyGuardState, IdenteditError> {
    let path_fingerprint = capture_path_fingerprint(path)?;
    let source_bytes = fs::read(path).map_err(|error| IdenteditError::io(path, error))?;
    Ok(ApplyGuardState {
        path_fingerprint,
        source_hash: hash_bytes(&source_bytes),
    })
}

pub(super) fn verify_apply_guard_state(
    path: &Path,
    expected: &ApplyGuardState,
) -> Result<(), IdenteditError> {
    let current_fingerprint = capture_path_fingerprint(path)?;
    if current_fingerprint != expected.path_fingerprint {
        return Err(IdenteditError::PathChanged {
            path: path.display().to_string(),
        });
    }

    let current_bytes = fs::read(path).map_err(|error| IdenteditError::io(path, error))?;
    let current_hash = hash_bytes(&current_bytes);
    if current_hash != expected.source_hash {
        return Err(IdenteditError::PreconditionFailed {
            expected_hash: expected.source_hash.clone(),
            actual_hash: current_hash,
        });
    }

    Ok(())
}

pub(super) fn write_text_atomically(
    path: &Path,
    contents: &str,
    expected_guard: Option<&ApplyGuardState>,
) -> Result<(), IdenteditError> {
    write_text_atomically_with_hook_and_guard(path, contents, expected_guard, |_| Ok(()))
}

#[cfg(test)]
pub(super) fn write_text_atomically_with_hook<F>(
    path: &Path,
    contents: &str,
    phase_hook: F,
) -> Result<(), IdenteditError>
where
    F: FnMut(AtomicWritePhase) -> std::io::Result<()>,
{
    write_text_atomically_with_hook_and_guard(path, contents, None, phase_hook)
}

#[cfg(test)]
pub(super) fn write_text_atomically_with_hook_and_rename<F, R>(
    path: &Path,
    contents: &str,
    phase_hook: F,
    rename_file: R,
) -> Result<(), IdenteditError>
where
    F: FnMut(AtomicWritePhase) -> std::io::Result<()>,
    R: FnMut(&Path, &Path) -> std::io::Result<()>,
{
    write_text_atomically_with_hook_guard_and_rename(path, contents, None, phase_hook, rename_file)
}

fn write_text_atomically_with_hook_and_guard<F>(
    path: &Path,
    contents: &str,
    expected_guard: Option<&ApplyGuardState>,
    phase_hook: F,
) -> Result<(), IdenteditError>
where
    F: FnMut(AtomicWritePhase) -> std::io::Result<()>,
{
    write_text_atomically_with_hook_guard_and_rename(
        path,
        contents,
        expected_guard,
        phase_hook,
        |from, to| fs::rename(from, to),
    )
}

fn write_text_atomically_with_hook_guard_and_rename<F, R>(
    path: &Path,
    contents: &str,
    expected_guard: Option<&ApplyGuardState>,
    mut phase_hook: F,
    mut rename_file: R,
) -> Result<(), IdenteditError>
where
    F: FnMut(AtomicWritePhase) -> std::io::Result<()>,
    R: FnMut(&Path, &Path) -> std::io::Result<()>,
{
    let target_permissions = fs::metadata(path)
        .map_err(|error| IdenteditError::io(path, error))?
        .permissions();
    let (temp_path, mut temp_file) = create_temp_file_adjacent(path)?;

    let result = (|| {
        temp_file
            .write_all(contents.as_bytes())
            .map_err(|error| IdenteditError::io(&temp_path, error))?;
        phase_hook(AtomicWritePhase::TempWritten)
            .map_err(|error| IdenteditError::io(path, error))?;

        temp_file
            .sync_all()
            .map_err(|error| IdenteditError::io(&temp_path, error))?;
        phase_hook(AtomicWritePhase::TempSynced)
            .map_err(|error| IdenteditError::io(path, error))?;

        if let Some(guard_state) = expected_guard {
            verify_apply_guard_state(path, guard_state)?;
        }

        fs::set_permissions(&temp_path, target_permissions.clone())
            .map_err(|error| IdenteditError::io(&temp_path, error))?;
        drop(temp_file);

        rename_file(&temp_path, path).map_err(|error| IdenteditError::io(path, error))?;
        phase_hook(AtomicWritePhase::Renamed).map_err(|error| IdenteditError::io(path, error))?;

        sync_parent_directory(path)
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }

    result
}

fn create_temp_file_adjacent(path: &Path) -> Result<(PathBuf, File), IdenteditError> {
    let parent = resolve_parent_directory(path);
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("identedit-target");

    for _ in 0..64 {
        let counter = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        let temp_name = format!(".{file_name}.identedit-tmp-{nanos}-{counter}");
        let temp_path = parent.join(temp_name);

        match OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp_path)
        {
            Ok(file) => return Ok((temp_path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(IdenteditError::io(&temp_path, error)),
        }
    }

    Err(IdenteditError::InvalidRequest {
        message: format!(
            "Failed to allocate an adjacent temporary file for '{}'",
            path.display()
        ),
    })
}

fn resolve_parent_directory(path: &Path) -> PathBuf {
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => PathBuf::from("."),
    }
}

pub(super) fn sync_parent_directory(path: &Path) -> Result<(), IdenteditError> {
    #[cfg(unix)]
    {
        let parent = resolve_parent_directory(path);
        let directory_handle =
            File::open(&parent).map_err(|error| IdenteditError::io(&parent, error))?;
        directory_handle
            .sync_all()
            .map_err(|error| IdenteditError::io(&parent, error))
    }

    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(())
    }
}
