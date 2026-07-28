use super::*;

#[test]
fn apply_json_mode_rejects_invalid_json_payload() {
    let output = run_identedit_with_stdin(&["apply", "--json"], "{");
    assert!(
        !output.status.success(),
        "apply should fail for malformed JSON request"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}
#[test]
fn apply_changeset_file_invalid_utf8_contents_return_io_error() {
    let mut changeset_file = Builder::new()
        .suffix(".json")
        .tempfile()
        .expect("changeset temp file should be created");
    changeset_file
        .write_all(&[0xFF, 0xFE, 0xFD])
        .expect("invalid utf8 payload write should succeed");

    let output = run_identedit(&[
        "apply",
        changeset_file
            .path()
            .to_str()
            .expect("changeset path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "apply should fail for invalid UTF-8 changeset file"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "io_error");
}
#[test]
fn apply_changeset_file_bom_only_payload_returns_invalid_request() {
    let mut changeset_file = Builder::new()
        .suffix(".json")
        .tempfile()
        .expect("changeset temp file should be created");
    changeset_file
        .write_all(&[0xEF, 0xBB, 0xBF])
        .expect("bom-only payload write should succeed");

    let output = run_identedit(&[
        "apply",
        changeset_file
            .path()
            .to_str()
            .expect("changeset path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "apply should reject BOM-only changeset payload"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}
#[test]
fn apply_changeset_file_empty_or_whitespace_payload_returns_invalid_request() {
    for payload in ["", " ", "\n\t", "\r\n    "] {
        let changeset_file = write_changeset_json(payload);
        let output = run_identedit(&[
            "apply",
            changeset_file
                .path()
                .to_str()
                .expect("changeset path should be utf-8"),
        ]);
        assert!(
            !output.status.success(),
            "apply should reject empty/whitespace file-mode payload"
        );

        let response: Value =
            serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
        assert_eq!(response["error"]["type"], "invalid_request");
    }
}
#[test]
fn apply_changeset_file_duplicate_field_is_deterministic_parse_error() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let file_literal = json_string_literal(&file_path);
    let payload = format!(
        "{{\"files\":[{{\"file\":\"{file_literal}\",\"operations\":[]}}],\"files\":[{{\"file\":\"{file_literal}\",\"operations\":[]}}]}}"
    );

    let changeset_file = write_raw_changeset_json(&payload);
    let output = run_identedit(&[
        "apply",
        changeset_file
            .path()
            .to_str()
            .expect("changeset path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "apply should reject duplicate fields in changeset file"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("duplicate field")),
        "expected deterministic duplicate-field parse message"
    );
}
#[test]
fn apply_changeset_file_unknown_field_rejected_by_strict_mode() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let changeset = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": [],
        "unexpected": true
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
        !output.status.success(),
        "apply should reject unknown fields in file-mode changeset"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("unknown field `unexpected`")),
        "expected deny_unknown_fields message in file mode"
    );
}
#[test]
fn apply_changeset_file_rejects_wrapped_command_payload() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let payload = json!({
        "command": "apply",
        "file": file_path.to_string_lossy().to_string(),
        "operations": []
    });

    let changeset_file = write_changeset_json(&payload.to_string());
    let output = run_identedit(&[
        "apply",
        changeset_file
            .path()
            .to_str()
            .expect("changeset path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "apply should reject command-wrapped payload in file mode"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("unknown field `command`")),
        "expected strict unknown-field rejection for wrapped command payload"
    );
}
#[test]
fn apply_changeset_file_raw_v1_payload_is_rejected_after_v2_cutover() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let file_literal = json_string_literal(&file_path);
    let payload = format!("{{\"file\":\"{file_literal}\",\"operations\":[]}}");

    let changeset_file = write_raw_changeset_json(&payload);
    let output = run_identedit(&[
        "apply",
        changeset_file
            .path()
            .to_str()
            .expect("changeset path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "apply file mode should reject raw v1 payload post v2 cutover"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("unknown field `file`")),
        "expected explicit v1->v2 parse diagnostic in file mode"
    );
}
#[test]
fn apply_changeset_file_rejects_non_object_top_level_payload() {
    for payload in ["[]", "null", "1"] {
        let changeset_file = write_changeset_json(payload);
        let output = run_identedit(&[
            "apply",
            changeset_file
                .path()
                .to_str()
                .expect("changeset path should be utf-8"),
        ]);
        assert!(
            !output.status.success(),
            "apply should reject non-object file-mode payload"
        );

        let response: Value =
            serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
        assert_eq!(response["error"]["type"], "invalid_request");
    }
}
#[test]
fn apply_changeset_file_utf8_bom_prefixed_payload_returns_invalid_request() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let changeset = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": []
    });

    let mut changeset_file = Builder::new()
        .suffix(".json")
        .tempfile()
        .expect("changeset temp file should be created");
    changeset_file
        .write_all(&[0xEF, 0xBB, 0xBF])
        .expect("bom prefix write should succeed");
    changeset_file
        .write_all(changeset.to_string().as_bytes())
        .expect("changeset payload write should succeed");

    let output = run_identedit(&[
        "apply",
        changeset_file
            .path()
            .to_str()
            .expect("changeset path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "apply should reject BOM-prefixed changeset file payload"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}
#[test]
fn apply_changeset_file_trailing_garbage_returns_invalid_request() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let changeset = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": []
    });
    let payload = format!("{}\ntrailing-garbage", changeset);

    let changeset_file = write_changeset_json(&payload);
    let output = run_identedit(&[
        "apply",
        changeset_file
            .path()
            .to_str()
            .expect("changeset path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "apply should reject trailing garbage in file-mode changeset payload"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}
#[test]
fn apply_changeset_file_trailing_nul_returns_invalid_request() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let changeset = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": []
    });

    let mut changeset_file = Builder::new()
        .suffix(".json")
        .tempfile()
        .expect("changeset temp file should be created");
    changeset_file
        .write_all(changeset.to_string().as_bytes())
        .expect("changeset payload write should succeed");
    changeset_file
        .write_all(&[0x00])
        .expect("trailing nul write should succeed");

    let output = run_identedit(&[
        "apply",
        changeset_file
            .path()
            .to_str()
            .expect("changeset path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "apply should reject file-mode changeset with trailing NUL byte"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}
#[test]
fn apply_changeset_file_nested_duplicate_fields_are_deterministic_parse_errors() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let file_literal = json_string_literal(&file_path);
    let payloads = [
        format!(
            "{{\"files\":[{{\"file\":\"{file_literal}\",\"operations\":[{{\"target\":{{\"identity\":\"a\",\"identity\":\"b\",\"kind\":\"function_definition\",\"expected_old_hash\":\"0000000000000000\"}},\"op\":{{\"type\":\"replace\",\"new_text\":\"x\"}},\"preview\":{{\"old_text\":\"a\",\"new_text\":\"x\",\"matched_span\":{{\"start\":0,\"end\":1}}}}}}]}}],\"transaction\":{{\"mode\":\"all_or_nothing\"}}}}"
        ),
        format!(
            "{{\"files\":[{{\"file\":\"{file_literal}\",\"operations\":[{{\"target\":{{\"identity\":\"a\",\"kind\":\"function_definition\",\"expected_old_hash\":\"0000000000000000\"}},\"op\":{{\"type\":\"replace\",\"new_text\":\"x\",\"new_text\":\"y\"}},\"preview\":{{\"old_text\":\"a\",\"new_text\":\"x\",\"matched_span\":{{\"start\":0,\"end\":1}}}}}}]}}],\"transaction\":{{\"mode\":\"all_or_nothing\"}}}}"
        ),
        format!(
            "{{\"files\":[{{\"file\":\"{file_literal}\",\"operations\":[{{\"target\":{{\"identity\":\"a\",\"kind\":\"function_definition\",\"expected_old_hash\":\"0000000000000000\"}},\"op\":{{\"type\":\"replace\",\"new_text\":\"x\"}},\"preview\":{{\"old_text\":\"a\",\"new_text\":\"x\",\"matched_span\":{{\"start\":0,\"start\":1,\"end\":1}}}}}}]}}],\"transaction\":{{\"mode\":\"all_or_nothing\"}}}}"
        ),
    ];

    for payload in payloads {
        let changeset_file = write_raw_changeset_json(&payload);
        let output = run_identedit(&[
            "apply",
            changeset_file
                .path()
                .to_str()
                .expect("changeset path should be utf-8"),
        ]);
        assert!(
            !output.status.success(),
            "apply should reject nested duplicate fields in file-mode changeset"
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
fn apply_changeset_file_duplicate_transaction_mode_key_is_parse_error() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let file_literal = json_string_literal(&file_path);
    let payload = format!(
        "{{\"files\":[{{\"file\":\"{file_literal}\",\"operations\":[]}}],\"transaction\":{{\"mode\":\"all_or_nothing\",\"mode\":\"all_or_nothing\"}}}}"
    );

    let changeset_file = write_raw_changeset_json(&payload);
    let output = run_identedit(&[
        "apply",
        changeset_file
            .path()
            .to_str()
            .expect("changeset path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "apply should reject duplicate transaction.mode in file-mode changeset"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("duplicate field `mode`")),
        "expected deterministic duplicate transaction.mode parse message"
    );
}
#[test]
fn apply_stdin_mode_rejects_unknown_field_in_bare_changeset_payload() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let payload = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": [],
        "unexpected": true
    });

    let output = run_identedit_with_stdin(&["apply"], &payload.to_string());
    assert!(
        !output.status.success(),
        "apply stdin mode should reject unknown fields in bare changeset"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("unknown field `unexpected`")),
        "expected deny_unknown_fields message in stdin mode"
    );
}
#[test]
fn apply_stdin_mode_raw_v1_payload_is_rejected_after_v2_cutover() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let file_literal = json_string_literal(&file_path);
    let payload = format!("{{\"file\":\"{file_literal}\",\"operations\":[]}}");

    let output = run_identedit_with_raw_stdin(&["apply"], payload.as_bytes());
    assert!(
        !output.status.success(),
        "apply stdin mode should reject raw v1 payload post v2 cutover"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("unknown field `file`")),
        "expected explicit v1->v2 parse diagnostic in stdin mode"
    );
}
#[test]
fn apply_stdin_mode_empty_file_path_returns_io_error() {
    let payload = json!({
        "file": "",
        "operations": []
    });

    let output = run_identedit_with_stdin(&["apply"], &payload.to_string());
    assert!(
        !output.status.success(),
        "apply should fail for bare stdin payload with empty file path"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "io_error");
}
#[test]
fn apply_stdin_mode_escaped_nul_file_path_returns_io_error() {
    let output = run_identedit_with_stdin(&["apply"], r#"{"file":"\u0000","operations":[]}"#);
    assert!(
        !output.status.success(),
        "apply should fail for bare stdin payload with escaped NUL file path"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "io_error");
}
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
            "{{\"files\":[{{\"file\":\"{file_literal}\",\"operations\":[{{\"target\":{{\"identity\":\"a\",\"identity\":\"b\",\"kind\":\"function_definition\",\"expected_old_hash\":\"0000000000000000\"}},\"op\":{{\"type\":\"replace\",\"new_text\":\"x\"}},\"preview\":{{\"old_text\":\"a\",\"new_text\":\"x\",\"matched_span\":{{\"start\":0,\"end\":1}}}}}}]}}],\"transaction\":{{\"mode\":\"all_or_nothing\"}}}}"
        ),
        format!(
            "{{\"files\":[{{\"file\":\"{file_literal}\",\"operations\":[{{\"target\":{{\"identity\":\"a\",\"kind\":\"function_definition\",\"expected_old_hash\":\"0000000000000000\"}},\"op\":{{\"type\":\"replace\",\"new_text\":\"x\",\"new_text\":\"y\"}},\"preview\":{{\"old_text\":\"a\",\"new_text\":\"x\",\"matched_span\":{{\"start\":0,\"end\":1}}}}}}]}}],\"transaction\":{{\"mode\":\"all_or_nothing\"}}}}"
        ),
        format!(
            "{{\"files\":[{{\"file\":\"{file_literal}\",\"operations\":[{{\"target\":{{\"identity\":\"a\",\"kind\":\"function_definition\",\"expected_old_hash\":\"0000000000000000\"}},\"op\":{{\"type\":\"replace\",\"new_text\":\"x\"}},\"preview\":{{\"old_text\":\"a\",\"new_text\":\"x\",\"matched_span\":{{\"start\":0,\"start\":1,\"end\":1}}}}}}]}}],\"transaction\":{{\"mode\":\"all_or_nothing\"}}}}"
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
    let expected_hash = crate::common::hash_text(old_text);
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
                        "expected_old_hash": "0000000000000000"
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
fn apply_json_mode_rejects_unknown_span_field() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_handle(&file_path, "process_*");
    let span = &handle["span"];
    let old_text = handle["text"].as_str().expect("text should be string");
    let expected_hash = crate::common::hash_text(old_text);
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
                            "end": span["end"],
                            "unexpected": 0
                        }
                    },
                    "op": {
                        "type": "replace",
                        "new_text": "def process_data(value):\n    return value + 3"
                    },
                    "preview": {
                        "old_text": old_text,
                        "new_text": "def process_data(value):\n    return value + 3",
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
        "apply should reject unknown span fields"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("unknown field `unexpected`")),
        "expected unknown span field message"
    );
}
#[test]
fn apply_json_mode_rejects_operations_null_type() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let request = json!({
        "command": "apply",
        "changeset": {
            "file": file_path.to_string_lossy().to_string(),
            "operations": Value::Null
        }
    });

    let output = run_identedit_with_stdin(&["apply", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "apply should reject null operations payload"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}

#[test]
fn apply_json_mode_rejects_edit_and_move_split_across_duplicate_file_entries() {
    let workspace = tempdir().expect("tempdir should be created");
    let source_path = workspace.path().join("source.py");
    let original = "def keep():\n    return 1\n";
    fs::write(&source_path, original).expect("fixture write should succeed");
    let destination = workspace.path().join("renamed.py");
    let move_target = file_move_target(&source_path);
    let expected_file_hash = move_target["expected_file_hash"].clone();

    let request = json!({
        "command": "apply",
        "changeset": {
            "files": [
                {
                    "file": source_path.to_string_lossy().to_string(),
                    "operations": [
                        {
                            "target": {
                                "type": "file_start",
                                "expected_file_hash": expected_file_hash
                            },
                            "op": {
                                "type": "insert",
                                "new_text": "# header\n"
                            },
                            "preview": {
                                "old_text": "",
                                "new_text": "# header\n",
                                "matched_span": {
                                    "start": 0,
                                    "end": 0
                                }
                            }
                        }
                    ]
                },
                {
                    "file": source_path.to_string_lossy().to_string(),
                    "operations": [
                        {
                            "target": move_target,
                            "op": {
                                "type": "move",
                                "to": destination.to_string_lossy().to_string()
                            },
                            "preview": file_move_preview(
                                source_path.to_string_lossy().to_string(),
                                destination.to_string_lossy().to_string()
                            )
                        }
                    ]
                }
            ],
            "transaction": {
                "mode": "all_or_nothing"
            }
        }
    });

    let output = run_identedit_with_stdin(&["apply", "--json", "--dry-run"], &request.to_string());
    assert!(
        !output.status.success(),
        "apply should reject duplicate file entries before preflight locking"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("Duplicate file entry in changeset.files")),
        "expected duplicate file entry diagnostic, got: {response}"
    );
    assert_eq!(
        fs::read_to_string(&source_path).expect("source should remain readable"),
        original
    );
    assert!(!destination.exists());
}

#[test]
fn apply_json_mode_treats_env_token_file_path_as_literal() {
    let request = json!({
        "command": "apply",
        "changeset": {
            "file": format!("${{IDENTEDIT_APPLY_JSON_PATH_{}}}/example.py", std::process::id()),
            "operations": []
        }
    });

    let output = run_identedit_with_stdin(&["apply", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "json-mode apply path should not expand env tokens"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "io_error");
}
#[test]
fn apply_json_mode_rejects_non_apply_command() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let request = json!({
        "command": "edit",
        "changeset": {
            "file": file_path.to_string_lossy().to_string(),
            "operations": []
        }
    });

    let output = run_identedit_with_stdin(&["apply", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "apply should reject command mismatch in JSON mode"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("expected 'apply'")),
        "expected command mismatch message"
    );
}
#[test]
fn apply_json_mode_rejects_missing_command_field() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let request = json!({
        "changeset": {
            "file": file_path.to_string_lossy().to_string(),
            "operations": []
        }
    });

    let output = run_identedit_with_stdin(&["apply", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "apply should reject missing command field in JSON mode"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("missing field `command`")),
        "expected missing command field message"
    );
}
#[test]
fn apply_json_mode_rejects_non_string_command_type() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let payloads = [
        json!({
            "command": 123,
            "changeset": {
                "file": file_path.to_string_lossy().to_string(),
                "operations": []
            }
        }),
        json!({
            "command": null,
            "changeset": {
                "file": file_path.to_string_lossy().to_string(),
                "operations": []
            }
        }),
        json!({
            "command": {"value": "apply"},
            "changeset": {
                "file": file_path.to_string_lossy().to_string(),
                "operations": []
            }
        }),
    ];

    for payload in payloads {
        let output = run_identedit_with_stdin(&["apply", "--json"], &payload.to_string());
        assert!(
            !output.status.success(),
            "apply should reject non-string command in JSON mode: {payload}"
        );

        let response: Value =
            serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
        assert_eq!(response["error"]["type"], "invalid_request");
        assert!(
            response["error"]["message"]
                .as_str()
                .is_some_and(|message| message.contains("invalid type")),
            "expected invalid type command diagnostic"
        );
    }
}
#[test]
fn apply_json_mode_rejects_command_with_trailing_whitespace() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let request = json!({
        "command": "apply ",
        "changeset": {
            "file": file_path.to_string_lossy().to_string(),
            "operations": []
        }
    });

    let output = run_identedit_with_stdin(&["apply", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "apply should reject trailing-whitespace command token"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}
#[test]
fn apply_json_mode_rejects_uppercase_command_token() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let request = json!({
        "command": "APPLY",
        "changeset": {
            "file": file_path.to_string_lossy().to_string(),
            "operations": []
        }
    });

    let output = run_identedit_with_stdin(&["apply", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "apply should reject uppercase command token"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}
#[test]
fn apply_changeset_argument_directory_returns_io_error() {
    let directory = tempdir().expect("tempdir should be created");
    let output = run_identedit(&[
        "apply",
        directory.path().to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "apply should fail when CHANGESET argument is a directory"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "io_error");
}
#[test]
fn apply_json_mode_directory_target_returns_io_error() {
    let directory = tempdir().expect("tempdir should be created");
    let request = json!({
        "command": "apply",
        "changeset": {
            "file": directory.path().to_string_lossy().to_string(),
            "operations": []
        }
    });

    let output = run_identedit_with_stdin(&["apply", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "apply JSON mode should fail when changeset file target is a directory"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "io_error");
}
