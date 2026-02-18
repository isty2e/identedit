use tempfile::tempdir;

use crate::changeset::{ChangeOp, ChangePreview, FileChange, OpKind, TransformTarget};
use crate::error::IdenteditError;
use crate::handle::Span;
use crate::provider::ProviderRegistry;
use crate::transform::{build_replace_changeset, parse_handles_for_file};

use super::super::{
    ApplyFileStatus, FileRollbackSnapshot, acquire_apply_lock, apply_changesets_with_hooks,
    commit_move_plan_with_rename, commit_preflight_batch, preflight_changesets_in_order,
    preflight_move_plans, prepare_commit_batch, rollback_committed_files,
    validate_move_operation_constraints,
};
use super::create_python_target;
use std::fs::FileTimes;

#[cfg(unix)]
use std::os::unix::fs::symlink;

fn process_identity_for(file_path: &std::path::Path) -> String {
    let handles = parse_handles_for_file(file_path).expect("handles should parse");
    handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("process_data"))
        .expect("process_data handle should exist")
        .identity
        .clone()
}

fn build_move_changeset(source: &std::path::Path, destination: &std::path::Path) -> FileChange {
    FileChange {
        file: source.to_path_buf(),
        operations: vec![ChangeOp {
            target: TransformTarget::node(
                "move-placeholder".to_string(),
                "file".to_string(),
                None,
                "move-placeholder".to_string(),
            ),
            op: OpKind::Move {
                to: destination.to_path_buf(),
            },
            preview: ChangePreview {
                old_text: Some(String::new()),
                old_hash: None,
                old_len: None,
                new_text: String::new(),
                matched_span: Span { start: 0, end: 0 },
                move_preview: Some(crate::changeset::MovePreview {
                    from: source.to_path_buf(),
                    to: destination.to_path_buf(),
                }),
            },
        }],
    }
}

#[test]
fn preflight_builds_file_plans_without_mutating_sources() {
    let directory = tempdir().expect("tempdir should be created");
    let file_a = create_python_target(directory.path());
    let file_b = directory.path().join("target_b.py");
    std::fs::write(
        &file_b,
        "def process_data(value):\n    result = value + 1\n    return result\n\n\ndef helper():\n    return \"helper\"\n",
    )
    .expect("fixture write should succeed");

    let before_a = std::fs::read_to_string(&file_a).expect("file_a should be readable");
    let before_b = std::fs::read_to_string(&file_b).expect("file_b should be readable");
    let changeset_a = build_replace_changeset(
        &file_a,
        &process_identity_for(&file_a),
        "def process_data(value):\n    return value * 10".to_string(),
    )
    .expect("changeset_a should be built");
    let changeset_b = build_replace_changeset(
        &file_b,
        &process_identity_for(&file_b),
        "def process_data(value):\n    return value * 11".to_string(),
    )
    .expect("changeset_b should be built");

    let registry = ProviderRegistry::default();
    let plans = preflight_changesets_in_order(&[changeset_a, changeset_b], &registry)
        .expect("preflight should succeed for both files");

    assert_eq!(plans.len(), 2);
    assert_eq!(plans[0].file, file_a);
    assert_eq!(plans[1].file, file_b);
    assert!(
        plans[0].updated_text.contains("return value * 10"),
        "first preflight plan should include replacement"
    );
    assert!(
        plans[1].updated_text.contains("return value * 11"),
        "second preflight plan should include replacement"
    );

    let after_a = std::fs::read_to_string(&file_a).expect("file_a should remain readable");
    let after_b = std::fs::read_to_string(&file_b).expect("file_b should remain readable");
    assert_eq!(after_a, before_a, "preflight must not write file_a");
    assert_eq!(after_b, before_b, "preflight must not write file_b");
}

#[test]
fn preflight_fails_fast_and_preserves_all_file_contents() {
    let directory = tempdir().expect("tempdir should be created");
    let file_a = create_python_target(directory.path());
    let file_b = directory.path().join("target_b.py");
    std::fs::write(
        &file_b,
        "def process_data(value):\n    result = value + 1\n    return result\n\n\ndef helper():\n    return \"helper\"\n",
    )
    .expect("fixture write should succeed");

    let before_a = std::fs::read_to_string(&file_a).expect("file_a should be readable");
    let before_b = std::fs::read_to_string(&file_b).expect("file_b should be readable");
    let changeset_a = build_replace_changeset(
        &file_a,
        &process_identity_for(&file_a),
        "def process_data(value):\n    return value * 10".to_string(),
    )
    .expect("changeset_a should be built");
    let mut changeset_b = build_replace_changeset(
        &file_b,
        &process_identity_for(&file_b),
        "def process_data(value):\n    return value * 11".to_string(),
    )
    .expect("changeset_b should be built");
    let stale_span_hint = match &changeset_b.operations[0].target {
        TransformTarget::Node { span_hint, .. } => *span_hint,
        _ => None,
    };
    changeset_b.operations[0].target = TransformTarget::node(
        "missing-preflight-identity".to_string(),
        "function_definition".to_string(),
        stale_span_hint,
        "stale-hash".to_string(),
    );

    let registry = ProviderRegistry::default();
    let error = preflight_changesets_in_order(&[changeset_a, changeset_b], &registry)
        .expect_err("preflight should fail when one file has an unresolved target");
    match error {
        IdenteditError::PreconditionFailed { .. } => {}
        other => panic!("unexpected error variant: {other}"),
    }

    let after_a = std::fs::read_to_string(&file_a).expect("file_a should remain readable");
    let after_b = std::fs::read_to_string(&file_b).expect("file_b should remain readable");
    assert_eq!(after_a, before_a, "preflight failure must not write file_a");
    assert_eq!(after_b, before_b, "preflight failure must not write file_b");
}

#[test]
fn preflight_holds_file_locks_until_plans_are_dropped() {
    let directory = tempdir().expect("tempdir should be created");
    let file_path = create_python_target(directory.path());
    let changeset = build_replace_changeset(
        &file_path,
        &process_identity_for(&file_path),
        "def process_data(value):\n    return value * 42".to_string(),
    )
    .expect("changeset should be built");

    let registry = ProviderRegistry::default();
    let plans =
        preflight_changesets_in_order(&[changeset], &registry).expect("preflight should succeed");
    assert_eq!(plans.len(), 1);

    let lock_error =
        acquire_apply_lock(&file_path).expect_err("preflight lock should block concurrent lock");
    match lock_error {
        IdenteditError::ResourceBusy { path } => {
            assert_eq!(path, file_path.display().to_string());
        }
        other => panic!("unexpected lock error variant: {other}"),
    }

    drop(plans);

    let post_drop_lock = acquire_apply_lock(&file_path);
    assert!(
        post_drop_lock.is_ok(),
        "lock should be acquirable after preflight plans are dropped"
    );
}

#[test]
fn preflight_orders_plans_by_canonical_path_not_input_order() {
    let directory = tempdir().expect("tempdir should be created");
    let file_a = directory.path().join("a_target.py");
    let file_b = directory.path().join("b_target.py");
    let fixture = "def process_data(value):\n    result = value + 1\n    return result\n\n\ndef helper():\n    return \"helper\"\n";
    std::fs::write(&file_a, fixture).expect("file_a fixture write should succeed");
    std::fs::write(&file_b, fixture).expect("file_b fixture write should succeed");

    let changeset_b = build_replace_changeset(
        &file_b,
        &process_identity_for(&file_b),
        "def process_data(value):\n    return value * 20".to_string(),
    )
    .expect("changeset_b should be built");
    let changeset_a = build_replace_changeset(
        &file_a,
        &process_identity_for(&file_a),
        "def process_data(value):\n    return value * 21".to_string(),
    )
    .expect("changeset_a should be built");

    let registry = ProviderRegistry::default();
    let plans = preflight_changesets_in_order(&[changeset_b, changeset_a], &registry)
        .expect("preflight should succeed");
    assert_eq!(plans.len(), 2);
    assert_eq!(
        plans[0].file, file_a,
        "preflight plans should be ordered by canonical path, not input order"
    );
    assert_eq!(plans[1].file, file_b);
}

#[test]
fn preflight_rejects_duplicate_logical_file_entries_before_locking() {
    let directory = tempdir().expect("tempdir should be created");
    let canonical = directory.path().join("target.py");
    std::fs::write(
        &canonical,
        "def process_data(value):\n    result = value + 1\n    return result\n\n\ndef helper():\n    return \"helper\"\n",
    )
    .expect("fixture write should succeed");

    let dot_alias = directory
        .path()
        .join(".")
        .join(canonical.file_name().expect("file name should exist"));
    let changeset_a = build_replace_changeset(
        &canonical,
        &process_identity_for(&canonical),
        "def process_data(value):\n    return value * 30".to_string(),
    )
    .expect("changeset_a should be built");
    let changeset_b = build_replace_changeset(
        &dot_alias,
        &process_identity_for(&dot_alias),
        "def process_data(value):\n    return value * 31".to_string(),
    )
    .expect("changeset_b should be built");

    let registry = ProviderRegistry::default();
    let error = preflight_changesets_in_order(&[changeset_a, changeset_b], &registry)
        .expect_err("duplicate logical path entries should be rejected");
    match error {
        IdenteditError::InvalidRequest { message } => {
            assert!(
                message.contains("Duplicate file entry in changeset.files"),
                "unexpected duplicate-entry message: {message}"
            );
        }
        other => panic!("unexpected error variant: {other}"),
    }
}

#[cfg(unix)]
#[test]
fn preflight_rejects_duplicate_entries_for_hardlink_aliases() {
    let directory = tempdir().expect("tempdir should be created");
    let canonical = directory.path().join("target.py");
    std::fs::write(
        &canonical,
        "def process_data(value):\n    result = value + 1\n    return result\n\n\ndef helper():\n    return \"helper\"\n",
    )
    .expect("fixture write should succeed");
    let hardlink_alias = directory.path().join("target_alias.py");
    std::fs::hard_link(&canonical, &hardlink_alias).expect("hardlink alias should be created");

    let changeset_a = build_replace_changeset(
        &canonical,
        &process_identity_for(&canonical),
        "def process_data(value):\n    return value * 34".to_string(),
    )
    .expect("changeset_a should be built");
    let changeset_b = build_replace_changeset(
        &hardlink_alias,
        &process_identity_for(&hardlink_alias),
        "def process_data(value):\n    return value * 35".to_string(),
    )
    .expect("changeset_b should be built");

    let registry = ProviderRegistry::default();
    let error = preflight_changesets_in_order(&[changeset_a, changeset_b], &registry)
        .expect_err("hardlink alias entries should be rejected as duplicate file entries");
    match error {
        IdenteditError::InvalidRequest { message } => {
            assert!(
                message.contains("Duplicate file entry in changeset.files"),
                "unexpected duplicate-entry message: {message}"
            );
        }
        other => panic!("unexpected error variant: {other}"),
    }
}

#[cfg(unix)]
#[test]
fn preflight_rejects_duplicate_entries_for_symlinked_dot_segment_aliases() {
    let directory = tempdir().expect("tempdir should be created");
    let real_dir = directory.path().join("real");
    std::fs::create_dir(&real_dir).expect("real directory should be created");
    let alias_dir = directory.path().join("alias");
    symlink(&real_dir, &alias_dir).expect("directory symlink should be created");

    let canonical = real_dir.join("target.py");
    std::fs::write(
        &canonical,
        "def process_data(value):\n    result = value + 1\n    return result\n\n\ndef helper():\n    return \"helper\"\n",
    )
    .expect("fixture write should succeed");

    let symlink_dot_alias = alias_dir
        .join("..")
        .join("alias")
        .join(canonical.file_name().expect("file name should exist"));
    let changeset_a = build_replace_changeset(
        &canonical,
        &process_identity_for(&canonical),
        "def process_data(value):\n    return value * 32".to_string(),
    )
    .expect("changeset_a should be built");
    let changeset_b = build_replace_changeset(
        &symlink_dot_alias,
        &process_identity_for(&symlink_dot_alias),
        "def process_data(value):\n    return value * 33".to_string(),
    )
    .expect("changeset_b should be built");

    let registry = ProviderRegistry::default();
    let error = preflight_changesets_in_order(&[changeset_a, changeset_b], &registry)
        .expect_err("duplicate canonical entries through symlink+dot aliases should be rejected");
    match error {
        IdenteditError::InvalidRequest { message } => {
            assert!(
                message.contains("Duplicate file entry in changeset.files"),
                "unexpected duplicate-entry message: {message}"
            );
        }
        other => panic!("unexpected error variant: {other}"),
    }
}

#[cfg(unix)]
#[test]
fn preflight_rejects_non_adjacent_hardlink_alias_entries() {
    let directory = tempdir().expect("tempdir should be created");
    let fixture = "def process_data(value):\n    result = value + 1\n    return result\n\n\ndef helper():\n    return \"helper\"\n";
    let canonical = directory.path().join("a_target.py");
    let middle = directory.path().join("m_middle.py");
    let hardlink_alias = directory.path().join("z_alias.py");
    std::fs::write(&canonical, fixture).expect("canonical fixture write should succeed");
    std::fs::write(&middle, fixture).expect("middle fixture write should succeed");
    std::fs::hard_link(&canonical, &hardlink_alias).expect("hardlink alias should be created");

    let before_canonical =
        std::fs::read_to_string(&canonical).expect("canonical should be readable");
    let before_middle = std::fs::read_to_string(&middle).expect("middle should be readable");
    let before_alias = std::fs::read_to_string(&hardlink_alias).expect("alias should be readable");

    let changeset_middle = build_replace_changeset(
        &middle,
        &process_identity_for(&middle),
        "def process_data(value):\n    return value * 40".to_string(),
    )
    .expect("middle changeset should be built");
    let changeset_alias = build_replace_changeset(
        &hardlink_alias,
        &process_identity_for(&hardlink_alias),
        "def process_data(value):\n    return value * 41".to_string(),
    )
    .expect("alias changeset should be built");
    let changeset_canonical = build_replace_changeset(
        &canonical,
        &process_identity_for(&canonical),
        "def process_data(value):\n    return value * 42".to_string(),
    )
    .expect("canonical changeset should be built");

    let registry = ProviderRegistry::default();
    let error = preflight_changesets_in_order(
        &[changeset_middle, changeset_alias, changeset_canonical],
        &registry,
    )
    .expect_err("non-adjacent hardlink alias entries should be rejected");

    match error {
        IdenteditError::InvalidRequest { message } => {
            assert!(
                message.contains("Duplicate file entry in changeset.files"),
                "unexpected duplicate-entry message: {message}"
            );
        }
        other => panic!("unexpected error variant: {other}"),
    }

    let after_canonical =
        std::fs::read_to_string(&canonical).expect("canonical should be readable");
    let after_middle = std::fs::read_to_string(&middle).expect("middle should be readable");
    let after_alias = std::fs::read_to_string(&hardlink_alias).expect("alias should be readable");
    assert_eq!(after_canonical, before_canonical);
    assert_eq!(after_middle, before_middle);
    assert_eq!(after_alias, before_alias);
}

#[cfg(unix)]
#[test]
fn preflight_rejects_non_adjacent_hardlink_alias_entries_before_locking_middle_file() {
    let directory = tempdir().expect("tempdir should be created");
    let fixture = "def process_data(value):\n    result = value + 1\n    return result\n\n\ndef helper():\n    return \"helper\"\n";
    let canonical = directory.path().join("a_target.py");
    let middle = directory.path().join("m_middle.py");
    let hardlink_alias = directory.path().join("z_alias.py");
    std::fs::write(&canonical, fixture).expect("canonical fixture write should succeed");
    std::fs::write(&middle, fixture).expect("middle fixture write should succeed");
    std::fs::hard_link(&canonical, &hardlink_alias).expect("hardlink alias should be created");

    let changeset_canonical = build_replace_changeset(
        &canonical,
        &process_identity_for(&canonical),
        "def process_data(value):\n    return value * 43".to_string(),
    )
    .expect("canonical changeset should be built");
    let changeset_middle = build_replace_changeset(
        &middle,
        &process_identity_for(&middle),
        "def process_data(value):\n    return value * 44".to_string(),
    )
    .expect("middle changeset should be built");
    let changeset_alias = build_replace_changeset(
        &hardlink_alias,
        &process_identity_for(&hardlink_alias),
        "def process_data(value):\n    return value * 45".to_string(),
    )
    .expect("alias changeset should be built");

    let _middle_lock = acquire_apply_lock(&middle).expect("middle lock should be acquired");
    let registry = ProviderRegistry::default();
    let error = preflight_changesets_in_order(
        &[changeset_canonical, changeset_middle, changeset_alias],
        &registry,
    )
    .expect_err("duplicate detection should fail before attempting middle lock");

    match error {
        IdenteditError::InvalidRequest { message } => {
            assert!(
                message.contains("Duplicate file entry in changeset.files"),
                "unexpected duplicate-entry message: {message}"
            );
        }
        other => panic!("unexpected error variant: {other}"),
    }
}

#[cfg(unix)]
#[test]
fn preflight_duplicate_hardlink_diagnostic_is_input_order_independent_with_non_adjacent_entries() {
    let directory = tempdir().expect("tempdir should be created");
    let fixture = "def process_data(value):\n    result = value + 1\n    return result\n\n\ndef helper():\n    return \"helper\"\n";
    let canonical = directory.path().join("a_target.py");
    let middle = directory.path().join("m_middle.py");
    let hardlink_alias = directory.path().join("z_alias.py");
    std::fs::write(&canonical, fixture).expect("canonical fixture write should succeed");
    std::fs::write(&middle, fixture).expect("middle fixture write should succeed");
    std::fs::hard_link(&canonical, &hardlink_alias).expect("hardlink alias should be created");

    let changeset_canonical = build_replace_changeset(
        &canonical,
        &process_identity_for(&canonical),
        "def process_data(value):\n    return value * 46".to_string(),
    )
    .expect("canonical changeset should be built");
    let changeset_middle = build_replace_changeset(
        &middle,
        &process_identity_for(&middle),
        "def process_data(value):\n    return value * 47".to_string(),
    )
    .expect("middle changeset should be built");
    let changeset_alias = build_replace_changeset(
        &hardlink_alias,
        &process_identity_for(&hardlink_alias),
        "def process_data(value):\n    return value * 48".to_string(),
    )
    .expect("alias changeset should be built");

    let registry = ProviderRegistry::default();
    let first = preflight_changesets_in_order(
        &[
            changeset_middle.clone(),
            changeset_alias.clone(),
            changeset_canonical.clone(),
        ],
        &registry,
    )
    .expect_err("first permutation should reject duplicate alias entries");
    let second = preflight_changesets_in_order(
        &[changeset_canonical, changeset_middle, changeset_alias],
        &registry,
    )
    .expect_err("second permutation should reject duplicate alias entries");

    let first_message = match first {
        IdenteditError::InvalidRequest { message } => message,
        other => panic!("unexpected first error variant: {other}"),
    };
    let second_message = match second {
        IdenteditError::InvalidRequest { message } => message,
        other => panic!("unexpected second error variant: {other}"),
    };
    assert!(
        first_message.contains("Duplicate file entry in changeset.files"),
        "unexpected duplicate-entry message: {first_message}"
    );
    assert_eq!(
        first_message, second_message,
        "duplicate-entry diagnostics should be deterministic across permutations"
    );
}

#[cfg(unix)]
#[test]
fn preflight_rejects_three_non_adjacent_hardlink_alias_entries() {
    let directory = tempdir().expect("tempdir should be created");
    let fixture = "def process_data(value):\n    result = value + 1\n    return result\n\n\ndef helper():\n    return \"helper\"\n";
    let canonical = directory.path().join("a_target.py");
    let middle = directory.path().join("m_middle.py");
    let alias_b = directory.path().join("b_alias.py");
    let alias_z = directory.path().join("z_alias.py");
    std::fs::write(&canonical, fixture).expect("canonical fixture write should succeed");
    std::fs::write(&middle, fixture).expect("middle fixture write should succeed");
    std::fs::hard_link(&canonical, &alias_b).expect("first hardlink alias should be created");
    std::fs::hard_link(&canonical, &alias_z).expect("second hardlink alias should be created");

    let changeset_alias_z = build_replace_changeset(
        &alias_z,
        &process_identity_for(&alias_z),
        "def process_data(value):\n    return value * 49".to_string(),
    )
    .expect("alias_z changeset should be built");
    let changeset_middle = build_replace_changeset(
        &middle,
        &process_identity_for(&middle),
        "def process_data(value):\n    return value * 50".to_string(),
    )
    .expect("middle changeset should be built");
    let changeset_alias_b = build_replace_changeset(
        &alias_b,
        &process_identity_for(&alias_b),
        "def process_data(value):\n    return value * 51".to_string(),
    )
    .expect("alias_b changeset should be built");
    let changeset_canonical = build_replace_changeset(
        &canonical,
        &process_identity_for(&canonical),
        "def process_data(value):\n    return value * 52".to_string(),
    )
    .expect("canonical changeset should be built");

    let registry = ProviderRegistry::default();
    let error = preflight_changesets_in_order(
        &[
            changeset_alias_z,
            changeset_middle,
            changeset_alias_b,
            changeset_canonical,
        ],
        &registry,
    )
    .expect_err("multiple non-adjacent hardlink aliases should be rejected");

    match error {
        IdenteditError::InvalidRequest { message } => {
            assert!(
                message.contains("Duplicate file entry in changeset.files"),
                "unexpected duplicate-entry message: {message}"
            );
        }
        other => panic!("unexpected error variant: {other}"),
    }
}

#[test]
fn preflight_allows_three_distinct_files_with_identical_contents() {
    let directory = tempdir().expect("tempdir should be created");
    let fixture = "def process_data(value):\n    result = value + 1\n    return result\n\n\ndef helper():\n    return \"helper\"\n";
    let file_a = directory.path().join("a_target.py");
    let file_m = directory.path().join("m_target.py");
    let file_z = directory.path().join("z_target.py");
    std::fs::write(&file_a, fixture).expect("file_a fixture write should succeed");
    std::fs::write(&file_m, fixture).expect("file_m fixture write should succeed");
    std::fs::write(&file_z, fixture).expect("file_z fixture write should succeed");

    let changeset_z = build_replace_changeset(
        &file_z,
        &process_identity_for(&file_z),
        "def process_data(value):\n    return value * 53".to_string(),
    )
    .expect("changeset_z should be built");
    let changeset_a = build_replace_changeset(
        &file_a,
        &process_identity_for(&file_a),
        "def process_data(value):\n    return value * 54".to_string(),
    )
    .expect("changeset_a should be built");
    let changeset_m = build_replace_changeset(
        &file_m,
        &process_identity_for(&file_m),
        "def process_data(value):\n    return value * 55".to_string(),
    )
    .expect("changeset_m should be built");

    let registry = ProviderRegistry::default();
    let plans = preflight_changesets_in_order(&[changeset_z, changeset_a, changeset_m], &registry)
        .expect("distinct files should be accepted");

    assert_eq!(plans.len(), 3);
    assert_eq!(plans[0].file, file_a);
    assert_eq!(plans[1].file, file_m);
    assert_eq!(plans[2].file, file_z);
}

#[test]
fn prepare_commit_batch_captures_rollback_snapshots_before_write() {
    let directory = tempdir().expect("tempdir should be created");
    let file_a = directory.path().join("a_target.py");
    let fixture = "def process_data(value):\n    result = value + 1\n    return result\n\n\ndef helper():\n    return \"helper\"\n";
    std::fs::write(&file_a, fixture).expect("file_a fixture write should succeed");
    let before = std::fs::read_to_string(&file_a).expect("file_a should be readable");

    let changeset = build_replace_changeset(
        &file_a,
        &process_identity_for(&file_a),
        "def process_data(value):\n    return value * 88".to_string(),
    )
    .expect("changeset should be built");
    let registry = ProviderRegistry::default();
    let preflight_plans =
        preflight_changesets_in_order(&[changeset], &registry).expect("preflight should succeed");
    let commit_batch = prepare_commit_batch(preflight_plans);

    assert_eq!(commit_batch.preflight_plans.len(), 1);
    assert_eq!(commit_batch.rollback_snapshots.len(), 1);
    assert_eq!(commit_batch.rollback_snapshots[0].file, file_a);
    assert_eq!(
        commit_batch.rollback_snapshots[0].original_text, before,
        "rollback snapshot text should capture original pre-write content"
    );
}

#[test]
fn commit_preflight_batch_applies_changes_in_deterministic_order() {
    let directory = tempdir().expect("tempdir should be created");
    let file_a = directory.path().join("a_target.py");
    let file_b = directory.path().join("b_target.py");
    let fixture = "def process_data(value):\n    result = value + 1\n    return result\n\n\ndef helper():\n    return \"helper\"\n";
    std::fs::write(&file_a, fixture).expect("file_a fixture write should succeed");
    std::fs::write(&file_b, fixture).expect("file_b fixture write should succeed");

    let changeset_b = build_replace_changeset(
        &file_b,
        &process_identity_for(&file_b),
        "def process_data(value):\n    return value * 91".to_string(),
    )
    .expect("changeset_b should be built");
    let changeset_a = build_replace_changeset(
        &file_a,
        &process_identity_for(&file_a),
        "def process_data(value):\n    return value * 90".to_string(),
    )
    .expect("changeset_a should be built");

    let registry = ProviderRegistry::default();
    let preflight_plans = preflight_changesets_in_order(&[changeset_b, changeset_a], &registry)
        .expect("preflight should succeed");
    let commit_batch = prepare_commit_batch(preflight_plans);
    let applied = commit_preflight_batch(commit_batch, || Ok(()), || Ok(()))
        .expect("commit batch should succeed");

    assert_eq!(applied.len(), 2);
    assert_eq!(
        applied[0].file,
        file_a.display().to_string(),
        "commit order should follow deterministic preflight path order"
    );
    assert_eq!(applied[1].file, file_b.display().to_string());
    assert_eq!(applied[0].operations_applied, 1);
    assert_eq!(applied[1].operations_applied, 1);
    assert_eq!(applied[0].status, ApplyFileStatus::Applied);
    assert_eq!(applied[1].status, ApplyFileStatus::Applied);

    let after_a = std::fs::read_to_string(&file_a).expect("file_a should be readable");
    let after_b = std::fs::read_to_string(&file_b).expect("file_b should be readable");
    assert!(after_a.contains("return value * 90"));
    assert!(after_b.contains("return value * 91"));
}

#[test]
fn commit_batch_failure_rolls_back_already_written_files() {
    let directory = tempdir().expect("tempdir should be created");
    let file_a = directory.path().join("a_target.py");
    let file_b = directory.path().join("b_target.py");
    let fixture = "def process_data(value):\n    result = value + 1\n    return result\n\n\ndef helper():\n    return \"helper\"\n";
    std::fs::write(&file_a, fixture).expect("file_a fixture write should succeed");
    std::fs::write(&file_b, fixture).expect("file_b fixture write should succeed");
    let before_a = std::fs::read_to_string(&file_a).expect("file_a should be readable");
    let before_b = std::fs::read_to_string(&file_b).expect("file_b should be readable");

    let changeset_b = build_replace_changeset(
        &file_b,
        &process_identity_for(&file_b),
        "def process_data(value):\n    return value * 101".to_string(),
    )
    .expect("changeset_b should be built");
    let changeset_a = build_replace_changeset(
        &file_a,
        &process_identity_for(&file_a),
        "def process_data(value):\n    return value * 100".to_string(),
    )
    .expect("changeset_a should be built");

    let registry = ProviderRegistry::default();
    let preflight_plans = preflight_changesets_in_order(&[changeset_b, changeset_a], &registry)
        .expect("preflight should succeed");
    let commit_batch = prepare_commit_batch(preflight_plans);

    let mut hook_calls = 0usize;
    let error = commit_preflight_batch(
        commit_batch,
        || Ok(()),
        || {
            hook_calls += 1;
            if hook_calls == 2 {
                return Err(IdenteditError::InvalidRequest {
                    message: "injected second-file commit failure".to_string(),
                });
            }
            Ok(())
        },
    )
    .expect_err("second-file failure should abort commit batch");

    match error {
        IdenteditError::InvalidRequest { message } => {
            assert!(
                message.contains("injected second-file commit failure"),
                "unexpected commit failure message: {message}"
            );
        }
        other => panic!("unexpected error variant: {other}"),
    }

    let after_a = std::fs::read_to_string(&file_a).expect("file_a should remain readable");
    let after_b = std::fs::read_to_string(&file_b).expect("file_b should remain readable");
    assert_eq!(
        after_a, before_a,
        "first committed file should be rolled back"
    );
    assert_eq!(after_b, before_b, "second file should remain unchanged");
}

#[test]
fn commit_batch_precondition_failure_rolls_back_already_written_files() {
    let directory = tempdir().expect("tempdir should be created");
    let file_a = directory.path().join("a_target.py");
    let file_b = directory.path().join("b_target.py");
    let fixture = "def process_data(value):\n    result = value + 1\n    return result\n\n\ndef helper():\n    return \"helper\"\n";
    std::fs::write(&file_a, fixture).expect("file_a fixture write should succeed");
    std::fs::write(&file_b, fixture).expect("file_b fixture write should succeed");
    let before_a = std::fs::read_to_string(&file_a).expect("file_a should be readable");
    let before_b = std::fs::read_to_string(&file_b).expect("file_b should be readable");
    let file_b_original_mtime = std::fs::metadata(&file_b)
        .expect("file_b metadata should be readable")
        .modified()
        .expect("file_b mtime should be readable");

    let changeset_b = build_replace_changeset(
        &file_b,
        &process_identity_for(&file_b),
        "def process_data(value):\n    return value * 121".to_string(),
    )
    .expect("changeset_b should be built");
    let changeset_a = build_replace_changeset(
        &file_a,
        &process_identity_for(&file_a),
        "def process_data(value):\n    return value * 120\n# rollback_probe".to_string(),
    )
    .expect("changeset_a should be built");

    let registry = ProviderRegistry::default();
    let preflight_plans = preflight_changesets_in_order(&[changeset_b, changeset_a], &registry)
        .expect("preflight should succeed");
    let commit_batch = prepare_commit_batch(preflight_plans);

    let file_b_for_hook = file_b.clone();
    let mut hook_calls = 0usize;
    let stale_text = before_b.replace("value + 1", "value + 2");
    assert_eq!(
        stale_text.len(),
        before_b.len(),
        "stale probe should preserve byte length for mtime/size collision"
    );
    let stale_text_for_hook = stale_text.clone();
    let error = commit_preflight_batch(
        commit_batch,
        || Ok(()),
        move || {
            hook_calls += 1;
            if hook_calls == 1 {
                std::fs::write(&file_b_for_hook, &stale_text_for_hook)
                    .expect("hook should rewrite second file before second guard check");
                let handle = std::fs::OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(&file_b_for_hook)
                    .expect("hook should reopen second file");
                handle
                    .set_times(FileTimes::new().set_modified(file_b_original_mtime))
                    .expect("hook should restore second file mtime");
            }
            Ok(())
        },
    )
    .expect_err("second-file stale content should abort commit batch");

    match error {
        IdenteditError::PreconditionFailed { .. } => {}
        other => panic!("unexpected error variant: {other}"),
    }

    let after_a = std::fs::read_to_string(&file_a).expect("file_a should remain readable");
    let after_b = std::fs::read_to_string(&file_b).expect("file_b should remain readable");
    assert_eq!(
        after_a, before_a,
        "first committed file should be rolled back on second-file stale precondition"
    );
    assert_eq!(
        after_b, stale_text,
        "second file should keep external stale rewrite because commit aborted before write"
    );
}

#[test]
fn commit_batch_path_swap_failure_rolls_back_already_written_files() {
    let directory = tempdir().expect("tempdir should be created");
    let file_a = directory.path().join("a_target.py");
    let file_b = directory.path().join("b_target.py");
    let file_b_backup = directory.path().join("b_target.py.backup");
    let fixture = "def process_data(value):\n    result = value + 1\n    return result\n\n\ndef helper():\n    return \"helper\"\n";
    std::fs::write(&file_a, fixture).expect("file_a fixture write should succeed");
    std::fs::write(&file_b, fixture).expect("file_b fixture write should succeed");
    let before_a = std::fs::read_to_string(&file_a).expect("file_a should be readable");
    let before_b = std::fs::read_to_string(&file_b).expect("file_b should be readable");

    let changeset_b = build_replace_changeset(
        &file_b,
        &process_identity_for(&file_b),
        "def process_data(value):\n    return value * 131".to_string(),
    )
    .expect("changeset_b should be built");
    let changeset_a = build_replace_changeset(
        &file_a,
        &process_identity_for(&file_a),
        "def process_data(value):\n    return value * 130\n# rollback_probe".to_string(),
    )
    .expect("changeset_a should be built");

    let registry = ProviderRegistry::default();
    let preflight_plans = preflight_changesets_in_order(&[changeset_b, changeset_a], &registry)
        .expect("preflight should succeed");
    let commit_batch = prepare_commit_batch(preflight_plans);

    let file_b_for_hook = file_b.clone();
    let file_b_backup_for_hook = file_b_backup.clone();
    let before_b_for_hook = before_b.clone();
    let mut hook_calls = 0usize;
    let error = commit_preflight_batch(
        commit_batch,
        || Ok(()),
        move || {
            hook_calls += 1;
            if hook_calls == 1 {
                std::fs::rename(&file_b_for_hook, &file_b_backup_for_hook)
                    .expect("hook should move second file to backup path");
                std::fs::write(&file_b_for_hook, &before_b_for_hook)
                    .expect("hook should recreate second file at original path");
            }
            Ok(())
        },
    )
    .expect_err("path swap should fail second-file guard verification");

    match error {
        IdenteditError::PathChanged { path } => {
            assert!(
                path.ends_with("b_target.py"),
                "path change should report swapped file path, got {path}"
            );
        }
        other => panic!("unexpected error variant: {other}"),
    }

    let after_a = std::fs::read_to_string(&file_a).expect("file_a should remain readable");
    let after_b = std::fs::read_to_string(&file_b).expect("file_b should remain readable");
    let backup_b = std::fs::read_to_string(&file_b_backup).expect("backup file should exist");
    assert_eq!(
        after_a, before_a,
        "first committed file should be rolled back on second-file path swap"
    );
    assert_eq!(
        after_b, before_b,
        "second file should remain hook replacement and never receive committed update"
    );
    assert_eq!(
        backup_b, before_b,
        "backup file should preserve original inode payload after swap"
    );
}

#[test]
fn rollback_failure_surfaces_rollback_failed_error() {
    let directory = tempdir().expect("tempdir should be created");
    let file_a = directory.path().join("a_target.py");
    let file_b = directory.path().join("b_target.py");
    let backup_a = directory.path().join("a_target.py.backup");
    let fixture = "def process_data(value):\n    result = value + 1\n    return result\n\n\ndef helper():\n    return \"helper\"\n";
    std::fs::write(&file_a, fixture).expect("file_a fixture write should succeed");
    std::fs::write(&file_b, fixture).expect("file_b fixture write should succeed");

    let changeset_b = build_replace_changeset(
        &file_b,
        &process_identity_for(&file_b),
        "def process_data(value):\n    return value * 111".to_string(),
    )
    .expect("changeset_b should be built");
    let changeset_a = build_replace_changeset(
        &file_a,
        &process_identity_for(&file_a),
        "def process_data(value):\n    return value * 110".to_string(),
    )
    .expect("changeset_a should be built");

    let registry = ProviderRegistry::default();
    let preflight_plans = preflight_changesets_in_order(&[changeset_b, changeset_a], &registry)
        .expect("preflight should succeed");
    let commit_batch = prepare_commit_batch(preflight_plans);

    let mut hook_calls = 0usize;
    let file_a_for_hook = file_a.clone();
    let backup_for_hook = backup_a.clone();
    let error = commit_preflight_batch(
        commit_batch,
        || Ok(()),
        || {
            hook_calls += 1;
            if hook_calls == 2 {
                std::fs::rename(&file_a_for_hook, &backup_for_hook)
                    .expect("first file should be moved aside");
                std::fs::create_dir(&file_a_for_hook)
                    .expect("first file path should be replaced with directory");
                return Err(IdenteditError::InvalidRequest {
                    message: "injected failure with rollback sabotage".to_string(),
                });
            }
            Ok(())
        },
    )
    .expect_err("rollback sabotage should surface rollback_failed");

    match error {
        IdenteditError::RollbackFailed { message } => {
            assert!(
                message.contains("rollback failed"),
                "rollback failure should be explicit in message: {message}"
            );
        }
        other => panic!("unexpected error variant: {other}"),
    }

    let backup_contents =
        std::fs::read_to_string(&backup_a).expect("backup file should remain readable");
    assert!(
        backup_contents.contains("return value * 110"),
        "first file committed state should remain in backup after rollback failure"
    );
}

#[test]
fn commit_preflight_batch_rejects_rollback_snapshot_count_mismatch() {
    let directory = tempdir().expect("tempdir should be created");
    let file = directory.path().join("target.py");
    std::fs::write(
        &file,
        "def process_data(value):\n    result = value + 1\n    return result\n",
    )
    .expect("fixture write should succeed");

    let changeset = build_replace_changeset(
        &file,
        &process_identity_for(&file),
        "def process_data(value):\n    return value * 999".to_string(),
    )
    .expect("changeset should be built");
    let registry = ProviderRegistry::default();
    let preflight_plans =
        preflight_changesets_in_order(&[changeset], &registry).expect("preflight should succeed");
    let mut commit_batch = prepare_commit_batch(preflight_plans);
    commit_batch.rollback_snapshots.clear();

    let error = commit_preflight_batch(commit_batch, || Ok(()), || Ok(()))
        .expect_err("snapshot-count mismatch should be rejected before commit");
    match error {
        IdenteditError::InvalidRequest { message } => {
            assert!(
                message.contains("rollback snapshot count mismatch"),
                "unexpected invariant error message: {message}"
            );
        }
        other => panic!("unexpected error variant: {other}"),
    }
}

#[test]
fn rollback_committed_files_reports_missing_snapshot_index_deterministically() {
    let directory = tempdir().expect("tempdir should be created");
    let file = directory.path().join("target.py");
    std::fs::write(
        &file,
        "def process_data(value):\n    result = value + 1\n    return result\n",
    )
    .expect("fixture write should succeed");
    let permissions = std::fs::metadata(&file)
        .expect("metadata should be readable")
        .permissions();

    let snapshots = vec![FileRollbackSnapshot {
        file: file.clone(),
        original_text: "def process_data(value):\n    return value + 1\n".to_string(),
        original_permissions: permissions,
    }];
    let error = rollback_committed_files(&snapshots, &[0, 1])
        .expect_err("out-of-range committed index should be rejected deterministically");
    match error {
        IdenteditError::InvalidRequest { message } => {
            assert!(
                message.contains("missing snapshot for committed index 1"),
                "unexpected missing-snapshot message: {message}"
            );
        }
        other => panic!("unexpected error variant: {other}"),
    }
}

#[test]
fn move_commit_failure_rolls_back_previously_committed_moves() {
    let directory = tempdir().expect("tempdir should be created");
    let source_a = directory.path().join("a.py");
    let source_b = directory.path().join("b.py");
    let destination_c = directory.path().join("c.py");
    std::fs::write(&source_a, "def from_a():\n    return 'a'\n")
        .expect("source_a fixture write should succeed");
    std::fs::write(&source_b, "def from_b():\n    return 'b'\n")
        .expect("source_b fixture write should succeed");
    let before_a = std::fs::read_to_string(&source_a).expect("source_a should be readable");
    let before_b = std::fs::read_to_string(&source_b).expect("source_b should be readable");

    let changeset_a_to_b = build_move_changeset(&source_a, &source_b);
    let changeset_b_to_c = build_move_changeset(&source_b, &destination_c);

    let mut hook_calls = 0usize;
    let error = apply_changesets_with_hooks(
        &[changeset_a_to_b, changeset_b_to_c],
        || Ok(()),
        || {
            hook_calls += 1;
            if hook_calls == 2 {
                return Err(IdenteditError::InvalidRequest {
                    message: "injected move commit failure".to_string(),
                });
            }
            Ok(())
        },
    )
    .expect_err("second move hook failure should abort commit and trigger rollback");

    match error {
        IdenteditError::InvalidRequest { message } => {
            assert!(
                message.contains("injected move commit failure"),
                "expected injected failure message, got: {message}"
            );
        }
        other => panic!("unexpected error variant: {other}"),
    }

    assert!(source_a.exists(), "rollback should restore source_a path");
    assert!(source_b.exists(), "rollback should restore source_b path");
    assert!(
        !destination_c.exists(),
        "rollback should remove intermediate destination_c path"
    );

    let after_a = std::fs::read_to_string(&source_a).expect("source_a should remain readable");
    let after_b = std::fs::read_to_string(&source_b).expect("source_b should remain readable");
    assert_eq!(
        after_a, before_a,
        "source_a content should be restored on rollback"
    );
    assert_eq!(
        after_b, before_b,
        "source_b content should be restored on rollback"
    );
}

#[test]
fn move_rollback_failure_surfaces_rollback_failed_error() {
    let directory = tempdir().expect("tempdir should be created");
    let source_a = directory.path().join("a.py");
    let source_b = directory.path().join("b.py");
    let destination_c = directory.path().join("c.py");
    std::fs::write(&source_a, "def from_a():\n    return 'a'\n")
        .expect("source_a fixture write should succeed");
    std::fs::write(&source_b, "def from_b():\n    return 'b'\n")
        .expect("source_b fixture write should succeed");

    let changeset_a_to_b = build_move_changeset(&source_a, &source_b);
    let changeset_b_to_c = build_move_changeset(&source_b, &destination_c);

    let destination_c_for_hook = destination_c.clone();
    let mut hook_calls = 0usize;
    let error = apply_changesets_with_hooks(
        &[changeset_a_to_b, changeset_b_to_c],
        || Ok(()),
        || {
            hook_calls += 1;
            if hook_calls == 2 {
                std::fs::remove_file(&destination_c_for_hook)
                    .expect("sabotage should delete destination_c before rollback");
                return Err(IdenteditError::InvalidRequest {
                    message: "injected move commit failure with rollback sabotage".to_string(),
                });
            }
            Ok(())
        },
    )
    .expect_err("rollback sabotage should surface rollback_failed");

    match error {
        IdenteditError::RollbackFailed { message } => {
            assert!(
                message.contains("move rollback failed"),
                "rollback_failed should include move rollback failure details: {message}"
            );
        }
        other => panic!("unexpected error variant: {other}"),
    }

    assert!(
        !source_b.exists(),
        "move rollback failure should leave source_b missing after first move was committed"
    );
}

#[test]
fn move_commit_exdev_like_error_does_not_fallback_to_copy() {
    const EXDEV_LIKE_RAW_CODE: i32 = 18;

    let directory = tempdir().expect("tempdir should be created");
    let source = directory.path().join("source.py");
    let destination = directory.path().join("destination.py");
    std::fs::write(&source, "def moved():\n    return 1\n")
        .expect("source fixture write should succeed");

    let changeset = build_move_changeset(&source, &destination);
    let execution_order = validate_move_operation_constraints(&[changeset])
        .expect("single move should pass graph validation");
    let plans =
        preflight_move_plans(&execution_order).expect("move preflight should produce one plan");
    assert_eq!(plans.len(), 1);

    let error = commit_move_plan_with_rename(
        &plans[0],
        || Ok(()),
        |_, _| Err(std::io::Error::from_raw_os_error(EXDEV_LIKE_RAW_CODE)),
    )
    .expect_err("exdev-like rename failure should bubble up as io error");

    match error {
        IdenteditError::Io {
            path,
            source: io_error,
        } => {
            let canonical_source =
                std::fs::canonicalize(&source).expect("source path should canonicalize");
            assert_eq!(path, canonical_source.display().to_string());
            assert_eq!(io_error.raw_os_error(), Some(EXDEV_LIKE_RAW_CODE));
        }
        other => panic!("unexpected error variant: {other}"),
    }

    assert!(
        source.exists(),
        "source file should remain at original path"
    );
    assert!(
        !destination.exists(),
        "destination file should not be created when rename fails"
    );
}

#[test]
fn mixed_edit_and_move_failure_rolls_back_committed_edit() {
    let directory = tempdir().expect("tempdir should be created");
    let edit_file = create_python_target(directory.path());
    let move_source = directory.path().join("move_source.py");
    let move_destination = directory.path().join("move_destination.py");
    std::fs::write(&move_source, "def move_me():\n    return 10\n")
        .expect("move source fixture write should succeed");

    let before_edit = std::fs::read_to_string(&edit_file).expect("edit file should be readable");
    let before_move =
        std::fs::read_to_string(&move_source).expect("move source should be readable");

    let edit_changeset = build_replace_changeset(
        &edit_file,
        &process_identity_for(&edit_file),
        "def process_data(value):\n    return value * 777".to_string(),
    )
    .expect("edit changeset should be built");
    let move_changeset = build_move_changeset(&move_source, &move_destination);

    let mut hook_calls = 0usize;
    let error = apply_changesets_with_hooks(
        &[edit_changeset, move_changeset],
        || Ok(()),
        || {
            hook_calls += 1;
            if hook_calls == 2 {
                return Err(IdenteditError::InvalidRequest {
                    message: "injected mixed edit/move failure".to_string(),
                });
            }
            Ok(())
        },
    )
    .expect_err("move-stage hook failure should rollback prior edit commit");

    match error {
        IdenteditError::InvalidRequest { message } => {
            assert!(
                message.contains("injected mixed edit/move failure"),
                "expected injected failure message, got: {message}"
            );
        }
        other => panic!("unexpected error variant: {other}"),
    }

    let after_edit = std::fs::read_to_string(&edit_file).expect("edit file should remain readable");
    let after_move =
        std::fs::read_to_string(&move_source).expect("move source should remain readable");
    assert_eq!(
        after_edit, before_edit,
        "edit content should rollback to original"
    );
    assert_eq!(
        after_move, before_move,
        "move source should remain unchanged"
    );
    assert!(
        !move_destination.exists(),
        "failed move should not leave destination file behind"
    );
}

#[test]
fn mixed_edit_and_move_rollback_sabotage_returns_rollback_failed() {
    let directory = tempdir().expect("tempdir should be created");
    let edit_file = create_python_target(directory.path());
    let edit_backup = directory.path().join("edit_file.backup.py");
    let move_source = directory.path().join("move_source.py");
    let move_destination = directory.path().join("move_destination.py");
    std::fs::write(&move_source, "def move_me():\n    return 20\n")
        .expect("move source fixture write should succeed");

    let edit_changeset = build_replace_changeset(
        &edit_file,
        &process_identity_for(&edit_file),
        "def process_data(value):\n    return value * 888".to_string(),
    )
    .expect("edit changeset should be built");
    let move_changeset = build_move_changeset(&move_source, &move_destination);

    let edit_file_for_hook = edit_file.clone();
    let backup_for_hook = edit_backup.clone();
    let mut hook_calls = 0usize;
    let error = apply_changesets_with_hooks(
        &[edit_changeset, move_changeset],
        || Ok(()),
        || {
            hook_calls += 1;
            if hook_calls == 2 {
                std::fs::rename(&edit_file_for_hook, &backup_for_hook)
                    .expect("sabotage should move committed edit file aside");
                std::fs::create_dir(&edit_file_for_hook)
                    .expect("sabotage should replace edit file path with directory");
                return Err(IdenteditError::InvalidRequest {
                    message: "injected mixed edit/move failure with rollback sabotage".to_string(),
                });
            }
            Ok(())
        },
    )
    .expect_err("rollback sabotage should surface rollback_failed");

    match error {
        IdenteditError::RollbackFailed { message } => {
            assert!(
                message.contains("content rollback failed"),
                "rollback_failed should report content rollback failure details: {message}"
            );
        }
        other => panic!("unexpected error variant: {other}"),
    }

    let backup_contents =
        std::fs::read_to_string(&edit_backup).expect("backup file should preserve committed edit");
    assert!(
        backup_contents.contains("return value * 888"),
        "backup should contain committed edit state after rollback sabotage"
    );
}

#[test]
fn move_only_batch_invokes_before_write_hook_once() {
    let directory = tempdir().expect("tempdir should be created");
    let source_a = directory.path().join("a.py");
    let source_b = directory.path().join("b.py");
    let destination_c = directory.path().join("c.py");
    std::fs::write(&source_a, "def from_a():\n    return 'a'\n")
        .expect("source_a fixture write should succeed");
    std::fs::write(&source_b, "def from_b():\n    return 'b'\n")
        .expect("source_b fixture write should succeed");

    let changeset_a_to_b = build_move_changeset(&source_a, &source_b);
    let changeset_b_to_c = build_move_changeset(&source_b, &destination_c);

    let mut before_calls = 0usize;
    let response = apply_changesets_with_hooks(
        &[changeset_a_to_b, changeset_b_to_c],
        || {
            before_calls += 1;
            Ok(())
        },
        || Ok(()),
    )
    .expect("move-only batch should commit successfully");

    assert_eq!(
        before_calls, 1,
        "before_write_hook should run once per batch"
    );
    assert_eq!(
        response.summary.operations_applied, 2,
        "both move operations should be applied"
    );
    assert!(
        !source_a.exists(),
        "first source path should be moved after successful chain commit"
    );
    assert!(
        source_b.exists(),
        "intermediate path should be recreated by chain"
    );
    assert!(
        destination_c.exists(),
        "final destination path should exist"
    );
}

#[test]
fn mixed_edit_move_batch_invokes_before_write_hook_once() {
    let directory = tempdir().expect("tempdir should be created");
    let edit_file = create_python_target(directory.path());
    let move_source = directory.path().join("move_source.py");
    let move_destination = directory.path().join("move_destination.py");
    std::fs::write(&move_source, "def move_me():\n    return 30\n")
        .expect("move source fixture write should succeed");

    let edit_changeset = build_replace_changeset(
        &edit_file,
        &process_identity_for(&edit_file),
        "def process_data(value):\n    return value * 321".to_string(),
    )
    .expect("edit changeset should be built");
    let move_changeset = build_move_changeset(&move_source, &move_destination);

    let mut before_calls = 0usize;
    let response = apply_changesets_with_hooks(
        &[edit_changeset, move_changeset],
        || {
            before_calls += 1;
            Ok(())
        },
        || Ok(()),
    )
    .expect("mixed edit/move batch should commit successfully");

    assert_eq!(
        before_calls, 1,
        "before_write_hook should run once per batch"
    );
    assert_eq!(
        response.summary.operations_applied, 2,
        "mixed batch should apply both edit and move operations"
    );
    let updated_edit = std::fs::read_to_string(&edit_file).expect("edit file should be readable");
    assert!(
        updated_edit.contains("return value * 321"),
        "edit change should be committed in mixed batch"
    );
    assert!(
        !move_source.exists(),
        "move source should be renamed away in mixed batch"
    );
    assert!(
        move_destination.exists(),
        "move destination should exist in mixed batch"
    );
}

#[test]
fn move_graph_validation_failure_skips_before_write_hook() {
    let directory = tempdir().expect("tempdir should be created");
    let source = directory.path().join("source.py");
    std::fs::write(&source, "def keep():\n    return 1\n").expect("fixture write should succeed");

    let self_move_changeset = build_move_changeset(&source, &source);
    let mut before_calls = 0usize;
    let error = apply_changesets_with_hooks(
        &[self_move_changeset],
        || {
            before_calls += 1;
            Ok(())
        },
        || Ok(()),
    )
    .expect_err("self-move should fail during validation");

    match error {
        IdenteditError::InvalidRequest { message } => {
            assert!(
                message.contains("self-move"),
                "expected self-move validation message, got: {message}"
            );
        }
        other => panic!("unexpected error variant: {other}"),
    }
    assert_eq!(
        before_calls, 0,
        "before_write_hook should not run when move validation fails"
    );
}

#[test]
fn move_validation_allows_missing_move_preview_payload_for_backward_compatibility() {
    let directory = tempdir().expect("tempdir should be created");
    let source = directory.path().join("source.py");
    let destination = directory.path().join("destination.py");
    std::fs::write(&source, "def move_me():\n    return 1\n").expect("fixture write should work");

    let mut move_changeset = build_move_changeset(&source, &destination);
    move_changeset.operations[0].preview.move_preview = None;

    let plans = validate_move_operation_constraints(&[move_changeset])
        .expect("missing move preview should remain backward-compatible");
    assert_eq!(plans.len(), 1);
}

#[test]
fn move_validation_rejects_mismatched_move_preview_paths() {
    let directory = tempdir().expect("tempdir should be created");
    let source = directory.path().join("source.py");
    let destination = directory.path().join("destination.py");
    std::fs::write(&source, "def move_me():\n    return 1\n").expect("fixture write should work");

    let mut move_changeset = build_move_changeset(&source, &destination);
    move_changeset.operations[0].preview.move_preview = Some(crate::changeset::MovePreview {
        from: directory.path().join("other.py"),
        to: destination.clone(),
    });

    let error = validate_move_operation_constraints(&[move_changeset])
        .expect_err("mismatched move preview should be rejected");
    match error {
        IdenteditError::InvalidRequest { message } => {
            assert!(
                message.contains("Move preview mismatch"),
                "expected move preview mismatch message, got: {message}"
            );
        }
        other => panic!("unexpected error variant: {other}"),
    }
}

#[test]
fn move_destination_exists_validation_preserves_mixed_batch_edit_file() {
    let directory = tempdir().expect("tempdir should be created");
    let edit_file = create_python_target(directory.path());
    let move_source = directory.path().join("move_source.py");
    let move_destination = directory.path().join("existing_destination.py");
    std::fs::write(&move_source, "def move_me():\n    return 40\n")
        .expect("move source fixture write should succeed");
    std::fs::write(&move_destination, "def occupied():\n    return 99\n")
        .expect("destination fixture write should succeed");
    let before_edit = std::fs::read_to_string(&edit_file).expect("edit file should be readable");

    let edit_changeset = build_replace_changeset(
        &edit_file,
        &process_identity_for(&edit_file),
        "def process_data(value):\n    return value * 654".to_string(),
    )
    .expect("edit changeset should be built");
    let move_changeset = build_move_changeset(&move_source, &move_destination);

    let error =
        apply_changesets_with_hooks(&[edit_changeset, move_changeset], || Ok(()), || Ok(()))
            .expect_err("destination-exists validation should reject mixed batch before commit");
    match error {
        IdenteditError::InvalidRequest { message } => {
            assert!(
                message.contains("Destination path already exists"),
                "expected destination-exists validation message, got: {message}"
            );
        }
        other => panic!("unexpected error variant: {other}"),
    }

    let after_edit = std::fs::read_to_string(&edit_file).expect("edit file should remain readable");
    assert_eq!(
        after_edit, before_edit,
        "edit file must remain unchanged when move validation rejects batch"
    );
}

#[test]
fn mixed_batch_with_missing_move_source_preserves_edit_file() {
    let directory = tempdir().expect("tempdir should be created");
    let edit_file = create_python_target(directory.path());
    let missing_move_source = directory.path().join("missing_move_source.py");
    let move_destination = directory.path().join("move_destination.py");
    let before_edit = std::fs::read_to_string(&edit_file).expect("edit file should be readable");

    let edit_changeset = build_replace_changeset(
        &edit_file,
        &process_identity_for(&edit_file),
        "def process_data(value):\n    return value * 987".to_string(),
    )
    .expect("edit changeset should be built");
    let move_changeset = build_move_changeset(&missing_move_source, &move_destination);

    let error =
        apply_changesets_with_hooks(&[edit_changeset, move_changeset], || Ok(()), || Ok(()))
            .expect_err("missing move source should fail before any commit");
    match error {
        IdenteditError::Io { path, .. } => {
            assert!(
                path.contains("missing_move_source.py"),
                "io error should reference missing move source path: {path}"
            );
        }
        other => panic!("unexpected error variant: {other}"),
    }

    let after_edit = std::fs::read_to_string(&edit_file).expect("edit file should remain readable");
    assert_eq!(
        after_edit, before_edit,
        "edit file must remain unchanged when move source is missing"
    );
}

#[test]
fn missing_move_source_failure_skips_before_write_hook() {
    let directory = tempdir().expect("tempdir should be created");
    let missing_move_source = directory.path().join("missing_move_source.py");
    let move_destination = directory.path().join("move_destination.py");
    let move_changeset = build_move_changeset(&missing_move_source, &move_destination);

    let mut before_calls = 0usize;
    let error = apply_changesets_with_hooks(
        &[move_changeset],
        || {
            before_calls += 1;
            Ok(())
        },
        || Ok(()),
    )
    .expect_err("missing move source should fail during validation/preflight");

    match error {
        IdenteditError::Io { path, .. } => {
            assert!(
                path.contains("missing_move_source.py"),
                "io error should reference missing move source path: {path}"
            );
        }
        other => panic!("unexpected error variant: {other}"),
    }
    assert_eq!(
        before_calls, 0,
        "before_write_hook should not run when move source canonicalization fails"
    );
}
