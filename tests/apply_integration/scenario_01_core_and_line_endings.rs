use super::*;

#[test]
fn apply_changeset_file_modifies_target_file() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_handle(&file_path, "process_*");
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");

    let replacement = "def process_data(value):\n    return value * 2";
    let transform_output = run_identedit(&[
        "edit",
        "--identity",
        identity,
        "--replace",
        replacement,
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        transform_output.status.success(),
        "transform failed: {}",
        String::from_utf8_lossy(&transform_output.stderr)
    );

    let changeset_file = write_changeset_json(
        std::str::from_utf8(&transform_output.stdout).expect("changeset should be utf-8"),
    );
    let apply_output = run_identedit(&[
        "apply",
        changeset_file
            .path()
            .to_str()
            .expect("changeset path should be utf-8"),
    ]);
    assert!(
        apply_output.status.success(),
        "apply failed: {}",
        String::from_utf8_lossy(&apply_output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&apply_output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["summary"]["files_modified"], 1);
    assert_eq!(response["summary"]["operations_applied"], 1);
    assert_eq!(response["summary"]["operations_failed"], 0);

    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    assert!(modified.contains("return value * 2"));
}

#[cfg(unix)]
#[test]
fn apply_changeset_argument_supports_shell_variable_expanded_path() {
    let workspace = tempdir().expect("tempdir should be created");
    let target_path = workspace.path().join("example.py");
    let source =
        fs::read_to_string(fixture_path("example.py")).expect("fixture should be readable");
    fs::write(&target_path, source).expect("fixture write should succeed");

    let handle = select_named_handle(&target_path, "process_*");
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");
    let transform_output = run_identedit(&[
        "edit",
        "--identity",
        identity,
        "--replace",
        "def process_data(value):\n    return value * 7",
        target_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        transform_output.status.success(),
        "transform failed: {}",
        String::from_utf8_lossy(&transform_output.stderr)
    );

    let changeset_path = workspace.path().join("changeset.json");
    fs::write(&changeset_path, &transform_output.stdout).expect("changeset write should succeed");

    let output = run_shell_script(
        "\"$IDENTEDIT_BIN\" apply \"${IDENTEDIT_ROOT}/changeset.json\"",
        workspace.path(),
    );
    assert!(
        output.status.success(),
        "apply via shell-expanded changeset path should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["summary"]["operations_applied"], 1);

    let modified = fs::read_to_string(&target_path).expect("target should be readable");
    assert!(modified.contains("return value * 7"));
}

#[cfg(unix)]
#[test]
fn apply_changeset_argument_single_quoted_env_token_path_remains_literal() {
    let workspace = tempdir().expect("tempdir should be created");
    let changeset_path = workspace.path().join("changeset.json");
    fs::write(&changeset_path, r#"{"file":"example.py","operations":[]}"#)
        .expect("changeset write should succeed");

    let output = run_shell_script(
        "\"$IDENTEDIT_BIN\" apply '${IDENTEDIT_ROOT}/changeset.json'",
        workspace.path(),
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
fn apply_handles_large_batches_of_operations_within_reasonable_time() {
    let file_path = create_large_python_file(150);
    let select_output = run_identedit(&[
        "read",
        "--json",
        "--verbose",
        "--kind",
        "function_definition",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        select_output.status.success(),
        "select failed: {}",
        String::from_utf8_lossy(&select_output.stderr)
    );

    let select_response: Value =
        serde_json::from_slice(&select_output.stdout).expect("stdout should be valid JSON");
    let handles = select_response["handles"]
        .as_array()
        .expect("handles should be array");
    assert!(
        handles.len() >= 80,
        "expected enough handles in generated fixture"
    );

    let operations = handles
        .iter()
        .take(80)
        .map(|handle| {
            let name = handle["name"]
                .as_str()
                .expect("name should be present for function");
            let replacement = format!("def {name}(value):\n    return value * 2");
            json!({
                "target": {
                    "identity": handle["identity"],
                    "kind": handle["kind"],
                    "span_hint": {
                        "start": handle["span"]["start"],
                        "end": handle["span"]["end"]
                    },
                    "expected_old_hash": identedit::changeset::hash_text(
                        handle["text"].as_str().expect("text should be string")
                    )
                },
                "op": {
                    "type": "replace",
                    "new_text": replacement
                },
                "preview": {
                    "old_text": handle["text"],
                    "new_text": replacement,
                    "matched_span": {
                        "start": handle["span"]["start"],
                        "end": handle["span"]["end"]
                    }
                }
            })
        })
        .collect::<Vec<_>>();

    let request = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": operations
    });

    let start = Instant::now();
    let output = run_identedit_with_stdin(&["apply"], &request.to_string());
    let elapsed = start.elapsed();

    assert!(
        output.status.success(),
        "apply should succeed for large batch: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["summary"]["operations_applied"], 80);
    assert_eq!(response["summary"]["operations_failed"], 0);
    assert!(
        elapsed < Duration::from_secs(8),
        "apply took too long for large operation batch: {elapsed:?}"
    );
}

#[test]
fn apply_handles_crlf_files_without_normalizing_line_endings() {
    let mut temp_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp python file should be created");
    let source = "def process_data(value):\r\n    result = value + 1\r\n    return result\r\n\r\n\r\ndef helper():\r\n    return \"helper\"\r\n";
    temp_file
        .write_all(source.as_bytes())
        .expect("crlf fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let handle = select_named_handle(&file_path, "process_*");
    let span = &handle["span"];
    let replacement = "def process_data(value):\r\n    return value + 11\r\n";
    let expected_hash =
        identedit::changeset::hash_text(handle["text"].as_str().expect("text should be string"));

    let changeset = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "identity": handle["identity"],
                    "kind": handle["kind"],
                    "span_hint": {"start": span["start"], "end": span["end"]},
                    "expected_old_hash": expected_hash
                },
                "op": {"type": "replace", "new_text": replacement},
                "preview": {
                    "old_text": handle["text"],
                    "new_text": replacement,
                    "matched_span": {"start": span["start"], "end": span["end"]}
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["apply"], &changeset.to_string());
    assert!(
        output.status.success(),
        "apply should handle CRLF inputs: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("file should be readable");
    assert!(
        modified.contains("\r\n"),
        "modified file should still contain CRLF line endings"
    );
    assert!(modified.contains("return value + 11\r\n"));
    assert!(modified.contains("def helper():\r\n"));
}

#[test]
fn apply_handles_cr_only_files_without_normalizing_line_endings() {
    let mut temp_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp python file should be created");
    let source = "def process_data(value):\r    result = value + 1\r    return result\r\rdef helper():\r    return \"helper\"\r";
    temp_file
        .write_all(source.as_bytes())
        .expect("cr-only fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let handle = select_named_handle(&file_path, "process_*");
    let span = &handle["span"];
    let replacement = "def process_data(value):\r    return value + 11\r";
    let expected_hash =
        identedit::changeset::hash_text(handle["text"].as_str().expect("text should be string"));

    let changeset = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "identity": handle["identity"],
                    "kind": handle["kind"],
                    "span_hint": {"start": span["start"], "end": span["end"]},
                    "expected_old_hash": expected_hash
                },
                "op": {"type": "replace", "new_text": replacement},
                "preview": {
                    "old_text": handle["text"],
                    "new_text": replacement,
                    "matched_span": {"start": span["start"], "end": span["end"]}
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["apply"], &changeset.to_string());
    assert!(
        output.status.success(),
        "apply should handle CR-only inputs: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("file should be readable");
    assert!(
        modified.contains("\r"),
        "modified file should retain CR-only line endings"
    );
    assert!(modified.contains("return value + 11\r"));
    assert!(modified.contains("def helper():\r"));
}

#[test]
fn apply_preserves_utf8_bom_prefix_after_replacement() {
    let mut temp_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp python file should be created");
    temp_file
        .write_all(b"\xEF\xBB\xBFdef process_data(value):\n    return value + 1\n\ndef helper():\n    return \"helper\"\n")
        .expect("bom python fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let handle = select_named_handle(&file_path, "process_*");
    let span = &handle["span"];
    let replacement = "def process_data(value):\n    return value + 11";
    let expected_hash =
        identedit::changeset::hash_text(handle["text"].as_str().expect("text should be string"));

    let changeset = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "identity": handle["identity"],
                    "kind": handle["kind"],
                    "span_hint": {"start": span["start"], "end": span["end"]},
                    "expected_old_hash": expected_hash
                },
                "op": {"type": "replace", "new_text": replacement},
                "preview": {
                    "old_text": handle["text"],
                    "new_text": replacement,
                    "matched_span": {"start": span["start"], "end": span["end"]}
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["apply"], &changeset.to_string());
    assert!(
        output.status.success(),
        "apply should support UTF-8 BOM python input: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let modified_bytes = fs::read(&file_path).expect("file should be readable as bytes");
    assert!(
        modified_bytes.starts_with(&[0xEF, 0xBB, 0xBF]),
        "apply should preserve leading UTF-8 BOM bytes"
    );
}

#[test]
fn apply_preserves_mixed_line_endings_outside_replaced_span() {
    let mut temp_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp python file should be created");
    let source = "def process_data(value):\r\n    first = value + 1\r    return first\n\ndef helper():\r\n    return \"helper\"\r";
    temp_file
        .write_all(source.as_bytes())
        .expect("mixed line-ending fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let handle = select_named_handle(&file_path, "process_*");
    let span = &handle["span"];
    let replacement = "def process_data(value):\n    return value + 11\n";
    let expected_hash =
        identedit::changeset::hash_text(handle["text"].as_str().expect("text should be string"));

    let changeset = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "identity": handle["identity"],
                    "kind": handle["kind"],
                    "span_hint": {"start": span["start"], "end": span["end"]},
                    "expected_old_hash": expected_hash
                },
                "op": {"type": "replace", "new_text": replacement},
                "preview": {
                    "old_text": handle["text"],
                    "new_text": replacement,
                    "matched_span": {"start": span["start"], "end": span["end"]}
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["apply"], &changeset.to_string());
    assert!(
        output.status.success(),
        "apply should support mixed line-ending sources: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("file should be readable");
    assert!(
        modified.contains("def helper():\r\n    return \"helper\"\r"),
        "apply must not normalize untouched mixed-ending segments"
    );
    assert!(
        modified.contains("return value + 11\n"),
        "replacement should be written exactly as requested"
    );
}

#[test]
fn apply_returns_precondition_failed_when_expected_hash_is_stale() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_handle(&file_path, "process_*");
    let span = &handle["span"];
    let request_changeset = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "identity": handle["identity"],
                    "kind": handle["kind"],
                    "span_hint": {
                        "start": span["start"],
                        "end": span["end"]
                    },
                    "expected_old_hash": "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
                },
                "op": {
                    "type": "replace",
                    "new_text": "def process_data(value):\n    return value"
                },
                "preview": {
                    "old_text": handle["text"],
                    "new_text": "def process_data(value):\n    return value",
                    "matched_span": {
                        "start": span["start"],
                        "end": span["end"]
                    }
                }
            }
        ]
    });

    let changeset_file = write_changeset_json(&request_changeset.to_string());
    let apply_output = run_identedit(&[
        "apply",
        changeset_file
            .path()
            .to_str()
            .expect("changeset path should be utf-8"),
    ]);
    assert!(
        !apply_output.status.success(),
        "apply should fail for stale hash"
    );

    let response: Value =
        serde_json::from_slice(&apply_output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "precondition_failed");
}

#[test]
fn apply_returns_precondition_failed_when_identity_is_stale_but_span_matches() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_handle(&file_path, "process_*");
    let span = &handle["span"];
    fs::write(
        &file_path,
        "def process_data(value):\n    result = value + 2\n    return result\n\n\ndef helper():\n    return \"helper\"",
    )
    .expect("fixture mutation should succeed");

    let request_changeset = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "identity": handle["identity"],
                    "kind": handle["kind"],
                    "span_hint": {
                        "start": span["start"],
                        "end": span["end"]
                    },
                    "expected_old_hash": identedit::changeset::hash_text(
                        handle["text"].as_str().expect("text should be string")
                    )
                },
                "op": {
                    "type": "replace",
                    "new_text": "def process_data(value):\n    return value * 2"
                },
                "preview": {
                    "old_text": handle["text"],
                    "new_text": "def process_data(value):\n    return value * 2",
                    "matched_span": {
                        "start": span["start"],
                        "end": span["end"]
                    }
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["apply"], &request_changeset.to_string());
    assert!(
        !output.status.success(),
        "apply should classify stale identity as precondition failure when span matches"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "precondition_failed");
}

#[test]
fn apply_treats_canonically_equivalent_unicode_reorder_as_stale_precondition() {
    let mut temp_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp python file should be created");
    let original_source = "def process_data(value):\n    return \"a\u{0301}\u{0323}\"\n";
    temp_file
        .write_all(original_source.as_bytes())
        .expect("unicode fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let handle = select_named_handle(&file_path, "process_*");
    let span = &handle["span"];
    let expected_hash =
        identedit::changeset::hash_text(handle["text"].as_str().expect("text should be string"));

    let mutated_source = "def process_data(value):\n    return \"a\u{0323}\u{0301}\"\n";
    fs::write(&file_path, mutated_source).expect("fixture mutation should succeed");

    let request_changeset = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "identity": handle["identity"],
                    "kind": handle["kind"],
                    "span_hint": {
                        "start": span["start"],
                        "end": span["end"]
                    },
                    "expected_old_hash": expected_hash
                },
                "op": {
                    "type": "replace",
                    "new_text": "def process_data(value):\n    return \"patched\""
                },
                "preview": {
                    "old_text": handle["text"],
                    "new_text": "def process_data(value):\n    return \"patched\"",
                    "matched_span": {
                        "start": span["start"],
                        "end": span["end"]
                    }
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["apply"], &request_changeset.to_string());
    assert!(
        !output.status.success(),
        "apply should fail for canonically equivalent but byte-different source"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "precondition_failed");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("Expected hash")),
        "expected hash mismatch detail in precondition failure"
    );
}

#[test]
fn apply_returns_io_error_for_non_utf8_files() {
    let mut temp_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp python file should be created");
    temp_file
        .write_all(&[
            0xff, 0xfe, b'd', b'e', b'f', b' ', b'x', b'(', b')', b':', b'\n',
        ])
        .expect("non-utf8 fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": []
    });
    let output = run_identedit_with_stdin(&["apply"], &request.to_string());
    assert!(
        !output.status.success(),
        "apply should fail for non-utf8 files"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "io_error");
}

#[test]
fn apply_returns_io_error_for_partially_binary_python_payload() {
    let mut temp_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp python file should be created");
    temp_file
        .write_all(b"def process_data(value):\n    return value + 1\n\xff\n")
        .expect("binary-like fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": []
    });
    let output = run_identedit_with_stdin(&["apply"], &request.to_string());
    assert!(
        !output.status.success(),
        "apply should fail for partially-binary python payload"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "io_error");
}

#[test]
fn apply_returns_parse_failure_for_nul_in_python_source() {
    let mut temp_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp python file should be created");
    temp_file
        .write_all(b"def process_data(value):\n    return value + 1\n\x00\n")
        .expect("nul fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": []
    });
    let output = run_identedit_with_stdin(&["apply"], &request.to_string());
    assert!(
        !output.status.success(),
        "apply should fail for python source containing embedded NUL"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "parse_failure");
}

#[test]
fn apply_returns_parse_failure_for_bom_plus_nul_python_source() {
    let mut temp_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp python file should be created");
    temp_file
        .write_all(b"\xEF\xBB\xBFdef process_data(value):\n    return value + 1\n\x00\n")
        .expect("bom+nul fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": []
    });
    let output = run_identedit_with_stdin(&["apply"], &request.to_string());
    assert!(
        !output.status.success(),
        "apply should fail for python source containing BOM and embedded NUL"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "parse_failure");
}

#[test]
fn apply_returns_parse_failure_for_syntax_invalid_python_file() {
    let mut temporary_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp file should be created");
    temporary_file
        .write_all(b"def broken(:\n    pass\n")
        .expect("invalid python fixture write should succeed");
    let file_path = temporary_file.keep().expect("temp file should persist").1;

    let request = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": []
    });
    let output = run_identedit_with_stdin(&["apply"], &request.to_string());
    assert!(
        !output.status.success(),
        "apply should fail for syntax-invalid python file"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "parse_failure");
}

#[test]
fn apply_json_mode_succeeds_with_command_wrapped_payload() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_handle(&file_path, "process_*");
    let span = &handle["span"];
    let replacement = "def process_data(value):\n    return value + 7";

    let changeset = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "identity": handle["identity"],
                    "kind": handle["kind"],
                    "span_hint": {
                        "start": span["start"],
                        "end": span["end"]
                    },
                    "expected_old_hash": identedit::changeset::hash_text(
                        handle["text"].as_str().expect("text should be string")
                    )
                },
                "op": {
                    "type": "replace",
                    "new_text": replacement
                },
                "preview": {
                    "old_text": handle["text"],
                    "new_text": replacement,
                    "matched_span": {
                        "start": span["start"],
                        "end": span["end"]
                    }
                }
            }
        ]
    });

    let request = json!({
        "command": "apply",
        "changeset": changeset
    });

    let output = run_identedit_with_stdin(&["apply", "--json"], &request.to_string());

    assert!(
        output.status.success(),
        "apply --json failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["summary"]["operations_applied"], 1);
    let modified = fs::read_to_string(&file_path).expect("file should be readable");
    assert!(modified.contains("return value + 7"));
}

#[test]
fn apply_supports_unicode_replacement_text() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_handle(&file_path, "process_*");
    let span = &handle["span"];
    let replacement = "def process_data(value):\n    return \"e\u{301} + 😸\"";
    let expected_hash =
        identedit::changeset::hash_text(handle["text"].as_str().expect("text should be string"));

    let changeset = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "identity": handle["identity"],
                    "kind": handle["kind"],
                    "span_hint": {"start": span["start"], "end": span["end"]},
                    "expected_old_hash": expected_hash
                },
                "op": {"type": "replace", "new_text": replacement},
                "preview": {
                    "old_text": handle["text"],
                    "new_text": replacement,
                    "matched_span": {"start": span["start"], "end": span["end"]}
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["apply"], &changeset.to_string());
    assert!(
        output.status.success(),
        "apply should succeed for unicode replacement text: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("file should be readable");
    assert!(modified.contains("\"e\u{301} + 😸\""));
}

#[test]
fn apply_preserves_nul_byte_in_replacement_text() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_handle(&file_path, "process_*");
    let span = &handle["span"];
    let replacement_with_nul = "def process_data(value):\n    return \"A\u{0000}B\"";
    let expected_hash =
        identedit::changeset::hash_text(handle["text"].as_str().expect("text should be string"));

    let changeset = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "identity": handle["identity"],
                    "kind": handle["kind"],
                    "span_hint": {"start": span["start"], "end": span["end"]},
                    "expected_old_hash": expected_hash
                },
                "op": {"type": "replace", "new_text": replacement_with_nul},
                "preview": {
                    "old_text": handle["text"],
                    "new_text": replacement_with_nul,
                    "matched_span": {"start": span["start"], "end": span["end"]}
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["apply"], &changeset.to_string());
    assert!(
        output.status.success(),
        "apply should accept NUL-containing replacement text: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let modified_bytes = fs::read(&file_path).expect("file should be readable as bytes");
    assert!(
        modified_bytes
            .windows(3)
            .any(|window| window == [b'A', 0u8, b'B']),
        "file bytes should preserve embedded NUL without truncation"
    );
}

#[test]
fn apply_rejects_tampered_preview_old_text() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_handle(&file_path, "process_*");
    let span = &handle["span"];
    let expected_hash =
        identedit::changeset::hash_text(handle["text"].as_str().expect("text should be string"));

    let changeset = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "identity": handle["identity"],
                    "kind": handle["kind"],
                    "span_hint": {"start": span["start"], "end": span["end"]},
                    "expected_old_hash": expected_hash
                },
                "op": {"type": "replace", "new_text": "def process_data(value):\n    return value + 99"},
                "preview": {
                    "old_text": "def tampered():\n    pass",
                    "new_text": "def process_data(value):\n    return value + 99",
                    "matched_span": {"start": span["start"], "end": span["end"]}
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["apply"], &changeset.to_string());
    assert!(
        !output.status.success(),
        "apply should reject changesets with tampered preview old_text"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("preview")),
        "expected preview validation message"
    );
}

#[test]
fn apply_rejects_tampered_preview_span() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_handle(&file_path, "process_*");
    let span = &handle["span"];
    let start = span["start"].as_u64().expect("span start") as usize;
    let end = span["end"].as_u64().expect("span end") as usize;
    let expected_hash =
        identedit::changeset::hash_text(handle["text"].as_str().expect("text should be string"));

    let changeset = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "identity": handle["identity"],
                    "kind": handle["kind"],
                    "span_hint": {"start": start, "end": end},
                    "expected_old_hash": expected_hash
                },
                "op": {"type": "replace", "new_text": "def process_data(value):\n    return value + 100"},
                "preview": {
                    "old_text": handle["text"],
                    "new_text": "def process_data(value):\n    return value + 100",
                    "matched_span": {"start": start + 1, "end": end + 1}
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["apply"], &changeset.to_string());
    assert!(
        !output.status.success(),
        "apply should reject changesets with tampered preview span"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("preview")),
        "expected preview validation message"
    );
}

#[test]
fn apply_rejects_tampered_preview_new_text() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_handle(&file_path, "process_*");
    let span = &handle["span"];
    let expected_hash =
        identedit::changeset::hash_text(handle["text"].as_str().expect("text should be string"));

    let changeset = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "identity": handle["identity"],
                    "kind": handle["kind"],
                    "span_hint": {"start": span["start"], "end": span["end"]},
                    "expected_old_hash": expected_hash
                },
                "op": {"type": "replace", "new_text": "def process_data(value):\n    return value + 101"},
                "preview": {
                    "old_text": handle["text"],
                    "new_text": "def process_data(value):\n    return value + 999",
                    "matched_span": {"start": span["start"], "end": span["end"]}
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["apply"], &changeset.to_string());
    assert!(
        !output.status.success(),
        "apply should reject changesets with tampered preview new_text"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("preview")),
        "expected preview validation message"
    );
}

#[test]
fn apply_rejects_delete_preview_tampering_matrix() {
    for tamper_kind in ["old_text", "new_text", "matched_span"] {
        let file_path = copy_fixture_to_temp_python("example.py");
        let before = fs::read_to_string(&file_path).expect("fixture should be readable");
        let handle = select_named_handle(&file_path, "process_*");
        let span = &handle["span"];
        let span_start = span["start"].as_u64().expect("span start");
        let span_end = span["end"].as_u64().expect("span end");
        let old_text = handle["text"].as_str().expect("text should be string");
        let expected_hash = identedit::changeset::hash_text(old_text);

        let mut operation = json!({
            "target": {
                "identity": handle["identity"],
                "kind": handle["kind"],
                "span_hint": {"start": span_start, "end": span_end},
                "expected_old_hash": expected_hash
            },
            "op": {"type": "delete"},
            "preview": {
                "old_text": old_text,
                "new_text": "",
                "matched_span": {"start": span_start, "end": span_end}
            }
        });

        match tamper_kind {
            "old_text" => {
                operation["preview"]["old_text"] =
                    json!("def tampered_delete_preview():\n    pass");
            }
            "new_text" => {
                operation["preview"]["new_text"] =
                    json!("# delete must keep preview.new_text empty");
            }
            "matched_span" => {
                operation["preview"]["matched_span"]["start"] = json!(span_start + 1);
            }
            _ => unreachable!("unknown tamper kind"),
        }

        let changeset = json!({
            "file": file_path.to_string_lossy().to_string(),
            "operations": [operation]
        });

        let output = run_identedit_with_stdin(&["apply"], &changeset.to_string());
        assert!(
            !output.status.success(),
            "apply should reject delete preview tampering for {tamper_kind}"
        );

        let response: Value =
            serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
        assert_eq!(response["error"]["type"], "invalid_request");
        assert!(
            response["error"]["message"]
                .as_str()
                .is_some_and(|message| message.contains("preview")),
            "expected preview validation error for {tamper_kind}"
        );

        let after = fs::read_to_string(&file_path).expect("fixture should be readable");
        assert_eq!(
            before, after,
            "apply must remain atomic for delete preview tamper {tamper_kind}"
        );
    }
}

#[test]
fn apply_rejects_multi_operation_partial_preview_tamper_matrix() {
    for tamper_kind in ["old_text", "new_text", "matched_span"] {
        let file_path = copy_fixture_to_temp_python("example.py");
        let before = fs::read_to_string(&file_path).expect("fixture should be readable");
        let process_handle = select_named_handle(&file_path, "process_*");
        let helper_handle = select_named_handle(&file_path, "helper");

        let process_span = &process_handle["span"];
        let process_start = process_span["start"].as_u64().expect("span start") as usize;
        let process_end = process_span["end"].as_u64().expect("span end") as usize;
        let helper_span = &helper_handle["span"];
        let process_replacement = "def process_data(value):\n    return value + 100";
        let helper_replacement = "def helper():\n    return \"updated\"";

        let mut changeset = json!({
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
                        "span_hint": {"start": process_start, "end": process_end},
                        "expected_old_hash": identedit::changeset::hash_text(
                            process_handle["text"].as_str().expect("text should be string")
                        )
                    },
                    "op": {"type": "replace", "new_text": process_replacement},
                    "preview": {
                        "old_text": process_handle["text"],
                        "new_text": process_replacement,
                        "matched_span": {"start": process_start, "end": process_end}
                    }
                }
            ]
        });

        match tamper_kind {
            "old_text" => {
                changeset["operations"][1]["preview"]["old_text"] =
                    json!("def tampered_old_text():\n    pass");
            }
            "new_text" => {
                changeset["operations"][1]["preview"]["new_text"] =
                    json!("def tampered_new_text():\n    pass");
            }
            "matched_span" => {
                changeset["operations"][1]["preview"]["matched_span"]["start"] =
                    json!(process_start + 1);
                changeset["operations"][1]["preview"]["matched_span"]["end"] =
                    json!(process_end + 1);
            }
            _ => unreachable!("unknown tamper kind"),
        }

        let output = run_identedit_with_stdin(&["apply"], &changeset.to_string());
        assert!(
            !output.status.success(),
            "apply should reject tampered {tamper_kind} in a multi-op payload"
        );

        let response: Value =
            serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
        assert_eq!(response["error"]["type"], "invalid_request");
        assert!(
            response["error"]["message"]
                .as_str()
                .is_some_and(|message| message.contains("Operation 1 preview")),
            "expected deterministic operation-indexed preview error for {tamper_kind}"
        );

        let after = fs::read_to_string(&file_path).expect("fixture should be readable");
        assert_eq!(
            before, after,
            "apply must remain atomic when preview tampering is detected ({tamper_kind})"
        );
    }
}

#[test]
fn apply_rejects_overlapping_operations() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_handle(&file_path, "process_*");
    let span = &handle["span"];

    let start = span["start"].as_u64().expect("span start") as usize;
    let end = span["end"].as_u64().expect("span end") as usize;
    let original_text = handle["text"]
        .as_str()
        .expect("text should be string")
        .to_string();
    let expected_hash = identedit::changeset::hash_text(&original_text);

    let overlapping_changeset = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "identity": handle["identity"],
                    "kind": handle["kind"],
                    "span_hint": {"start": start, "end": end},
                    "expected_old_hash": expected_hash
                },
                "op": {"type": "replace", "new_text": "def process_data(value):\n    return value + 2"},
                "preview": {
                    "old_text": original_text,
                    "new_text": "def process_data(value):\n    return value + 2",
                    "matched_span": {"start": start, "end": end}
                }
            },
            {
                "target": {
                    "identity": handle["identity"],
                    "kind": handle["kind"],
                    "span_hint": {"start": start, "end": end},
                    "expected_old_hash": expected_hash
                },
                "op": {"type": "replace", "new_text": "overlap"},
                "preview": {
                    "old_text": original_text,
                    "new_text": "overlap",
                    "matched_span": {"start": start, "end": end}
                }
            }
        ]
    });

    let changeset_file = write_changeset_json(&overlapping_changeset.to_string());
    let output = run_identedit(&[
        "apply",
        changeset_file
            .path()
            .to_str()
            .expect("changeset path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "apply should fail for overlapping operations"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("Overlapping operations")),
        "expected overlapping operation message"
    );
}

#[test]
fn apply_supports_insert_before_and_after_for_same_anchor() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_handle(&file_path, "process_*");
    let span = &handle["span"];
    let span_start = span["start"].as_u64().expect("span start") as usize;
    let span_end = span["end"].as_u64().expect("span end") as usize;
    let expected_hash =
        identedit::changeset::hash_text(handle["text"].as_str().expect("text should be string"));

    let before_insert = "# inserted before process\n";
    let after_insert = "\n# inserted after process";
    let changeset = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "identity": handle["identity"],
                    "kind": handle["kind"],
                    "span_hint": {"start": span_start, "end": span_end},
                    "expected_old_hash": expected_hash
                },
                "op": {"type": "insert_before", "new_text": before_insert},
                "preview": {
                    "old_text": "",
                    "new_text": before_insert,
                    "matched_span": {"start": span_start, "end": span_start}
                }
            },
            {
                "target": {
                    "identity": handle["identity"],
                    "kind": handle["kind"],
                    "span_hint": {"start": span_start, "end": span_end},
                    "expected_old_hash": expected_hash
                },
                "op": {"type": "insert_after", "new_text": after_insert},
                "preview": {
                    "old_text": "",
                    "new_text": after_insert,
                    "matched_span": {"start": span_end, "end": span_end}
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["apply"], &changeset.to_string());
    assert!(
        output.status.success(),
        "apply should allow insert_before + insert_after on same anchor: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["summary"]["operations_applied"], 2);
    assert_eq!(response["summary"]["operations_failed"], 0);

    let modified = fs::read_to_string(&file_path).expect("file should be readable");
    assert!(
        modified.contains("# inserted before process\ndef process_data"),
        "insert_before text should appear immediately before the anchor"
    );
    assert!(
        modified.contains("return result\n# inserted after process"),
        "insert_after text should appear immediately after the anchor"
    );
}

#[test]
fn apply_supports_file_end_insert_target() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let before = fs::read_to_string(&file_path).expect("file should be readable");
    let expected_file_hash = identedit::changeset::hash_text(&before);
    let insert_text = "\n# appended-at-file-end\n";

    let changeset = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "type": "file_end",
                    "expected_file_hash": expected_file_hash
                },
                "op": {"type": "insert", "new_text": insert_text},
                "preview": {
                    "old_text": "",
                    "new_text": insert_text,
                    "matched_span": {"start": before.len(), "end": before.len()}
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["apply"], &changeset.to_string());
    assert!(
        output.status.success(),
        "apply should support file_end insert: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["summary"]["operations_applied"], 1);
    assert_eq!(response["summary"]["operations_failed"], 0);

    let modified = fs::read_to_string(&file_path).expect("file should be readable");
    assert!(
        modified.ends_with(insert_text),
        "insert text should be appended at file end"
    );
}

#[test]
fn apply_supports_file_start_insert_target() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let before = fs::read_to_string(&file_path).expect("file should be readable");
    let expected_file_hash = identedit::changeset::hash_text(&before);
    let insert_text = "# prepended-at-file-start\n";

    let changeset = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "type": "file_start",
                    "expected_file_hash": expected_file_hash
                },
                "op": {"type": "insert", "new_text": insert_text},
                "preview": {
                    "old_text": "",
                    "new_text": insert_text,
                    "matched_span": {"start": 0, "end": 0}
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["apply"], &changeset.to_string());
    assert!(
        output.status.success(),
        "apply should support file_start insert: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["summary"]["operations_applied"], 1);
    assert_eq!(response["summary"]["operations_failed"], 0);

    let modified = fs::read_to_string(&file_path).expect("file should be readable");
    assert!(
        modified.starts_with(insert_text),
        "insert text should be prepended at file start"
    );
}

#[test]
fn apply_rejects_file_start_target_missing_expected_file_hash() {
    let file_path = copy_fixture_to_temp_python("example.py");

    let changeset = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "type": "file_start"
                },
                "op": {"type": "insert", "new_text": "# missing-hash\n"},
                "preview": {
                    "old_text": "",
                    "new_text": "# missing-hash\n",
                    "matched_span": {"start": 0, "end": 0}
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["apply"], &changeset.to_string());
    assert!(
        !output.status.success(),
        "apply should reject file_start target missing expected_file_hash"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("missing field `expected_file_hash`")),
        "expected explicit missing expected_file_hash message"
    );
}

#[test]
fn apply_rejects_node_target_with_expected_file_hash_field() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_handle(&file_path, "process_*");
    let span = &handle["span"];
    let old_text = handle["text"].as_str().expect("text should be string");
    let expected_hash = identedit::changeset::hash_text(old_text);

    let changeset = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "type": "node",
                    "identity": handle["identity"],
                    "kind": handle["kind"],
                    "span_hint": {
                        "start": span["start"],
                        "end": span["end"]
                    },
                    "expected_old_hash": expected_hash,
                    "expected_file_hash": "not-allowed"
                },
                "op": {"type": "replace", "new_text": old_text},
                "preview": {
                    "old_text": old_text,
                    "new_text": old_text,
                    "matched_span": {
                        "start": span["start"],
                        "end": span["end"]
                    }
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["apply"], &changeset.to_string());
    assert!(
        !output.status.success(),
        "apply should reject expected_file_hash on node target"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"].as_str().is_some_and(
            |message| message.contains("node target does not accept expected_file_hash")
        ),
        "expected node/file hash schema rejection message"
    );
}

#[test]
fn apply_rejects_file_start_insert_when_file_hash_is_stale() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let insert_text = "# stale-prepend\n";

    let changeset = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "type": "file_start",
                    "expected_file_hash": "stale-file-hash"
                },
                "op": {"type": "insert", "new_text": insert_text},
                "preview": {
                    "old_text": "",
                    "new_text": insert_text,
                    "matched_span": {"start": 0, "end": 0}
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["apply"], &changeset.to_string());
    assert!(
        !output.status.success(),
        "apply should reject stale file hash for file_start insert"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "precondition_failed");
}

#[test]
fn apply_rejects_file_start_and_file_end_inserts_on_empty_file() {
    let mut empty_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp python file should be created");
    empty_file
        .write_all(b"")
        .expect("empty fixture write should succeed");
    let file_path = empty_file.keep().expect("temp file should persist").1;
    let expected_file_hash = identedit::changeset::hash_text("");

    let changeset = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "type": "file_start",
                    "expected_file_hash": expected_file_hash
                },
                "op": {"type": "insert", "new_text": "A"},
                "preview": {
                    "old_text": "",
                    "new_text": "A",
                    "matched_span": {"start": 0, "end": 0}
                }
            },
            {
                "target": {
                    "type": "file_end",
                    "expected_file_hash": expected_file_hash
                },
                "op": {"type": "insert", "new_text": "B"},
                "preview": {
                    "old_text": "",
                    "new_text": "B",
                    "matched_span": {"start": 0, "end": 0}
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["apply"], &changeset.to_string());
    assert!(
        !output.status.success(),
        "apply should reject file_start/file_end inserts at same position for empty file"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("Overlapping operations")),
        "expected overlap rejection for same-position inserts"
    );
}

#[test]
fn apply_file_start_insert_from_transform_preserves_utf8_bom_prefix() {
    let mut temp_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp python file should be created");
    temp_file
        .write_all(b"\xEF\xBB\xBFdef process_data(value):\n    return value + 1\n")
        .expect("bom python fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let expected_file_hash = identedit::changeset::hash_text(&before);

    let transform_request = json!({
        "command": "edit",
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "type": "file_start",
                    "expected_file_hash": expected_file_hash
                },
                "op": {
                    "type": "insert",
                    "new_text": "# inserted-at-file-start\n"
                }
            }
        ]
    });

    let transform_output =
        run_identedit_with_stdin(&["edit", "--json"], &transform_request.to_string());
    assert!(
        transform_output.status.success(),
        "transform should succeed for BOM file_start insert: {}",
        String::from_utf8_lossy(&transform_output.stderr)
    );

    let changeset_text =
        String::from_utf8(transform_output.stdout).expect("changeset should be valid UTF-8");
    let apply_output = run_identedit_with_stdin(&["apply"], &changeset_text);
    assert!(
        apply_output.status.success(),
        "apply should succeed for BOM file_start insert changeset: {}",
        String::from_utf8_lossy(&apply_output.stderr)
    );

    let modified_bytes = fs::read(&file_path).expect("file bytes should be readable");
    assert!(
        modified_bytes.starts_with(&[0xEF, 0xBB, 0xBF]),
        "UTF-8 BOM must remain at file start after file_start insert"
    );

    let modified_text = String::from_utf8(modified_bytes).expect("modified file should be utf-8");
    assert!(
        modified_text.starts_with("\u{feff}# inserted-at-file-start\ndef process_data"),
        "insert text should be placed after BOM for file_start insert"
    );
}

#[test]
fn apply_rejects_file_start_insert_and_insert_before_same_boundary() {
    let mut temp_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp python file should be created");
    temp_file
        .write_all(b"def only(value):\n    return value + 1\n")
        .expect("python fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let source = fs::read_to_string(&file_path).expect("fixture should be readable");
    let expected_file_hash = identedit::changeset::hash_text(&source);
    let handle = select_first_handle(&file_path, "function_definition", Some("only"));
    let span = &handle["span"];
    let span_start = span["start"].as_u64().expect("span start") as usize;
    let span_end = span["end"].as_u64().expect("span end") as usize;
    let expected_old_hash =
        identedit::changeset::hash_text(handle["text"].as_str().expect("text should be string"));
    assert_eq!(
        span_start, 0,
        "fixture precondition: first function should start at byte 0"
    );

    let changeset = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "type": "file_start",
                    "expected_file_hash": expected_file_hash
                },
                "op": {"type": "insert", "new_text": "# file-header\n"},
                "preview": {
                    "old_text": "",
                    "new_text": "# file-header\n",
                    "matched_span": {"start": 0, "end": 0}
                }
            },
            {
                "target": {
                    "identity": handle["identity"],
                    "kind": handle["kind"],
                    "span_hint": {"start": span_start, "end": span_end},
                    "expected_old_hash": expected_old_hash
                },
                "op": {"type": "insert_before", "new_text": "# node-header\n"},
                "preview": {
                    "old_text": "",
                    "new_text": "# node-header\n",
                    "matched_span": {"start": span_start, "end": span_start}
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["apply"], &changeset.to_string());
    assert!(
        !output.status.success(),
        "apply should reject file_start insert colliding with insert_before"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("Overlapping operations")),
        "expected overlap conflict message"
    );
}

#[test]
fn apply_rejects_file_start_and_file_end_inserts_on_bom_only_file() {
    let mut bom_only_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp python file should be created");
    bom_only_file
        .write_all(b"\xEF\xBB\xBF")
        .expect("bom-only fixture write should succeed");
    let file_path = bom_only_file.keep().expect("temp file should persist").1;
    let source = fs::read_to_string(&file_path).expect("fixture should be readable");
    let expected_file_hash = identedit::changeset::hash_text(&source);

    let changeset = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "type": "file_start",
                    "expected_file_hash": expected_file_hash
                },
                "op": {"type": "insert", "new_text": "# start\n"},
                "preview": {
                    "old_text": "",
                    "new_text": "# start\n",
                    "matched_span": {"start": 3, "end": 3}
                }
            },
            {
                "target": {
                    "type": "file_end",
                    "expected_file_hash": expected_file_hash
                },
                "op": {"type": "insert", "new_text": "# end\n"},
                "preview": {
                    "old_text": "",
                    "new_text": "# end\n",
                    "matched_span": {"start": 3, "end": 3}
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["apply"], &changeset.to_string());
    assert!(
        !output.status.success(),
        "apply should reject file_start/file_end insert overlap on BOM-only file"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("Overlapping operations")),
        "expected overlap conflict message"
    );
}
