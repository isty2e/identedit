use std::fs::FileTimes;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use tempfile::tempdir;

use crate::error::IdenteditError;
use crate::transform::{build_replace_changeset, parse_handles_for_file};

use super::super::{
    ApplyGuardState, apply_changeset, apply_changeset_with_hook, apply_changeset_with_hooks,
    capture_path_fingerprint, verify_apply_guard_state,
};
use super::create_python_target;

#[test]
fn apply_detects_path_swap_before_write() {
    let directory = tempdir().expect("tempdir should be created");
    let file_path = create_python_target(directory.path());
    let handles = parse_handles_for_file(&file_path).expect("handles should parse");
    let process_handle = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("process_data"))
        .expect("process_data handle should exist");

    let changeset = build_replace_changeset(
        &file_path,
        &process_handle.identity,
        "def process_data(value):\n    return value * 10".to_string(),
    )
    .expect("changeset should be created");

    let mut before_write = || -> Result<(), IdenteditError> {
        let swapped = directory.path().join("swapped.py");
        std::fs::write(
            &swapped,
            "def process_data(value):\n    return value - 1\n\n\ndef helper():\n    return \"helper\"\n",
        )
        .expect("swapped fixture should be written");
        std::fs::rename(&swapped, &file_path).expect("target file should be swapped");
        Ok(())
    };

    let error = apply_changeset_with_hook(&changeset, &mut before_write)
        .expect_err("path swap should be detected");
    match error {
        IdenteditError::PathChanged { path } => {
            assert_eq!(path, file_path.display().to_string());
        }
        other => panic!("unexpected error variant: {other}"),
    }

    let final_contents = std::fs::read_to_string(&file_path).expect("target should be readable");
    assert!(
        final_contents.contains("return value - 1"),
        "swapped file should remain in place"
    );
    assert!(
        !final_contents.contains("return value * 10"),
        "apply must not overwrite after detecting path swap"
    );
}

#[test]
fn apply_detects_alias_write_race_without_inode_swap() {
    let directory = tempdir().expect("tempdir should be created");
    let file_path = create_python_target(directory.path());
    let alias_path = directory.path().join("alias.py");
    std::fs::hard_link(&file_path, &alias_path).expect("hardlink alias should be created");
    let original_text = std::fs::read_to_string(&file_path).expect("fixture should be readable");
    let mutated_text = original_text.replacen("value + 1", "value - 9", 1);
    assert_eq!(
        original_text.len(),
        mutated_text.len(),
        "alias race fixture must preserve byte length to avoid fingerprint drift"
    );
    let original_modified_time = std::fs::metadata(&file_path)
        .expect("metadata should be readable")
        .modified()
        .expect("mtime should be readable");

    let handles = parse_handles_for_file(&file_path).expect("handles should parse");
    let process_handle = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("process_data"))
        .expect("process_data handle should exist");
    let changeset = build_replace_changeset(
        &file_path,
        &process_handle.identity,
        "def process_data(value):\n    return value * 10".to_string(),
    )
    .expect("changeset should be created");

    let mut before_write = || -> Result<(), IdenteditError> {
        std::fs::write(&alias_path, &mutated_text).expect("alias mutation should succeed");
        let alias_handle = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&alias_path)
            .expect("alias should be openable");
        alias_handle
            .set_times(FileTimes::new().set_modified(original_modified_time))
            .expect("mtime reset should succeed");
        Ok(())
    };

    let error = apply_changeset_with_hook(&changeset, &mut before_write)
        .expect_err("alias mutation should be detected as stale content");
    match error {
        IdenteditError::PreconditionFailed { .. } => {}
        other => panic!("unexpected error variant: {other}"),
    }

    let final_contents = std::fs::read_to_string(&file_path).expect("target should be readable");
    assert!(
        final_contents == mutated_text,
        "alias mutation should persist while apply aborts"
    );
    assert!(
        !final_contents.contains("return value * 10"),
        "apply must abort instead of overwriting stale file"
    );
}

#[cfg(unix)]
#[test]
fn apply_detects_symlinked_ancestor_alias_write_race_without_inode_swap() {
    use std::os::unix::fs::symlink;

    let directory = tempdir().expect("tempdir should be created");
    let real_dir = directory.path().join("real");
    std::fs::create_dir(&real_dir).expect("real directory should be created");
    let alias_dir = directory.path().join("alias");
    symlink(&real_dir, &alias_dir).expect("directory symlink should be created");

    let canonical_path = create_python_target(&real_dir);
    let alias_path = alias_dir.join("target.py");
    let original_text =
        std::fs::read_to_string(&alias_path).expect("alias fixture should be readable");
    let mutated_text = original_text.replacen("value + 1", "value - 9", 1);
    assert_eq!(
        original_text.len(),
        mutated_text.len(),
        "alias race fixture must preserve byte length to avoid fingerprint drift"
    );
    let original_modified_time = std::fs::metadata(&alias_path)
        .expect("metadata should be readable")
        .modified()
        .expect("mtime should be readable");

    let handles = parse_handles_for_file(&alias_path).expect("handles should parse");
    let process_handle = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("process_data"))
        .expect("process_data handle should exist");
    let changeset = build_replace_changeset(
        &alias_path,
        &process_handle.identity,
        "def process_data(value):\n    return value * 10".to_string(),
    )
    .expect("changeset should be created");

    let mut before_write = || -> Result<(), IdenteditError> {
        std::fs::write(&canonical_path, &mutated_text).expect("canonical mutation should succeed");
        let canonical_handle = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&canonical_path)
            .expect("canonical file should be openable");
        canonical_handle
            .set_times(FileTimes::new().set_modified(original_modified_time))
            .expect("mtime reset should succeed");
        Ok(())
    };

    let error = apply_changeset_with_hook(&changeset, &mut before_write)
        .expect_err("symlink alias mutation should be detected as stale content");
    match error {
        IdenteditError::PreconditionFailed { .. } => {}
        other => panic!("unexpected error variant: {other}"),
    }

    let final_contents = std::fs::read_to_string(&alias_path).expect("target should be readable");
    assert!(
        final_contents == mutated_text,
        "alias mutation should persist while apply aborts"
    );
    assert!(
        !final_contents.contains("return value * 10"),
        "apply must abort instead of overwriting stale file"
    );
}

#[test]
fn apply_detects_post_verify_mutation_before_atomic_write() {
    let directory = tempdir().expect("tempdir should be created");
    let file_path = create_python_target(directory.path());
    let original_text = std::fs::read_to_string(&file_path).expect("fixture should be readable");
    let mutated_text = original_text.replacen("value + 1", "value - 9", 1);
    assert_eq!(
        original_text.len(),
        mutated_text.len(),
        "fixture mutation should preserve byte length"
    );
    let original_modified_time = std::fs::metadata(&file_path)
        .expect("metadata should be readable")
        .modified()
        .expect("mtime should be readable");

    let handles = parse_handles_for_file(&file_path).expect("handles should parse");
    let process_handle = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("process_data"))
        .expect("process_data handle should exist");
    let changeset = build_replace_changeset(
        &file_path,
        &process_handle.identity,
        "def process_data(value):\n    return value * 10".to_string(),
    )
    .expect("changeset should be created");

    let mut after_verify = || -> Result<(), IdenteditError> {
        std::fs::write(&file_path, &mutated_text).expect("post-verify mutation should succeed");
        let file_handle = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&file_path)
            .expect("file should be openable");
        file_handle
            .set_times(FileTimes::new().set_modified(original_modified_time))
            .expect("mtime reset should succeed");
        Ok(())
    };
    let error = apply_changeset_with_hooks(&changeset, || Ok(()), &mut after_verify)
        .expect_err("post-verify mutation should be detected");
    match error {
        IdenteditError::PreconditionFailed { .. } => {}
        other => panic!("unexpected error variant: {other}"),
    }

    let final_contents = std::fs::read_to_string(&file_path).expect("target should be readable");
    assert_eq!(
        final_contents, mutated_text,
        "concurrent post-verify mutation should remain intact"
    );
    assert!(
        !final_contents.contains("return value * 10"),
        "apply must abort instead of clobbering concurrent post-verify edits"
    );
}

#[cfg(unix)]
#[test]
fn apply_detects_symlink_swap_after_verify_before_atomic_write() {
    use std::os::unix::fs::symlink;

    let directory = tempdir().expect("tempdir should be created");
    let file_path = create_python_target(directory.path());
    let symlink_target = directory.path().join("symlink-target.py");
    std::fs::write(
        &symlink_target,
        "def process_data(value):\n    return value - 3\n\n\ndef helper():\n    return \"alt\"\n",
    )
    .expect("symlink target fixture should be written");

    let handles = parse_handles_for_file(&file_path).expect("handles should parse");
    let process_handle = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("process_data"))
        .expect("process_data handle should exist");
    let changeset = build_replace_changeset(
        &file_path,
        &process_handle.identity,
        "def process_data(value):\n    return value * 10".to_string(),
    )
    .expect("changeset should be created");

    let mut after_verify = || -> Result<(), IdenteditError> {
        std::fs::remove_file(&file_path).expect("target file should be removed");
        symlink(&symlink_target, &file_path).expect("target path should be replaced by symlink");
        Ok(())
    };
    let error = apply_changeset_with_hooks(&changeset, || Ok(()), &mut after_verify)
        .expect_err("symlink swap after verify should be detected");
    match error {
        IdenteditError::InvalidRequest { message } => {
            assert!(
                message.contains("symbolic link"),
                "expected symbolic link rejection message"
            );
        }
        other => panic!("unexpected error variant: {other}"),
    }

    let symlink_contents =
        std::fs::read_to_string(&symlink_target).expect("symlink target should be readable");
    assert!(
        symlink_contents.contains("return value - 3"),
        "symlink target contents should remain unchanged"
    );
    let link_metadata = std::fs::symlink_metadata(&file_path).expect("swapped path should exist");
    assert!(
        link_metadata.file_type().is_symlink(),
        "swapped path should remain a symlink after aborted apply"
    );
}

#[cfg(unix)]
#[test]
fn apply_fails_atomically_when_directory_permissions_flip_mid_apply() {
    let directory = tempdir().expect("tempdir should be created");
    let file_path = create_python_target(directory.path());
    let before = std::fs::read_to_string(&file_path).expect("fixture should be readable");
    let parent = file_path.parent().expect("target should have parent");
    let original_permissions = std::fs::metadata(parent)
        .expect("parent metadata should be readable")
        .permissions();

    let handles = parse_handles_for_file(&file_path).expect("handles should parse");
    let process_handle = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("process_data"))
        .expect("process_data handle should exist");
    let changeset = build_replace_changeset(
        &file_path,
        &process_handle.identity,
        "def process_data(value):\n    return value * 10".to_string(),
    )
    .expect("changeset should be created");

    let mut before_write = || -> Result<(), IdenteditError> {
        let mut read_only_permissions = original_permissions.clone();
        read_only_permissions.set_mode(0o555);
        std::fs::set_permissions(parent, read_only_permissions)
            .expect("parent directory permissions should be flipped");
        Ok(())
    };

    let result = apply_changeset_with_hook(&changeset, &mut before_write);
    std::fs::set_permissions(parent, original_permissions)
        .expect("parent directory permissions should be restorable");

    let error = result.expect_err("apply should fail once directory becomes read-only");
    match error {
        IdenteditError::Io { .. } => {}
        other => panic!("unexpected error variant: {other}"),
    }

    let after = std::fs::read_to_string(&file_path).expect("target should remain readable");
    assert_eq!(
        before, after,
        "apply must remain atomic under permission-flip failure"
    );
}

#[test]
fn apply_failure_path_releases_file_lock_for_subsequent_retry() {
    let directory = tempdir().expect("tempdir should be created");
    let file_path = create_python_target(directory.path());
    let handles = parse_handles_for_file(&file_path).expect("handles should parse");
    let process_handle = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("process_data"))
        .expect("process_data handle should exist");
    let changeset = build_replace_changeset(
        &file_path,
        &process_handle.identity,
        "def process_data(value):\n    return value * 10".to_string(),
    )
    .expect("changeset should be created");

    let mut failing_hook = || -> Result<(), IdenteditError> {
        Err(IdenteditError::InvalidRequest {
            message: "injected hook failure".to_string(),
        })
    };
    let first_error = apply_changeset_with_hook(&changeset, &mut failing_hook)
        .expect_err("first attempt should fail via injected hook");
    assert!(
        first_error.to_string().contains("injected hook failure"),
        "expected injected hook failure to propagate"
    );

    let retry_response = apply_changeset(&changeset)
        .expect("retry should succeed if failed path released file lock");
    assert_eq!(retry_response.summary.operations_applied, 1);
}

#[test]
fn apply_reports_io_error_when_target_is_deleted_mid_flight() {
    let directory = tempdir().expect("tempdir should be created");
    let file_path = create_python_target(directory.path());
    let handles = parse_handles_for_file(&file_path).expect("handles should parse");
    let process_handle = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("process_data"))
        .expect("process_data handle should exist");
    let changeset = build_replace_changeset(
        &file_path,
        &process_handle.identity,
        "def process_data(value):\n    return value * 10".to_string(),
    )
    .expect("changeset should be created");

    let mut before_write = || -> Result<(), IdenteditError> {
        std::fs::remove_file(&file_path).expect("target should be deleted");
        Ok(())
    };
    let error = apply_changeset_with_hook(&changeset, &mut before_write)
        .expect_err("deleted target should fail apply");
    match error {
        IdenteditError::Io { .. } => {}
        other => panic!("unexpected error variant: {other}"),
    }

    assert!(
        !file_path.exists(),
        "apply should not recreate deleted target after failure"
    );
}

#[test]
fn apply_reports_path_changed_when_target_is_swapped_to_directory() {
    let directory = tempdir().expect("tempdir should be created");
    let file_path = create_python_target(directory.path());
    let handles = parse_handles_for_file(&file_path).expect("handles should parse");
    let process_handle = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("process_data"))
        .expect("process_data handle should exist");
    let changeset = build_replace_changeset(
        &file_path,
        &process_handle.identity,
        "def process_data(value):\n    return value * 10".to_string(),
    )
    .expect("changeset should be created");

    let mut before_write = || -> Result<(), IdenteditError> {
        std::fs::remove_file(&file_path).expect("target file should be removed");
        std::fs::create_dir(&file_path).expect("directory swap should succeed");
        Ok(())
    };
    let error = apply_changeset_with_hook(&changeset, &mut before_write)
        .expect_err("directory swap should fail apply");
    match error {
        IdenteditError::PathChanged { .. } => {}
        other => panic!("unexpected error variant: {other}"),
    }

    let metadata = std::fs::metadata(&file_path).expect("swapped path should still exist");
    assert!(
        metadata.is_dir(),
        "apply should not overwrite swapped directory path"
    );
}

#[test]
fn apply_guard_uses_hash_when_mtime_and_size_collide() {
    let directory = tempdir().expect("tempdir should be created");
    let file_path = directory.path().join("target.py");
    let original_text = "abcdef\n";
    let replacement_text = "ghijkl\n";
    assert_eq!(
        original_text.len(),
        replacement_text.len(),
        "fixture must preserve byte length to model coarse mtime/size collisions"
    );
    std::fs::write(&file_path, original_text).expect("fixture write should succeed");

    let original_fingerprint =
        capture_path_fingerprint(&file_path).expect("fingerprint should be captured");
    let original_modified_time = std::fs::metadata(&file_path)
        .expect("metadata should be readable")
        .modified()
        .expect("mtime should be readable");
    let expected = ApplyGuardState {
        path_fingerprint: original_fingerprint.clone(),
        source_hash: crate::changeset::hash_text(original_text),
    };

    std::fs::write(&file_path, replacement_text).expect("mutation should succeed");
    let file_handle = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&file_path)
        .expect("target should be openable");
    file_handle
        .set_times(FileTimes::new().set_modified(original_modified_time))
        .expect("mtime reset should succeed");

    let collided_fingerprint =
        capture_path_fingerprint(&file_path).expect("fingerprint should be recaptured");
    assert_eq!(
        collided_fingerprint, original_fingerprint,
        "test precondition failed: fingerprint should collide after mtime reset and same-size rewrite"
    );

    let error = verify_apply_guard_state(&file_path, &expected)
        .expect_err("hash guard should catch stale content under fingerprint collision");
    match error {
        IdenteditError::PreconditionFailed {
            expected_hash,
            actual_hash,
        } => {
            assert_eq!(expected_hash, crate::changeset::hash_text(original_text));
            assert_eq!(actual_hash, crate::changeset::hash_text(replacement_text));
        }
        other => panic!("unexpected guard result: {other}"),
    }
}
