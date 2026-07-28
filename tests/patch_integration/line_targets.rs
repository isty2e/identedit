use super::*;

#[test]
fn patch_line_replace_range_accepts_stdin_text_payload() {
    let file_path = create_temp_text_file("alpha\nbeta\ngamma\ndelta\n");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let start_anchor = line_ref(&before, 2);
    let end_anchor = line_ref(&before, 3);

    let output = run_identedit_with_stdin(
        &[
            "patch",
            "--at",
            start_anchor.as_str(),
            "--replace-range",
            "--end-anchor",
            end_anchor.as_str(),
            "--stdin-text",
            file_path.to_str().expect("path should be utf-8"),
        ],
        "BETA\nGAMMA",
    );

    assert!(
        output.status.success(),
        "patch replace-range with stdin text failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    assert_eq!(modified, "alpha\nBETA\nGAMMA\ndelta\n");
}

#[test]
fn patch_flag_rejects_inline_text_and_text_file_together() {
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
        "def process_data(value):\n    return value * 22",
        "--text-file",
        payload_path.to_str().expect("payload path should be utf-8"),
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        !output.status.success(),
        "inline and file payload should conflict"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let message = response["error"]["message"]
        .as_str()
        .expect("error message should be present");
    assert!(
        message.contains("--replace")
            && message.contains("--text-file")
            && message.contains("--stdin-text"),
        "error should explain text source conflict, got: {message}"
    );
}

#[test]
fn patch_line_set_line_text_file_preserves_crlf() {
    let file_path = create_temp_text_file("alpha\r\nbeta\r\ngamma\r\n");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let anchor = line_ref(&before, 2);
    let payload_path = create_temp_text_file("BETA");

    let output = run_identedit(&[
        "patch",
        "--at",
        anchor.as_str(),
        "--set-line",
        "--text-file",
        payload_path.to_str().expect("payload path should be utf-8"),
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "set-line with text file failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    assert_eq!(modified, "alpha\r\nBETA\r\ngamma\r\n");
}

#[test]
fn patch_line_replace_range_empty_stdin_deletes_range() {
    let file_path = create_temp_text_file("alpha\nbeta\ngamma\ndelta\n");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let start_anchor = line_ref(&before, 2);
    let end_anchor = line_ref(&before, 3);

    let output = run_identedit_with_stdin(
        &[
            "patch",
            "--at",
            start_anchor.as_str(),
            "--replace-range",
            "--end-anchor",
            end_anchor.as_str(),
            "--stdin-text",
            file_path.to_str().expect("path should be utf-8"),
        ],
        "",
    );

    assert!(
        output.status.success(),
        "replace-range with empty stdin failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    assert_eq!(modified, "alpha\ndelta\n");
}

#[test]
fn patch_line_set_line_empty_stdin_preserves_crlf_line_endings() {
    let file_path = create_temp_text_file("alpha\r\nbeta\r\ngamma\r\n");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let anchor = line_ref(&before, 2);

    let output = run_identedit_with_stdin(
        &[
            "patch",
            "--at",
            anchor.as_str(),
            "--set-line",
            "--stdin-text",
            file_path.to_str().expect("path should be utf-8"),
        ],
        "",
    );

    assert!(
        output.status.success(),
        "set-line with empty stdin failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    assert_eq!(modified, "alpha\r\n\r\ngamma\r\n");
}

#[test]
fn patch_line_insert_after_line_text_file_multiline_preserves_crlf() {
    let file_path = create_temp_text_file("alpha\r\nbeta\r\n");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let anchor = line_ref(&before, 1);
    let payload_path = create_temp_text_file("middle\r\ntail");

    let output = run_identedit(&[
        "patch",
        "--at",
        anchor.as_str(),
        "--insert-after-line",
        "--text-file",
        payload_path.to_str().expect("payload path should be utf-8"),
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "insert-after-line with text file failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    assert_eq!(modified, "alpha\r\nmiddle\r\ntail\r\nbeta\r\n");
}

#[test]
fn patch_set_line_stdin_text_preserves_literal_dash_payload() {
    let file_path = create_temp_text_file("alpha\nbeta\n");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let anchor = line_ref(&before, 2);

    let output = run_identedit_with_stdin(
        &[
            "patch",
            "--at",
            anchor.as_str(),
            "--set-line",
            "--stdin-text",
            file_path.to_str().expect("path should be utf-8"),
        ],
        "--help",
    );

    assert!(
        output.status.success(),
        "set-line with literal dash payload failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    assert_eq!(modified, "alpha\n--help\n");
}

#[test]
fn patch_node_replace_stdin_text_with_line_only_flag_reports_node_guidance() {
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
            "--replace",
            "--stdin-text",
            "--end-anchor",
            "1:aaaaaaaaaaaa",
            file_path.to_str().expect("path should be utf-8"),
        ],
        "def process_data(value):\n    return value * 44",
    );

    assert!(
        !output.status.success(),
        "line-only flags should be rejected before node patch runs"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let message = response["error"]["message"]
        .as_str()
        .expect("error message should be present");
    assert!(
        message.contains("--replace") && message.contains("--insert-before"),
        "error should keep node guidance, got: {message}"
    );
}

#[test]
fn patch_line_set_line_text_file_directory_returns_io_error_without_mutation() {
    let file_path = create_temp_text_file("alpha\nbeta\n");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let anchor = line_ref(&before, 2);
    let payload_dir = Builder::new()
        .prefix("identedit-line-payload-dir")
        .tempdir()
        .expect("temp dir should be created");

    let output = run_identedit(&[
        "patch",
        "--at",
        anchor.as_str(),
        "--set-line",
        "--text-file",
        payload_dir
            .path()
            .to_str()
            .expect("payload path should be utf-8"),
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        !output.status.success(),
        "directory payload should fail for line set"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "io_error");
    let after = fs::read_to_string(&file_path).expect("file should remain readable");
    assert_eq!(after, before);
}

#[test]
fn patch_line_insert_after_line_stdin_dry_run_does_not_modify_file() {
    let file_path = create_temp_text_file("alpha\nbeta\n");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let anchor = line_ref(&before, 1);

    let output = run_identedit_with_stdin(
        &[
            "patch",
            "--at",
            anchor.as_str(),
            "--insert-after-line",
            "--stdin-text",
            "--dry-run",
            file_path.to_str().expect("path should be utf-8"),
        ],
        "middle\nline",
    );

    assert!(
        output.status.success(),
        "insert-after-line stdin dry-run failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let after = fs::read_to_string(&file_path).expect("file should remain readable");
    assert_eq!(after, before);
}

#[cfg(unix)]
#[test]
fn patch_line_rejects_direct_symlink_without_mutating_target() {
    use std::os::unix::fs::symlink;

    let target_path = create_temp_text_file("alpha\nbeta\n");
    let link_directory = tempfile::tempdir().expect("symlink tempdir should be created");
    let link_path = link_directory.path().join("linked.txt");
    symlink(&target_path, &link_path).expect("symlink should be created");
    let before = fs::read_to_string(&target_path).expect("target should be readable");
    let anchor = line_ref(&before, 2);

    let output = run_identedit(&[
        "patch",
        "--at",
        anchor.as_str(),
        "--set-line",
        "changed",
        link_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        !output.status.success(),
        "line patch should reject a direct symlink"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("symbolic link"))
    );
    assert_eq!(
        fs::read_to_string(&target_path).expect("target should remain readable"),
        before
    );
}

#[cfg(unix)]
#[test]
fn patch_line_preserves_file_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let file_path = create_temp_text_file("alpha\nbeta\n");
    let mut permissions = fs::metadata(&file_path)
        .expect("fixture metadata should be readable")
        .permissions();
    permissions.set_mode(0o640);
    fs::set_permissions(&file_path, permissions).expect("permissions should be set");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let anchor = line_ref(&before, 2);

    let output = run_identedit(&[
        "patch",
        "--at",
        anchor.as_str(),
        "--set-line",
        "changed",
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "line patch should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::metadata(&file_path)
            .expect("patched metadata should be readable")
            .permissions()
            .mode()
            & 0o777,
        0o640
    );
}

#[test]
fn patch_set_line_stdin_utf8_bom_payload_preserves_bytes() {
    let file_path = create_temp_text_file("alpha\nbeta\n");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let anchor = line_ref(&before, 2);

    let output = run_identedit_with_stdin(
        &[
            "patch",
            "--at",
            anchor.as_str(),
            "--set-line",
            "--stdin-text",
            file_path.to_str().expect("path should be utf-8"),
        ],
        "\u{FEFF}beta",
    );

    assert!(
        output.status.success(),
        "set-line with BOM stdin payload failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    assert_eq!(modified, "alpha\n\u{FEFF}beta\n");
}

#[test]
fn patch_kind_name_replace_dry_run_diff_does_not_hide_trailing_newline_only_change() {
    let source = "def sample():\n    return 1";
    let file_path = create_temp_python_source(source);

    let output = run_identedit(&[
        "patch",
        "--symbol",
        "sample",
        "--replace",
        "def sample():\n    return 1\n",
        "--dry-run",
        "--diff",
        "--color",
        "never",
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "trailing-newline-only diff should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let diff = String::from_utf8(output.stdout).expect("stdout should be utf-8");
    assert!(
        !diff.trim().is_empty(),
        "raw text changes must not render as an empty diff"
    );
    assert!(diff.contains("@@ -1,2 +1,2 @@"));
    assert!(diff.contains("-def sample():"));
    assert!(diff.contains("+def sample():"));
    assert_eq!(
        fs::read_to_string(&file_path).expect("file should be readable"),
        source,
        "dry-run diff must not modify the source file"
    );
}

#[test]
fn patch_insert_before_writes_at_anchor_start() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_function_handle(&file_path, "helper");
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");

    let output = run_identedit(&[
        "patch",
        "--at",
        identity,
        "--insert-before",
        "# inserted-before-helper\n",
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "patch insert-before failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    assert!(
        modified.contains("# inserted-before-helper\ndef helper():"),
        "insert-before text should appear immediately before helper definition"
    );
}

#[test]
fn patch_insert_after_writes_at_anchor_end() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_function_handle(&file_path, "helper");
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");

    let output = run_identedit(&[
        "patch",
        "--at",
        identity,
        "--insert-after",
        "\n# inserted-after-helper\n",
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "patch insert-after failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    let helper_offset = modified
        .find("def helper():")
        .expect("helper function should still exist");
    let marker_offset = modified
        .find("# inserted-after-helper")
        .expect("insert-after marker should exist");
    assert!(
        marker_offset > helper_offset,
        "insert-after marker should appear after helper definition"
    );
}

#[test]
fn patch_line_flag_set_line_applies_change() {
    let source = "a\nb\n";
    let mut temp_file = Builder::new()
        .suffix(".txt")
        .tempfile()
        .expect("temp text file should be created");
    temp_file
        .write_all(source.as_bytes())
        .expect("fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let anchor = line_ref(source, 2);

    let output = run_identedit(&[
        "patch",
        "--at",
        &anchor,
        "--set-line",
        "B",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "patch line flag set-line failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["applied_mode"], "strict");
    assert_eq!(response["operations_applied"], 1);

    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    assert_eq!(modified, "a\nB\n");
}

#[test]
fn patch_line_flag_set_line_dry_run_previews_without_writing() {
    let source = "a\nb\n";
    let mut temp_file = Builder::new()
        .suffix(".txt")
        .tempfile()
        .expect("temp text file should be created");
    temp_file
        .write_all(source.as_bytes())
        .expect("fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let anchor = line_ref(source, 2);

    let output = run_identedit(&[
        "patch",
        "--at",
        &anchor,
        "--set-line",
        "B",
        "--dry-run",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "patch line flag dry-run failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["dry_run"], true);
    assert_eq!(response["operations_applied"], 1);
    assert_eq!(response["changed"], true);

    let after = fs::read_to_string(&file_path).expect("file should be readable");
    assert_eq!(after, source, "line-mode dry-run must not modify the file");
}

#[test]
fn patch_line_flag_set_line_dry_run_diff_outputs_file_diff_without_writing() {
    let source = "a\nb\n";
    let mut temp_file = Builder::new()
        .suffix(".txt")
        .tempfile()
        .expect("temp text file should be created");
    temp_file
        .write_all(source.as_bytes())
        .expect("fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let anchor = line_ref(source, 2);

    let output = run_identedit(&[
        "patch",
        "--at",
        &anchor,
        "--set-line",
        "B",
        "--dry-run",
        "--diff",
        "--color",
        "never",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "line dry-run diff failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let diff = String::from_utf8(output.stdout).expect("stdout should be utf-8");
    assert!(diff.contains("@@ -1,2 +1,2 @@"));
    assert!(diff.contains("-b"));
    assert!(diff.contains("+B"));
    assert_eq!(
        fs::read_to_string(&file_path).expect("file should be readable"),
        source,
        "line dry-run diff must not modify the file"
    );
}

#[test]
fn patch_line_flag_replace_range_supports_end_anchor() {
    let source = "a\nb\nc\n";
    let mut temp_file = Builder::new()
        .suffix(".txt")
        .tempfile()
        .expect("temp text file should be created");
    temp_file
        .write_all(source.as_bytes())
        .expect("fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let anchor = line_ref(source, 2);
    let end_anchor = line_ref(source, 3);

    let output = run_identedit(&[
        "patch",
        "--at",
        &anchor,
        "--end-anchor",
        &end_anchor,
        "--replace-range",
        "x\ny",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "patch line flag replace-range failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    assert_eq!(modified, "a\nx\ny\n");
}

#[test]
fn patch_line_flag_insert_after_line_applies_change() {
    let source = "a\nb\n";
    let mut temp_file = Builder::new()
        .suffix(".txt")
        .tempfile()
        .expect("temp text file should be created");
    temp_file
        .write_all(source.as_bytes())
        .expect("fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let anchor = line_ref(source, 1);

    let output = run_identedit(&[
        "patch",
        "--at",
        &anchor,
        "--insert-after-line",
        "x",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "patch line flag insert-after-line failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    assert_eq!(modified, "a\nx\nb\n");
}

#[test]
fn patch_line_flag_supports_auto_repair() {
    let source = "a\nb\na\n";
    let mut temp_file = Builder::new()
        .suffix(".txt")
        .tempfile()
        .expect("temp text file should be created");
    temp_file
        .write_all(source.as_bytes())
        .expect("fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let stale_anchor = format!("1:{}", crate::common::compute_line_hash("b"));

    let output = run_identedit(&[
        "patch",
        "--at",
        &stale_anchor,
        "--set-line",
        "B",
        "--auto-repair",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "patch line flag auto-repair failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["applied_mode"], "repair");
    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    assert_eq!(modified, "a\nB\na\n");
}

#[test]
fn patch_line_flag_auto_repair_dry_run_does_not_modify_file() {
    let source = "a\nb\na\n";
    let mut temp_file = Builder::new()
        .suffix(".txt")
        .tempfile()
        .expect("temp text file should be created");
    temp_file
        .write_all(source.as_bytes())
        .expect("fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let stale_anchor = format!("1:{}", crate::common::compute_line_hash("b"));

    let output = run_identedit(&[
        "patch",
        "--at",
        &stale_anchor,
        "--set-line",
        "B",
        "--auto-repair",
        "--dry-run",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "patch line flag auto-repair dry-run failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["applied_mode"], "repair");
    assert_eq!(response["dry_run"], true);
    assert_eq!(
        fs::read_to_string(&file_path).expect("file should be readable"),
        source,
        "dry-run repair must not modify the file"
    );
}

#[test]
fn patch_flag_rejects_at_and_config_path_together() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_function_handle(&file_path, "process_*");
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");

    let output = run_identedit(&[
        "patch",
        "--at",
        identity,
        "--config-path",
        "process_data",
        "--delete",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "patch should reject mixed target selection"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let message = response["error"]["message"]
        .as_str()
        .expect("error message should be present");
    assert!(
        message.contains("--config-path cannot be combined")
            && message.contains("--at")
            && message.contains("--config-path"),
        "mixed target error should list the valid selector families"
    );
}

#[test]
fn patch_flag_rejects_line_target_with_node_operation() {
    let source = "a\nb\n";
    let mut temp_file = Builder::new()
        .suffix(".txt")
        .tempfile()
        .expect("temp text file should be created");
    temp_file
        .write_all(source.as_bytes())
        .expect("fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let anchor = line_ref(source, 1);

    let output = run_identedit(&[
        "patch",
        "--at",
        &anchor,
        "--replace",
        "x",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "line target should reject node operation flags"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let message = response["error"]["message"]
        .as_str()
        .expect("error message should be present");
    assert!(
        message.contains("--set-line")
            && message.contains("--replace-range")
            && message.contains("--insert-after-line")
            && message.contains("--at"),
        "line-mode error should list valid line flags and point back to node targeting"
    );
}

#[test]
fn patch_flag_rejects_node_target_with_line_operation() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_function_handle(&file_path, "process_*");
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");

    let output = run_identedit(&[
        "patch",
        "--at",
        identity,
        "--set-line",
        "x",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "node target should reject line operation flags"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let message = response["error"]["message"]
        .as_str()
        .expect("error message should be present");
    assert!(
        message.contains("--replace")
            && message.contains("--delete")
            && message.contains("--insert-before")
            && message.contains("--at"),
        "node-mode error should list valid node flags and point to line targeting"
    );
}

#[test]
fn patch_flag_rejects_end_anchor_without_replace_range() {
    let source = "a\nb\n";
    let mut temp_file = Builder::new()
        .suffix(".txt")
        .tempfile()
        .expect("temp text file should be created");
    temp_file
        .write_all(source.as_bytes())
        .expect("fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let anchor = line_ref(source, 1);
    let end_anchor = line_ref(source, 2);

    let output = run_identedit(&[
        "patch",
        "--at",
        &anchor,
        "--end-anchor",
        &end_anchor,
        "--set-line",
        "x",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "--end-anchor should be rejected when --replace-range is not selected"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}

#[test]
fn patch_flag_rejects_multiple_line_operations() {
    let source = "a\nb\n";
    let mut temp_file = Builder::new()
        .suffix(".txt")
        .tempfile()
        .expect("temp text file should be created");
    temp_file
        .write_all(source.as_bytes())
        .expect("fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let anchor = line_ref(source, 1);

    let output = run_identedit(&[
        "patch",
        "--at",
        &anchor,
        "--set-line",
        "x",
        "--replace-range",
        "y",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "patch should reject multiple line operations in one request"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}

#[test]
fn patch_json_line_target_set_line_applies_change() {
    let source = "a\nb\n";
    let mut temp_file = Builder::new()
        .suffix(".txt")
        .tempfile()
        .expect("temp text file should be created");
    temp_file
        .write_all(source.as_bytes())
        .expect("fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "line",
            "anchor": line_ref(source, 2)
        },
        "op": {
            "type": "set_line",
            "new_text": "B"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "patch --json line set_line failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["applied_mode"], "strict");
    assert_eq!(response["operations_applied"], 1);

    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    assert_eq!(modified, "a\nB\n");
}

#[test]
fn patch_json_line_target_options_dry_run_does_not_modify_file() {
    let source = "a\nb\n";
    let mut temp_file = Builder::new()
        .suffix(".txt")
        .tempfile()
        .expect("temp text file should be created");
    temp_file
        .write_all(source.as_bytes())
        .expect("fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let anchor = line_ref(source, 2);
    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "line",
            "anchor": anchor
        },
        "op": {
            "type": "set_line",
            "new_text": "B"
        },
        "options": {
            "dry_run": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "json line dry-run should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["dry_run"], true);
    assert_eq!(
        fs::read_to_string(&file_path).expect("file should be readable"),
        source,
        "json line dry-run must not modify the file"
    );
}

#[test]
fn patch_json_cli_dry_run_overrides_line_request_and_does_not_modify_file() {
    let source = "a\nb\n";
    let mut temp_file = Builder::new()
        .suffix(".txt")
        .tempfile()
        .expect("temp text file should be created");
    temp_file
        .write_all(source.as_bytes())
        .expect("fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "line",
            "anchor": line_ref(source, 2)
        },
        "op": {
            "type": "set_line",
            "new_text": "B"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json", "--dry-run"], &request.to_string());
    assert!(
        output.status.success(),
        "json cli line dry-run should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["dry_run"], true);
    assert_eq!(
        fs::read_to_string(&file_path).expect("file should be readable"),
        source,
        "cli dry-run in json mode must not modify line target files"
    );
}

#[test]
fn patch_json_line_target_replace_lines_supports_end_anchor() {
    let source = "a\nb\nc\n";
    let mut temp_file = Builder::new()
        .suffix(".txt")
        .tempfile()
        .expect("temp text file should be created");
    temp_file
        .write_all(source.as_bytes())
        .expect("fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "line",
            "anchor": line_ref(source, 2),
            "end_anchor": line_ref(source, 3)
        },
        "op": {
            "type": "replace_lines",
            "new_text": "x\ny"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "patch --json line replace_lines failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    assert_eq!(modified, "a\nx\ny\n");
}

#[test]
fn patch_json_line_target_can_auto_repair() {
    let source = "a\nb\na\n";
    let mut temp_file = Builder::new()
        .suffix(".txt")
        .tempfile()
        .expect("temp text file should be created");
    temp_file
        .write_all(source.as_bytes())
        .expect("fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let stale_anchor = format!("1:{}", crate::common::compute_line_hash("b"));
    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "line",
            "anchor": stale_anchor
        },
        "op": {
            "type": "set_line",
            "new_text": "B"
        },
        "options": {
            "auto_repair": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "patch --json line auto-repair failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["applied_mode"], "repair");

    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    assert_eq!(modified, "a\nB\na\n");
}

#[test]
fn patch_json_line_target_auto_repair_dry_run_ambiguous_keeps_file_unchanged() {
    let source = "a\nb\na\n";
    let mut temp_file = Builder::new()
        .suffix(".txt")
        .tempfile()
        .expect("temp text file should be created");
    temp_file
        .write_all(source.as_bytes())
        .expect("fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let stale_anchor = format!("2:{}", crate::common::compute_line_hash("z"));
    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "line",
            "anchor": stale_anchor
        },
        "op": {
            "type": "set_line",
            "new_text": "B"
        },
        "options": {
            "dry_run": true,
            "auto_repair": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "mismatched repair dry-run should still fail"
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert_eq!(
        fs::read_to_string(&file_path).expect("file should be readable"),
        source,
        "failing json line dry-run must not modify the file"
    );
}

#[test]
fn patch_json_rejects_node_target_with_line_only_op() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_function_handle(&file_path, "process_*");
    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "node",
            "identity": handle["identity"],
            "kind": handle["kind"],
            "expected_old_hash": crate::common::hash_text(
                handle["text"].as_str().expect("text should be string")
            )
        },
        "op": {
            "type": "set_line",
            "new_text": "x"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "node target should reject line-only operation payload"
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}

#[test]
fn patch_json_rejects_line_target_with_node_only_op() {
    let source = "a\nb\n";
    let mut temp_file = Builder::new()
        .suffix(".txt")
        .tempfile()
        .expect("temp text file should be created");
    temp_file
        .write_all(source.as_bytes())
        .expect("fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "line",
            "anchor": line_ref(source, 2)
        },
        "op": {
            "type": "delete"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "line target should reject node-only operation payload"
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}
