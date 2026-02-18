use std::io::Write;

use serde_json::Value;
use tempfile::Builder;

mod common;

#[test]
fn select_mode_line_returns_line_handles_with_anchors() {
    let source = "alpha\nbeta\n";
    let mut temp_file = Builder::new()
        .suffix(".txt")
        .tempfile()
        .expect("temp text file should be created");
    temp_file
        .write_all(source.as_bytes())
        .expect("fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let output = common::run_identedit(&[
        "select",
        "--mode",
        "line",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "select --mode line should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    let handles = response["handles"]
        .as_array()
        .expect("handles should be array");
    assert_eq!(handles.len(), 2);
    assert_eq!(handles[0]["target_type"], "line");
    assert_eq!(handles[0]["line"], 1);
    assert_eq!(handles[0]["text"], "alpha");
    assert_eq!(
        handles[0]["anchor"],
        format!("1:{}", identedit::hashline::compute_line_hash("alpha"))
    );
}

#[test]
fn select_mode_line_rejects_selector_flags() {
    let fixture = common::fixture_path("example.py");
    let output = common::run_identedit(&[
        "select",
        "--mode",
        "line",
        "--kind",
        "function_definition",
        fixture.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "select --mode line should reject selector flags"
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}

#[test]
fn select_mode_ast_sets_node_target_type() {
    let fixture = common::fixture_path("example.py");
    let output = common::run_identedit(&[
        "select",
        "--kind",
        "function_definition",
        fixture.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "select --mode ast should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    let handles = response["handles"]
        .as_array()
        .expect("handles should be array");
    assert!(!handles.is_empty());
    assert_eq!(handles[0]["target_type"], "node");
}

#[test]
fn select_json_mode_rejects_mode_line() {
    let fixture = common::fixture_path("example.py");
    let request = serde_json::json!({
      "command": "select",
      "file": fixture.to_string_lossy().to_string(),
      "selector": { "kind": "function_definition" }
    });
    let output = common::run_identedit_with_stdin(
        &["select", "--json", "--mode", "line"],
        &request.to_string(),
    );
    assert!(
        !output.status.success(),
        "select --json should reject --mode line"
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}
