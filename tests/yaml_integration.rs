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
fn select_covers_yaml_kinds_and_extension_alias() {
    let yaml_file = fixture_path("example.yaml");
    let yml_file = fixture_path("example_alias.yml");

    assert_select_kind(&yaml_file, "document");
    assert_select_kind(&yaml_file, "block_mapping_pair");
    assert_select_kind(&yml_file, "block_mapping_pair");
}

#[test]
fn transform_replace_and_apply_support_yaml_mapping_pair() {
    let file_path = copy_fixture_to_temp("example.yaml", ".yaml");
    let select_output = run_identedit(&[
        "read",
        "--json",
        "--verbose",
        "--kind",
        "block_mapping_pair",
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
                .is_some_and(|text| text.contains("environment: dev"))
        })
        .and_then(|handle| handle["identity"].as_str())
        .expect("environment mapping identity should be present");

    let replacement = "environment: prod";
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
    assert!(modified.contains("environment: prod"));
    assert!(!modified.contains("environment: dev"));
}

#[test]
fn select_handles_multi_document_anchors_and_aliases_stably() {
    let yaml_file = fixture_path("multi_document_anchors.yaml");

    let document_output = run_identedit(&[
        "read",
        "--json",
        "--verbose",
        "--kind",
        "document",
        yaml_file.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        document_output.status.success(),
        "select failed: {}",
        String::from_utf8_lossy(&document_output.stderr)
    );

    let document_response: Value =
        serde_json::from_slice(&document_output.stdout).expect("stdout should be valid JSON");
    let document_handles = document_response["handles"]
        .as_array()
        .expect("handles should be an array");
    assert_eq!(document_handles.len(), 2);

    let anchor_output = run_identedit(&[
        "read",
        "--json",
        "--verbose",
        "--kind",
        "anchor",
        yaml_file.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        anchor_output.status.success(),
        "select failed: {}",
        String::from_utf8_lossy(&anchor_output.stderr)
    );

    let anchor_response: Value =
        serde_json::from_slice(&anchor_output.stdout).expect("stdout should be valid JSON");
    let anchor_handles = anchor_response["handles"]
        .as_array()
        .expect("handles should be an array");
    assert!(anchor_handles.len() >= 2);

    let alias_output = run_identedit(&[
        "read",
        "--json",
        "--verbose",
        "--kind",
        "alias",
        yaml_file.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        alias_output.status.success(),
        "select failed: {}",
        String::from_utf8_lossy(&alias_output.stderr)
    );

    let alias_response: Value =
        serde_json::from_slice(&alias_output.stdout).expect("stdout should be valid JSON");
    let alias_handles = alias_response["handles"]
        .as_array()
        .expect("handles should be an array");
    assert!(alias_handles.len() >= 2);
    assert!(alias_handles.iter().any(|handle| {
        handle["text"]
            .as_str()
            .is_some_and(|text| text.contains("*shared"))
    }));

    let second_alias_output = run_identedit(&[
        "read",
        "--json",
        "--verbose",
        "--kind",
        "alias",
        yaml_file.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        second_alias_output.status.success(),
        "second select failed: {}",
        String::from_utf8_lossy(&second_alias_output.stderr)
    );

    let second_alias_response: Value =
        serde_json::from_slice(&second_alias_output.stdout).expect("stdout should be valid JSON");
    let second_alias_handles = second_alias_response["handles"]
        .as_array()
        .expect("handles should be an array");

    let alias_texts = alias_handles
        .iter()
        .map(|handle| {
            handle["text"]
                .as_str()
                .expect("alias text should be present")
                .to_string()
        })
        .collect::<Vec<_>>();
    let second_alias_texts = second_alias_handles
        .iter()
        .map(|handle| {
            handle["text"]
                .as_str()
                .expect("alias text should be present")
                .to_string()
        })
        .collect::<Vec<_>>();

    assert_eq!(alias_texts, second_alias_texts);
}

#[test]
fn transform_reports_ambiguous_target_for_duplicate_yaml_mapping_pair_identity() {
    let file_path = copy_fixture_to_temp("duplicate_pairs.yaml", ".yaml");
    let select_output = run_identedit(&[
        "read",
        "--json",
        "--verbose",
        "--kind",
        "block_mapping_pair",
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
        .filter(|handle| {
            handle["text"]
                .as_str()
                .is_some_and(|text| text.trim() == "enabled: true")
        })
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
        .expect("fixture should include duplicate identity for enabled mapping pairs");

    let output = run_identedit(&[
        "edit",
        "--identity",
        duplicate_identity,
        "--replace",
        "enabled: false",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "transform should fail for ambiguous duplicate YAML identity"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "ambiguous_target");
}

#[test]
fn transform_json_span_hint_disambiguates_duplicate_yaml_mapping_pair_identity() {
    let file_path = copy_fixture_to_temp("duplicate_pairs.yaml", ".yaml");
    let select_output = run_identedit(&[
        "read",
        "--json",
        "--verbose",
        "--kind",
        "block_mapping_pair",
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
                .is_some_and(|text| text.trim() == "enabled: true")
        })
        .collect::<Vec<_>>();
    assert!(
        duplicate_handles.len() >= 2,
        "fixture should include at least two duplicate enabled mapping pairs"
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
                "new_text": "enabled: false"
            }
        }]
    });
    let request_body = serde_json::to_string(&request).expect("request should serialize");

    let transform_output = run_identedit_with_stdin(&["edit", "--json"], &request_body);
    assert!(
        transform_output.status.success(),
        "transform --json should disambiguate duplicate YAML identity: {}",
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
    assert_eq!(modified.matches("enabled: false").count(), 1);
    assert_eq!(modified.matches("enabled: true").count(), 1);
}

#[test]
fn transform_json_duplicate_yaml_identity_with_missed_span_hint_returns_ambiguous_target() {
    let file_path = copy_fixture_to_temp("duplicate_pairs.yaml", ".yaml");
    let select_output = run_identedit(&[
        "read",
        "--json",
        "--verbose",
        "--kind",
        "block_mapping_pair",
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
        .find(|handle| {
            handle["text"]
                .as_str()
                .is_some_and(|text| text.trim() == "enabled: true")
        })
        .expect("expected duplicate enabled mapping handle");

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
                "new_text": "enabled: false"
            }
        }]
    });
    let request_body = serde_json::to_string(&request).expect("request should serialize");

    let output = run_identedit_with_stdin(&["edit", "--json"], &request_body);
    assert!(
        !output.status.success(),
        "transform --json should fail when span_hint misses duplicate YAML targets"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "ambiguous_target");
}

#[test]
fn select_reports_parse_failure_for_syntax_invalid_yaml() {
    let file_path = write_temp_source(".yaml", "service:\n  name: identedit\n  retries: [1,2\n");
    let output = run_identedit(&[
        "read",
        "--json",
        "--verbose",
        "--kind",
        "document",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "syntax-invalid yaml should fail under the yaml provider"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "parse_failure");
    let message = response["error"]["message"]
        .as_str()
        .expect("error.message should be a string");
    assert!(message.contains("tree-sitter-yaml"));
    assert!(message.contains("Syntax errors detected"));
}

#[test]
fn transform_replace_and_apply_support_yaml_second_document_pair_rewrite() {
    let file_path = copy_fixture_to_temp("multi_document_anchors.yaml", ".yaml");
    let select_output = run_identedit(&[
        "read",
        "--json",
        "--verbose",
        "--kind",
        "block_mapping_pair",
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
                .is_some_and(|text| text.contains("timeout: 30"))
        })
        .and_then(|handle| handle["identity"].as_str())
        .expect("second-document timeout mapping identity should be present");

    let transform_output = run_identedit(&[
        "edit",
        "--identity",
        identity,
        "--replace",
        "timeout: 45",
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
    assert!(modified.contains("timeout: 45"));
    assert!(modified.contains("port: 8080"));
}

#[test]
fn transform_reports_ambiguous_target_for_duplicate_yaml_sequence_pair_identity() {
    let file_path = copy_fixture_to_temp("duplicate_sequence_items.yaml", ".yaml");
    let select_output = run_identedit(&[
        "read",
        "--json",
        "--verbose",
        "--kind",
        "block_mapping_pair",
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
        .filter(|handle| handle["text"] == "name: build")
        .map(|handle| {
            handle["identity"]
                .as_str()
                .expect("identity should be string")
        })
        .find(|identity| {
            handles
                .iter()
                .filter(|h| h["identity"] == *identity && h["text"] == "name: build")
                .count()
                >= 2
        })
        .expect("fixture should include duplicate sequence-pair identity");

    let output = run_identedit(&[
        "edit",
        "--identity",
        duplicate_identity,
        "--replace",
        "name: deploy",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "transform should fail for duplicate block_mapping_pair identity in sequence entries"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "ambiguous_target");
}

#[test]
fn transform_json_span_hint_disambiguates_duplicate_yaml_sequence_pair_identity() {
    let file_path = copy_fixture_to_temp("duplicate_sequence_items.yaml", ".yaml");
    let select_output = run_identedit(&[
        "read",
        "--json",
        "--verbose",
        "--kind",
        "block_mapping_pair",
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
        .filter(|handle| handle["text"] == "name: build")
        .collect::<Vec<_>>();
    assert!(
        duplicate_handles.len() >= 2,
        "fixture should include at least two duplicate sequence-pair handles"
    );
    let duplicate_identity = duplicate_handles[0]["identity"]
        .as_str()
        .expect("identity should be string");
    assert!(
        duplicate_handles
            .iter()
            .all(|handle| handle["identity"] == duplicate_identity),
        "duplicate sequence-pair handles should share identity"
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
                "new_text": "name: deploy"
            }
        }]
    });
    let request_body = serde_json::to_string(&request).expect("request should serialize");

    let transform_output = run_identedit_with_stdin(&["edit", "--json"], &request_body);
    assert!(
        transform_output.status.success(),
        "transform --json should disambiguate duplicate YAML sequence pair: {}",
        String::from_utf8_lossy(&transform_output.stderr)
    );

    let transform_json =
        std::str::from_utf8(&transform_output.stdout).expect("transform output should be utf-8");
    let apply_output = run_identedit_with_stdin(&["apply"], transform_json);
    assert!(
        apply_output.status.success(),
        "apply failed after sequence-pair disambiguation: {}",
        String::from_utf8_lossy(&apply_output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("file should be readable");
    assert_eq!(modified.matches("name: deploy").count(), 1);
    assert_eq!(modified.matches("name: build").count(), 1);
}

#[test]
fn select_supports_uppercase_yml_extension_alias() {
    let file_path = copy_fixture_to_temp("duplicate_sequence_items.yaml", ".YML");
    assert_select_kind(&file_path, "block_mapping_pair");
}
