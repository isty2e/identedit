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

fn assert_select_kind_and_optional_name(file: &Path, kind: &str, expected_name: Option<&str>) {
    let output = run_identedit(&[
        "select",
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

    if let Some(name) = expected_name {
        assert!(
            handles.iter().any(|handle| handle["name"] == name),
            "expected handle with name '{name}' for {kind}"
        );
    }
}

#[test]
fn select_covers_c_kinds_and_provider() {
    let c_file = fixture_path("example.c");

    assert_select_kind_and_optional_name(&c_file, "translation_unit", None);
    assert_select_kind_and_optional_name(&c_file, "function_definition", None);
    assert_select_kind_and_optional_name(&c_file, "struct_specifier", None);
}

#[test]
fn select_supports_case_insensitive_c_extension() {
    let file_path = copy_fixture_to_temp("example.c", ".C");
    assert_select_kind_and_optional_name(&file_path, "function_definition", None);
}

#[test]
fn select_reports_parse_failure_for_syntax_invalid_c() {
    let file_path = write_temp_source(".c", "int broken( {\n    return 1;\n}\n");
    let output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "function_definition",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "syntax-invalid c should fail under the c provider"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "parse_failure");
    let message = response["error"]["message"]
        .as_str()
        .expect("error.message should be a string");
    assert!(message.contains("tree-sitter-c"));
    assert!(message.contains("Syntax errors detected in C source"));
}

#[test]
fn transform_replace_and_apply_support_c_function_definition() {
    let file_path = copy_fixture_to_temp("example.c", ".c");
    let select_output = run_identedit(&[
        "select",
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
    let identity = select_response["handles"]
        .as_array()
        .expect("handles should be an array")
        .iter()
        .find(|handle| {
            handle["text"]
                .as_str()
                .is_some_and(|text| text.contains("process_data"))
        })
        .and_then(|handle| handle["identity"].as_str())
        .expect("process_data identity should be present");

    let replacement = "int process_data(int value) {\n    return value - 1;\n}";
    let transform_output = run_identedit(&[
        "transform",
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

    let transform_response: Value =
        serde_json::from_slice(&transform_output.stdout).expect("stdout should be valid JSON");
    assert_eq!(
        transform_response["files"][0]["operations"][0]["op"]["type"],
        "replace"
    );
    assert_eq!(
        transform_response["files"][0]["operations"][0]["op"]["new_text"],
        replacement
    );

    let transform_json =
        std::str::from_utf8(&transform_output.stdout).expect("transform output should be utf-8");
    let apply_output = run_identedit_with_stdin(&["apply"], transform_json);
    assert!(
        apply_output.status.success(),
        "apply failed: {}",
        String::from_utf8_lossy(&apply_output.stderr)
    );

    let apply_response: Value =
        serde_json::from_slice(&apply_output.stdout).expect("stdout should be valid JSON");
    assert_eq!(apply_response["summary"]["files_modified"], 1);
    assert_eq!(apply_response["summary"]["operations_applied"], 1);

    let modified = fs::read_to_string(&file_path).expect("file should be readable");
    assert!(modified.contains("return value - 1;"));
}

#[test]
fn transform_reports_ambiguous_target_for_duplicate_c_function_identity() {
    let file_path = copy_fixture_to_temp("duplicate_functions.c", ".c");
    let select_output = run_identedit(&[
        "select",
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
                .filter(|h| h["identity"] == *identity)
                .count()
                >= 2
        })
        .expect("fixture should include duplicate function identity");

    let output = run_identedit(&[
        "transform",
        "--identity",
        duplicate_identity,
        "--replace",
        "int configure(int value) {\n    return value + 2;\n}",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "transform should fail for ambiguous duplicate C identity"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "ambiguous_target");
}

#[test]
fn transform_json_span_hint_disambiguates_duplicate_c_function_identity() {
    let file_path = copy_fixture_to_temp("duplicate_functions.c", ".c");
    let select_output = run_identedit(&[
        "select",
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
    let duplicate_handles = select_response["handles"]
        .as_array()
        .expect("handles should be an array")
        .iter()
        .filter(|handle| {
            handle["text"]
                .as_str()
                .is_some_and(|text| text.contains("configure"))
        })
        .collect::<Vec<_>>();
    assert!(
        duplicate_handles.len() >= 2,
        "fixture should include at least two configure functions"
    );

    let target = duplicate_handles[1];
    let span = &target["span"];
    let request = json!({
        "command": "transform",
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
                "new_text": "int configure(int value) {\n    return value + 2;\n}"
            }
        }]
    });
    let request_body = serde_json::to_string(&request).expect("request should serialize");

    let transform_output = run_identedit_with_stdin(&["transform", "--json"], &request_body);
    assert!(
        transform_output.status.success(),
        "transform --json should disambiguate duplicate C identity: {}",
        String::from_utf8_lossy(&transform_output.stderr)
    );

    let transform_json =
        std::str::from_utf8(&transform_output.stdout).expect("transform output should be utf-8");
    let apply_output = run_identedit_with_stdin(&["apply"], transform_json);
    assert!(
        apply_output.status.success(),
        "apply failed after C span_hint disambiguation: {}",
        String::from_utf8_lossy(&apply_output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("file should be readable");
    assert_eq!(modified.matches("return value + 2;").count(), 1);
    assert_eq!(modified.matches("return value + 1;").count(), 1);
}

#[test]
fn transform_json_duplicate_c_identity_with_missed_span_hint_returns_ambiguous_target() {
    let file_path = copy_fixture_to_temp("duplicate_functions.c", ".c");
    let select_output = run_identedit(&[
        "select",
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
    let target = select_response["handles"]
        .as_array()
        .expect("handles should be an array")
        .iter()
        .find(|handle| {
            handle["text"]
                .as_str()
                .is_some_and(|text| text.contains("configure"))
        })
        .expect("configure handle should be present");

    let request = json!({
        "command": "transform",
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
                "new_text": "int configure(int value) {\n    return value + 2;\n}"
            }
        }]
    });
    let request_body = serde_json::to_string(&request).expect("request should serialize");

    let output = run_identedit_with_stdin(&["transform", "--json"], &request_body);
    assert!(
        !output.status.success(),
        "transform --json should fail when span_hint misses duplicate C functions"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "ambiguous_target");
}

#[test]
fn apply_reports_precondition_failed_after_c_source_mutation() {
    let file_path = copy_fixture_to_temp("example.c", ".c");
    let select_output = run_identedit(&[
        "select",
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
    let identity = select_response["handles"]
        .as_array()
        .expect("handles should be an array")
        .iter()
        .find(|handle| {
            handle["text"]
                .as_str()
                .is_some_and(|text| text.contains("process_data"))
        })
        .and_then(|handle| handle["identity"].as_str())
        .expect("process_data identity should be present");

    let transform_output = run_identedit(&[
        "transform",
        "--identity",
        identity,
        "--replace",
        "int process_data(int value) {\n    return value + 2;\n}",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        transform_output.status.success(),
        "transform failed: {}",
        String::from_utf8_lossy(&transform_output.stderr)
    );

    let before_apply = fs::read_to_string(&file_path).expect("file should be readable");
    let mutated = before_apply.replacen("return value + 1;", "return value + 9;", 1);
    assert_ne!(
        before_apply, mutated,
        "fixture should contain return value + 1; for stale apply test"
    );
    fs::write(&file_path, mutated).expect("mutated source write should succeed");

    let transform_json =
        std::str::from_utf8(&transform_output.stdout).expect("transform output should be utf-8");
    let apply_output = run_identedit_with_stdin(&["apply"], transform_json);
    assert!(
        !apply_output.status.success(),
        "apply should fail when C source mutates after transform"
    );

    let response: Value =
        serde_json::from_slice(&apply_output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "precondition_failed");
}

#[test]
fn select_supports_utf8_bom_prefixed_c_files() {
    let source = b"\xEF\xBB\xBFint bom_run(void) {\n    return 1;\n}\n";
    let file_path = write_temp_bytes(".c", source);
    let output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "function_definition",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "select should support BOM-prefixed C source: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    let handle = response["handles"]
        .as_array()
        .expect("handles should be an array")
        .first()
        .expect("function handle should exist");
    assert_eq!(handle["span"]["start"], 3);
}

#[test]
fn transform_replace_and_apply_support_crlf_c_source() {
    let source = "int run(int value) {\r\n    return value + 1;\r\n}\r\n";
    let file_path = write_temp_source(".c", source);
    let select_output = run_identedit(&[
        "select",
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
    let identity = select_response["handles"][0]["identity"]
        .as_str()
        .expect("identity should be present");

    let replacement = "int run(int value) {\r\n    return value + 2;\r\n}";
    let transform_output = run_identedit(&[
        "transform",
        "--identity",
        identity,
        "--replace",
        replacement,
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        transform_output.status.success(),
        "transform failed on CRLF C source: {}",
        String::from_utf8_lossy(&transform_output.stderr)
    );

    let transform_json =
        std::str::from_utf8(&transform_output.stdout).expect("transform output should be utf-8");
    let apply_output = run_identedit_with_stdin(&["apply"], transform_json);
    assert!(
        apply_output.status.success(),
        "apply failed on CRLF C source: {}",
        String::from_utf8_lossy(&apply_output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("file should be readable");
    assert!(modified.contains("return value + 2;\r\n"));
    assert!(modified.contains("\r\n"));
}
