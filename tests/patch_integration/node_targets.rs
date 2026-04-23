use super::*;

#[test]
fn patch_replace_applies_change_in_single_command() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_function_handle(&file_path, "process_*");
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");
    let replacement = "def process_data(value):\n    return value * 9";

    let output = run_identedit(&[
        "patch",
        "--identity",
        identity,
        "--replace",
        replacement,
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "patch replace failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["summary"]["files_modified"], 1);
    assert_eq!(response["summary"]["operations_applied"], 1);
    assert_eq!(response["summary"]["operations_failed"], 0);

    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    assert!(
        modified.contains("return value * 9"),
        "replacement text should be written"
    );
}

#[test]
fn patch_scoped_replacement_stdin_without_pattern_reports_pairing_error() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_function_handle(&file_path, "process_*");
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");

    let output = run_identedit_with_stdin(
        &[
            "patch",
            "--identity",
            identity,
            "--scoped-replacement",
            "--stdin-text",
            file_path.to_str().expect("path should be utf-8"),
        ],
        "payload",
    );

    assert!(
        !output.status.success(),
        "scoped replacement without pattern should fail"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let message = response["error"]["message"]
        .as_str()
        .expect("error message should be present");
    assert!(
        message.contains("--scoped-regex") && message.contains("--scoped-replacement"),
        "error should preserve scoped pairing guidance, got: {message}"
    );
}

#[test]
fn patch_delete_removes_target_node() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_function_handle(&file_path, "helper");
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");

    let output = run_identedit(&[
        "patch",
        "--identity",
        identity,
        "--delete",
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "patch delete failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["summary"]["operations_applied"], 1);

    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    assert!(
        !modified.contains("def helper():"),
        "target function should be deleted"
    );
}

#[test]
fn patch_rejects_multiple_operations_in_single_request() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_function_handle(&file_path, "process_*");
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");

    let output = run_identedit(&[
        "patch",
        "--identity",
        identity,
        "--replace",
        "def process_data(value):\n    return value + 1",
        "--delete",
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        !output.status.success(),
        "patch should reject multiple operations"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}

#[test]
fn patch_returns_ambiguous_target_error_for_duplicate_identity() {
    let file_path = copy_fixture_to_temp_python("ambiguous.py");
    let handle = select_named_function_handle(&file_path, "duplicate");
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");

    let output = run_identedit(&[
        "patch",
        "--identity",
        identity,
        "--replace",
        "def duplicate():\n    return 2",
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        !output.status.success(),
        "patch should fail when identity is ambiguous"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "ambiguous_target");
    let candidates = response["error"]["candidates"]
        .as_array()
        .expect("ambiguous identity response should include candidate contexts");
    assert_eq!(candidates.len(), 2);
    assert!(
        candidates
            .iter()
            .all(|candidate| candidate["identity"] == identity)
    );
    assert_eq!(candidates[0]["line"], 1);
    assert_eq!(candidates[1]["line"], 5);
}

#[test]
fn patch_verbose_includes_applied_file_results() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_function_handle(&file_path, "process_*");
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");

    let output = run_identedit(&[
        "patch",
        "--identity",
        identity,
        "--replace",
        "def process_data(value):\n    return value * 5",
        "--verbose",
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "patch verbose failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["summary"]["operations_applied"], 1);
    let applied = response["applied"]
        .as_array()
        .expect("verbose patch response should include applied array");
    assert_eq!(applied.len(), 1);
    assert_eq!(applied[0]["operations_applied"], 1);
}

#[test]
fn patch_ambiguous_target_failure_keeps_source_file_unchanged() {
    let file_path = copy_fixture_to_temp_python("ambiguous.py");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let handle = select_named_function_handle(&file_path, "duplicate");
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");

    let output = run_identedit(&[
        "patch",
        "--identity",
        identity,
        "--replace",
        "def duplicate():\n    return 999",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "patch should fail for ambiguous identity"
    );

    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(
        before, after,
        "source file must remain unchanged when patch fails"
    );
}

#[test]
fn patch_reports_io_error_for_missing_file() {
    let output = run_identedit(&[
        "patch",
        "--identity",
        "deadbeefdeadbeef",
        "--replace",
        "def process_data(value):\n    return value",
        "/tmp/identedit-missing-file-should-not-exist.py",
    ]);
    assert!(
        !output.status.success(),
        "patch should fail for missing file path"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "io_error");
}

#[test]
fn patch_insert_before_preserves_utf8_bom() {
    let mut temp_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp python file should be created");
    temp_file
        .write_all(
            b"\xEF\xBB\xBFdef process_data(value):\n    return value + 1\n\ndef helper():\n    return value + 2\n",
        )
        .expect("bom fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let handle = select_named_function_handle(&file_path, "helper");
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");

    let output = run_identedit(&[
        "patch",
        "--identity",
        identity,
        "--insert-before",
        "# before helper\n",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "patch insert-before should support BOM files: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let bytes = fs::read(&file_path).expect("modified file should be readable");
    assert!(
        bytes.starts_with(&[0xEF, 0xBB, 0xBF]),
        "UTF-8 BOM prefix must remain intact after patch"
    );
}

#[test]
fn patch_replace_supports_crlf_files() {
    let mut temp_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp python file should be created");
    let source = "def process_data(value):\r\n    return value + 1\r\n\r\ndef helper():\r\n    return value + 2\r\n";
    temp_file
        .write_all(source.as_bytes())
        .expect("crlf fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let handle = select_named_function_handle(&file_path, "process_*");
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");

    let output = run_identedit(&[
        "patch",
        "--identity",
        identity,
        "--replace",
        "def process_data(value):\r\n    return value * 10\r\n",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "patch replace should support CRLF source: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    assert!(
        modified.contains("return value * 10\r\n"),
        "replacement should preserve CRLF sequence"
    );
    assert!(
        modified.contains("def helper():\r\n"),
        "non-target sections should keep CRLF endings"
    );
}

#[test]
fn patch_file_target_rejects_node_operation_with_actionable_message() {
    let file_path = copy_fixture_to_temp_python("example.py");

    let output = run_identedit(&[
        "patch",
        "--at",
        "file-end",
        "--replace",
        "x",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "file target should reject node operations"
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let message = response["error"]["message"]
        .as_str()
        .expect("error message should be present");
    assert!(
        message.contains("--insert")
            && message.contains("file-start")
            && message.contains("file-end"),
        "file-mode error should explain that file targets only support insert"
    );
}

#[test]
fn patch_flag_array_oob_rejection_reports_append_and_keeps_file() {
    let file_path = copy_fixture_to_temp_with_suffix("example.json", ".json");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let output = run_identedit(&[
        "patch",
        "--config-path",
        "items[99]",
        "--set-value",
        "10",
        "--create-missing",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(!output.status.success(), "operation should fail");
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("append operation")),
        "error should include append-operation hint"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(
        before, after,
        "rejected flag-mode operation must not mutate file"
    );
}
