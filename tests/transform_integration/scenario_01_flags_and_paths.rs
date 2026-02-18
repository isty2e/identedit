use super::*;

fn assert_compact_preview_old_state(preview: &Value, expected_old_text: &str) {
    assert!(
        preview.get("old_text").is_none(),
        "compact preview should omit old_text by default"
    );
    assert_eq!(
        preview["old_hash"],
        identedit::changeset::hash_text(expected_old_text),
        "compact preview should include old_hash"
    );
    assert_eq!(
        preview["old_len"],
        expected_old_text.len(),
        "compact preview should include old_len"
    );
}

#[test]
fn transform_flags_mode_builds_changeset_preview() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let handle = select_first_handle(&file_path, "function_definition", Some("process_*"));
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");

    let replacement = "def process_data(x, y):\n    return x + y";
    let output = run_identedit(&[
        "transform",
        "--identity",
        identity,
        "--replace",
        replacement,
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "transform failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(
        response["files"][0]["operations"].as_array().map(Vec::len),
        Some(1)
    );
    assert_eq!(
        response["files"][0]["operations"][0]["target"]["identity"],
        identity
    );
    assert_eq!(
        response["files"][0]["operations"][0]["op"]["type"],
        "replace"
    );
    assert_eq!(
        response["files"][0]["operations"][0]["preview"]["new_text"],
        replacement
    );

    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(
        before, after,
        "transform must remain dry-run and never mutate source files on success"
    );
}

#[test]
fn transform_flags_mode_supports_crlf_source_files() {
    let mut temp_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp python file should be created");
    let source = "def process_data(value):\r\n    result = value + 1\r\n    return result\r\n\r\n\r\ndef helper():\r\n    return \"helper\"\r\n";
    temp_file
        .write_all(source.as_bytes())
        .expect("crlf fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let handle = select_first_handle(&file_path, "function_definition", Some("process_*"));
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");
    let replacement = "def process_data(value):\r\n    return value + 2\r\n";

    let output = run_identedit(&[
        "transform",
        "--identity",
        identity,
        "--replace",
        replacement,
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "transform should support CRLF sources: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    let old_text = handle["text"].as_str().expect("text should be string");
    assert!(
        old_text.contains("\r\n"),
        "selected function text should preserve CRLF line endings"
    );
    let preview = &response["files"][0]["operations"][0]["preview"];
    assert_compact_preview_old_state(preview, old_text);
}

#[test]
fn transform_flags_mode_supports_cr_only_source_files() {
    let mut temp_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp python file should be created");
    let source = "def process_data(value):\r    result = value + 1\r    return result\r\rdef helper():\r    return \"helper\"\r";
    temp_file
        .write_all(source.as_bytes())
        .expect("cr-only fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let handle = select_first_handle(&file_path, "function_definition", Some("process_*"));
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");
    let replacement = "def process_data(value):\r    return value + 2\r";

    let output = run_identedit(&[
        "transform",
        "--identity",
        identity,
        "--replace",
        replacement,
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "transform should support CR-only sources: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    let old_text = handle["text"].as_str().expect("text should be string");
    let preview = &response["files"][0]["operations"][0]["preview"];
    assert_compact_preview_old_state(preview, old_text);
    assert!(
        old_text.contains("\r"),
        "selected function text should preserve CR line endings for CR-only source"
    );
}

#[test]
fn transform_flags_mode_supports_utf8_bom_prefixed_python_files() {
    let mut temp_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp python file should be created");
    temp_file
        .write_all(b"\xEF\xBB\xBFdef process_data(value):\n    return value + 1\n")
        .expect("bom python fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let handle = select_first_handle(&file_path, "function_definition", Some("process_*"));
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");

    let output = run_identedit(&[
        "transform",
        "--identity",
        identity,
        "--replace",
        "def process_data(value):\n    return value + 2",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "transform should support UTF-8 BOM python source: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(
        response["files"][0]["operations"].as_array().map(Vec::len),
        Some(1)
    );
    assert_eq!(
        response["files"][0]["operations"][0]["preview"]["matched_span"]["start"],
        3
    );
}

#[test]
fn transform_flags_mode_preserves_mixed_line_endings_in_preview() {
    let mut temp_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp python file should be created");
    let source = "def process_data(value):\r\n    first = value + 1\r    return first\n\ndef helper():\r\n    return \"helper\"\r";
    temp_file
        .write_all(source.as_bytes())
        .expect("mixed line-ending fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let handle = select_first_handle(&file_path, "function_definition", Some("process_*"));
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");
    let replacement = "def process_data(value):\n    return value + 2\n";

    let output = run_identedit(&[
        "transform",
        "--identity",
        identity,
        "--replace",
        replacement,
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "transform should support mixed line endings: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    let old_text = handle["text"].as_str().expect("text should be string");
    let preview = &response["files"][0]["operations"][0]["preview"];
    assert_compact_preview_old_state(preview, old_text);
    assert!(
        old_text.contains("\r\n"),
        "selected function text should retain CRLF segments in mixed-ending source"
    );
    assert!(
        old_text.contains("\r    return"),
        "selected function text should retain bare CR segments in mixed-ending source"
    );
}

#[test]
fn transform_json_mode_preserves_nul_in_replacement_preview() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_first_handle(&file_path, "function_definition", Some("process_*"));
    let replacement_with_nul = "def process_data(value):\n    return \"A\u{0000}B\"";
    let request = json!({
        "command": "transform",
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "identity": handle["identity"],
                "kind": handle["kind"],
                "span_hint": {
                    "start": handle["span"]["start"],
                    "end": handle["span"]["end"]
                },
                "expected_old_hash": identedit::changeset::hash_text(
                    handle["text"].as_str().expect("text should be string")
                ),
                "op": {
                    "type": "replace",
                    "new_text": replacement_with_nul
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["transform", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "transform should accept NUL-containing replacement text: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    let new_text = response["files"][0]["operations"][0]["preview"]["new_text"]
        .as_str()
        .expect("new_text should be string");
    assert!(
        new_text.contains('\0'),
        "preview new_text should preserve embedded NUL"
    );
}

#[test]
fn transform_handles_large_python_files_within_reasonable_time() {
    let file_path = create_large_python_file(400);
    let handle = select_first_handle(&file_path, "function_definition", Some("function_0320"));
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");
    let start = Instant::now();

    let output = run_identedit(&[
        "transform",
        "--identity",
        identity,
        "--replace",
        "def function_0320(value):\n    return value * 2",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    let elapsed = start.elapsed();

    assert!(
        output.status.success(),
        "transform should succeed on large fixture: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(
        response["files"][0]["operations"].as_array().map(Vec::len),
        Some(1)
    );
    assert!(
        elapsed < Duration::from_secs(8),
        "transform took too long on large fixture: {elapsed:?}"
    );
}

#[test]
fn transform_flags_mode_requires_identity_argument() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let output = run_identedit(&[
        "transform",
        "--replace",
        "def process_data(value):\n    return value",
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        !output.status.success(),
        "transform should fail without --identity"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("--identity is required")),
        "expected missing identity message"
    );
}

#[test]
fn transform_flags_mode_requires_operation_argument() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_first_handle(&file_path, "function_definition", Some("process_*"));
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");

    let output = run_identedit(&[
        "transform",
        "--identity",
        identity,
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        !output.status.success(),
        "transform should fail without --replace/--delete"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("--replace or --delete is required")),
        "expected missing operation message"
    );
}

#[test]
fn transform_flags_mode_supports_delete_argument() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let handle = select_first_handle(&file_path, "function_definition", Some("process_*"));
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");

    let output = run_identedit(&[
        "transform",
        "--identity",
        identity,
        "--delete",
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "transform should support --delete: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(
        response["files"][0]["operations"].as_array().map(Vec::len),
        Some(1)
    );
    assert_eq!(
        response["files"][0]["operations"][0]["op"]["type"],
        "delete"
    );
    let old_text = handle["text"].as_str().expect("text should be string");
    let preview = &response["files"][0]["operations"][0]["preview"];
    assert_compact_preview_old_state(preview, old_text);
    assert_eq!(preview["new_text"], "");
    assert_eq!(preview["matched_span"]["start"], handle["span"]["start"]);
    assert_eq!(preview["matched_span"]["end"], handle["span"]["end"]);

    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "transform flags mode must stay dry-run");
}

#[test]
fn transform_flags_mode_rejects_replace_and_delete_together() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_first_handle(&file_path, "function_definition", Some("process_*"));
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");

    let output = run_identedit(&[
        "transform",
        "--identity",
        identity,
        "--replace",
        "def process_data(value):\n    return value + 100",
        "--delete",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "transform should reject --replace with --delete"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("cannot be used together")),
        "expected mutually-exclusive operation message"
    );
}

#[cfg(unix)]
#[test]
fn transform_flags_mode_supports_shell_variable_expanded_path() {
    let workspace = tempdir().expect("tempdir should be created");
    let file_path = workspace.path().join("example.py");
    let source =
        fs::read_to_string(fixture_path("example.py")).expect("fixture should be readable");
    fs::write(&file_path, source).expect("fixture write should succeed");

    let handle = select_first_handle(&file_path, "function_definition", Some("process_*"));
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");

    let output = run_shell_script(
        "\"$IDENTEDIT_BIN\" transform --identity \"$IDENTEDIT_IDENTITY\" --replace \"def process_data(value): return value + 9\" \"${IDENTEDIT_ROOT}/example.py\"",
        workspace.path(),
        Some(identity),
    );
    assert!(
        output.status.success(),
        "transform via shell-expanded path should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(
        response["files"][0]["operations"].as_array().map(Vec::len),
        Some(1)
    );
}

#[cfg(unix)]
#[test]
fn transform_flags_mode_single_quoted_env_token_path_remains_literal() {
    let workspace = tempdir().expect("tempdir should be created");
    let file_path = workspace.path().join("example.py");
    let source =
        fs::read_to_string(fixture_path("example.py")).expect("fixture should be readable");
    fs::write(&file_path, source).expect("fixture write should succeed");

    let output = run_shell_script(
        "\"$IDENTEDIT_BIN\" transform --identity placeholder --replace \"def process_data(value): return value\" '${IDENTEDIT_ROOT}/example.py'",
        workspace.path(),
        None,
    );
    assert!(
        !output.status.success(),
        "single-quoted env token path should remain literal and fail"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "io_error");
}

#[test]
fn transform_returns_parse_failure_for_syntax_invalid_python_file() {
    let mut temporary_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp file should be created");
    temporary_file
        .write_all(b"def broken(:\n    pass\n")
        .expect("invalid python fixture write should succeed");
    let temp_path = temporary_file.path().to_path_buf();

    let output = run_identedit(&[
        "transform",
        "--identity",
        "irrelevant",
        "--replace",
        "def replacement():\n    return 1",
        temp_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "transform should fail for syntax-invalid python input"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "parse_failure");
}

#[test]
fn transform_parse_failure_does_not_modify_invalid_source_file() {
    let mut temporary_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp file should be created");
    temporary_file
        .write_all(b"def broken(:\n    pass\n")
        .expect("invalid fixture write should succeed");
    let file_path = temporary_file.path().to_path_buf();
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let output = run_identedit(&[
        "transform",
        "--identity",
        "irrelevant",
        "--replace",
        "def fixed():\n    return 1",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "transform should fail for syntax-invalid python input"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "parse_failure");

    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(
        before, after,
        "transform parse-failure path must not mutate source files"
    );
}

#[test]
fn transform_returns_parse_failure_for_partially_binary_python_file() {
    let mut temporary_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp file should be created");
    temporary_file
        .write_all(b"def process_data(value):\n    return value + 1\n\xff\n")
        .expect("binary-like python fixture write should succeed");
    let temp_path = temporary_file.path().to_path_buf();

    let output = run_identedit(&[
        "transform",
        "--identity",
        "irrelevant",
        "--replace",
        "def replacement():\n    return 1",
        temp_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "transform should fail for partially-binary python payload"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "parse_failure");
}

#[test]
fn transform_returns_parse_failure_for_nul_in_python_source() {
    let mut temporary_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp file should be created");
    temporary_file
        .write_all(b"def process_data(value):\n    return value + 1\n\x00\n")
        .expect("nul python fixture write should succeed");
    let temp_path = temporary_file.path().to_path_buf();

    let output = run_identedit(&[
        "transform",
        "--identity",
        "irrelevant",
        "--replace",
        "def replacement():\n    return 1",
        temp_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "transform should fail for python source containing embedded NUL"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "parse_failure");
}

#[test]
fn transform_returns_parse_failure_for_bom_plus_nul_python_source() {
    let mut temporary_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp file should be created");
    temporary_file
        .write_all(b"\xEF\xBB\xBFdef process_data(value):\n    return value + 1\n\x00\n")
        .expect("bom+nul python fixture write should succeed");
    let temp_path = temporary_file.path().to_path_buf();

    let output = run_identedit(&[
        "transform",
        "--identity",
        "irrelevant",
        "--replace",
        "def replacement():\n    return 1",
        temp_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "transform should fail for python source containing BOM and embedded NUL"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "parse_failure");
}

#[test]
fn transform_returns_target_missing_for_unknown_identity() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let output = run_identedit(&[
        "transform",
        "--identity",
        "does-not-exist",
        "--replace",
        "def process_data(x):\n    return x",
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        !output.status.success(),
        "transform should fail for unknown identity"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "target_missing");
}

#[test]
fn transform_returns_ambiguous_target_when_identity_matches_multiple_nodes() {
    let fixture = fixture_path("ambiguous.py");
    let handle = select_first_handle(&fixture, "function_definition", Some("duplicate"));
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");

    let output = run_identedit(&[
        "transform",
        "--identity",
        identity,
        "--replace",
        "def duplicate():\n    return 2",
        fixture.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        !output.status.success(),
        "transform should fail for ambiguous identity"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "ambiguous_target");
}
