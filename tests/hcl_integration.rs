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
            "expected handle with name '{name}' for kind {kind}"
        );
    }
}

fn find_attribute_handle_with_text<'a>(handles: &'a [Value], snippet: &str) -> &'a Value {
    handles
        .iter()
        .find(|handle| {
            handle["kind"] == "attribute"
                && handle["text"]
                    .as_str()
                    .is_some_and(|text| text.contains(snippet))
        })
        .expect("attribute handle with expected text should be present")
}

#[test]
fn select_covers_hcl_kinds_and_provider() {
    let hcl_file = fixture_path("example.hcl");

    assert_select_kind_and_optional_name(&hcl_file, "config_file", None);
    assert_select_kind_and_optional_name(&hcl_file, "block", None);
    assert_select_kind_and_optional_name(&hcl_file, "attribute", None);
}

#[test]
fn select_supports_case_insensitive_hcl_and_tf_extensions() {
    let hcl_path = copy_fixture_to_temp("example.hcl", ".HCL");
    assert_select_kind_and_optional_name(&hcl_path, "attribute", None);

    let tf_path = copy_fixture_to_temp("example.hcl", ".TF");
    assert_select_kind_and_optional_name(&tf_path, "attribute", None);
}

#[test]
fn select_supports_utf8_bom_prefixed_hcl_files() {
    let fixture = fs::read(fixture_path("example.hcl")).expect("fixture should be readable");
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(&fixture);
    let file_path = write_temp_bytes(".hcl", &bytes);

    assert_select_kind_and_optional_name(&file_path, "attribute", None);
}

#[test]
fn select_reports_parse_failure_for_syntax_invalid_hcl() {
    let file_path = write_temp_source(
        ".hcl",
        "resource \"aws_s3_bucket\" \"logs\" {\n  bucket = \"identedit-logs\"\n",
    );
    let output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "block",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "syntax-invalid hcl should fail under the hcl provider"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "parse_failure");
    let message = response["error"]["message"]
        .as_str()
        .expect("error.message should be a string");
    assert!(message.contains("tree-sitter-hcl"));
    assert!(message.contains("Syntax errors detected in HCL source"));
}

#[test]
fn transform_replace_and_apply_support_hcl_attribute() {
    let file_path = copy_fixture_to_temp("example.hcl", ".hcl");
    let select_output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "attribute",
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
    let target = find_attribute_handle_with_text(handles, "project = \"identedit\"");
    let identity = target["identity"]
        .as_str()
        .expect("identity should be present");

    let replacement = "project = \"identedit-next\"";
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

    let transform_json =
        std::str::from_utf8(&transform_output.stdout).expect("transform output should be utf-8");
    let apply_output = run_identedit_with_stdin(&["apply"], transform_json);
    assert!(
        apply_output.status.success(),
        "apply failed: {}",
        String::from_utf8_lossy(&apply_output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("file should be readable");
    assert!(modified.contains("project = \"identedit-next\""));
}

#[test]
fn transform_reports_ambiguous_target_for_duplicate_hcl_attribute_identity() {
    let source =
        "locals {\n  project = \"identedit\"\n}\n\nlocals {\n  project = \"identedit\"\n}\n";
    let file_path = write_temp_source(".hcl", source);
    let select_output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "attribute",
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
        .expect("fixture should include duplicate attribute identity");

    let output = run_identedit(&[
        "transform",
        "--identity",
        duplicate_identity,
        "--replace",
        "project = \"identedit-next\"",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "transform should fail for ambiguous duplicate HCL attribute identity"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "ambiguous_target");
}

#[test]
fn transform_json_span_hint_disambiguates_duplicate_hcl_attribute_identity() {
    let source =
        "locals {\n  project = \"identedit\"\n}\n\nlocals {\n  project = \"identedit\"\n}\n";
    let file_path = write_temp_source(".hcl", source);
    let select_output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "attribute",
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
            handle["kind"] == "attribute"
                && handle["text"]
                    .as_str()
                    .is_some_and(|text| text.contains("project = \"identedit\""))
        })
        .collect::<Vec<_>>();
    assert!(
        duplicate_handles.len() >= 2,
        "fixture should include at least two project attributes"
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
                "new_text": "project = \"identedit-next\""
            }
        }]
    });
    let request_body = serde_json::to_string(&request).expect("request should serialize");

    let transform_output = run_identedit_with_stdin(&["transform", "--json"], &request_body);
    assert!(
        transform_output.status.success(),
        "transform --json should disambiguate duplicate HCL attribute identity: {}",
        String::from_utf8_lossy(&transform_output.stderr)
    );

    let transform_json =
        std::str::from_utf8(&transform_output.stdout).expect("transform output should be utf-8");
    let apply_output = run_identedit_with_stdin(&["apply"], transform_json);
    assert!(
        apply_output.status.success(),
        "apply failed after HCL span_hint disambiguation: {}",
        String::from_utf8_lossy(&apply_output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("file should be readable");
    assert_eq!(modified.matches("project = \"identedit-next\"").count(), 1);
    assert_eq!(modified.matches("project = \"identedit\"").count(), 1);
}

#[test]
fn transform_json_duplicate_hcl_identity_with_missed_span_hint_returns_ambiguous_target() {
    let source =
        "locals {\n  project = \"identedit\"\n}\n\nlocals {\n  project = \"identedit\"\n}\n";
    let file_path = write_temp_source(".hcl", source);
    let select_output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "attribute",
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
    let target = find_attribute_handle_with_text(handles, "project = \"identedit\"");

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
                "new_text": "project = \"identedit-next\""
            }
        }]
    });
    let request_body = serde_json::to_string(&request).expect("request should serialize");

    let output = run_identedit_with_stdin(&["transform", "--json"], &request_body);
    assert!(
        !output.status.success(),
        "transform --json should fail when span_hint misses duplicate HCL attributes"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "ambiguous_target");
}

#[test]
fn apply_reports_precondition_failed_after_hcl_source_mutation() {
    let file_path = copy_fixture_to_temp("example.hcl", ".hcl");
    let select_output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "attribute",
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
    let target = find_attribute_handle_with_text(handles, "project = \"identedit\"");
    let identity = target["identity"]
        .as_str()
        .expect("identity should be present");

    let transform_output = run_identedit(&[
        "transform",
        "--identity",
        identity,
        "--replace",
        "project = \"identedit-next\"",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        transform_output.status.success(),
        "transform failed: {}",
        String::from_utf8_lossy(&transform_output.stderr)
    );

    let original = fs::read_to_string(&file_path).expect("file should be readable");
    // Keep replacement width stable so span_hint still points to the same range and
    // stale detection reports a precondition failure instead of target_missing.
    let mutated = original.replace("project = \"identedit\"", "project = \"othername\"");
    fs::write(&file_path, mutated).expect("mutated source write should succeed");

    let transform_json =
        std::str::from_utf8(&transform_output.stdout).expect("transform output should be utf-8");
    let apply_output = run_identedit_with_stdin(&["apply"], transform_json);
    assert!(
        !apply_output.status.success(),
        "apply should fail when HCL source changes after transform"
    );

    let response: Value =
        serde_json::from_slice(&apply_output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "precondition_failed");
}
