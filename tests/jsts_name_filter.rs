use std::path::PathBuf;
use std::process::{Command, Output};

use serde_json::Value;

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn run_select(arguments: &[&str], file: &PathBuf) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_identedit"));
    command.env("IDENTEDIT_ALLOW_LEGACY", "1");
    command.arg("select");
    command.args(arguments);
    command.arg(file);
    command.output().expect("failed to run identedit binary")
}

#[test]
fn js_arrow_function_handles_are_nameless_and_excluded_by_name_filter() {
    let fixture = fixture_path("example.js");

    let output = run_select(&["--kind", "arrow_function"], &fixture);
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
        "expected arrow_function handles in fixture"
    );
    assert!(
        handles
            .iter()
            .all(|handle| handle["name"].is_null() || handle["name"] == Value::Null),
        "arrow_function handles should be nameless"
    );

    let filtered_output = run_select(&["--kind", "arrow_function", "--name", "*"], &fixture);
    assert!(
        filtered_output.status.success(),
        "filtered select failed: {}",
        String::from_utf8_lossy(&filtered_output.stderr)
    );
    let filtered_response: Value =
        serde_json::from_slice(&filtered_output.stdout).expect("stdout should be valid JSON");
    let filtered_handles = filtered_response["handles"]
        .as_array()
        .expect("handles should be an array");
    assert!(
        filtered_handles.is_empty(),
        "name filter should exclude nameless arrow_function handles"
    );
}

#[test]
fn tsx_arrow_function_name_filter_never_false_positives_from_binding_identifier() {
    let fixture = fixture_path("example.tsx");

    let output = run_select(&["--kind", "arrow_function"], &fixture);
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
        "expected arrow_function handles in fixture"
    );

    let filtered_output = run_select(&["--kind", "arrow_function", "--name", "Row"], &fixture);
    assert!(
        filtered_output.status.success(),
        "filtered select failed: {}",
        String::from_utf8_lossy(&filtered_output.stderr)
    );
    let filtered_response: Value =
        serde_json::from_slice(&filtered_output.stdout).expect("stdout should be valid JSON");
    let filtered_handles = filtered_response["handles"]
        .as_array()
        .expect("handles should be an array");
    assert!(
        filtered_handles.is_empty(),
        "name filter should not match binding identifier for nameless arrow_function node"
    );
}
