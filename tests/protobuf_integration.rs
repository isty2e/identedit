use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use serde_json::{Value, json};
use tempfile::Builder;

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn copy_fixture_to_temp(name: &str, suffix: &str) -> PathBuf {
    let source = fixture_path(name);
    let content = fs::read_to_string(&source).expect("fixture should be readable");
    let mut temp_file = Builder::new()
        .suffix(suffix)
        .tempfile()
        .expect("temp source file should be created");
    temp_file
        .write_all(content.as_bytes())
        .expect("temp fixture write should succeed");
    temp_file.keep().expect("temp file should persist").1
}

fn write_temp_source(suffix: &str, source: &str) -> PathBuf {
    let mut temp_file = Builder::new()
        .suffix(suffix)
        .tempfile()
        .expect("temp source file should be created");
    temp_file
        .write_all(source.as_bytes())
        .expect("temp source write should succeed");
    temp_file.keep().expect("temp file should persist").1
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
    command.args(arguments);
    command.output().expect("failed to run identedit binary")
}

fn run_identedit_with_stdin(arguments: &[&str], input: &str) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_identedit"));
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

fn assert_select_kind_contains_text(file: &Path, kind: &str, snippet: &str) {
    let output = run_identedit(&[
        "read",
        "--json",
        "--verbose",
        "--kind",
        kind,
        file.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "select failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    let handles = response["handles"]
        .as_array()
        .expect("handles should be an array");
    assert!(
        !handles.is_empty(),
        "expected at least one handle for {kind}"
    );
    assert!(
        handles.iter().any(|handle| {
            handle["text"]
                .as_str()
                .is_some_and(|text| text.contains(snippet))
        }),
        "expected at least one {kind} handle containing '{snippet}'"
    );
}

#[test]
fn select_covers_protobuf_kinds_and_provider() {
    let proto_file = fixture_path("example.proto");

    assert_select_kind_contains_text(&proto_file, "syntax", "proto3");
    assert_select_kind_contains_text(&proto_file, "enum", "enum Status");
    assert_select_kind_contains_text(&proto_file, "message", "message Request");
    assert_select_kind_contains_text(&proto_file, "service", "service KannaService");
    assert_select_kind_contains_text(&proto_file, "rpc", "GetStatus");
}

#[test]
fn select_supports_case_insensitive_protobuf_extension() {
    let file_path = copy_fixture_to_temp("example.proto", ".PROTO");
    assert_select_kind_contains_text(&file_path, "message", "message Request");
}

#[test]
fn select_supports_utf8_bom_prefixed_protobuf_files() {
    let fixture = fs::read(fixture_path("example.proto")).expect("fixture should be readable");
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(&fixture);
    let file_path = write_temp_bytes(".proto", &bytes);

    assert_select_kind_contains_text(&file_path, "service", "service KannaService");
}

#[test]
fn select_reports_parse_failure_for_syntax_invalid_protobuf() {
    let file_path = write_temp_source(
        ".proto",
        "syntax = \"proto3\";\n\nmessage Broken {\n  string id = 1;\n",
    );
    let output = run_identedit(&[
        "read",
        "--json",
        "--verbose",
        "--kind",
        "message",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "syntax-invalid protobuf should fail under the protobuf provider"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "parse_failure");
    let message = response["error"]["message"]
        .as_str()
        .expect("error.message should be a string");
    assert!(message.contains("tree-sitter-proto"));
    assert!(message.contains("Syntax errors detected in Protobuf source"));
}

#[test]
fn transform_replace_and_apply_support_protobuf_message() {
    let file_path = copy_fixture_to_temp("example.proto", ".proto");
    let select_output = run_identedit(&[
        "read",
        "--json",
        "--verbose",
        "--kind",
        "message",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        select_output.status.success(),
        "select failed: {}",
        String::from_utf8_lossy(&select_output.stderr)
    );

    let select_response: Value =
        serde_json::from_slice(&select_output.stdout).expect("stdout should be valid JSON");
    let identity = select_response["handles"]
        .as_array()
        .expect("handles should be an array")
        .iter()
        .find(|handle| {
            handle["text"]
                .as_str()
                .is_some_and(|text| text.contains("message Response"))
        })
        .and_then(|handle| handle["identity"].as_str())
        .expect("response message identity should be present");

    let replacement = "message Response {\n  string message = 1;\n  string trace_id = 2;\n}";
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

    let transform_json =
        std::str::from_utf8(&transform_output.stdout).expect("transform output should be utf-8");
    let apply_output = run_identedit_with_stdin(&["apply"], transform_json);
    assert!(
        apply_output.status.success(),
        "apply failed: {}",
        String::from_utf8_lossy(&apply_output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("file should be readable");
    assert!(modified.contains("string trace_id = 2;"));
}

#[test]
fn transform_reports_ambiguous_target_for_duplicate_protobuf_message_identity() {
    let source = "syntax = \"proto3\";\n\nmessage Repeat {\n  string value = 1;\n}\n\nmessage Repeat {\n  string value = 1;\n}\n";
    let file_path = write_temp_source(".proto", source);
    let select_output = run_identedit(&[
        "read",
        "--json",
        "--verbose",
        "--kind",
        "message",
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
        .expect("handles should be an array");
    let duplicate_identity = handles
        .iter()
        .map(|handle| {
            handle["identity"]
                .as_str()
                .expect("identity should be string")
        })
        .find(|identity| {
            handles
                .iter()
                .filter(|handle| handle["identity"] == *identity)
                .count()
                >= 2
        })
        .expect("fixture should include duplicate message identity");

    let output = run_identedit(&[
        "edit",
        "--identity",
        duplicate_identity,
        "--replace",
        "message Repeat {\n  string value = 1;\n  string updated = 2;\n}",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "transform should fail for ambiguous duplicate Protobuf message identity"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "ambiguous_target");
}

#[test]
fn transform_json_span_hint_disambiguates_duplicate_protobuf_message_identity() {
    let source = "syntax = \"proto3\";\n\nmessage Repeat {\n  string value = 1;\n}\n\nmessage Repeat {\n  string value = 1;\n}\n";
    let file_path = write_temp_source(".proto", source);
    let select_output = run_identedit(&[
        "read",
        "--json",
        "--verbose",
        "--kind",
        "message",
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
        .expect("handles should be an array");
    let duplicate_identity = handles
        .iter()
        .map(|handle| {
            handle["identity"]
                .as_str()
                .expect("identity should be string")
        })
        .find(|identity| {
            handles
                .iter()
                .filter(|handle| handle["identity"].as_str() == Some(*identity))
                .count()
                >= 2
        })
        .expect("fixture should include duplicate message identity");
    let duplicate_handles = handles
        .iter()
        .filter(|handle| handle["identity"].as_str() == Some(duplicate_identity))
        .collect::<Vec<_>>();
    assert!(
        duplicate_handles.len() >= 2,
        "fixture should include duplicate message handles"
    );

    let target = duplicate_handles[1];
    let span = &target["span"];
    let request = json!({
        "command": "edit",
        "file": file_path.to_string_lossy(),
        "operations": [{
            "target": {
                "type": "node",
                "identity": target["identity"],
                "kind": target["kind"],
                "expected_old_hash": target["expected_old_hash"],
                "span_hint": {"start": span["start"], "end": span["end"]}
            },
            "op": {
                "type": "replace",
                "new_text": "message Repeat {\n  string value = 1;\n  string updated = 2;\n}"
            }
        }]
    });
    let request_body = serde_json::to_string(&request).expect("request should serialize");

    let transform_output = run_identedit_with_stdin(&["edit", "--json"], &request_body);
    assert!(
        transform_output.status.success(),
        "transform --json should disambiguate duplicate Protobuf message identity: {}",
        String::from_utf8_lossy(&transform_output.stderr)
    );

    let transform_json =
        std::str::from_utf8(&transform_output.stdout).expect("transform output should be utf-8");
    let apply_output = run_identedit_with_stdin(&["apply"], transform_json);
    assert!(
        apply_output.status.success(),
        "apply failed after Protobuf span_hint disambiguation: {}",
        String::from_utf8_lossy(&apply_output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("file should be readable");
    assert_eq!(modified.matches("string updated = 2;").count(), 1);
    assert_eq!(modified.matches("message Repeat").count(), 2);
}

#[test]
fn transform_json_duplicate_protobuf_identity_with_missed_span_hint_returns_ambiguous_target() {
    let source = "syntax = \"proto3\";\n\nmessage Repeat {\n  string value = 1;\n}\n\nmessage Repeat {\n  string value = 1;\n}\n";
    let file_path = write_temp_source(".proto", source);
    let select_output = run_identedit(&[
        "read",
        "--json",
        "--verbose",
        "--kind",
        "message",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        select_output.status.success(),
        "select failed: {}",
        String::from_utf8_lossy(&select_output.stderr)
    );

    let select_response: Value =
        serde_json::from_slice(&select_output.stdout).expect("stdout should be valid JSON");
    let target = select_response["handles"]
        .as_array()
        .expect("handles should be an array")
        .first()
        .expect("message handle should be present");

    let request = json!({
        "command": "edit",
        "file": file_path.to_string_lossy(),
        "operations": [{
            "target": {
                "type": "node",
                "identity": target["identity"],
                "kind": target["kind"],
                "expected_old_hash": target["expected_old_hash"],
                "span_hint": {"start": 1, "end": 2}
            },
            "op": {
                "type": "replace",
                "new_text": "message Repeat {\n  string value = 1;\n  string updated = 2;\n}"
            }
        }]
    });
    let request_body = serde_json::to_string(&request).expect("request should serialize");

    let output = run_identedit_with_stdin(&["edit", "--json"], &request_body);
    assert!(
        !output.status.success(),
        "transform --json should fail when span_hint misses duplicate Protobuf targets"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "ambiguous_target");
}

#[test]
fn apply_reports_precondition_failed_after_protobuf_source_mutation() {
    let file_path = copy_fixture_to_temp("example.proto", ".proto");
    let select_output = run_identedit(&[
        "read",
        "--json",
        "--verbose",
        "--kind",
        "service",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        select_output.status.success(),
        "select failed: {}",
        String::from_utf8_lossy(&select_output.stderr)
    );

    let select_response: Value =
        serde_json::from_slice(&select_output.stdout).expect("stdout should be valid JSON");
    let identity = select_response["handles"]
        .as_array()
        .expect("handles should be an array")
        .iter()
        .find(|handle| {
            handle["text"]
                .as_str()
                .is_some_and(|text| text.contains("service KannaService"))
        })
        .and_then(|handle| handle["identity"].as_str())
        .expect("service identity should be present");

    let transform_output = run_identedit(&[
        "edit",
        "--identity",
        identity,
        "--replace",
        "service KannaService {\n  rpc GetStatus(Request) returns (Response);\n  rpc Health(Request) returns (Response);\n}",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        transform_output.status.success(),
        "transform failed: {}",
        String::from_utf8_lossy(&transform_output.stderr)
    );

    let original = fs::read_to_string(&file_path).expect("file should be readable");
    let mutated = original.replace("service KannaService", "service KannaServiceChanged");
    fs::write(&file_path, mutated).expect("mutated source write should succeed");

    let transform_json =
        std::str::from_utf8(&transform_output.stdout).expect("transform output should be utf-8");
    let apply_output = run_identedit_with_stdin(&["apply"], transform_json);
    assert!(
        !apply_output.status.success(),
        "apply should fail when Protobuf source changes after transform"
    );

    let response: Value =
        serde_json::from_slice(&apply_output.stdout).expect("stdout should be valid JSON");
    let error_type = response["error"]["type"]
        .as_str()
        .expect("error.type should be a string");
    assert!(
        error_type == "precondition_failed" || error_type == "target_missing",
        "expected stale Protobuf apply to fail with precondition_failed or target_missing, got {error_type}"
    );
}

#[test]
fn select_ignores_message_like_tokens_inside_comments() {
    let source = "syntax = \"proto3\";\n\n// message Fake { string x = 1; }\n/*\nmessage AlsoFake {\n  string y = 1;\n}\n*/\nmessage Real {\n  string id = 1;\n}\n";
    let file_path = write_temp_source(".proto", source);
    let output = run_identedit(&[
        "read",
        "--json",
        "--verbose",
        "--kind",
        "message",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "select failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    let handles = response["handles"]
        .as_array()
        .expect("handles should be an array");

    assert_eq!(
        handles.len(),
        1,
        "only real message declaration should match"
    );
    assert!(
        handles[0]["text"]
            .as_str()
            .is_some_and(|text| text.contains("message Real")),
        "selected message should be the real declaration"
    );
}

#[test]
fn transform_replace_and_apply_preserve_crlf_protobuf_source_segments() {
    let source = "syntax = \"proto3\";\r\n\r\nmessage Request {\r\n  string id = 1;\r\n}\r\n";
    let file_path = write_temp_source(".proto", source);
    let select_output = run_identedit(&[
        "read",
        "--json",
        "--verbose",
        "--kind",
        "message",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        select_output.status.success(),
        "select failed: {}",
        String::from_utf8_lossy(&select_output.stderr)
    );

    let select_response: Value =
        serde_json::from_slice(&select_output.stdout).expect("stdout should be valid JSON");
    let identity = select_response["handles"][0]["identity"]
        .as_str()
        .expect("message identity should be present");

    let replacement = "message Request {\r\n  string id = 1;\r\n  string trace_id = 2;\r\n}";
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

    let transform_json =
        std::str::from_utf8(&transform_output.stdout).expect("transform output should be utf-8");
    let apply_output = run_identedit_with_stdin(&["apply"], transform_json);
    assert!(
        apply_output.status.success(),
        "apply failed: {}",
        String::from_utf8_lossy(&apply_output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("file should be readable");
    assert!(modified.contains("\r\n"));
    assert!(modified.contains("string trace_id = 2;"));
}
