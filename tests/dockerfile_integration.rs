use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

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

fn write_temp_named_source(file_name: &str, source: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be after epoch")
        .as_nanos();
    let dir_path = std::env::temp_dir().join(format!("identedit-dockerfile-{nanos}"));
    fs::create_dir_all(&dir_path).expect("temporary directory should be created");
    let file_path = dir_path.join(file_name);
    fs::write(&file_path, source).expect("temporary named source should be written");
    file_path
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

fn find_handle_with_text<'a>(handles: &'a [Value], kind: &str, snippet: &str) -> &'a Value {
    handles
        .iter()
        .find(|handle| {
            handle["kind"] == kind
                && handle["text"]
                    .as_str()
                    .is_some_and(|text| text.contains(snippet))
        })
        .expect("handle with expected kind and text should be present")
}

#[test]
fn select_covers_dockerfile_kinds_and_provider() {
    let dockerfile = fixture_path("example.dockerfile");

    assert_select_kind(&dockerfile, "source_file");
    assert_select_kind(&dockerfile, "from_instruction");
    assert_select_kind(&dockerfile, "run_instruction");
}

#[test]
fn select_supports_case_insensitive_dockerfile_extension() {
    let file_path = copy_fixture_to_temp("example.dockerfile", ".DOCKERFILE");
    assert_select_kind(&file_path, "run_instruction");
}

#[test]
fn select_supports_dockerfile_basename_alias() {
    let source =
        fs::read_to_string(fixture_path("example.dockerfile")).expect("fixture should be readable");
    let dockerfile_path = write_temp_named_source("Dockerfile", &source);
    assert_select_kind(&dockerfile_path, "run_instruction");

    let containerfile_path = write_temp_named_source("Containerfile", &source);
    assert_select_kind(&containerfile_path, "run_instruction");
}

#[test]
fn select_supports_utf8_bom_prefixed_dockerfile_files() {
    let fixture = fs::read(fixture_path("example.dockerfile")).expect("fixture should be readable");
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(&fixture);
    let file_path = write_temp_bytes(".dockerfile", &bytes);

    assert_select_kind(&file_path, "run_instruction");
}

#[test]
fn select_reports_parse_failure_for_syntax_invalid_dockerfile() {
    let file_path = write_temp_source(
        ".dockerfile",
        "FROM alpine:3.20\nRUN [\"echo\", \"hello\"\n",
    );
    let output = run_identedit(&[
        "read",
        "--json",
        "--verbose",
        "--kind",
        "run_instruction",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "syntax-invalid dockerfile should fail under dockerfile provider"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "parse_failure");
    let message = response["error"]["message"]
        .as_str()
        .expect("error.message should be a string");
    assert!(message.contains("tree-sitter-dockerfile"));
    assert!(message.contains("Syntax errors detected in Dockerfile source"));
}

#[test]
fn transform_replace_and_apply_support_dockerfile_env_instruction() {
    let file_path = copy_fixture_to_temp("example.dockerfile", ".dockerfile");
    let select_output = run_identedit(&[
        "read",
        "--json",
        "--verbose",
        "--kind",
        "env_instruction",
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
    let target = find_handle_with_text(handles, "env_instruction", "APP_HOME=/app");
    let identity = target["identity"]
        .as_str()
        .expect("identity should be present");

    let transform_output = run_identedit(&[
        "edit",
        "--identity",
        identity,
        "--replace",
        "ENV APP_HOME=/tmp",
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
    assert!(modified.contains("ENV APP_HOME=/tmp"));
}

#[test]
fn transform_reports_ambiguous_target_for_duplicate_dockerfile_run_identity() {
    let source = "FROM alpine:3.20\nRUN echo hello\nRUN echo hello\n";
    let file_path = write_temp_source(".dockerfile", source);
    let select_output = run_identedit(&[
        "read",
        "--json",
        "--verbose",
        "--kind",
        "run_instruction",
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
        .expect("fixture should include duplicate run instruction identity");

    let output = run_identedit(&[
        "edit",
        "--identity",
        duplicate_identity,
        "--replace",
        "RUN echo patched",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "transform should fail for ambiguous duplicate Dockerfile identity"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "ambiguous_target");
}

#[test]
fn transform_json_span_hint_disambiguates_duplicate_dockerfile_run_identity() {
    let source = "FROM alpine:3.20\nRUN echo hello\nRUN echo hello\n";
    let file_path = write_temp_source(".dockerfile", source);
    let select_output = run_identedit(&[
        "read",
        "--json",
        "--verbose",
        "--kind",
        "run_instruction",
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
    let target = handles
        .first()
        .expect("fixture should contain at least one run instruction handle");
    let identity = target["identity"]
        .as_str()
        .expect("identity should be present");
    let kind = target["kind"].as_str().expect("kind should be present");
    let expected_old_hash = target["expected_old_hash"]
        .as_str()
        .expect("expected_old_hash should be present");
    let start = target["span"]["start"]
        .as_u64()
        .expect("span.start should be u64");
    let end = target["span"]["end"]
        .as_u64()
        .expect("span.end should be u64");

    let request = json!({
        "command": "edit",
        "file": file_path,
        "operations": [
            {
                "target": {
                    "type": "node",
                    "identity": identity,
                    "kind": kind,
                    "expected_old_hash": expected_old_hash,
                    "span_hint": {
                        "start": start,
                        "end": end
                    }
                },
                "op": {
                    "type": "replace",
                    "new_text": "RUN echo patched"
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["edit", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "span_hint should disambiguate duplicate identity: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    let operations = response["files"][0]["operations"]
        .as_array()
        .expect("changeset should contain operations");
    assert_eq!(operations.len(), 1);

    let preview = &operations[0]["preview"];
    assert_eq!(preview["new_text"], "RUN echo patched");
}

#[test]
fn transform_json_duplicate_dockerfile_identity_with_missed_span_hint_returns_ambiguous_target() {
    let source = "FROM alpine:3.20\nRUN echo hello\nRUN echo hello\n";
    let file_path = write_temp_source(".dockerfile", source);
    let select_output = run_identedit(&[
        "read",
        "--json",
        "--verbose",
        "--kind",
        "run_instruction",
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
    let target = handles
        .first()
        .expect("fixture should contain at least one run instruction handle");
    let identity = target["identity"]
        .as_str()
        .expect("identity should be present");
    let kind = target["kind"].as_str().expect("kind should be present");
    let expected_old_hash = target["expected_old_hash"]
        .as_str()
        .expect("expected_old_hash should be present");

    let request = json!({
        "command": "edit",
        "file": file_path,
        "operations": [
            {
                "target": {
                    "type": "node",
                    "identity": identity,
                    "kind": kind,
                    "expected_old_hash": expected_old_hash,
                    "span_hint": {
                        "start": 4096,
                        "end": 4100
                    }
                },
                "op": {
                    "type": "replace",
                    "new_text": "RUN echo patched"
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["edit", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "missed span_hint should fall back to ambiguous target"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "ambiguous_target");
}

#[test]
fn apply_reports_precondition_failed_after_dockerfile_source_mutation() {
    let file_path = copy_fixture_to_temp("example.dockerfile", ".dockerfile");

    let select_output = run_identedit(&[
        "read",
        "--json",
        "--verbose",
        "--kind",
        "env_instruction",
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
    let target = find_handle_with_text(handles, "env_instruction", "APP_HOME=/app");
    let identity = target["identity"]
        .as_str()
        .expect("identity should be present");

    let transform_output = run_identedit(&[
        "edit",
        "--identity",
        identity,
        "--replace",
        "ENV APP_HOME=/opt",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        transform_output.status.success(),
        "transform failed: {}",
        String::from_utf8_lossy(&transform_output.stderr)
    );

    let original = fs::read_to_string(&file_path).expect("file should be readable");
    let mutated = original.replacen("APP_HOME=/app", "APP_HOME=/tmp", 1);
    fs::write(&file_path, mutated).expect("mutation should succeed");

    let transform_json =
        std::str::from_utf8(&transform_output.stdout).expect("transform output should be utf-8");
    let apply_output = run_identedit_with_stdin(&["apply"], transform_json);
    assert!(
        !apply_output.status.success(),
        "apply should fail when source changed after transform"
    );

    let response: Value =
        serde_json::from_slice(&apply_output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "precondition_failed");
}
