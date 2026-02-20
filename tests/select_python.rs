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
    command.arg("read").arg("--mode").arg("ast").arg("--json");

    for argument in arguments {
        command.arg(argument);
    }

    command.arg(file);
    command.output().expect("failed to run identedit binary")
}

#[test]
fn selects_python_function_definitions() {
    let fixture = fixture_path("example.py");
    let output = run_select(&["--kind", "function_definition"], &fixture);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    let handles = response["handles"]
        .as_array()
        .expect("handles should be an array");

    assert!(
        !handles.is_empty(),
        "expected at least one selection handle"
    );
    assert!(
        handles
            .iter()
            .any(|handle| handle["name"] == "process_data")
    );
}

#[test]
fn applies_name_pattern_filter_for_python_functions() {
    let fixture = fixture_path("example.py");
    let output = run_select(
        &["--kind", "function_definition", "--name", "process_*"],
        &fixture,
    );

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    let handles = response["handles"]
        .as_array()
        .expect("handles should be an array");

    assert_eq!(handles.len(), 1, "expected one function matching process_*");
    assert_eq!(handles[0]["name"], "process_data");
}
