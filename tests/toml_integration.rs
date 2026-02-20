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

fn assert_select_kind(file: &Path, kind: &str) {
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
}

#[test]
fn select_covers_toml_kinds_and_provider_routing() {
    let toml_file = fixture_path("example.toml");

    assert_select_kind(&toml_file, "document");
    assert_select_kind(&toml_file, "pair");
    assert_select_kind(&toml_file, "table");
    assert_select_kind(&toml_file, "table_array_element");

    let malformed = write_temp_source(".toml", "title = \"broken\"\n[server\nport = 8080\n");
    let output = run_identedit(&[
        "read",
        "--json",
        "--verbose",
        "--kind",
        "document",
        malformed.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        !output.status.success(),
        "syntax-invalid toml should fail under the toml provider"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "parse_failure");
    let message = response["error"]["message"]
        .as_str()
        .expect("error.message should be a string");
    assert!(message.contains("tree-sitter-toml"));
    assert!(message.contains("Syntax errors detected"));
}

#[test]
fn transform_replace_and_apply_support_toml_pair_rewrite() {
    let file_path = copy_fixture_to_temp("example.toml", ".toml");
    let select_output = run_identedit(&[
        "read",
        "--json",
        "--verbose",
        "--kind",
        "pair",
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
        .find(|handle| handle["text"] == "title = \"identedit\"")
        .and_then(|handle| handle["identity"].as_str())
        .expect("title pair identity should be present");

    let replacement = "title = \"identedit-updated\"";
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
    assert!(modified.contains("title = \"identedit-updated\""));
    assert!(modified.contains("[server]"));
}

#[test]
fn stress_select_and_transform_cover_inline_tables_dotted_keys_and_array_tables() {
    let file_path = copy_fixture_to_temp("stress_inline_table.toml", ".toml");

    let inline_table_output = run_identedit(&[
        "read",
        "--json",
        "--verbose",
        "--kind",
        "inline_table",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        inline_table_output.status.success(),
        "inline_table select failed: {}",
        String::from_utf8_lossy(&inline_table_output.stderr)
    );

    let inline_table_response: Value =
        serde_json::from_slice(&inline_table_output.stdout).expect("stdout should be valid JSON");
    let inline_table_handles = inline_table_response["handles"]
        .as_array()
        .expect("handles should be an array");
    assert!(
        inline_table_handles.len() >= 2,
        "expected multiple inline_table handles in stress fixture"
    );

    let table_array_output = run_identedit(&[
        "read",
        "--json",
        "--verbose",
        "--kind",
        "table_array_element",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        table_array_output.status.success(),
        "table_array_element select failed: {}",
        String::from_utf8_lossy(&table_array_output.stderr)
    );

    let table_array_response: Value =
        serde_json::from_slice(&table_array_output.stdout).expect("stdout should be valid JSON");
    let table_array_handles = table_array_response["handles"]
        .as_array()
        .expect("handles should be an array");
    assert!(
        table_array_handles.len() >= 2,
        "expected table array entries in stress fixture"
    );

    let dotted_key_output = run_identedit(&[
        "read",
        "--json",
        "--verbose",
        "--kind",
        "dotted_key",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        dotted_key_output.status.success(),
        "dotted_key select failed: {}",
        String::from_utf8_lossy(&dotted_key_output.stderr)
    );

    let dotted_key_response: Value =
        serde_json::from_slice(&dotted_key_output.stdout).expect("stdout should be valid JSON");
    let dotted_key_identity = dotted_key_response["handles"]
        .as_array()
        .expect("handles should be an array")
        .iter()
        .find(|handle| handle["text"] == "servers.alpha")
        .and_then(|handle| handle["identity"].as_str())
        .expect("servers.alpha dotted_key should be present");

    let transform_output = run_identedit(&[
        "edit",
        "--identity",
        dotted_key_identity,
        "--replace",
        "servers.primary",
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
    assert!(modified.contains("servers.primary = { ip = \"10.0.0.1\""));
    assert!(modified.contains("[[deploy.targets]]"));
    assert!(modified.contains("metadata = { zone = \"us-east-1\", active = true }"));
}

#[test]
fn transform_reports_ambiguous_target_for_duplicate_toml_pair_identity() {
    let file_path = copy_fixture_to_temp("duplicate_pairs.toml", ".toml");
    let select_output = run_identedit(&[
        "read",
        "--json",
        "--verbose",
        "--kind",
        "pair",
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
        .filter(|handle| handle["text"] == "enabled = true")
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
        .expect("fixture should include duplicate identity for enabled pairs");

    let output = run_identedit(&[
        "edit",
        "--identity",
        duplicate_identity,
        "--replace",
        "enabled = false",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "transform should fail for ambiguous duplicate TOML identity"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "ambiguous_target");
}

#[test]
fn transform_json_span_hint_disambiguates_duplicate_toml_pair_identity() {
    let file_path = copy_fixture_to_temp("duplicate_pairs.toml", ".toml");
    let select_output = run_identedit(&[
        "read",
        "--json",
        "--verbose",
        "--kind",
        "pair",
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
        .filter(|handle| handle["text"] == "enabled = true")
        .collect::<Vec<_>>();
    assert!(
        duplicate_handles.len() >= 2,
        "fixture should include at least two duplicate enabled pairs"
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
                "span_hint": {
                    "start": span["start"],
                    "end": span["end"]
                }
            },
            "op": {
                "type": "replace",
                "new_text": "enabled = false"
            }
        }]
    });
    let request_body = serde_json::to_string(&request).expect("request should serialize");

    let transform_output = run_identedit_with_stdin(&["edit", "--json"], &request_body);
    assert!(
        transform_output.status.success(),
        "transform --json should disambiguate duplicate TOML identity: {}",
        String::from_utf8_lossy(&transform_output.stderr)
    );

    let transform_json =
        std::str::from_utf8(&transform_output.stdout).expect("transform output should be utf-8");
    let apply_output = run_identedit_with_stdin(&["apply"], transform_json);
    assert!(
        apply_output.status.success(),
        "apply failed after span_hint disambiguation: {}",
        String::from_utf8_lossy(&apply_output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("file should be readable");
    assert_eq!(modified.matches("enabled = false").count(), 1);
    assert_eq!(modified.matches("enabled = true").count(), 1);
}

#[test]
fn transform_json_duplicate_toml_identity_with_missed_span_hint_returns_ambiguous_target() {
    let file_path = copy_fixture_to_temp("duplicate_pairs.toml", ".toml");
    let select_output = run_identedit(&[
        "read",
        "--json",
        "--verbose",
        "--kind",
        "pair",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        select_output.status.success(),
        "select failed: {}",
        String::from_utf8_lossy(&select_output.stderr)
    );

    let select_response: Value =
        serde_json::from_slice(&select_output.stdout).expect("stdout should be valid JSON");
    let duplicate_handle = select_response["handles"]
        .as_array()
        .expect("handles should be an array")
        .iter()
        .find(|handle| handle["text"] == "enabled = true")
        .expect("expected duplicate enabled pair handle");

    let request = json!({
        "command": "edit",
        "file": file_path.to_string_lossy(),
        "operations": [{
            "target": {
                "type": "node",
                "identity": duplicate_handle["identity"],
                "kind": duplicate_handle["kind"],
                "expected_old_hash": duplicate_handle["expected_old_hash"],
                "span_hint": {"start": 1, "end": 2}
            },
            "op": {
                "type": "replace",
                "new_text": "enabled = false"
            }
        }]
    });
    let request_body = serde_json::to_string(&request).expect("request should serialize");

    let output = run_identedit_with_stdin(&["edit", "--json"], &request_body);
    assert!(
        !output.status.success(),
        "transform --json should fail when span_hint misses duplicate TOML targets"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "ambiguous_target");
}

#[test]
fn select_supports_uppercase_toml_extension_alias() {
    let file_path = copy_fixture_to_temp("example.toml", ".TOML");
    assert_select_kind(&file_path, "pair");
}

#[test]
fn select_handles_quoted_dotted_key_pair() {
    let source = "title = \"quoted\"\n\"servers.alpha\" = 1\n";
    let file_path = write_temp_source(".toml", source);
    let output = run_identedit(&[
        "read",
        "--json",
        "--verbose",
        "--kind",
        "pair",
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
    assert!(
        handles
            .iter()
            .any(|handle| handle["text"] == "\"servers.alpha\" = 1"),
        "quoted dotted-key pair should be selectable as a TOML pair node"
    );
}

#[test]
fn transform_reports_ambiguous_target_for_duplicate_toml_table_array_pair_identity() {
    let file_path = copy_fixture_to_temp("duplicate_table_array_elements.toml", ".toml");
    let select_output = run_identedit(&[
        "read",
        "--json",
        "--verbose",
        "--kind",
        "pair",
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
        .filter(|handle| handle["text"] == "name = \"edge\"")
        .map(|handle| {
            handle["identity"]
                .as_str()
                .expect("identity should be string")
        })
        .find(|identity| {
            handles
                .iter()
                .filter(|h| h["identity"] == *identity && h["text"] == "name = \"edge\"")
                .count()
                >= 2
        })
        .expect("fixture should include duplicate name-pair identity");
    assert!(
        handles
            .iter()
            .filter(|h| h["identity"] == duplicate_identity)
            .count()
            >= 2,
        "duplicate identity should appear at least twice"
    );

    let output = run_identedit(&[
        "edit",
        "--identity",
        duplicate_identity,
        "--replace",
        "name = \"deploy\"\nlabels = { team = \"platform\" }",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "transform should fail for duplicate TOML pair identity in table-array entries"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "ambiguous_target");
}

#[test]
fn transform_json_span_hint_disambiguates_duplicate_toml_table_array_pair_identity() {
    let file_path = copy_fixture_to_temp("duplicate_table_array_elements.toml", ".toml");
    let select_output = run_identedit(&[
        "read",
        "--json",
        "--verbose",
        "--kind",
        "pair",
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
    let duplicate_handles = handles
        .iter()
        .filter(|handle| handle["text"] == "name = \"edge\"")
        .collect::<Vec<_>>();
    assert!(
        duplicate_handles.len() >= 2,
        "fixture should include two name-pair handles inside table-array elements"
    );
    let duplicate_identity = duplicate_handles[0]["identity"]
        .as_str()
        .expect("identity should be string");
    assert!(
        duplicate_handles
            .iter()
            .all(|handle| handle["identity"] == duplicate_identity),
        "duplicate handles should share identity"
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
                "new_text": "name = \"deploy\"\nlabels = { team = \"platform\" }"
            }
        }]
    });
    let request_body = serde_json::to_string(&request).expect("request should serialize");

    let transform_output = run_identedit_with_stdin(&["edit", "--json"], &request_body);
    assert!(
        transform_output.status.success(),
        "transform --json should disambiguate duplicate TOML pair identity: {}",
        String::from_utf8_lossy(&transform_output.stderr)
    );

    let transform_json =
        std::str::from_utf8(&transform_output.stdout).expect("transform output should be utf-8");
    let apply_output = run_identedit_with_stdin(&["apply"], transform_json);
    assert!(
        apply_output.status.success(),
        "apply failed after TOML pair disambiguation: {}",
        String::from_utf8_lossy(&apply_output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("file should be readable");
    assert_eq!(modified.matches("name = \"deploy\"").count(), 1);
    assert_eq!(modified.matches("name = \"edge\"").count(), 1);
}

#[test]
fn select_duplicate_toml_table_array_elements_is_deterministic() {
    let file_path = fixture_path("duplicate_table_array_elements.toml");
    let output_a = run_identedit(&[
        "read",
        "--json",
        "--verbose",
        "--kind",
        "table_array_element",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    let output_b = run_identedit(&[
        "read",
        "--json",
        "--verbose",
        "--kind",
        "table_array_element",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output_a.status.success() && output_b.status.success(),
        "repeated selects should succeed"
    );

    let response_a: Value =
        serde_json::from_slice(&output_a.stdout).expect("stdout should be valid JSON");
    let response_b: Value =
        serde_json::from_slice(&output_b.stdout).expect("stdout should be valid JSON");
    assert_eq!(response_a["handles"], response_b["handles"]);
}
