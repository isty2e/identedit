use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use proptest::prelude::*;
use serde_json::Value;
use tempfile::Builder;

fn run_identedit_with_raw_stdin(arguments: &[&str], input: &[u8]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_identedit"));
    command.args(arguments);
    command.stdin(Stdio::piped());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());

    let mut child = command.spawn().expect("failed to spawn identedit binary");
    child
        .stdin
        .as_mut()
        .expect("stdin should be available")
        .write_all(input)
        .expect("stdin write should succeed");
    child
        .wait_with_output()
        .expect("failed to read process output")
}

fn assert_structured_error(output: &Output, expected_type: &str) {
    assert!(
        !output.status.success(),
        "command should fail for malformed payload, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value = serde_json::from_slice(&output.stdout)
        .expect("stdout should remain structured JSON even for malformed payload");
    assert_eq!(
        response["error"]["type"], expected_type,
        "unexpected error classification for malformed payload"
    );
}

fn assert_json_mode_error_contract(input: &[u8], expected_type: &str) {
    for command in ["read", "edit", "apply"] {
        let output = run_identedit_with_raw_stdin(&[command, "--json"], input);
        assert_structured_error(&output, expected_type);
    }
}

fn assert_structured_success(output: &Output) -> Value {
    assert!(
        output.status.success(),
        "command should succeed for valid payload, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("stdout should remain structured JSON")
}

fn create_temp_python_file() -> PathBuf {
    let mut temp_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp python file should be created");
    temp_file
        .write_all(b"def process_data(value):\n    return value + 1\n")
        .expect("temp fixture write should succeed");
    temp_file.keep().expect("temp file should persist").1
}

fn valid_json_mode_payloads(file_path: &Path) -> [(&'static str, String); 3] {
    let file_literal = file_path
        .to_str()
        .expect("path should be utf-8")
        .replace('\\', "\\\\")
        .replace('"', "\\\"");

    let select_payload = format!(
        "{{\"command\":\"select\",\"file\":\"{file_literal}\",\"selector\":{{\"kind\":\"function_definition\",\"exclude_kinds\":[]}}}}"
    );
    let transform_payload =
        format!("{{\"command\":\"transform\",\"file\":\"{file_literal}\",\"operations\":[]}}");
    let apply_payload = format!(
        "{{\"command\":\"apply\",\"changeset\":{{\"files\":[{{\"file\":\"{file_literal}\",\"operations\":[]}}],\"transaction\":{{\"mode\":\"all_or_nothing\"}}}}}}"
    );

    [
        ("read", select_payload),
        ("edit", transform_payload),
        ("apply", apply_payload),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    #[test]
    fn malformed_utf8_payloads_return_invalid_request(
        mut tail in proptest::collection::vec(0x20u8..=0x7Eu8, 0..128)
    ) {
        let mut payload = Vec::with_capacity(tail.len() + 1);
        payload.push(b'!');
        payload.append(&mut tail);

        assert_json_mode_error_contract(&payload, "invalid_request");
    }

    #[test]
    fn non_utf8_payloads_return_io_error(
        mut tail in proptest::collection::vec(any::<u8>(), 0..128)
    ) {
        let mut payload = Vec::with_capacity(tail.len() + 1);
        payload.push(0xFF);
        payload.append(&mut tail);

        assert_json_mode_error_contract(&payload, "io_error");
    }
}

#[test]
fn json_mode_utf8_bom_prefixed_payloads_return_invalid_request() {
    let file_path = create_temp_python_file();

    for (command, payload) in valid_json_mode_payloads(&file_path) {
        let mut bom_prefixed = Vec::with_capacity(payload.len() + 3);
        bom_prefixed.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
        bom_prefixed.extend_from_slice(payload.as_bytes());

        let output = run_identedit_with_raw_stdin(&[command, "--json"], &bom_prefixed);
        assert_structured_error(&output, "invalid_request");
    }
}

#[test]
fn json_mode_empty_or_whitespace_payloads_return_invalid_request() {
    let payloads: [&[u8]; 4] = [b"", b" ", b"\n\t", b"\r\n   \t"];

    for payload in payloads {
        assert_json_mode_error_contract(payload, "invalid_request");
    }
}

#[test]
fn json_mode_non_object_top_level_payloads_return_invalid_request() {
    for payload in [b"[]" as &[u8], b"null", b"1", b"\"text\""] {
        assert_json_mode_error_contract(payload, "invalid_request");
    }
}

#[test]
fn json_mode_empty_file_path_returns_io_error_for_all_commands() {
    let payloads = [
        (
            "read",
            r#"{"command": "read","file":"","selector":{"kind":"function_definition","exclude_kinds":[]}}"#,
        ),
        ("edit", r#"{"command":"edit","file":"","operations":[]}"#),
        (
            "apply",
            r#"{"command":"apply","changeset":{"files":[{"file":"","operations":[]}],"transaction":{"mode":"all_or_nothing"}}}"#,
        ),
    ];

    for (command, payload) in payloads {
        let output = run_identedit_with_raw_stdin(&[command, "--json"], payload.as_bytes());
        assert_structured_error(&output, "io_error");
    }
}

#[test]
fn json_mode_escaped_nul_file_path_returns_io_error_for_all_commands() {
    let payloads = [
        (
            "read",
            r#"{"command": "read","file":"\u0000","selector":{"kind":"function_definition","exclude_kinds":[]}}"#,
        ),
        (
            "edit",
            r#"{"command":"edit","file":"\u0000","operations":[]}"#,
        ),
        (
            "apply",
            r#"{"command":"apply","changeset":{"files":[{"file":"\u0000","operations":[]}],"transaction":{"mode":"all_or_nothing"}}}"#,
        ),
    ];

    for (command, payload) in payloads {
        let output = run_identedit_with_raw_stdin(&[command, "--json"], payload.as_bytes());
        assert_structured_error(&output, "io_error");
    }
}

#[test]
fn json_mode_bom_only_payload_returns_invalid_request() {
    assert_json_mode_error_contract(&[0xEF, 0xBB, 0xBF], "invalid_request");
}

#[test]
fn json_mode_trailing_garbage_after_valid_payload_returns_invalid_request() {
    let file_path = create_temp_python_file();

    for (command, payload) in valid_json_mode_payloads(&file_path) {
        let mut tailed = payload.into_bytes();
        tailed.extend_from_slice(b"\ntrailing-garbage");

        let output = run_identedit_with_raw_stdin(&[command, "--json"], &tailed);
        assert_structured_error(&output, "invalid_request");
    }
}

#[test]
fn json_mode_trailing_nul_after_valid_payload_returns_invalid_request() {
    let file_path = create_temp_python_file();

    for (command, payload) in valid_json_mode_payloads(&file_path) {
        let mut tailed = payload.into_bytes();
        tailed.push(0x00);

        let output = run_identedit_with_raw_stdin(&[command, "--json"], &tailed);
        assert_structured_error(&output, "invalid_request");
    }
}

#[test]
fn json_mode_command_type_mismatch_returns_invalid_request() {
    let file_path = create_temp_python_file();
    let file_literal = file_path
        .to_str()
        .expect("path should be utf-8")
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    let command_tokens = ["1", "null", "{}", "[]"];

    for command_token in command_tokens {
        let select_payload = format!(
            "{{\"command\":{command_token},\"file\":\"{file_literal}\",\"selector\":{{\"kind\":\"function_definition\",\"exclude_kinds\":[]}}}}"
        );
        let transform_payload = format!(
            "{{\"command\":{command_token},\"file\":\"{file_literal}\",\"operations\":[]}}"
        );
        let apply_payload = format!(
            "{{\"command\":{command_token},\"changeset\":{{\"files\":[{{\"file\":\"{file_literal}\",\"operations\":[]}}],\"transaction\":{{\"mode\":\"all_or_nothing\"}}}}}}"
        );

        for (command, payload) in [
            ("read", select_payload),
            ("edit", transform_payload),
            ("apply", apply_payload),
        ] {
            let output = run_identedit_with_raw_stdin(&[command, "--json"], payload.as_bytes());
            assert_structured_error(&output, "invalid_request");
        }
    }
}

#[test]
fn json_mode_missing_command_field_returns_invalid_request() {
    let file_path = create_temp_python_file();
    let file_literal = file_path
        .to_str()
        .expect("path should be utf-8")
        .replace('\\', "\\\\")
        .replace('"', "\\\"");

    let select_payload = format!(
        "{{\"file\":\"{file_literal}\",\"selector\":{{\"kind\":\"function_definition\",\"exclude_kinds\":[]}}}}"
    );
    let transform_payload = format!("{{\"file\":\"{file_literal}\",\"operations\":[]}}");
    let apply_payload = format!(
        "{{\"changeset\":{{\"files\":[{{\"file\":\"{file_literal}\",\"operations\":[]}}],\"transaction\":{{\"mode\":\"all_or_nothing\"}}}}}}"
    );

    for (command, payload) in [
        ("read", select_payload),
        ("edit", transform_payload),
        ("apply", apply_payload),
    ] {
        let output = run_identedit_with_raw_stdin(&[command, "--json"], payload.as_bytes());
        assert_structured_error(&output, "invalid_request");
    }
}

#[test]
fn json_mode_whitespace_framed_valid_payloads_still_succeed() {
    let file_path = create_temp_python_file();

    for (command, payload) in valid_json_mode_payloads(&file_path) {
        let framed_payload = format!("\n\t {payload} \r\n");
        let output = run_identedit_with_raw_stdin(&[command, "--json"], framed_payload.as_bytes());
        let response = assert_structured_success(&output);

        match command {
            "read" => assert_eq!(response["summary"]["files_scanned"], 1),
            "edit" => assert_eq!(
                response["files"][0]["operations"].as_array().map(Vec::len),
                Some(0)
            ),
            "apply" => assert_eq!(response["summary"]["operations_applied"], 0),
            _ => panic!("unexpected command {command}"),
        }
    }
}

#[test]
fn duplicate_command_keys_produce_deterministic_parse_errors() {
    let file_path = create_temp_python_file();
    let file_literal = file_path
        .to_str()
        .expect("path should be utf-8")
        .replace('\\', "\\\\")
        .replace('"', "\\\"");

    let select_payload = format!(
        "{{\"command\":\"select\",\"command\":\"apply\",\"file\":\"{file_literal}\",\"selector\":{{\"kind\":\"function_definition\",\"exclude_kinds\":[]}}}}"
    );
    let transform_payload = format!(
        "{{\"command\":\"transform\",\"command\":\"select\",\"file\":\"{file_literal}\",\"operations\":[]}}"
    );
    let apply_payload = format!(
        "{{\"command\":\"apply\",\"command\":\"transform\",\"changeset\":{{\"files\":[{{\"file\":\"{file_literal}\",\"operations\":[]}}],\"transaction\":{{\"mode\":\"all_or_nothing\"}}}}}}"
    );

    let cases = [
        ("read", select_payload),
        ("edit", transform_payload),
        ("apply", apply_payload),
    ];

    for (command, payload) in cases {
        let output = run_identedit_with_raw_stdin(&[command, "--json"], payload.as_bytes());
        assert!(
            !output.status.success(),
            "{command} should reject duplicate command-key payload"
        );

        let response: Value =
            serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
        assert_eq!(response["error"]["type"], "invalid_request");
        assert!(
            response["error"]["message"]
                .as_str()
                .is_some_and(|message| message.contains("duplicate field `command`")),
            "{command} should report deterministic duplicate-field parse error"
        );
    }
}

#[test]
fn apply_json_duplicate_nested_keys_produce_parse_errors() {
    let file_path = create_temp_python_file();
    let file_literal = file_path
        .to_str()
        .expect("path should be utf-8")
        .replace('\\', "\\\\")
        .replace('"', "\\\"");

    let payloads = [
        format!(
            "{{\"command\":\"apply\",\"changeset\":{{\"files\":[{{\"file\":\"{file_literal}\",\"operations\":[{{\"target\":{{\"identity\":\"a\",\"identity\":\"b\",\"kind\":\"function_definition\",\"expected_old_hash\":\"00\"}},\"op\":{{\"type\":\"replace\",\"new_text\":\"x\"}},\"preview\":{{\"old_text\":\"a\",\"new_text\":\"x\",\"matched_span\":{{\"start\":0,\"end\":1}}}}}}]}}],\"transaction\":{{\"mode\":\"all_or_nothing\"}}}}}}"
        ),
        format!(
            "{{\"command\":\"apply\",\"changeset\":{{\"files\":[{{\"file\":\"{file_literal}\",\"operations\":[{{\"target\":{{\"identity\":\"a\",\"kind\":\"function_definition\",\"expected_old_hash\":\"00\"}},\"op\":{{\"type\":\"replace\",\"new_text\":\"x\",\"new_text\":\"y\"}},\"preview\":{{\"old_text\":\"a\",\"new_text\":\"x\",\"matched_span\":{{\"start\":0,\"end\":1}}}}}}]}}],\"transaction\":{{\"mode\":\"all_or_nothing\"}}}}}}"
        ),
        format!(
            "{{\"command\":\"apply\",\"changeset\":{{\"files\":[{{\"file\":\"{file_literal}\",\"operations\":[{{\"target\":{{\"identity\":\"a\",\"kind\":\"function_definition\",\"expected_old_hash\":\"00\"}},\"op\":{{\"type\":\"replace\",\"new_text\":\"x\"}},\"preview\":{{\"old_text\":\"a\",\"new_text\":\"x\",\"matched_span\":{{\"start\":0,\"start\":1,\"end\":1}}}}}}]}}],\"transaction\":{{\"mode\":\"all_or_nothing\"}}}}}}"
        ),
    ];

    for payload in payloads {
        let output = run_identedit_with_raw_stdin(&["apply", "--json"], payload.as_bytes());
        assert!(
            !output.status.success(),
            "apply should reject nested duplicate-key payload"
        );

        let response: Value =
            serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
        assert_eq!(response["error"]["type"], "invalid_request");
        assert!(
            response["error"]["message"]
                .as_str()
                .is_some_and(|message| message.contains("duplicate field")),
            "expected deterministic duplicate-field parse error"
        );
    }
}

#[test]
fn select_json_duplicate_nested_keys_produce_parse_errors() {
    let file_path = create_temp_python_file();
    let file_literal = file_path
        .to_str()
        .expect("path should be utf-8")
        .replace('\\', "\\\\")
        .replace('"', "\\\"");

    let payloads = [
        format!(
            "{{\"command\":\"select\",\"file\":\"{file_literal}\",\"selector\":{{\"kind\":\"function_definition\",\"kind\":\"class_definition\",\"exclude_kinds\":[]}}}}"
        ),
        format!(
            "{{\"command\":\"select\",\"file\":\"{file_literal}\",\"selector\":{{\"kind\":\"function_definition\",\"name_pattern\":\"process_*\",\"name_pattern\":\"helper*\",\"exclude_kinds\":[]}}}}"
        ),
        format!(
            "{{\"command\":\"select\",\"file\":\"{file_literal}\",\"selector\":{{\"kind\":\"function_definition\",\"exclude_kinds\":[],\"exclude_kinds\":[\"comment\"]}}}}"
        ),
    ];

    for payload in payloads {
        let output = run_identedit_with_raw_stdin(
            &["read", "--json"],
            payload.as_bytes(),
        );
        assert!(
            !output.status.success(),
            "select should reject nested duplicate-key payload"
        );

        let response: Value =
            serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
        assert_eq!(response["error"]["type"], "invalid_request");
        assert!(
            response["error"]["message"]
                .as_str()
                .is_some_and(|message| message.contains("duplicate field")),
            "expected deterministic duplicate-field parse error"
        );
    }
}

#[test]
fn transform_json_duplicate_nested_keys_produce_parse_errors() {
    let file_path = create_temp_python_file();
    let file_literal = file_path
        .to_str()
        .expect("path should be utf-8")
        .replace('\\', "\\\\")
        .replace('"', "\\\"");

    let payloads = [
        format!(
            "{{\"command\":\"transform\",\"file\":\"{file_literal}\",\"operations\":[{{\"identity\":\"id1\",\"identity\":\"id2\",\"kind\":\"function_definition\",\"expected_old_hash\":\"00\",\"op\":{{\"type\":\"replace\",\"new_text\":\"x\"}}}}]}}"
        ),
        format!(
            "{{\"command\":\"transform\",\"file\":\"{file_literal}\",\"operations\":[{{\"identity\":\"id1\",\"kind\":\"function_definition\",\"expected_old_hash\":\"00\",\"op\":{{\"type\":\"replace\",\"new_text\":\"x\",\"new_text\":\"y\"}}}}]}}"
        ),
        format!(
            "{{\"command\":\"transform\",\"file\":\"{file_literal}\",\"operations\":[{{\"identity\":\"id1\",\"kind\":\"function_definition\",\"span_hint\":{{\"start\":0,\"start\":1,\"end\":2}},\"expected_old_hash\":\"00\",\"op\":{{\"type\":\"replace\",\"new_text\":\"x\"}}}}]}}"
        ),
    ];

    for payload in payloads {
        let output = run_identedit_with_raw_stdin(&["edit", "--json"], payload.as_bytes());
        assert!(
            !output.status.success(),
            "transform should reject nested duplicate-key payload"
        );

        let response: Value =
            serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
        assert_eq!(response["error"]["type"], "invalid_request");
        assert!(
            response["error"]["message"]
                .as_str()
                .is_some_and(|message| message.contains("duplicate field")),
            "expected deterministic duplicate-field parse error"
        );
    }
}
