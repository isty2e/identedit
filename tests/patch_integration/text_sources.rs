use super::*;

#[test]
fn patch_replace_accepts_text_file_payload() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_function_handle(&file_path, "process_*");
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");
    let payload_path = create_temp_text_file("def process_data(value):\n    return value * 21");

    let output = run_identedit(&[
        "patch",
        "--at",
        identity,
        "--replace",
        "--text-file",
        payload_path.to_str().expect("payload path should be utf-8"),
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "patch replace with text file failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    assert!(
        modified.contains("return value * 21"),
        "replacement text from file should be written"
    );
}

#[test]
fn patch_file_start_insert_accepts_text_file_payload() {
    let file_path = create_temp_text_file("body\n");
    let payload_path = create_temp_text_file("# generated header\n");

    let output = run_identedit(&[
        "patch",
        "--at",
        "file-start",
        "--insert",
        "--text-file",
        payload_path.to_str().expect("payload path should be utf-8"),
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "file-start insert with text file failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    assert_eq!(modified, "# generated header\nbody\n");
}

#[test]
fn patch_flag_rejects_text_file_and_stdin_text_together() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_function_handle(&file_path, "process_*");
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");
    let payload_path = create_temp_text_file("def process_data(value):\n    return value * 21");

    let output = run_identedit_with_stdin(
        &[
            "patch",
            "--at",
            identity,
            "--replace",
            "--text-file",
            payload_path.to_str().expect("payload path should be utf-8"),
            "--stdin-text",
            file_path.to_str().expect("path should be utf-8"),
        ],
        "def process_data(value):\n    return value * 22",
    );

    assert!(
        !output.status.success(),
        "multiple external text sources should conflict"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let message = response["error"]["message"]
        .as_str()
        .expect("error message should be present");
    assert!(
        message.contains("--text-file") && message.contains("--stdin-text"),
        "error should explain external text source conflict, got: {message}"
    );
}

#[test]
fn patch_delete_rejects_external_text_source() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_function_handle(&file_path, "process_*");
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");
    let payload_path = create_temp_text_file("unused");

    let output = run_identedit(&[
        "patch",
        "--at",
        identity,
        "--delete",
        "--text-file",
        payload_path.to_str().expect("payload path should be utf-8"),
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        !output.status.success(),
        "delete should reject external text source"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let message = response["error"]["message"]
        .as_str()
        .expect("error message should be present");
    assert!(
        message.contains("--text-file") && message.contains("--stdin-text"),
        "error should explain that delete cannot consume external text, got: {message}"
    );
}

#[test]
fn patch_replace_text_file_non_utf8_returns_io_error_without_mutation() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let handle = select_named_function_handle(&file_path, "process_*");
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");
    let payload_path = create_temp_binary_file(&[0x66, 0x6f, 0x80]);

    let output = run_identedit(&[
        "patch",
        "--at",
        identity,
        "--replace",
        "--text-file",
        payload_path.to_str().expect("payload path should be utf-8"),
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(!output.status.success(), "non-utf8 text file should fail");
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "io_error");
    let after = fs::read_to_string(&file_path).expect("file should still be readable");
    assert_eq!(after, before);
}

#[test]
fn patch_file_end_insert_stdin_text_preserves_trailing_newline() {
    let file_path = create_temp_text_file("body\n");

    let output = run_identedit_with_stdin(
        &[
            "patch",
            "--at",
            "file-end",
            "--insert",
            "--stdin-text",
            file_path.to_str().expect("path should be utf-8"),
        ],
        "tail\n",
    );

    assert!(
        output.status.success(),
        "file-end insert with stdin text failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    assert_eq!(modified, "body\ntail\n");
}

#[test]
fn patch_replace_stdin_text_dry_run_does_not_modify_file() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let handle = select_named_function_handle(&file_path, "process_*");
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");

    let output = run_identedit_with_stdin(
        &[
            "patch",
            "--at",
            identity,
            "--replace",
            "--stdin-text",
            "--dry-run",
            file_path.to_str().expect("path should be utf-8"),
        ],
        "def process_data(value):\n    return value * 33",
    );

    assert!(
        output.status.success(),
        "replace with stdin dry-run failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let after = fs::read_to_string(&file_path).expect("file should remain readable");
    assert_eq!(after, before);
}

#[test]
fn patch_file_end_insert_text_file_with_utf8_bom_preserves_payload_bytes() {
    let file_path = create_temp_text_file("body\n");
    let payload_path = create_temp_text_file("\u{FEFF}tail\n");

    let output = run_identedit(&[
        "patch",
        "--at",
        "file-end",
        "--insert",
        "--text-file",
        payload_path.to_str().expect("payload path should be utf-8"),
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "file-end insert with BOM payload failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    assert_eq!(modified, format!("body\n{}\n", "\u{FEFF}tail"));
}

#[test]
fn patch_missing_operation_with_stdin_text_reports_invalid_request() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_function_handle(&file_path, "process_*");
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");

    let output = run_identedit_with_stdin(
        &[
            "patch",
            "--at",
            identity,
            "--stdin-text",
            file_path.to_str().expect("path should be utf-8"),
        ],
        "unused",
    );

    assert!(
        !output.status.success(),
        "missing operation should fail even when stdin text is present"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let message = response["error"]["message"]
        .as_str()
        .expect("error message should be present");
    assert!(
        message.contains("Choose exactly one node operation"),
        "error should prioritize missing operation, got: {message}"
    );
}

#[test]
fn patch_replace_text_file_directory_returns_io_error_without_mutation() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let handle = select_named_function_handle(&file_path, "process_*");
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");
    let payload_dir = Builder::new()
        .prefix("identedit-payload-dir")
        .tempdir()
        .expect("temp dir should be created");

    let output = run_identedit(&[
        "patch",
        "--at",
        identity,
        "--replace",
        "--text-file",
        payload_dir
            .path()
            .to_str()
            .expect("payload path should be utf-8"),
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(!output.status.success(), "directory payload should fail");
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "io_error");
    let after = fs::read_to_string(&file_path).expect("file should remain readable");
    assert_eq!(after, before);
}

#[test]
fn patch_file_insert_text_file_directory_returns_io_error_without_mutation() {
    let file_path = create_temp_text_file("body\n");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let payload_dir = Builder::new()
        .prefix("identedit-file-insert-dir")
        .tempdir()
        .expect("temp dir should be created");

    let output = run_identedit(&[
        "patch",
        "--at",
        "file-end",
        "--insert",
        "--text-file",
        payload_dir
            .path()
            .to_str()
            .expect("payload path should be utf-8"),
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        !output.status.success(),
        "directory payload should fail for file insert"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "io_error");
    let after = fs::read_to_string(&file_path).expect("file should remain readable");
    assert_eq!(after, before);
}

#[test]
fn patch_replace_text_file_path_with_spaces_applies() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_function_handle(&file_path, "process_*");
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");
    let payload_dir = Builder::new()
        .prefix("identedit payload dir ")
        .tempdir()
        .expect("temp dir should be created");
    let payload_path = payload_dir.path().join("replacement body.txt");
    fs::write(
        &payload_path,
        "def process_data(value):\n    return value * 55",
    )
    .expect("payload file should be written");

    let output = run_identedit(&[
        "patch",
        "--at",
        identity,
        "--replace",
        "--text-file",
        payload_path.to_str().expect("payload path should be utf-8"),
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "text-file path with spaces should work: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    assert!(modified.contains("return value * 55"));
}

#[test]
fn patch_file_insert_stdin_text_with_node_flag_reports_file_guidance() {
    let file_path = create_temp_text_file("body\n");

    let output = run_identedit_with_stdin(
        &[
            "patch",
            "--at",
            "file-end",
            "--insert",
            "--stdin-text",
            "--delete",
            file_path.to_str().expect("path should be utf-8"),
        ],
        "tail\n",
    );

    assert!(
        !output.status.success(),
        "file mode should reject node-only flags before patching"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let message = response["error"]["message"]
        .as_str()
        .expect("error message should be present");
    assert!(
        message.contains("File target mode supports only --insert"),
        "error should keep file guidance, got: {message}"
    );
}
