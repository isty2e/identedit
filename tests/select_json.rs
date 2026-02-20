use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

use serde_json::Value;
use tempfile::tempdir;

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

fn run_select_json(arguments: &[&str], payload: &str) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_identedit"));
    command.arg("read").arg("--mode").arg("ast").arg("--json");
    for argument in arguments {
        command.arg(argument);
    }
    command.stdin(Stdio::piped());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());

    let mut child = command.spawn().expect("failed to spawn identedit binary");
    child
        .stdin
        .as_mut()
        .expect("stdin should be available")
        .write_all(payload.as_bytes())
        .expect("stdin write should succeed");
    child
        .wait_with_output()
        .expect("failed to read process output")
}

#[test]
fn selects_json_objects() {
    let fixture = fixture_path("example.json");
    let output = run_select(&["--kind", "object"], &fixture);

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
        "expected object matches in JSON fixture"
    );
}

#[test]
fn select_default_omits_handle_text_field() {
    let fixture = fixture_path("example.json");
    let output = run_select(&["--kind", "object"], &fixture);

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

    assert!(!handles.is_empty(), "expected at least one selected handle");
    assert!(
        handles.iter().all(|handle| handle.get("text").is_none()),
        "default select output should omit handle text for compact payloads"
    );
}

#[test]
fn select_verbose_includes_handle_text_field() {
    let fixture = fixture_path("example.json");
    let source_text = fs::read_to_string(&fixture).expect("fixture should be readable");
    let output = run_select(&["--verbose", "--kind", "object"], &fixture);

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

    let nested_object = handles
        .iter()
        .find(|handle| handle["name"] == "config")
        .expect("expected nested object handle named config");
    let start = nested_object["span"]["start"]
        .as_u64()
        .expect("span.start should be number");
    let end = nested_object["span"]["end"]
        .as_u64()
        .expect("span.end should be number");
    let sliced = &source_text.as_bytes()[start as usize..end as usize];
    let text_from_span = String::from_utf8_lossy(sliced);

    assert_eq!(
        nested_object["text"],
        text_from_span.as_ref(),
        "verbose select output should include full matched text"
    );
}

#[test]
fn select_json_mode_verbose_includes_handle_text_field() {
    let fixture = fixture_path("example.json");
    let payload = serde_json::json!({
        "command": "read",
        "file": fixture.to_string_lossy().to_string(),
        "selector": {
            "kind": "object",
            "name_pattern": serde_json::Value::Null,
            "exclude_kinds": []
        }
    });
    let output = run_select_json(&["--json", "--verbose"], &payload.to_string());

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
        handles
            .iter()
            .all(|handle| handle.get("text").and_then(Value::as_str).is_some()),
        "json mode with --verbose should include handle text"
    );
}

#[test]
fn returns_precise_json_spans_for_nested_nodes() {
    let fixture = fixture_path("example.json");
    let source_text = fs::read_to_string(&fixture).expect("fixture should be readable");
    let output = run_select(&["--verbose", "--kind", "object"], &fixture);

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

    let file_len = source_text.len() as u64;
    let nested_object = handles
        .iter()
        .find(|handle| handle["name"] == "config")
        .expect("expected nested object handle named config");

    let start = nested_object["span"]["start"]
        .as_u64()
        .expect("span.start should be number");
    let end = nested_object["span"]["end"]
        .as_u64()
        .expect("span.end should be number");

    assert!(start < end, "span should be non-empty");
    assert!(end <= file_len, "span should be inside file boundaries");
    assert!(
        end - start < file_len,
        "nested object span should not be full-file"
    );

    let sliced = &source_text.as_bytes()[start as usize..end as usize];
    let text_from_span = String::from_utf8_lossy(sliced);
    assert_eq!(nested_object["text"], text_from_span.as_ref());
}

#[test]
fn returns_precise_json_spans_for_keys() {
    let fixture = fixture_path("example.json");
    let source_text = fs::read_to_string(&fixture).expect("fixture should be readable");
    let output = run_select(&["--verbose", "--kind", "key"], &fixture);

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

    let config_key = handles
        .iter()
        .find(|handle| handle["name"] == "config")
        .expect("expected key handle named config");

    let expected_start = source_text
        .find("\"config\"")
        .expect("config key should exist") as u64;
    let expected_end = expected_start + "\"config\"".len() as u64;

    assert_eq!(config_key["span"]["start"], expected_start);
    assert_eq!(config_key["span"]["end"], expected_end);
    assert_eq!(config_key["text"], "config");
}

#[test]
fn unsupported_extension_routes_to_fallback_provider() {
    let temporary_directory = tempdir().expect("tempdir should be created");
    let file_path = temporary_directory.path().join("unsupported.txt");
    fs::write(&file_path, "plain text").expect("fixture file should be written");

    let output = run_select(&["--kind", "object"], &file_path);

    assert!(
        output.status.success(),
        "fallback provider should handle unsupported extension: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["summary"]["matches"], 0);
}
