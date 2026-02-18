use super::*;

#[test]
fn apply_stdin_mode_rejects_wrapped_payload_without_json_flag() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let payload = json!({
        "command": "apply",
        "changeset": {
            "file": file_path.to_string_lossy().to_string(),
            "operations": []
        }
    });

    let output = run_identedit_with_stdin(&["apply"], &payload.to_string());
    assert!(
        !output.status.success(),
        "bare apply stdin should reject command-wrapped payload when --json is not set"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| {
                message.contains("unknown field `command`")
                    || message.contains("unknown field `changeset`")
            }),
        "expected strict-mode unknown-field error in bare stdin mode regardless of field iteration order"
    );
}

#[test]
fn apply_stdin_mode_rejects_non_object_top_level_payload() {
    for payload in ["[]", "null", "1"] {
        let output = run_identedit_with_stdin(&["apply"], payload);
        assert!(
            !output.status.success(),
            "apply should reject non-object top-level payload in bare stdin mode"
        );

        let response: Value =
            serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
        assert_eq!(response["error"]["type"], "invalid_request");
    }
}

#[test]
fn apply_stdin_mode_utf8_bom_prefixed_payload_returns_invalid_request() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let payload = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": []
    })
    .to_string();

    let mut bom_prefixed_payload = Vec::with_capacity(payload.len() + 3);
    bom_prefixed_payload.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
    bom_prefixed_payload.extend_from_slice(payload.as_bytes());

    let output = run_identedit_with_raw_stdin(&["apply"], &bom_prefixed_payload);
    assert!(
        !output.status.success(),
        "apply should reject bare stdin payload with UTF-8 BOM prefix"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}

#[test]
fn apply_stdin_mode_empty_or_whitespace_payload_returns_invalid_request() {
    for payload in ["", " ", "\n\t", "\r\n   "] {
        let output = run_identedit_with_stdin(&["apply"], payload);
        assert!(
            !output.status.success(),
            "apply should reject empty/whitespace bare stdin payload"
        );

        let response: Value =
            serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
        assert_eq!(response["error"]["type"], "invalid_request");
    }
}

#[test]
fn apply_stdin_mode_trailing_garbage_returns_invalid_request() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let payload = format!(
        "{}\ntrailing-garbage",
        json!({
            "file": file_path.to_string_lossy().to_string(),
            "operations": []
        })
    );

    let output = run_identedit_with_stdin(&["apply"], &payload);
    assert!(
        !output.status.success(),
        "apply should reject bare stdin payload with trailing garbage"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}

#[test]
fn apply_stdin_mode_trailing_nul_returns_invalid_request() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let mut payload = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": []
    })
    .to_string()
    .into_bytes();
    payload.push(0x00);

    let output = run_identedit_with_raw_stdin(&["apply"], &payload);
    assert!(
        !output.status.success(),
        "apply should reject bare stdin payload with trailing NUL byte"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}

#[test]
fn apply_stdin_mode_non_utf8_payload_returns_io_error() {
    let output = run_identedit_with_raw_stdin(&["apply"], &[0xFF, 0xFE, 0xFD]);
    assert!(
        !output.status.success(),
        "apply should fail for bare stdin non-UTF8 payload"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "io_error");
}

#[test]
fn apply_stdin_mode_nested_duplicate_fields_are_deterministic_parse_errors() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let file_literal = json_string_literal(&file_path);
    let payloads = [
        format!(
            "{{\"files\":[{{\"file\":\"{file_literal}\",\"operations\":[{{\"target\":{{\"identity\":\"a\",\"identity\":\"b\",\"kind\":\"function_definition\",\"expected_old_hash\":\"00\"}},\"op\":{{\"type\":\"replace\",\"new_text\":\"x\"}},\"preview\":{{\"old_text\":\"a\",\"new_text\":\"x\",\"matched_span\":{{\"start\":0,\"end\":1}}}}}}]}}],\"transaction\":{{\"mode\":\"all_or_nothing\"}}}}"
        ),
        format!(
            "{{\"files\":[{{\"file\":\"{file_literal}\",\"operations\":[{{\"target\":{{\"identity\":\"a\",\"kind\":\"function_definition\",\"expected_old_hash\":\"00\"}},\"op\":{{\"type\":\"replace\",\"new_text\":\"x\",\"new_text\":\"y\"}},\"preview\":{{\"old_text\":\"a\",\"new_text\":\"x\",\"matched_span\":{{\"start\":0,\"end\":1}}}}}}]}}],\"transaction\":{{\"mode\":\"all_or_nothing\"}}}}"
        ),
        format!(
            "{{\"files\":[{{\"file\":\"{file_literal}\",\"operations\":[{{\"target\":{{\"identity\":\"a\",\"kind\":\"function_definition\",\"expected_old_hash\":\"00\"}},\"op\":{{\"type\":\"replace\",\"new_text\":\"x\"}},\"preview\":{{\"old_text\":\"a\",\"new_text\":\"x\",\"matched_span\":{{\"start\":0,\"start\":1,\"end\":1}}}}}}]}}],\"transaction\":{{\"mode\":\"all_or_nothing\"}}}}"
        ),
    ];

    for payload in payloads {
        let output = run_identedit_with_raw_stdin(&["apply"], payload.as_bytes());
        assert!(
            !output.status.success(),
            "apply should reject nested duplicate fields in bare stdin mode"
        );

        let response: Value =
            serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
        assert_eq!(response["error"]["type"], "invalid_request");
        assert!(
            response["error"]["message"]
                .as_str()
                .is_some_and(|message| message.contains("duplicate field")),
            "expected deterministic duplicate-field parse error message"
        );
    }
}

#[test]
fn apply_stdin_mode_duplicate_field_is_deterministic_parse_error() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let file_literal = json_string_literal(&file_path);
    let payload = format!(
        "{{\"files\":[{{\"file\":\"{file_literal}\",\"operations\":[]}}],\"files\":[{{\"file\":\"{file_literal}\",\"operations\":[]}}]}}"
    );

    let output = run_identedit_with_raw_stdin(&["apply"], payload.as_bytes());
    assert!(
        !output.status.success(),
        "apply stdin mode should reject duplicate fields in bare changeset payload"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("duplicate field")),
        "expected deterministic duplicate-field parse message in stdin mode"
    );
}

#[test]
fn apply_stdin_mode_duplicate_transaction_mode_key_is_parse_error() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let file_literal = json_string_literal(&file_path);
    let payload = format!(
        "{{\"files\":[{{\"file\":\"{file_literal}\",\"operations\":[]}}],\"transaction\":{{\"mode\":\"all_or_nothing\",\"mode\":\"all_or_nothing\"}}}}"
    );

    let output = run_identedit_with_raw_stdin(&["apply"], payload.as_bytes());
    assert!(
        !output.status.success(),
        "apply stdin mode should reject duplicate transaction.mode key"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("duplicate field `mode`")),
        "expected deterministic duplicate transaction.mode parse message in stdin mode"
    );
}

#[test]
fn apply_json_mode_rejects_missing_changeset_field() {
    let request = json!({
        "command": "apply"
    });

    let output = run_identedit_with_stdin(&["apply", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "apply should reject missing changeset field"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}

#[test]
fn apply_json_mode_duplicate_transaction_mode_key_is_parse_error() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let file_literal = json_string_literal(&file_path);
    let payload = format!(
        "{{\"command\":\"apply\",\"changeset\":{{\"files\":[{{\"file\":\"{file_literal}\",\"operations\":[]}}],\"transaction\":{{\"mode\":\"all_or_nothing\",\"mode\":\"all_or_nothing\"}}}}}}"
    );

    let output = run_identedit_with_raw_stdin(&["apply", "--json"], payload.as_bytes());
    assert!(
        !output.status.success(),
        "apply --json should reject duplicate transaction.mode key"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("duplicate field `mode`")),
        "expected deterministic duplicate transaction.mode parse message in --json mode"
    );
}

#[test]
fn apply_json_mode_rejects_unknown_transaction_mode_variant() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let request = json!({
        "command": "apply",
        "changeset": {
            "files": [
                {
                    "file": file_path.to_string_lossy().to_string(),
                    "operations": []
                }
            ],
            "transaction": {
                "mode": "partial_commit"
            }
        }
    });

    let request_json = request.to_string();
    let output = run_identedit_with_raw_stdin(&["apply", "--json"], request_json.as_bytes());
    assert!(
        !output.status.success(),
        "apply --json should reject unknown transaction.mode variants"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(
                |message| message.contains("unknown variant") && message.contains("partial_commit")
            ),
        "expected deterministic unknown-variant transaction.mode diagnostic"
    );
}

#[test]
fn apply_json_mode_rejects_non_string_transaction_mode_type() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let request = json!({
        "command": "apply",
        "changeset": {
            "files": [
                {
                    "file": file_path.to_string_lossy().to_string(),
                    "operations": []
                }
            ],
            "transaction": {
                "mode": true
            }
        }
    });

    let request_json = request.to_string();
    let output = run_identedit_with_raw_stdin(&["apply", "--json"], request_json.as_bytes());
    assert!(
        !output.status.success(),
        "apply --json should reject non-string transaction.mode types"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let message = response["error"]["message"].as_str().unwrap_or_default();
    assert!(
        message.contains("expected value") || message.contains("invalid type"),
        "expected deterministic non-string transaction.mode type diagnostic, got: {message}"
    );
}

#[test]
fn apply_json_mode_rejects_raw_v1_wrapped_changeset_after_v2_cutover() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let file_literal = json_string_literal(&file_path);
    let payload = format!(
        "{{\"command\":\"apply\",\"changeset\":{{\"file\":\"{file_literal}\",\"operations\":[]}}}}"
    );

    let output = run_identedit_with_raw_stdin(&["apply", "--json"], payload.as_bytes());
    assert!(
        !output.status.success(),
        "apply --json should reject raw wrapped v1 changeset post v2 cutover"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("unknown field `file`")),
        "expected explicit v1->v2 parse diagnostic in --json mode"
    );
}

#[test]
fn apply_json_mode_rejects_empty_changeset_files_array() {
    let request = json!({
        "command": "apply",
        "changeset": {
            "files": [],
            "transaction": {
                "mode": "all_or_nothing"
            }
        }
    });

    let request_json = request.to_string();
    let output = run_identedit_with_raw_stdin(&["apply", "--json"], request_json.as_bytes());
    assert!(
        !output.status.success(),
        "apply --json should reject empty changeset.files array"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"].as_str().is_some_and(
            |message| message.contains("changeset.files must contain at least one file")
        ),
        "expected explicit empty files-array diagnostic"
    );
}

#[test]
fn apply_json_mode_rejects_invalid_changeset_files_shapes() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let file_literal = json_string_literal(&file_path);
    let payloads = [
        "{\"command\":\"apply\",\"changeset\":{\"files\":null,\"transaction\":{\"mode\":\"all_or_nothing\"}}}".to_string(),
        "{\"command\":\"apply\",\"changeset\":{\"files\":{},\"transaction\":{\"mode\":\"all_or_nothing\"}}}".to_string(),
        "{\"command\":\"apply\",\"changeset\":{\"files\":[{}],\"transaction\":{\"mode\":\"all_or_nothing\"}}}".to_string(),
        "{\"command\":\"apply\",\"changeset\":{\"files\":[{\"operations\":[]}],\"transaction\":{\"mode\":\"all_or_nothing\"}}}".to_string(),
        format!(
            "{{\"command\":\"apply\",\"changeset\":{{\"files\":[{{\"file\":\"{file_literal}\"}}],\"transaction\":{{\"mode\":\"all_or_nothing\"}}}}}}"
        ),
    ];

    for payload in payloads {
        let output = run_identedit_with_raw_stdin(&["apply", "--json"], payload.as_bytes());
        assert!(
            !output.status.success(),
            "apply --json should reject malformed changeset.files entry: {payload}"
        );

        let response: Value =
            serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
        assert_eq!(response["error"]["type"], "invalid_request");
    }
}

#[test]
fn apply_json_mode_rejects_missing_file_operations_field() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let request = json!({
        "command": "apply",
        "changeset": {
            "files": [
                {
                    "file": file_path.to_string_lossy().to_string()
                }
            ],
            "transaction": {
                "mode": "all_or_nothing"
            }
        }
    });

    let request_json = request.to_string();
    let output = run_identedit_with_raw_stdin(&["apply", "--json"], request_json.as_bytes());
    assert!(
        !output.status.success(),
        "apply --json should reject changeset.files entries missing operations"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("missing field `operations`")),
        "expected explicit missing operations diagnostic"
    );
}

#[test]
fn apply_json_mode_v2_without_transaction_uses_default_and_succeeds() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let request = json!({
        "command": "apply",
        "changeset": {
            "files": [
                {
                    "file": file_path.to_string_lossy().to_string(),
                    "operations": []
                }
            ]
        }
    });

    let request_json = request.to_string();
    let output = run_identedit_with_raw_stdin(&["apply", "--json"], request_json.as_bytes());
    assert!(
        output.status.success(),
        "apply --json should accept missing transaction and default all_or_nothing: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["summary"]["operations_applied"], 0);
}

#[test]
fn apply_json_mode_rejects_invalid_transaction_mode() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let request = json!({
        "command": "apply",
        "changeset": {
            "files": [
                {
                    "file": file_path.to_string_lossy().to_string(),
                    "operations": []
                }
            ],
            "transaction": {
                "mode": "partial"
            }
        }
    });

    let request_json = request.to_string();
    let output = run_identedit_with_raw_stdin(&["apply", "--json"], request_json.as_bytes());
    assert!(
        !output.status.success(),
        "apply --json should reject invalid transaction mode"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("unknown variant")),
        "expected unknown-variant parse message"
    );
}

#[test]
fn apply_json_mode_rejects_unknown_transaction_field() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let request = json!({
        "command": "apply",
        "changeset": {
            "files": [
                {
                    "file": file_path.to_string_lossy().to_string(),
                    "operations": []
                }
            ],
            "transaction": {
                "mode": "all_or_nothing",
                "unexpected": true
            }
        }
    });

    let request_json = request.to_string();
    let output = run_identedit_with_raw_stdin(&["apply", "--json"], request_json.as_bytes());
    assert!(
        !output.status.success(),
        "apply --json should reject unknown fields under transaction"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("unknown field `unexpected`")),
        "expected unknown transaction field diagnostic"
    );
}

#[test]
fn apply_json_mode_rejects_non_object_or_null_transaction() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let file_literal = json_string_literal(&file_path);
    let payloads = [
        format!(
            "{{\"command\":\"apply\",\"changeset\":{{\"files\":[{{\"file\":\"{file_literal}\",\"operations\":[]}}],\"transaction\":null}}}}"
        ),
        format!(
            "{{\"command\":\"apply\",\"changeset\":{{\"files\":[{{\"file\":\"{file_literal}\",\"operations\":[]}}],\"transaction\":[]}}}}"
        ),
        format!(
            "{{\"command\":\"apply\",\"changeset\":{{\"files\":[{{\"file\":\"{file_literal}\",\"operations\":[]}}],\"transaction\":1}}}}"
        ),
        format!(
            "{{\"command\":\"apply\",\"changeset\":{{\"files\":[{{\"file\":\"{file_literal}\",\"operations\":[]}}],\"transaction\":{{\"mode\":null}}}}}}"
        ),
    ];

    for payload in payloads {
        let output = run_identedit_with_raw_stdin(&["apply", "--json"], payload.as_bytes());
        assert!(
            !output.status.success(),
            "apply --json should reject invalid transaction shape/type: {payload}"
        );

        let response: Value =
            serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
        assert_eq!(response["error"]["type"], "invalid_request");
    }
}

#[test]
fn apply_json_mode_rejects_unknown_file_entry_field_in_v2_changeset() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let request = json!({
        "command": "apply",
        "changeset": {
            "files": [
                {
                    "file": file_path.to_string_lossy().to_string(),
                    "operations": [],
                    "extra": 1
                }
            ],
            "transaction": {
                "mode": "all_or_nothing"
            }
        }
    });

    let request_json = request.to_string();
    let output = run_identedit_with_raw_stdin(&["apply", "--json"], request_json.as_bytes());
    assert!(
        !output.status.success(),
        "apply --json should reject unknown fields inside file entries"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("unknown field `extra`")),
        "expected unknown file-entry field diagnostic"
    );
}

#[test]
fn apply_stdin_mode_rejects_empty_files_array_in_v2_payload() {
    let payload = r#"{"files":[],"transaction":{"mode":"all_or_nothing"}}"#;
    let output = run_identedit_with_raw_stdin(&["apply"], payload.as_bytes());
    assert!(
        !output.status.success(),
        "bare apply stdin should reject empty files array in v2 payload"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"].as_str().is_some_and(
            |message| message.contains("changeset.files must contain at least one file")
        ),
        "expected explicit empty files-array diagnostic in bare stdin mode"
    );
}

#[test]
fn apply_json_mode_empty_file_path_returns_io_error() {
    let request = json!({
        "command": "apply",
        "changeset": {
            "files": [
                {
                    "file": "",
                    "operations": []
                }
            ],
            "transaction": {
                "mode": "all_or_nothing"
            }
        }
    });

    let output = run_identedit_with_stdin(&["apply", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "apply --json should fail for changeset.file empty path"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "io_error");
}

#[test]
fn apply_json_mode_escaped_nul_file_path_returns_io_error() {
    let output = run_identedit_with_stdin(
        &["apply", "--json"],
        r#"{"command":"apply","changeset":{"files":[{"file":"\u0000","operations":[]}],"transaction":{"mode":"all_or_nothing"}}}"#,
    );
    assert!(
        !output.status.success(),
        "apply --json should fail for changeset.file escaped NUL path"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "io_error");
}

#[test]
fn apply_json_mode_rejects_missing_changeset_file_field() {
    let request = json!({
        "command": "apply",
        "changeset": {
            "operations": []
        }
    });

    let output = run_identedit_with_stdin(&["apply", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "apply should reject missing changeset.file"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}

#[test]
fn apply_json_mode_rejects_unknown_top_level_field() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let request = json!({
        "command": "apply",
        "changeset": {
            "file": file_path.to_string_lossy().to_string(),
            "operations": []
        },
        "unexpected": true
    });

    let output = run_identedit_with_stdin(&["apply", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "apply should reject unknown top-level fields"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("unknown field `unexpected`")),
        "expected unknown top-level field message"
    );
}

#[test]
fn apply_json_mode_rejects_unknown_changeset_field() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let request = json!({
        "command": "apply",
        "changeset": {
            "file": file_path.to_string_lossy().to_string(),
            "operations": [],
            "unexpected_changeset": 1
        }
    });

    let output = run_identedit_with_stdin(&["apply", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "apply should reject unknown changeset fields"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("unknown field `unexpected_changeset`")),
        "expected unknown changeset field message"
    );
}

#[test]
fn apply_json_mode_rejects_operations_object_type() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let request = json!({
        "command": "apply",
        "changeset": {
            "file": file_path.to_string_lossy().to_string(),
            "operations": {
                "target": "invalid"
            }
        }
    });

    let output = run_identedit_with_stdin(&["apply", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "apply should reject non-array operations payload"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}

#[test]
fn apply_json_mode_rejects_unknown_target_field() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_handle(&file_path, "process_*");
    let span = &handle["span"];
    let old_text = handle["text"].as_str().expect("text should be string");
    let expected_hash = identedit::changeset::hash_text(old_text);
    let request = json!({
        "command": "apply",
        "changeset": {
            "file": file_path.to_string_lossy().to_string(),
            "operations": [
                {
                    "target": {
                        "identity": handle["identity"],
                        "kind": handle["kind"],
                        "expected_old_hash": expected_hash,
                        "span_hint": {
                            "start": span["start"],
                            "end": span["end"]
                        },
                        "identiy": handle["identity"]
                    },
                    "op": {
                        "type": "replace",
                        "new_text": "def process_data(value):\n    return value + 2"
                    },
                    "preview": {
                        "old_text": old_text,
                        "new_text": "def process_data(value):\n    return value + 2",
                        "matched_span": {
                            "start": span["start"],
                            "end": span["end"]
                        }
                    }
                }
            ]
        }
    });

    let output = run_identedit_with_stdin(&["apply", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "apply should reject unknown target fields"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("unknown field `identiy`")),
        "expected unknown target field message"
    );
}

#[test]
fn apply_json_mode_rejects_target_missing_expected_old_hash_field() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_handle(&file_path, "process_*");
    let span = &handle["span"];
    let old_text = handle["text"].as_str().expect("text should be string");
    let request = json!({
        "command": "apply",
        "changeset": {
            "file": file_path.to_string_lossy().to_string(),
            "operations": [
                {
                    "target": {
                        "identity": handle["identity"],
                        "kind": handle["kind"],
                        "span_hint": {
                            "start": span["start"],
                            "end": span["end"]
                        }
                    },
                    "op": {
                        "type": "replace",
                        "new_text": "def process_data(value):\n    return value + 2"
                    },
                    "preview": {
                        "old_text": old_text,
                        "new_text": "def process_data(value):\n    return value + 2",
                        "matched_span": {
                            "start": span["start"],
                            "end": span["end"]
                        }
                    }
                }
            ]
        }
    });

    let output = run_identedit_with_stdin(&["apply", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "apply should reject targets missing expected_old_hash"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("missing field `expected_old_hash`")),
        "expected missing target field message"
    );
}

#[test]
fn apply_json_mode_rejects_unsupported_operation_type() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let request = json!({
        "command": "apply",
        "changeset": {
            "file": file_path.to_string_lossy().to_string(),
            "operations": [
                {
                    "target": {
                        "identity": "id-1",
                        "kind": "function_definition",
                        "expected_old_hash": "00"
                    },
                    "op": {
                        "type": "rename"
                    },
                    "preview": {
                        "old_text": "def process_data(value):\n    return value + 1",
                        "new_text": "",
                        "matched_span": {
                            "start": 0,
                            "end": 1
                        }
                    }
                }
            ]
        }
    });

    let output = run_identedit_with_stdin(&["apply", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "apply should reject unsupported op.type"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}

#[test]
fn apply_json_mode_rejects_multiple_move_operations_per_file() {
    let workspace = tempdir().expect("tempdir should be created");
    let source_path = workspace.path().join("source.py");
    fs::write(&source_path, "def keep():\n    return 1\n").expect("fixture write should succeed");
    let destination_a = workspace.path().join("renamed_a.py");
    let destination_b = workspace.path().join("renamed_b.py");

    let request = json!({
        "command": "apply",
        "changeset": {
            "files": [
                {
                    "file": source_path.to_string_lossy().to_string(),
                    "operations": [
                        {
                            "target": {
                                "identity": "unused-identity-a",
                                "kind": "function_definition",
                                "expected_old_hash": "unused-hash-a"
                            },
                            "op": {
                                "type": "move",
                                "to": destination_a.to_string_lossy().to_string()
                            },
                            "preview": {
                                "old_text": "",
                                "new_text": "",
                                "matched_span": {
                                    "start": 0,
                                    "end": 0
                                }
                            }
                        },
                        {
                            "target": {
                                "identity": "unused-identity-b",
                                "kind": "function_definition",
                                "expected_old_hash": "unused-hash-b"
                            },
                            "op": {
                                "type": "move",
                                "to": destination_b.to_string_lossy().to_string()
                            },
                            "preview": {
                                "old_text": "",
                                "new_text": "",
                                "matched_span": {
                                    "start": 0,
                                    "end": 0
                                }
                            }
                        }
                    ]
                }
            ],
            "transaction": {
                "mode": "all_or_nothing"
            }
        }
    });

    let output = run_identedit_with_stdin(&["apply", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "apply should reject multiple move operations in a single file change"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| {
                message.contains("Only one move operation is allowed per file")
            }),
        "expected explicit multiple-move validation error"
    );
}

#[test]
fn apply_json_mode_rejects_move_mixed_with_content_edits_for_same_file() {
    let workspace = tempdir().expect("tempdir should be created");
    let source_path = workspace.path().join("source.py");
    fs::write(&source_path, "def keep():\n    return 1\n").expect("fixture write should succeed");
    let destination = workspace.path().join("renamed.py");

    let request = json!({
        "command": "apply",
        "changeset": {
            "files": [
                {
                    "file": source_path.to_string_lossy().to_string(),
                    "operations": [
                        {
                            "target": {
                                "identity": "unused-identity-move",
                                "kind": "function_definition",
                                "expected_old_hash": "unused-hash-move"
                            },
                            "op": {
                                "type": "move",
                                "to": destination.to_string_lossy().to_string()
                            },
                            "preview": {
                                "old_text": "",
                                "new_text": "",
                                "matched_span": {
                                    "start": 0,
                                    "end": 0
                                }
                            }
                        },
                        {
                            "target": {
                                "identity": "unused-identity-edit",
                                "kind": "function_definition",
                                "expected_old_hash": "unused-hash-edit"
                            },
                            "op": {
                                "type": "replace",
                                "new_text": "def keep():\n    return 2\n"
                            },
                            "preview": {
                                "old_text": "def keep():\n    return 1\n",
                                "new_text": "def keep():\n    return 2\n",
                                "matched_span": {
                                    "start": 0,
                                    "end": 1
                                }
                            }
                        }
                    ]
                }
            ],
            "transaction": {
                "mode": "all_or_nothing"
            }
        }
    });

    let output = run_identedit_with_stdin(&["apply", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "apply should reject move + content-edit mix within the same file change"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| {
                message.contains(
                    "Move cannot be combined with content-edit operations for the same file",
                )
            }),
        "expected move/edit mix validation error"
    );
}

#[test]
fn apply_json_mode_executes_single_move_operation() {
    let workspace = tempdir().expect("tempdir should be created");
    let source_path = workspace.path().join("source.py");
    fs::write(&source_path, "def keep():\n    return 1\n").expect("fixture write should succeed");
    let destination = workspace.path().join("renamed.py");

    let request = json!({
        "command": "apply",
        "changeset": {
            "files": [
                {
                    "file": source_path.to_string_lossy().to_string(),
                    "operations": [
                        {
                            "target": {
                                "identity": "unused-identity-move",
                                "kind": "function_definition",
                                "expected_old_hash": "unused-hash-move"
                            },
                            "op": {
                                "type": "move",
                                "to": destination.to_string_lossy().to_string()
                            },
                            "preview": {
                                "old_text": "",
                                "new_text": "",
                                "matched_span": {
                                    "start": 0,
                                    "end": 0
                                }
                            }
                        }
                    ]
                }
            ],
            "transaction": {
                "mode": "all_or_nothing"
            }
        }
    });

    let output = run_identedit_with_stdin(&["apply", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "single move should execute successfully: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["transaction"]["status"], "committed");
    assert_eq!(response["summary"]["files_modified"], 1);
    assert_eq!(response["summary"]["operations_applied"], 1);
    assert_eq!(response["summary"]["operations_failed"], 0);
    assert!(
        !source_path.exists(),
        "source path should be moved away after successful move apply"
    );
    let destination_text =
        fs::read_to_string(&destination).expect("destination should contain moved source content");
    assert!(
        destination_text.contains("def keep():"),
        "destination file should contain original source text"
    );
}

#[test]
fn apply_json_mode_move_graph_rejects_self_move() {
    let workspace = tempdir().expect("tempdir should be created");
    let source_path = workspace.path().join("source.py");
    fs::write(&source_path, "def keep():\n    return 1\n").expect("fixture write should succeed");

    let request = json!({
        "command": "apply",
        "changeset": {
            "files": [
                {
                    "file": source_path.to_string_lossy().to_string(),
                    "operations": [
                        {
                            "target": {
                                "identity": "unused-identity-move",
                                "kind": "function_definition",
                                "expected_old_hash": "unused-hash-move"
                            },
                            "op": {
                                "type": "move",
                                "to": source_path.to_string_lossy().to_string()
                            },
                            "preview": {
                                "old_text": "",
                                "new_text": "",
                                "matched_span": {
                                    "start": 0,
                                    "end": 0
                                }
                            }
                        }
                    ]
                }
            ],
            "transaction": {
                "mode": "all_or_nothing"
            }
        }
    });

    let output = run_identedit_with_stdin(&["apply", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "self-move should be rejected during move graph validation"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("self-move")),
        "expected self-move validation error"
    );
}

#[test]
fn apply_json_mode_move_graph_rejects_duplicate_destination_paths() {
    let workspace = tempdir().expect("tempdir should be created");
    let source_a = workspace.path().join("a.py");
    let source_b = workspace.path().join("b.py");
    fs::write(&source_a, "def a():\n    return 1\n").expect("fixture write should succeed");
    fs::write(&source_b, "def b():\n    return 2\n").expect("fixture write should succeed");
    let destination = workspace.path().join("renamed.py");

    let request = json!({
        "command": "apply",
        "changeset": {
            "files": [
                {
                    "file": source_a.to_string_lossy().to_string(),
                    "operations": [
                        {
                            "target": {
                                "identity": "unused-identity-a",
                                "kind": "function_definition",
                                "expected_old_hash": "unused-hash-a"
                            },
                            "op": {
                                "type": "move",
                                "to": destination.to_string_lossy().to_string()
                            },
                            "preview": {
                                "old_text": "",
                                "new_text": "",
                                "matched_span": {
                                    "start": 0,
                                    "end": 0
                                }
                            }
                        }
                    ]
                },
                {
                    "file": source_b.to_string_lossy().to_string(),
                    "operations": [
                        {
                            "target": {
                                "identity": "unused-identity-b",
                                "kind": "function_definition",
                                "expected_old_hash": "unused-hash-b"
                            },
                            "op": {
                                "type": "move",
                                "to": destination.to_string_lossy().to_string()
                            },
                            "preview": {
                                "old_text": "",
                                "new_text": "",
                                "matched_span": {
                                    "start": 0,
                                    "end": 0
                                }
                            }
                        }
                    ]
                }
            ],
            "transaction": {
                "mode": "all_or_nothing"
            }
        }
    });

    let output = run_identedit_with_stdin(&["apply", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "duplicate move destinations should be rejected"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("Duplicate move destination")),
        "expected duplicate destination validation error"
    );
}

#[test]
fn apply_json_mode_move_graph_rejects_existing_destination_when_not_chain() {
    let workspace = tempdir().expect("tempdir should be created");
    let source = workspace.path().join("source.py");
    let destination = workspace.path().join("existing.py");
    fs::write(&source, "def source():\n    return 1\n").expect("fixture write should succeed");
    fs::write(&destination, "def existing():\n    return 2\n")
        .expect("fixture write should succeed");

    let request = json!({
        "command": "apply",
        "changeset": {
            "files": [
                {
                    "file": source.to_string_lossy().to_string(),
                    "operations": [
                        {
                            "target": {
                                "identity": "unused-identity-source",
                                "kind": "function_definition",
                                "expected_old_hash": "unused-hash-source"
                            },
                            "op": {
                                "type": "move",
                                "to": destination.to_string_lossy().to_string()
                            },
                            "preview": {
                                "old_text": "",
                                "new_text": "",
                                "matched_span": {
                                    "start": 0,
                                    "end": 0
                                }
                            }
                        }
                    ]
                }
            ],
            "transaction": {
                "mode": "all_or_nothing"
            }
        }
    });

    let output = run_identedit_with_stdin(&["apply", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "existing destination without chain source should be rejected"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| { message.contains("Destination path already exists") }),
        "expected overwrite-policy validation error"
    );
}

#[test]
fn apply_json_mode_move_graph_rejects_cycle() {
    let workspace = tempdir().expect("tempdir should be created");
    let source_a = workspace.path().join("a.py");
    let source_b = workspace.path().join("b.py");
    fs::write(&source_a, "def a():\n    return 1\n").expect("fixture write should succeed");
    fs::write(&source_b, "def b():\n    return 2\n").expect("fixture write should succeed");

    let request = json!({
        "command": "apply",
        "changeset": {
            "files": [
                {
                    "file": source_a.to_string_lossy().to_string(),
                    "operations": [
                        {
                            "target": {
                                "identity": "unused-identity-a",
                                "kind": "function_definition",
                                "expected_old_hash": "unused-hash-a"
                            },
                            "op": {
                                "type": "move",
                                "to": source_b.to_string_lossy().to_string()
                            },
                            "preview": {
                                "old_text": "",
                                "new_text": "",
                                "matched_span": {
                                    "start": 0,
                                    "end": 0
                                }
                            }
                        }
                    ]
                },
                {
                    "file": source_b.to_string_lossy().to_string(),
                    "operations": [
                        {
                            "target": {
                                "identity": "unused-identity-b",
                                "kind": "function_definition",
                                "expected_old_hash": "unused-hash-b"
                            },
                            "op": {
                                "type": "move",
                                "to": source_a.to_string_lossy().to_string()
                            },
                            "preview": {
                                "old_text": "",
                                "new_text": "",
                                "matched_span": {
                                    "start": 0,
                                    "end": 0
                                }
                            }
                        }
                    ]
                }
            ],
            "transaction": {
                "mode": "all_or_nothing"
            }
        }
    });

    let output = run_identedit_with_stdin(&["apply", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "move cycles should be rejected by graph validation"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| {
                message.contains(
                    "Move graph contains a cycle; move operations must form an acyclic chain",
                )
            }),
        "expected explicit cycle validation error"
    );
}

#[test]
fn apply_json_mode_move_graph_executes_chain_in_reverse_topological_order() {
    let workspace = tempdir().expect("tempdir should be created");
    let source_a = workspace.path().join("a.py");
    let source_b = workspace.path().join("b.py");
    let destination_c = workspace.path().join("c.py");
    fs::write(&source_a, "def from_a():\n    return 'a'\n").expect("fixture write should succeed");
    fs::write(&source_b, "def from_b():\n    return 'b'\n").expect("fixture write should succeed");

    let request = json!({
        "command": "apply",
        "changeset": {
            "files": [
                {
                    "file": source_a.to_string_lossy().to_string(),
                    "operations": [
                        {
                            "target": {
                                "identity": "unused-identity-a",
                                "kind": "function_definition",
                                "expected_old_hash": "unused-hash-a"
                            },
                            "op": {
                                "type": "move",
                                "to": source_b.to_string_lossy().to_string()
                            },
                            "preview": {
                                "old_text": "",
                                "new_text": "",
                                "matched_span": {
                                    "start": 0,
                                    "end": 0
                                }
                            }
                        }
                    ]
                },
                {
                    "file": source_b.to_string_lossy().to_string(),
                    "operations": [
                        {
                            "target": {
                                "identity": "unused-identity-b",
                                "kind": "function_definition",
                                "expected_old_hash": "unused-hash-b"
                            },
                            "op": {
                                "type": "move",
                                "to": destination_c.to_string_lossy().to_string()
                            },
                            "preview": {
                                "old_text": "",
                                "new_text": "",
                                "matched_span": {
                                    "start": 0,
                                    "end": 0
                                }
                            }
                        }
                    ]
                }
            ],
            "transaction": {
                "mode": "all_or_nothing"
            }
        }
    });

    let output = run_identedit_with_stdin(&["apply", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "move chain should execute successfully: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["transaction"]["status"], "committed");
    assert_eq!(response["summary"]["files_modified"], 2);
    assert_eq!(response["summary"]["operations_applied"], 2);
    assert_eq!(response["summary"]["operations_failed"], 0);

    assert!(
        !source_a.exists(),
        "first chain source should no longer exist after successful move commit"
    );
    assert!(
        source_b.exists(),
        "intermediate chain path should be recreated as destination of the second move"
    );
    assert!(
        destination_c.exists(),
        "final chain destination should exist after successful move commit"
    );

    let moved_to_b =
        fs::read_to_string(&source_b).expect("intermediate destination should be readable");
    let moved_to_c =
        fs::read_to_string(&destination_c).expect("final destination should be readable");
    assert!(
        moved_to_b.contains("from_a"),
        "a->b should happen after b->c so b ends with source_a content"
    );
    assert!(
        moved_to_c.contains("from_b"),
        "b->c should run first so c ends with original source_b content"
    );
}

#[test]
fn apply_json_mode_move_rejects_duplicate_source_alias_paths() {
    let workspace = tempdir().expect("tempdir should be created");
    let source_path = workspace.path().join("source.py");
    fs::write(&source_path, "def keep():\n    return 1\n").expect("fixture write should succeed");

    let request = json!({
        "command": "apply",
        "changeset": {
            "files": [
                {
                    "file": "source.py",
                    "operations": [
                        {
                            "target": {
                                "identity": "unused-identity-a",
                                "kind": "function_definition",
                                "expected_old_hash": "unused-hash-a"
                            },
                            "op": {
                                "type": "move",
                                "to": "renamed_a.py"
                            },
                            "preview": {
                                "old_text": "",
                                "new_text": "",
                                "matched_span": {
                                    "start": 0,
                                    "end": 0
                                }
                            }
                        }
                    ]
                },
                {
                    "file": "./source.py",
                    "operations": [
                        {
                            "target": {
                                "identity": "unused-identity-b",
                                "kind": "function_definition",
                                "expected_old_hash": "unused-hash-b"
                            },
                            "op": {
                                "type": "move",
                                "to": "renamed_b.py"
                            },
                            "preview": {
                                "old_text": "",
                                "new_text": "",
                                "matched_span": {
                                    "start": 0,
                                    "end": 0
                                }
                            }
                        }
                    ]
                }
            ],
            "transaction": {
                "mode": "all_or_nothing"
            }
        }
    });

    let output = run_identedit_with_stdin_in_dir(
        workspace.path(),
        &["apply", "--json"],
        &request.to_string(),
    );
    assert!(
        !output.status.success(),
        "duplicate canonical source paths should be rejected"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("Duplicate move source path")),
        "expected duplicate move source validation error"
    );
    assert!(source_path.exists(), "source file should remain untouched");
    assert!(
        !workspace.path().join("renamed_a.py").exists(),
        "no destination should be created on validation failure"
    );
    assert!(
        !workspace.path().join("renamed_b.py").exists(),
        "no destination should be created on validation failure"
    );
}
