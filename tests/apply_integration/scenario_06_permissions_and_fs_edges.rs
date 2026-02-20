use super::*;

#[cfg(unix)]
#[test]
fn apply_noop_on_read_only_file_returns_io_error() {
    let file_path = copy_fixture_to_temp_python("example.py");
    fs::set_permissions(&file_path, fs::Permissions::from_mode(0o444))
        .expect("fixture should be made read-only");

    let request = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": []
    });
    let output = run_identedit_with_stdin(&["apply"], &request.to_string());
    assert!(
        !output.status.success(),
        "current contract requires write-open lock even for no-op apply"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "io_error");
}

#[test]
fn apply_applies_multiple_non_overlapping_operations() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let process_handle = select_named_handle(&file_path, "process_*");
    let helper_handle = select_named_handle(&file_path, "helper");
    let process_span = &process_handle["span"];
    let helper_span = &helper_handle["span"];
    let process_replacement = "def process_data(value):\n    return value + 10";
    let helper_replacement = "def helper():\n    return \"updated\"";

    let changeset = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "identity": helper_handle["identity"],
                    "kind": helper_handle["kind"],
                    "span_hint": {"start": helper_span["start"], "end": helper_span["end"]},
                    "expected_old_hash": identedit::changeset::hash_text(
                        helper_handle["text"].as_str().expect("text should be string")
                    )
                },
                "op": {"type": "replace", "new_text": helper_replacement},
                "preview": {
                    "old_text": helper_handle["text"],
                    "new_text": helper_replacement,
                    "matched_span": {"start": helper_span["start"], "end": helper_span["end"]}
                }
            },
            {
                "target": {
                    "identity": process_handle["identity"],
                    "kind": process_handle["kind"],
                    "span_hint": {"start": process_span["start"], "end": process_span["end"]},
                    "expected_old_hash": identedit::changeset::hash_text(
                        process_handle["text"].as_str().expect("text should be string")
                    )
                },
                "op": {"type": "replace", "new_text": process_replacement},
                "preview": {
                    "old_text": process_handle["text"],
                    "new_text": process_replacement,
                    "matched_span": {"start": process_span["start"], "end": process_span["end"]}
                }
            }
        ]
    });

    let changeset_file = write_changeset_json(&changeset.to_string());
    let output = run_identedit(&[
        "apply",
        changeset_file
            .path()
            .to_str()
            .expect("changeset path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "apply should succeed for independent operations: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["summary"]["files_modified"], 1);
    assert_eq!(response["summary"]["operations_applied"], 2);
    assert_eq!(response["summary"]["operations_failed"], 0);

    let modified = fs::read_to_string(&file_path).expect("file should be readable");
    assert!(modified.contains("return value + 10"));
    assert!(modified.contains("return \"updated\""));
}

#[test]
fn apply_non_overlapping_operations_are_order_independent() {
    let process_replacement = "def process_data(value):\n    return value + 10";
    let helper_replacement = "def helper():\n    return \"updated\"";

    let file_path_a = copy_fixture_to_temp_python("example.py");
    let process_handle_a = select_named_handle(&file_path_a, "process_*");
    let helper_handle_a = select_named_handle(&file_path_a, "helper");
    let process_span_a = &process_handle_a["span"];
    let helper_span_a = &helper_handle_a["span"];
    let changeset_a = json!({
        "file": file_path_a.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "identity": helper_handle_a["identity"],
                    "kind": helper_handle_a["kind"],
                    "span_hint": {"start": helper_span_a["start"], "end": helper_span_a["end"]},
                    "expected_old_hash": identedit::changeset::hash_text(
                        helper_handle_a["text"].as_str().expect("text should be string")
                    )
                },
                "op": {"type": "replace", "new_text": helper_replacement},
                "preview": {
                    "old_text": helper_handle_a["text"],
                    "new_text": helper_replacement,
                    "matched_span": {"start": helper_span_a["start"], "end": helper_span_a["end"]}
                }
            },
            {
                "target": {
                    "identity": process_handle_a["identity"],
                    "kind": process_handle_a["kind"],
                    "span_hint": {"start": process_span_a["start"], "end": process_span_a["end"]},
                    "expected_old_hash": identedit::changeset::hash_text(
                        process_handle_a["text"].as_str().expect("text should be string")
                    )
                },
                "op": {"type": "replace", "new_text": process_replacement},
                "preview": {
                    "old_text": process_handle_a["text"],
                    "new_text": process_replacement,
                    "matched_span": {"start": process_span_a["start"], "end": process_span_a["end"]}
                }
            }
        ]
    });

    let apply_output_a = run_identedit_with_stdin(&["apply"], &changeset_a.to_string());
    assert!(
        apply_output_a.status.success(),
        "apply should succeed for helper->process order: {}",
        String::from_utf8_lossy(&apply_output_a.stderr)
    );
    let response_a: Value =
        serde_json::from_slice(&apply_output_a.stdout).expect("stdout should be valid JSON");
    assert_eq!(response_a["summary"]["operations_applied"], 2);
    let after_a = fs::read_to_string(&file_path_a).expect("file should be readable");

    let file_path_b = copy_fixture_to_temp_python("example.py");
    let process_handle_b = select_named_handle(&file_path_b, "process_*");
    let helper_handle_b = select_named_handle(&file_path_b, "helper");
    let process_span_b = &process_handle_b["span"];
    let helper_span_b = &helper_handle_b["span"];
    let changeset_b = json!({
        "file": file_path_b.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "identity": process_handle_b["identity"],
                    "kind": process_handle_b["kind"],
                    "span_hint": {"start": process_span_b["start"], "end": process_span_b["end"]},
                    "expected_old_hash": identedit::changeset::hash_text(
                        process_handle_b["text"].as_str().expect("text should be string")
                    )
                },
                "op": {"type": "replace", "new_text": process_replacement},
                "preview": {
                    "old_text": process_handle_b["text"],
                    "new_text": process_replacement,
                    "matched_span": {"start": process_span_b["start"], "end": process_span_b["end"]}
                }
            },
            {
                "target": {
                    "identity": helper_handle_b["identity"],
                    "kind": helper_handle_b["kind"],
                    "span_hint": {"start": helper_span_b["start"], "end": helper_span_b["end"]},
                    "expected_old_hash": identedit::changeset::hash_text(
                        helper_handle_b["text"].as_str().expect("text should be string")
                    )
                },
                "op": {"type": "replace", "new_text": helper_replacement},
                "preview": {
                    "old_text": helper_handle_b["text"],
                    "new_text": helper_replacement,
                    "matched_span": {"start": helper_span_b["start"], "end": helper_span_b["end"]}
                }
            }
        ]
    });

    let apply_output_b = run_identedit_with_stdin(&["apply"], &changeset_b.to_string());
    assert!(
        apply_output_b.status.success(),
        "apply should succeed for process->helper order: {}",
        String::from_utf8_lossy(&apply_output_b.stderr)
    );
    let response_b: Value =
        serde_json::from_slice(&apply_output_b.stdout).expect("stdout should be valid JSON");
    assert_eq!(response_b["summary"]["operations_applied"], 2);
    let after_b = fs::read_to_string(&file_path_b).expect("file should be readable");

    assert_eq!(
        after_a, after_b,
        "operation ordering should not change final file content for non-overlapping targets"
    );
}

#[test]
fn apply_precondition_failure_is_order_independent_when_one_target_is_stale() {
    let stale_hash = "0000000000000000000000000000000000000000000000000000000000000000";
    let process_replacement = "def process_data(value):\n    return value * 3";
    let helper_replacement = "def helper():\n    return \"broken\"";

    let file_path_a = copy_fixture_to_temp_python("example.py");
    let process_handle_a = select_named_handle(&file_path_a, "process_*");
    let helper_handle_a = select_named_handle(&file_path_a, "helper");
    let process_span_a = &process_handle_a["span"];
    let helper_span_a = &helper_handle_a["span"];
    let changeset_a = json!({
        "file": file_path_a.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "identity": process_handle_a["identity"],
                    "kind": process_handle_a["kind"],
                    "span_hint": {"start": process_span_a["start"], "end": process_span_a["end"]},
                    "expected_old_hash": identedit::changeset::hash_text(
                        process_handle_a["text"].as_str().expect("text should be string")
                    )
                },
                "op": {"type": "replace", "new_text": process_replacement},
                "preview": {
                    "old_text": process_handle_a["text"],
                    "new_text": process_replacement,
                    "matched_span": {"start": process_span_a["start"], "end": process_span_a["end"]}
                }
            },
            {
                "target": {
                    "identity": helper_handle_a["identity"],
                    "kind": helper_handle_a["kind"],
                    "span_hint": {"start": helper_span_a["start"], "end": helper_span_a["end"]},
                    "expected_old_hash": stale_hash
                },
                "op": {"type": "replace", "new_text": helper_replacement},
                "preview": {
                    "old_text": helper_handle_a["text"],
                    "new_text": helper_replacement,
                    "matched_span": {"start": helper_span_a["start"], "end": helper_span_a["end"]}
                }
            }
        ]
    });
    let output_a = run_identedit_with_stdin(&["apply"], &changeset_a.to_string());
    assert!(
        !output_a.status.success(),
        "apply should fail with stale helper precondition in process->helper order"
    );
    let response_a: Value =
        serde_json::from_slice(&output_a.stdout).expect("stdout should be valid JSON");
    assert_eq!(response_a["error"]["type"], "precondition_failed");
    let message_a = response_a["error"]["message"]
        .as_str()
        .expect("error message should be a string")
        .to_string();

    let file_path_b = copy_fixture_to_temp_python("example.py");
    let process_handle_b = select_named_handle(&file_path_b, "process_*");
    let helper_handle_b = select_named_handle(&file_path_b, "helper");
    let process_span_b = &process_handle_b["span"];
    let helper_span_b = &helper_handle_b["span"];
    let changeset_b = json!({
        "file": file_path_b.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "identity": helper_handle_b["identity"],
                    "kind": helper_handle_b["kind"],
                    "span_hint": {"start": helper_span_b["start"], "end": helper_span_b["end"]},
                    "expected_old_hash": stale_hash
                },
                "op": {"type": "replace", "new_text": helper_replacement},
                "preview": {
                    "old_text": helper_handle_b["text"],
                    "new_text": helper_replacement,
                    "matched_span": {"start": helper_span_b["start"], "end": helper_span_b["end"]}
                }
            },
            {
                "target": {
                    "identity": process_handle_b["identity"],
                    "kind": process_handle_b["kind"],
                    "span_hint": {"start": process_span_b["start"], "end": process_span_b["end"]},
                    "expected_old_hash": identedit::changeset::hash_text(
                        process_handle_b["text"].as_str().expect("text should be string")
                    )
                },
                "op": {"type": "replace", "new_text": process_replacement},
                "preview": {
                    "old_text": process_handle_b["text"],
                    "new_text": process_replacement,
                    "matched_span": {"start": process_span_b["start"], "end": process_span_b["end"]}
                }
            }
        ]
    });
    let output_b = run_identedit_with_stdin(&["apply"], &changeset_b.to_string());
    assert!(
        !output_b.status.success(),
        "apply should fail with stale helper precondition in helper->process order"
    );
    let response_b: Value =
        serde_json::from_slice(&output_b.stdout).expect("stdout should be valid JSON");
    assert_eq!(response_b["error"]["type"], "precondition_failed");
    let message_b = response_b["error"]["message"]
        .as_str()
        .expect("error message should be a string")
        .to_string();

    assert_eq!(
        message_a, message_b,
        "precondition failure message should stay deterministic regardless of operation order"
    );
}

#[test]
fn apply_is_atomic_when_any_operation_fails_precondition() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let process_handle = select_named_handle(&file_path, "process_*");
    let helper_handle = select_named_handle(&file_path, "helper");
    let process_span = &process_handle["span"];
    let helper_span = &helper_handle["span"];

    let changeset = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "identity": process_handle["identity"],
                    "kind": process_handle["kind"],
                    "span_hint": {"start": process_span["start"], "end": process_span["end"]},
                    "expected_old_hash": identedit::changeset::hash_text(
                        process_handle["text"].as_str().expect("text should be string")
                    )
                },
                "op": {"type": "replace", "new_text": "def process_data(value):\n    return value * 3"},
                "preview": {
                    "old_text": process_handle["text"],
                    "new_text": "def process_data(value):\n    return value * 3",
                    "matched_span": {"start": process_span["start"], "end": process_span["end"]}
                }
            },
            {
                "target": {
                    "identity": helper_handle["identity"],
                    "kind": helper_handle["kind"],
                    "span_hint": {"start": helper_span["start"], "end": helper_span["end"]},
                    "expected_old_hash": "0000000000000000000000000000000000000000000000000000000000000000"
                },
                "op": {"type": "replace", "new_text": "def helper():\n    return \"broken\""},
                "preview": {
                    "old_text": helper_handle["text"],
                    "new_text": "def helper():\n    return \"broken\"",
                    "matched_span": {"start": helper_span["start"], "end": helper_span["end"]}
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["apply"], &changeset.to_string());
    assert!(
        !output.status.success(),
        "apply should fail when any target precondition mismatches"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "precondition_failed");

    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(
        before, after,
        "apply must be atomic: file content should remain unchanged on failure"
    );
}

#[test]
fn apply_returns_resource_busy_when_lock_is_already_held() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let lock_file = OpenOptions::new()
        .truncate(false)
        .read(true)
        .write(true)
        .open(&file_path)
        .expect("target file should be opened");
    lock_file
        .lock_exclusive()
        .expect("lock should be acquired for test");

    let empty_changeset = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": []
    });
    let output = run_identedit_with_stdin(&["apply"], &empty_changeset.to_string());
    assert!(
        !output.status.success(),
        "apply should fail while file lock is held by another actor"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "resource_busy");
}

#[cfg(unix)]
#[test]
fn apply_non_utf8_changeset_path_argument_returns_io_error_without_panicking() {
    let mut command = Command::new(env!("CARGO_BIN_EXE_identedit"));
    command.arg("apply");
    command.arg(OsString::from_vec(vec![0xFF, 0x2E, 0x6A, 0x73, 0x6F, 0x6E]));

    let output = command.output().expect("failed to run identedit binary");
    assert!(
        !output.status.success(),
        "apply should fail for non-UTF8 changeset path arguments"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "io_error");
}

#[cfg(unix)]
#[test]
fn apply_rejects_symbolic_link_target_path() {
    use std::os::unix::fs::symlink;

    let directory = tempdir().expect("tempdir should be created");
    let real_target = directory.path().join("real.py");
    std::fs::write(
        &real_target,
        "def process_data(value):\n    return value + 1\n",
    )
    .expect("real target should be written");
    let symlink_target = directory.path().join("link.py");
    symlink(&real_target, &symlink_target).expect("symlink should be created");

    let request = json!({
        "file": symlink_target.to_string_lossy().to_string(),
        "operations": []
    });
    let output = run_identedit_with_stdin(&["apply"], &request.to_string());
    assert!(
        !output.status.success(),
        "apply should fail for symlink targets"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("symbolic link")),
        "expected symlink rejection message"
    );
}
