use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

use serde_json::{Value, json};
use tempfile::Builder;

struct BinaryCase {
    suffix: &'static str,
    provider: &'static str,
}

fn jsts_cases() -> Vec<BinaryCase> {
    vec![
        BinaryCase {
            suffix: ".js",
            provider: "tree-sitter-javascript",
        },
        BinaryCase {
            suffix: ".jsx",
            provider: "tree-sitter-javascript",
        },
        BinaryCase {
            suffix: ".ts",
            provider: "tree-sitter-typescript",
        },
        BinaryCase {
            suffix: ".tsx",
            provider: "tree-sitter-tsx",
        },
    ]
}

fn write_temp_bytes(suffix: &str, bytes: &[u8]) -> PathBuf {
    let mut temp_file = Builder::new()
        .suffix(suffix)
        .tempfile()
        .expect("temp source file should be created");
    temp_file
        .write_all(bytes)
        .expect("temp source write should succeed");
    temp_file.keep().expect("temp file should persist").1
}

fn run_identedit(arguments: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_identedit"));
    command.env("IDENTEDIT_ALLOW_LEGACY", "1");
    command.args(arguments);
    command.output().expect("failed to run identedit binary")
}

fn run_identedit_with_stdin(arguments: &[&str], input: &str) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_identedit"));
    command.env("IDENTEDIT_ALLOW_LEGACY", "1");
    command.args(arguments);
    command.stdin(Stdio::piped());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());

    let mut child = command.spawn().expect("failed to spawn identedit binary");
    let stdin = child.stdin.as_mut().expect("stdin should be available");
    stdin
        .write_all(input.as_bytes())
        .expect("stdin write should succeed");

    child
        .wait_with_output()
        .expect("failed to read process output")
}

fn assert_error_type(output: Output, expected_type: &str) -> Value {
    assert!(!output.status.success(), "command should fail");
    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], expected_type);
    response
}

#[test]
fn jsts_non_utf8_select_transform_are_parse_failure() {
    for case in jsts_cases() {
        let mut bytes = b"function processData(value) {\n  return value + 1;\n}\n".to_vec();
        bytes.push(0xff);
        let file_path = write_temp_bytes(case.suffix, &bytes);
        let path = file_path.to_str().expect("path should be utf-8");

        let select_output = run_identedit(&["select", "--kind", "function_declaration", path]);
        let select_response = assert_error_type(select_output, "parse_failure");
        let select_message = select_response["error"]["message"]
            .as_str()
            .expect("error.message should be a string");
        assert!(
            select_message.contains(case.provider),
            "select parse failure should mention '{}', got: {select_message}",
            case.provider
        );

        let transform_output = run_identedit(&[
            "transform",
            "--identity",
            "deadbeef",
            "--replace",
            "function replacement() { return 0; }",
            path,
        ]);
        let transform_response = assert_error_type(transform_output, "parse_failure");
        let transform_message = transform_response["error"]["message"]
            .as_str()
            .expect("error.message should be a string");
        assert!(
            transform_message.contains(case.provider),
            "transform parse failure should mention '{}', got: {transform_message}",
            case.provider
        );
    }
}

#[test]
fn jsts_non_utf8_apply_is_io_error() {
    for case in jsts_cases() {
        let mut bytes = b"function processData(value) {\n  return value + 1;\n}\n".to_vec();
        bytes.push(0xff);
        let file_path = write_temp_bytes(case.suffix, &bytes);
        let request = json!({
            "files": [{
                "file": file_path.to_str().expect("path should be utf-8"),
                "operations": [{
                    "target": {
                        "identity": "deadbeef",
                        "kind": "function_declaration",
                        "span_hint": { "start": 0, "end": 1 },
                        "expected_old_hash": "00"
                    },
                    "op": { "type": "replace", "new_text": "function replacement() { return 0; }" },
                    "preview": {
                        "old_text": "x",
                        "new_text": "function replacement() { return 0; }",
                        "matched_span": { "start": 0, "end": 1 }
                    }
                }]
            }],
            "transaction": { "mode": "all_or_nothing" }
        });

        let output = run_identedit_with_stdin(&["apply"], &request.to_string());
        assert_error_type(output, "io_error");
    }
}

#[test]
fn jsts_embedded_nul_select_transform_apply_are_parse_failure() {
    for case in jsts_cases() {
        let bytes = b"function processData(value) {\n  return value + 1;\n}\0\n".to_vec();
        let file_path = write_temp_bytes(case.suffix, &bytes);
        let path = file_path.to_str().expect("path should be utf-8");

        let select_output = run_identedit(&["select", "--kind", "function_declaration", path]);
        let select_response = assert_error_type(select_output, "parse_failure");
        let select_message = select_response["error"]["message"]
            .as_str()
            .expect("error.message should be a string");
        assert!(
            select_message.contains(case.provider),
            "select parse failure should mention '{}', got: {select_message}",
            case.provider
        );

        let transform_output = run_identedit(&[
            "transform",
            "--identity",
            "deadbeef",
            "--replace",
            "function replacement() { return 0; }",
            path,
        ]);
        let transform_response = assert_error_type(transform_output, "parse_failure");
        let transform_message = transform_response["error"]["message"]
            .as_str()
            .expect("error.message should be a string");
        assert!(
            transform_message.contains(case.provider),
            "transform parse failure should mention '{}', got: {transform_message}",
            case.provider
        );

        let request = json!({
            "files": [{
                "file": path,
                "operations": [{
                    "target": {
                        "identity": "deadbeef",
                        "kind": "function_declaration",
                        "span_hint": { "start": 0, "end": 1 },
                        "expected_old_hash": "00"
                    },
                    "op": { "type": "replace", "new_text": "function replacement() { return 0; }" },
                    "preview": {
                        "old_text": "x",
                        "new_text": "function replacement() { return 0; }",
                        "matched_span": { "start": 0, "end": 1 }
                    }
                }]
            }],
            "transaction": { "mode": "all_or_nothing" }
        });
        let apply_output = run_identedit_with_stdin(&["apply"], &request.to_string());
        let apply_response = assert_error_type(apply_output, "parse_failure");
        let apply_message = apply_response["error"]["message"]
            .as_str()
            .expect("error.message should be a string");
        assert!(
            apply_message.contains(case.provider),
            "apply parse failure should mention '{}', got: {apply_message}",
            case.provider
        );
    }
}
