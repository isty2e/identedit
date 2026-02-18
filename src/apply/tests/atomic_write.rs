#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use tempfile::tempdir;

use crate::error::IdenteditError;

use super::super::{
    AtomicWritePhase, write_text_atomically_with_hook, write_text_atomically_with_hook_and_rename,
};
use super::fail_on_phase;

#[test]
fn atomic_write_replaces_contents_and_leaves_no_temp_files() {
    let directory = tempdir().expect("tempdir should be created");
    let file_path = directory.path().join("target.txt");
    std::fs::write(&file_path, "old content").expect("fixture write should succeed");

    write_text_atomically_with_hook(&file_path, "new content", |_| Ok(()))
        .expect("atomic write should succeed");

    let actual = std::fs::read_to_string(&file_path).expect("target should be readable");
    assert_eq!(actual, "new content");

    let entries = std::fs::read_dir(directory.path()).expect("directory should be readable");
    let temp_entries = entries
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .filter(|name| name.contains(".identedit-tmp-"))
        .collect::<Vec<_>>();
    assert!(
        temp_entries.is_empty(),
        "atomic write should clean temporary files: {temp_entries:?}"
    );
}

#[test]
fn atomic_write_preserves_original_contents_when_failure_occurs_before_rename() {
    let directory = tempdir().expect("tempdir should be created");
    let file_path = directory.path().join("target.txt");
    std::fs::write(&file_path, "stable content").expect("fixture write should succeed");

    let mut hook = fail_on_phase(AtomicWritePhase::TempSynced);
    let error = write_text_atomically_with_hook(&file_path, "new content", &mut hook)
        .expect_err("injected failure should surface");
    assert!(
        error.to_string().contains("injected atomic-write failure"),
        "expected injected failure to propagate"
    );

    let actual = std::fs::read_to_string(&file_path).expect("target should remain readable");
    assert_eq!(actual, "stable content");

    let entries = std::fs::read_dir(directory.path()).expect("directory should be readable");
    let temp_entries = entries
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .filter(|name| name.contains(".identedit-tmp-"))
        .collect::<Vec<_>>();
    assert!(
        temp_entries.is_empty(),
        "failed atomic write should clean temporary files: {temp_entries:?}"
    );
}

#[test]
fn atomic_write_failure_at_temp_written_preserves_contents_and_cleans_temp_files() {
    let directory = tempdir().expect("tempdir should be created");
    let file_path = directory.path().join("target.txt");
    std::fs::write(&file_path, "stable content").expect("fixture write should succeed");

    let mut hook = fail_on_phase(AtomicWritePhase::TempWritten);
    let error = write_text_atomically_with_hook(&file_path, "new content", &mut hook)
        .expect_err("injected TempWritten failure should surface");
    assert!(
        error.to_string().contains("injected atomic-write failure"),
        "expected injected failure to propagate"
    );

    let actual = std::fs::read_to_string(&file_path).expect("target should remain readable");
    assert_eq!(actual, "stable content");

    let entries = std::fs::read_dir(directory.path()).expect("directory should be readable");
    let temp_entries = entries
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .filter(|name| name.contains(".identedit-tmp-"))
        .collect::<Vec<_>>();
    assert!(
        temp_entries.is_empty(),
        "TempWritten failure should clean temporary files: {temp_entries:?}"
    );
}

#[test]
fn atomic_write_reports_error_if_failure_occurs_after_rename_and_leaves_new_contents() {
    let directory = tempdir().expect("tempdir should be created");
    let file_path = directory.path().join("target.txt");
    std::fs::write(&file_path, "stable content").expect("fixture write should succeed");

    let mut hook = fail_on_phase(AtomicWritePhase::Renamed);
    let error = write_text_atomically_with_hook(&file_path, "new content", &mut hook)
        .expect_err("post-rename failure should be surfaced");
    assert!(
        error.to_string().contains("injected atomic-write failure"),
        "expected injected failure to propagate"
    );

    let actual = std::fs::read_to_string(&file_path).expect("target should remain readable");
    assert_eq!(
        actual, "new content",
        "post-rename failure should not roll back committed rename"
    );

    let entries = std::fs::read_dir(directory.path()).expect("directory should be readable");
    let temp_entries = entries
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .filter(|name| name.contains(".identedit-tmp-"))
        .collect::<Vec<_>>();
    assert!(
        temp_entries.is_empty(),
        "post-rename failure should not leak temporary files: {temp_entries:?}"
    );
}

#[cfg(unix)]
#[test]
fn atomic_write_preserves_existing_file_mode_bits() {
    let directory = tempdir().expect("tempdir should be created");
    let file_path = directory.path().join("target.sh");
    std::fs::write(&file_path, "echo old").expect("fixture write should succeed");
    std::fs::set_permissions(&file_path, std::fs::Permissions::from_mode(0o751))
        .expect("fixture permissions should be set");

    write_text_atomically_with_hook(&file_path, "echo new", |_| Ok(()))
        .expect("atomic write should succeed");

    let mode = std::fs::metadata(&file_path)
        .expect("metadata should be readable")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(
        mode, 0o751,
        "atomic write should preserve existing mode bits"
    );
}

#[cfg(unix)]
#[test]
fn atomic_write_failure_in_read_only_directory_preserves_file_and_cleans_temp() {
    let directory = tempdir().expect("tempdir should be created");
    let locked_directory = directory.path().join("locked");
    std::fs::create_dir(&locked_directory).expect("subdir should be created");
    let file_path = locked_directory.join("target.txt");
    std::fs::write(&file_path, "stable content").expect("fixture write should succeed");
    let original_permissions = std::fs::metadata(&locked_directory)
        .expect("metadata should be readable")
        .permissions();

    let mut read_only_permissions = original_permissions.clone();
    read_only_permissions.set_mode(0o555);
    std::fs::set_permissions(&locked_directory, read_only_permissions)
        .expect("directory permissions should be updated");

    let write_result = write_text_atomically_with_hook(&file_path, "new content", |_| Ok(()));

    std::fs::set_permissions(&locked_directory, original_permissions)
        .expect("directory permissions should be restorable");

    let error = write_result.expect_err("atomic write should fail in read-only directory");
    assert!(
        error.to_string().contains("Permission denied"),
        "expected permission error, got: {error}"
    );

    let actual = std::fs::read_to_string(&file_path).expect("target should remain readable");
    assert_eq!(actual, "stable content");

    let entries = std::fs::read_dir(&locked_directory).expect("directory should be readable");
    let temp_entries = entries
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .filter(|name| name.contains(".identedit-tmp-"))
        .collect::<Vec<_>>();
    assert!(
        temp_entries.is_empty(),
        "permission failure should not leak temp files: {temp_entries:?}"
    );
}

#[test]
fn atomic_write_rename_failure_from_target_swap_to_directory_cleans_temp_file() {
    let directory = tempdir().expect("tempdir should be created");
    let file_path = directory.path().join("target.txt");
    let backup_path = directory.path().join("backup.txt");
    std::fs::write(&file_path, "stable content").expect("fixture write should succeed");

    let mut hook = |phase| {
        if phase == AtomicWritePhase::TempSynced {
            std::fs::rename(&file_path, &backup_path)
                .expect("target file should be renamed to backup");
            std::fs::create_dir(&file_path).expect("target path should be replaced with directory");
        }

        Ok(())
    };
    let error = write_text_atomically_with_hook(&file_path, "new content", &mut hook)
        .expect_err("rename should fail once target path becomes a directory");
    match error {
        IdenteditError::Io { .. } => {}
        other => panic!("unexpected error variant: {other}"),
    }

    let backup_contents = std::fs::read_to_string(&backup_path).expect("backup should be readable");
    assert_eq!(
        backup_contents, "stable content",
        "original file should remain in backup after forced target-path swap"
    );

    let target_metadata = std::fs::metadata(&file_path).expect("swapped target path should exist");
    assert!(
        target_metadata.is_dir(),
        "target path should remain the swapped directory"
    );

    let entries = std::fs::read_dir(directory.path()).expect("directory should be readable");
    let temp_entries = entries
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .filter(|name| name.contains(".identedit-tmp-"))
        .collect::<Vec<_>>();
    assert!(
        temp_entries.is_empty(),
        "rename failure should still clean temporary files: {temp_entries:?}"
    );
}

#[cfg(unix)]
#[test]
fn atomic_write_exdev_like_rename_failure_preserves_contents_and_cleans_temp_file() {
    let directory = tempdir().expect("tempdir should be created");
    let file_path = directory.path().join("target.txt");
    std::fs::write(&file_path, "stable content").expect("fixture write should succeed");

    let error = write_text_atomically_with_hook_and_rename(
        &file_path,
        "new content",
        |_| Ok(()),
        |_, _| Err(std::io::Error::from_raw_os_error(18)),
    )
    .expect_err("injected EXDEV-like rename failure should surface");
    match error {
        IdenteditError::Io { source, .. } => {
            assert_eq!(
                source.raw_os_error(),
                Some(18),
                "error should preserve EXDEV-like raw os code"
            );
        }
        other => panic!("unexpected error variant: {other}"),
    }

    let actual = std::fs::read_to_string(&file_path).expect("target should remain readable");
    assert_eq!(actual, "stable content");

    let entries = std::fs::read_dir(directory.path()).expect("directory should be readable");
    let temp_entries = entries
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .filter(|name| name.contains(".identedit-tmp-"))
        .collect::<Vec<_>>();
    assert!(
        temp_entries.is_empty(),
        "EXDEV-like rename failure should clean temporary files: {temp_entries:?}"
    );
}
