use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use tempfile::tempdir;

use crate::changeset::{OpKind, TransformTarget, hash_text};
use crate::error::IdenteditError;
use crate::transform::{
    TransformInstruction, build_changeset, build_delete_changeset, build_replace_changeset,
    parse_handles_for_file,
};

use super::super::{acquire_apply_lock, apply_changeset, apply_changeset_with_hook};

#[test]
fn apply_lock_rejects_second_concurrent_holder_on_same_file() {
    let directory = tempdir().expect("tempdir should be created");
    let file_path = directory.path().join("target.py");
    std::fs::write(&file_path, "def a():\n    return 1").expect("fixture write should succeed");

    let first_lock = acquire_apply_lock(&file_path).expect("first lock should succeed");
    let second_attempt = acquire_apply_lock(&file_path);
    let error = second_attempt.expect_err("second lock should fail while first lock is held");

    match error {
        IdenteditError::ResourceBusy { path } => {
            assert_eq!(path, file_path.display().to_string());
        }
        other => panic!("unexpected lock error variant: {other}"),
    }

    drop(first_lock);

    let third_attempt = acquire_apply_lock(&file_path);
    assert!(
        third_attempt.is_ok(),
        "lock should be acquirable after previous holder is dropped"
    );
}

#[test]
fn apply_lock_allows_independent_files_in_parallel() {
    let directory = tempdir().expect("tempdir should be created");
    let first_file = directory.path().join("first.py");
    let second_file = directory.path().join("second.py");
    std::fs::write(&first_file, "def first():\n    return 1")
        .expect("first fixture write should succeed");
    std::fs::write(&second_file, "def second():\n    return 2")
        .expect("second fixture write should succeed");

    let first_lock = acquire_apply_lock(&first_file).expect("first file lock should succeed");
    let second_lock = acquire_apply_lock(&second_file).expect("second file lock should succeed");

    drop(first_lock);
    drop(second_lock);
}

#[test]
fn apply_lock_rejects_hardlink_alias_for_same_underlying_file() {
    let directory = tempdir().expect("tempdir should be created");
    let canonical = directory.path().join("canonical.py");
    let alias = directory.path().join("alias.py");
    std::fs::write(&canonical, "def value():\n    return 1").expect("fixture write should succeed");
    std::fs::hard_link(&canonical, &alias).expect("hard link should be created");

    let first_lock = acquire_apply_lock(&canonical).expect("first lock should succeed");
    let second_attempt = acquire_apply_lock(&alias);
    let error = second_attempt.expect_err("hardlink alias should contend for the same file lock");

    match error {
        IdenteditError::ResourceBusy { path } => {
            assert_eq!(path, alias.display().to_string());
        }
        other => panic!("unexpected lock error variant: {other}"),
    }

    drop(first_lock);
}

#[test]
fn apply_on_hardlinked_target_splits_alias_after_atomic_rename() {
    let directory = tempdir().expect("tempdir should be created");
    let canonical = directory.path().join("canonical.py");
    let alias = directory.path().join("alias.py");
    std::fs::write(
        &canonical,
        "def process_data(value):\n    return value + 1\n\n\ndef helper():\n    return \"helper\"\n",
    )
    .expect("fixture write should succeed");
    std::fs::hard_link(&canonical, &alias).expect("hard link should be created");

    let handles = parse_handles_for_file(&canonical).expect("handles should parse");
    let process_handle = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("process_data"))
        .expect("process_data handle should exist");
    let changeset = build_replace_changeset(
        &canonical,
        &process_handle.identity,
        "def process_data(value):\n    return value * 10".to_string(),
    )
    .expect("changeset should be created");

    apply_changeset(&changeset).expect("apply should succeed on canonical path");

    let canonical_contents =
        std::fs::read_to_string(&canonical).expect("canonical should be readable");
    let alias_contents = std::fs::read_to_string(&alias).expect("alias should be readable");
    assert!(
        canonical_contents.contains("return value * 10"),
        "canonical path should reflect replacement"
    );
    assert!(
        alias_contents.contains("return value + 1"),
        "hardlink alias should remain on previous inode contents"
    );
    assert_ne!(
        canonical_contents, alias_contents,
        "atomic rename should split canonical path from hardlink alias"
    );
}

#[test]
fn apply_lock_rejects_dot_segment_alias_for_same_file() {
    let directory = tempdir().expect("tempdir should be created");
    let nested = directory.path().join("nested");
    std::fs::create_dir(&nested).expect("nested directory should be created");
    let canonical = nested.join("target.py");
    std::fs::write(&canonical, "def value():\n    return 1").expect("fixture write should succeed");
    let alias = nested.join("..").join("nested").join("target.py");

    let first_lock = acquire_apply_lock(&canonical).expect("first lock should succeed");
    let second_attempt = acquire_apply_lock(&alias);
    let error =
        second_attempt.expect_err("dot-segment alias should contend for the same file lock");

    match error {
        IdenteditError::ResourceBusy { path } => {
            assert_eq!(path, alias.display().to_string());
        }
        other => panic!("unexpected lock error variant: {other}"),
    }

    drop(first_lock);
}

#[cfg(unix)]
#[test]
fn apply_lock_rejects_symlinked_ancestor_alias_for_same_file() {
    use std::os::unix::fs::symlink;

    let directory = tempdir().expect("tempdir should be created");
    let real_dir = directory.path().join("real");
    std::fs::create_dir(&real_dir).expect("real directory should be created");
    let alias_dir = directory.path().join("alias");
    symlink(&real_dir, &alias_dir).expect("directory symlink should be created");

    let canonical = real_dir.join("target.py");
    let alias = alias_dir.join("target.py");
    std::fs::write(&canonical, "def value():\n    return 1").expect("fixture write should succeed");

    let first_lock = acquire_apply_lock(&canonical).expect("first lock should succeed");
    let second_attempt = acquire_apply_lock(&alias);
    let error =
        second_attempt.expect_err("symlinked-ancestor alias should contend for the same lock");

    match error {
        IdenteditError::ResourceBusy { path } => {
            assert_eq!(path, alias.display().to_string());
        }
        other => panic!("unexpected lock error variant: {other}"),
    }

    drop(first_lock);
}

#[test]
fn concurrent_apply_with_delete_and_insert_has_single_winner_and_resource_busy_loser() {
    let directory = tempdir().expect("tempdir should be created");
    let file_path = directory.path().join("target.py");
    std::fs::write(
        &file_path,
        "def process_data(value):\n    return value + 1\n\n\ndef helper():\n    return \"helper\"\n",
    )
    .expect("fixture write should succeed");

    let handles = parse_handles_for_file(&file_path).expect("handles should parse");
    let process_handle = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("process_data"))
        .expect("process_data handle should exist");

    let delete_changeset = build_delete_changeset(&file_path, &process_handle.identity)
        .expect("delete changeset should be built");
    let insert_changeset = build_changeset(
        &file_path,
        vec![TransformInstruction {
            target: TransformTarget::node(
                process_handle.identity.clone(),
                process_handle.kind.clone(),
                Some(process_handle.span),
                hash_text(&process_handle.text),
            ),
            op: OpKind::InsertBefore {
                new_text: "# inserted-by-loser\n".to_string(),
            },
        }],
    )
    .expect("insert changeset should be built");

    let (hook_entered_tx, hook_entered_rx) = mpsc::channel::<()>();
    let (release_hook_tx, release_hook_rx) = mpsc::channel::<()>();
    let delete_worker = thread::spawn(move || {
        apply_changeset_with_hook(&delete_changeset, || {
            hook_entered_tx
                .send(())
                .expect("hook entry signal should be sent");
            release_hook_rx
                .recv_timeout(Duration::from_secs(2))
                .expect("hook release signal should be received");
            Ok(())
        })
    });

    hook_entered_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("delete worker should enter hook while lock is held");

    let loser_error =
        apply_changeset(&insert_changeset).expect_err("concurrent apply should lose lock race");
    match loser_error {
        IdenteditError::ResourceBusy { path } => {
            assert_eq!(path, file_path.display().to_string());
        }
        other => panic!("unexpected concurrent apply loser error: {other}"),
    }

    release_hook_tx
        .send(())
        .expect("release signal should be sent");
    let winner_result = delete_worker
        .join()
        .expect("delete worker should not panic");
    winner_result.expect("delete worker should complete successfully");

    let updated = std::fs::read_to_string(&file_path).expect("file should be readable");
    assert!(
        !updated.contains("def process_data"),
        "winner delete apply should remove process_data function"
    );
    assert!(
        !updated.contains("# inserted-by-loser"),
        "loser insert apply must not modify file"
    );
}

#[test]
fn concurrent_apply_loser_retry_returns_target_missing_after_winner_commits() {
    let directory = tempdir().expect("tempdir should be created");
    let file_path = directory.path().join("target.py");
    std::fs::write(
        &file_path,
        "def process_data(value):\n    return value + 1\n\n\ndef helper():\n    return \"helper\"\n",
    )
    .expect("fixture write should succeed");

    let handles = parse_handles_for_file(&file_path).expect("handles should parse");
    let process_handle = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("process_data"))
        .expect("process_data handle should exist");
    let expected_identity = process_handle.identity.clone();

    let delete_changeset = build_delete_changeset(&file_path, &process_handle.identity)
        .expect("delete changeset should be built");
    let insert_changeset = build_changeset(
        &file_path,
        vec![TransformInstruction {
            target: TransformTarget::node(
                process_handle.identity.clone(),
                process_handle.kind.clone(),
                Some(process_handle.span),
                hash_text(&process_handle.text),
            ),
            op: OpKind::InsertBefore {
                new_text: "# inserted-by-loser\n".to_string(),
            },
        }],
    )
    .expect("insert changeset should be built");

    let (hook_entered_tx, hook_entered_rx) = mpsc::channel::<()>();
    let (release_hook_tx, release_hook_rx) = mpsc::channel::<()>();
    let delete_worker = thread::spawn(move || {
        apply_changeset_with_hook(&delete_changeset, || {
            hook_entered_tx
                .send(())
                .expect("hook entry signal should be sent");
            release_hook_rx
                .recv_timeout(Duration::from_secs(2))
                .expect("hook release signal should be received");
            Ok(())
        })
    });

    hook_entered_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("delete worker should enter hook while lock is held");

    let first_error = apply_changeset(&insert_changeset)
        .expect_err("first loser attempt should fail with resource busy");
    match first_error {
        IdenteditError::ResourceBusy { path } => {
            assert_eq!(path, file_path.display().to_string());
        }
        other => panic!("unexpected first loser error: {other}"),
    }

    release_hook_tx
        .send(())
        .expect("release signal should be sent");
    delete_worker
        .join()
        .expect("delete worker should not panic")
        .expect("delete worker should complete successfully");

    let retry_error =
        apply_changeset(&insert_changeset).expect_err("retry should fail after anchor deletion");
    match retry_error {
        IdenteditError::TargetMissing { identity, file } => {
            assert_eq!(identity, expected_identity);
            assert_eq!(file, file_path.display().to_string());
        }
        other => panic!("retry should fail with semantic stale-target error, got: {other}"),
    }
}
