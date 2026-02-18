#[cfg(unix)]
use std::os::unix::fs::symlink;

use tempfile::tempdir;

use crate::changeset::MultiFileChangeset;
use crate::transform::{build_replace_changeset, parse_handles_for_file};

use super::super::{apply_changeset, apply_multi_file_changeset};
use super::create_python_target;

#[cfg(unix)]
#[test]
fn apply_rejects_symbolic_link_target() {
    let directory = tempdir().expect("tempdir should be created");
    let real_target = create_python_target(directory.path());
    let symlink_path = directory.path().join("target-link.py");
    symlink(&real_target, &symlink_path).expect("symlink should be created");

    let changeset = crate::changeset::FileChange {
        file: symlink_path.clone(),
        operations: vec![],
    };

    let error = apply_changeset(&changeset).expect_err("symlink target should be rejected");
    assert!(
        error.to_string().contains("symbolic link"),
        "expected symbolic link rejection message"
    );
}

#[cfg(unix)]
#[test]
fn apply_allows_symlinked_ancestor_when_target_entry_is_regular_file() {
    let directory = tempdir().expect("tempdir should be created");
    let real_dir = directory.path().join("real");
    std::fs::create_dir(&real_dir).expect("real directory should be created");
    let link_dir = directory.path().join("link");
    symlink(&real_dir, &link_dir).expect("directory symlink should be created");

    let target = real_dir.join("target.py");
    std::fs::write(
        &target,
        "def process_data(value):\n    return value + 1\n\n\ndef helper():\n    return \"helper\"\n",
    )
    .expect("fixture write should succeed");

    let linked_path = link_dir.join("target.py");
    let handles = parse_handles_for_file(&linked_path).expect("handles should parse");
    let process_handle = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("process_data"))
        .expect("process_data handle should exist");
    let changeset = build_replace_changeset(
        &linked_path,
        &process_handle.identity,
        "def process_data(value):\n    return value * 10".to_string(),
    )
    .expect("changeset should be created");

    let response = apply_changeset(&changeset).expect("apply should succeed");
    assert_eq!(response.summary.operations_applied, 1);
}

#[cfg(unix)]
#[test]
fn apply_multi_file_rejects_symlink_target_without_mutating_regular_siblings() {
    let directory = tempdir().expect("tempdir should be created");

    let regular_target = create_python_target(directory.path());
    let regular_before =
        std::fs::read_to_string(&regular_target).expect("regular fixture should be readable");
    let regular_handles = parse_handles_for_file(&regular_target).expect("handles should parse");
    let regular_process = regular_handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("process_data"))
        .expect("process_data handle should exist");
    let regular_changeset = build_replace_changeset(
        &regular_target,
        &regular_process.identity,
        "def process_data(value):\n    return value * 77".to_string(),
    )
    .expect("regular changeset should be created");

    let symlink_target = directory.path().join("symlink-target.py");
    std::fs::write(
        &symlink_target,
        "def process_data(value):\n    return value + 1\n\n\ndef helper():\n    return \"helper\"\n",
    )
    .expect("symlink target fixture should be written");
    let symlink_path = directory.path().join("target-link.py");
    symlink(&symlink_target, &symlink_path).expect("symlink should be created");
    let symlink_changeset = crate::changeset::FileChange {
        file: symlink_path,
        operations: vec![],
    };

    let multi = MultiFileChangeset {
        files: vec![regular_changeset, symlink_changeset],
        transaction: Default::default(),
    };

    let error = apply_multi_file_changeset(&multi)
        .expect_err("multi-file apply should reject symbolic link entry");
    assert!(
        error.to_string().contains("symbolic link"),
        "expected symbolic link rejection in multi-file preflight"
    );

    let regular_after =
        std::fs::read_to_string(&regular_target).expect("regular file should remain readable");
    assert_eq!(
        regular_after, regular_before,
        "symlink preflight rejection must leave regular sibling untouched"
    );
}
