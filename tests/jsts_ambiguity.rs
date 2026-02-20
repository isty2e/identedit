use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

use serde_json::{Value, json};
use tempfile::Builder;

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

fn hash_text(value: &str) -> String {
    identedit::changeset::hash_text(value)
}

#[test]
fn transform_reports_ambiguous_target_for_duplicate_js_identity_without_span_hint() {
    let file_path = write_temp_source(
        ".js",
        "function duplicate() {\n  return 1;\n}\n\nfunction duplicate() {\n  return 1;\n}\n",
    );
    let path = file_path.to_str().expect("path should be utf-8");

    let select_output = run_identedit(&[
        "read",
        "--json",
        "--verbose",
        "--kind",
        "function_declaration",
        path,
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
        handles.len() >= 2,
        "expected at least two duplicate handles"
    );
    let identity = handles[0]["identity"]
        .as_str()
        .expect("identity should be present");

    let transform_output = run_identedit(&[
        "edit",
        "--identity",
        identity,
        "--replace",
        "function duplicate() {\n  return 2;\n}",
        path,
    ]);
    assert!(
        !transform_output.status.success(),
        "transform should fail without span_hint for duplicate identity"
    );
    let response: Value =
        serde_json::from_slice(&transform_output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "ambiguous_target");
}

#[test]
fn transform_with_span_hint_disambiguates_duplicate_ts_identity_and_applies_single_edit() {
    let file_path = write_temp_source(
        ".ts",
        "function duplicate(value: number): number {\n  return value + 1;\n}\n\nfunction duplicate(value: number): number {\n  return value + 1;\n}\n",
    );
    let path = file_path.to_str().expect("path should be utf-8");

    let select_output = run_identedit(&[
        "read",
        "--json",
        "--verbose",
        "--kind",
        "function_declaration",
        path,
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
        handles.len() >= 2,
        "expected at least two duplicate handles"
    );

    let first = handles
        .iter()
        .find(|handle| handle["kind"] == "function_declaration")
        .expect("first function handle should exist");
    let target_identity = first["identity"]
        .as_str()
        .expect("identity should be present");
    let target_kind = first["kind"].as_str().expect("kind should be present");
    let target_old_text = first["text"].as_str().expect("text should be present");
    let target_span = first["span"].clone();

    let replacement = "function duplicate(value: number): number {\n  return value - 1;\n}";
    let request = json!({
        "command": "edit",
        "file": path,
        "operations": [{
            "identity": target_identity,
            "kind": target_kind,
            "span_hint": target_span,
            "expected_old_hash": hash_text(target_old_text),
            "op": {
                "type": "replace",
                "new_text": replacement
            }
        }]
    });

    let transform_output = run_identedit_with_stdin(&["edit", "--json"], &request.to_string());
    assert!(
        transform_output.status.success(),
        "transform should succeed with span_hint: {}",
        String::from_utf8_lossy(&transform_output.stderr)
    );
    let transform_response: Value =
        serde_json::from_slice(&transform_output.stdout).expect("stdout should be valid JSON");
    assert_eq!(
        transform_response["files"][0]["operations"][0]["preview"]["new_text"],
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

    let modified = fs::read_to_string(&file_path).expect("file should be readable");
    assert_eq!(
        modified.matches("return value - 1;").count(),
        1,
        "exactly one duplicate function should be rewritten"
    );
    assert_eq!(
        modified.matches("return value + 1;").count(),
        1,
        "the other duplicate function should remain unchanged"
    );
}
