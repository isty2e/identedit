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

fn assert_select_kind(file: &Path, kind: &str) -> Value {
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

    response
}

#[test]
fn select_covers_markdown_kinds_and_provider() {
    let markdown_file = fixture_path("example.md");

    assert_select_kind(&markdown_file, "document");
    assert_select_kind(&markdown_file, "atx_heading");
    assert_select_kind(&markdown_file, "list_item");
    assert_select_kind(&markdown_file, "fenced_code_block");
}

#[test]
fn select_supports_case_insensitive_markdown_extensions() {
    let md_file = copy_fixture_to_temp("example.md", ".MD");
    let markdown_file = copy_fixture_to_temp("example.md", ".MARKDOWN");

    assert_select_kind(&md_file, "atx_heading");
    assert_select_kind(&markdown_file, "atx_heading");
}

#[test]
fn select_supports_utf8_bom_prefixed_markdown_files() {
    let fixture = fs::read(fixture_path("example.md")).expect("fixture should be readable");
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(&fixture);
    let file_path = write_temp_bytes(".md", &bytes);

    assert_select_kind(&file_path, "atx_heading");
}

#[test]
fn select_response_includes_hash_preconditions_for_markdown_targets() {
    let file_path = copy_fixture_to_temp("example.md", ".md");
    let response = assert_select_kind(&file_path, "atx_heading");

    let handles = response["handles"]
        .as_array()
        .expect("handles should be an array");
    let first = handles
        .first()
        .expect("fixture should contain at least one atx_heading");
    let expected_old_hash = first["expected_old_hash"]
        .as_str()
        .expect("expected_old_hash should be string");
    assert_eq!(expected_old_hash.len(), 16);

    let file_preconditions = response["file_preconditions"]
        .as_array()
        .expect("file_preconditions should be an array");
    assert_eq!(file_preconditions.len(), 1);
    let expected_file_hash = file_preconditions[0]["expected_file_hash"]
        .as_str()
        .expect("expected_file_hash should be string");
    assert_eq!(expected_file_hash.len(), 16);
}

#[test]
fn transform_replace_and_apply_support_markdown_heading() {
    let file_path = copy_fixture_to_temp("example.md", ".md");
    let select_output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "atx_heading",
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
                .is_some_and(|text| text.starts_with("# Identedit"))
        })
        .and_then(|handle| handle["identity"].as_str())
        .expect("top-level heading identity should be present");

    let replacement = "# Identedit Engine";
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
    assert!(modified.contains("# Identedit Engine"));
}

#[test]
fn transform_reports_ambiguous_target_for_duplicate_markdown_heading_identity() {
    let source = "## Repeat\n\nalpha\n\n## Repeat\n\nalpha\n";
    let file_path = write_temp_source(".md", source);
    let select_output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "atx_heading",
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
        .expect("fixture should include duplicate heading identity");

    let output = run_identedit(&[
        "transform",
        "--identity",
        duplicate_identity,
        "--replace",
        "## Updated",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "transform should fail for ambiguous duplicate Markdown identity"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "ambiguous_target");
}

#[test]
fn transform_json_span_hint_disambiguates_duplicate_markdown_heading_identity() {
    let source = "## Repeat\n\nalpha\n\n## Repeat\n\nalpha\n";
    let file_path = write_temp_source(".md", source);
    let select_output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "atx_heading",
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
                .filter(|handle| handle["identity"].as_str() == Some(*identity))
                .count()
                >= 2
        })
        .expect("fixture should include duplicate heading identity");
    let duplicate_handles = handles
        .iter()
        .filter(|handle| handle["identity"].as_str() == Some(duplicate_identity))
        .collect::<Vec<_>>();
    assert!(
        duplicate_handles.len() >= 2,
        "fixture should include duplicate repeat headings"
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
                "new_text": "## Updated"
            }
        }]
    });
    let request_body = serde_json::to_string(&request).expect("request should serialize");

    let transform_output = run_identedit_with_stdin(&["transform", "--json"], &request_body);
    assert!(
        transform_output.status.success(),
        "transform --json should disambiguate duplicate Markdown heading identity: {}",
        String::from_utf8_lossy(&transform_output.stderr)
    );

    let transform_json =
        std::str::from_utf8(&transform_output.stdout).expect("transform output should be utf-8");
    let apply_output = run_identedit_with_stdin(&["apply"], transform_json);
    assert!(
        apply_output.status.success(),
        "apply failed after Markdown span_hint disambiguation: {}",
        String::from_utf8_lossy(&apply_output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("file should be readable");
    assert_eq!(modified.matches("## Updated").count(), 1);
    assert_eq!(modified.matches("## Repeat").count(), 1);
}

#[test]
fn transform_json_duplicate_markdown_identity_with_missed_span_hint_returns_ambiguous_target() {
    let source = "## Repeat\n\nalpha\n\n## Repeat\n\nalpha\n";
    let file_path = write_temp_source(".md", source);
    let select_output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "atx_heading",
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
                .filter(|handle| handle["identity"].as_str() == Some(*identity))
                .count()
                >= 2
        })
        .expect("fixture should include duplicate heading identity");
    let target = handles
        .iter()
        .find(|handle| handle["identity"].as_str() == Some(duplicate_identity))
        .expect("duplicate heading handle should be present");

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
                "new_text": "## Updated"
            }
        }]
    });
    let request_body = serde_json::to_string(&request).expect("request should serialize");

    let output = run_identedit_with_stdin(&["transform", "--json"], &request_body);
    assert!(
        !output.status.success(),
        "transform --json should fail when span_hint misses duplicate Markdown headings"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "ambiguous_target");
}

#[test]
fn apply_reports_precondition_failed_after_markdown_source_mutation() {
    let file_path = copy_fixture_to_temp("example.md", ".md");
    let select_output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "atx_heading",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        select_output.status.success(),
        "select failed: {}",
        String::from_utf8_lossy(&select_output.stderr)
    );

    let select_response: Value =
        serde_json::from_slice(&select_output.stdout).expect("stdout should be valid JSON");
    let target_handle = select_response["handles"]
        .as_array()
        .expect("handles should be an array")
        .iter()
        .find(|handle| {
            handle["text"]
                .as_str()
                .is_some_and(|text| text.starts_with("## Pipeline"))
        })
        .expect("pipeline heading handle should be present");
    let identity = target_handle["identity"]
        .as_str()
        .expect("pipeline heading identity should be present");

    let transform_output = run_identedit(&[
        "transform",
        "--identity",
        identity,
        "--replace",
        "## Build Pipeline",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        transform_output.status.success(),
        "transform failed: {}",
        String::from_utf8_lossy(&transform_output.stderr)
    );

    let original = fs::read_to_string(&file_path).expect("file should be readable");
    let mutated = original.replace("## Pipeline", "## Pipeline Changed");
    fs::write(&file_path, mutated).expect("mutated source write should succeed");

    let transform_json =
        std::str::from_utf8(&transform_output.stdout).expect("transform output should be utf-8");
    let apply_output = run_identedit_with_stdin(&["apply"], transform_json);
    assert!(
        !apply_output.status.success(),
        "apply should fail when Markdown source changes after transform"
    );

    let response: Value =
        serde_json::from_slice(&apply_output.stdout).expect("stdout should be valid JSON");
    let error_type = response["error"]["type"]
        .as_str()
        .expect("error.type should be a string");
    assert!(
        error_type == "precondition_failed" || error_type == "target_missing",
        "expected stale markdown apply to fail with precondition_failed or target_missing, got {error_type}"
    );
}

#[test]
fn select_ignores_heading_like_tokens_inside_fenced_code_blocks() {
    let source = "# Real Heading\n\n```python\n# fake heading\ndef helper():\n    return 1\n```\n";
    let file_path = write_temp_source(".md", source);
    let output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "atx_heading",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "select failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    let headings = response["handles"]
        .as_array()
        .expect("handles should be an array")
        .iter()
        .filter_map(|handle| handle["text"].as_str())
        .collect::<Vec<_>>();

    assert!(
        headings
            .iter()
            .any(|text| text.starts_with("# Real Heading")),
        "expected to find real heading"
    );
    assert!(
        !headings.iter().any(|text| text.contains("fake heading")),
        "fenced code content must not be parsed as Markdown heading"
    );
}

#[test]
fn transform_replace_and_apply_preserve_crlf_markdown_source_segments() {
    let source = "# Identedit\r\n\r\n## Pipeline\r\n\r\n- Select targets\r\n- Transform safely\r\n";
    let file_path = write_temp_source(".md", source);
    let select_output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "atx_heading",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        select_output.status.success(),
        "select failed: {}",
        String::from_utf8_lossy(&select_output.stderr)
    );

    let select_response: Value =
        serde_json::from_slice(&select_output.stdout).expect("stdout should be valid JSON");
    let target_handle = select_response["handles"]
        .as_array()
        .expect("handles should be an array")
        .iter()
        .find(|handle| {
            handle["text"]
                .as_str()
                .is_some_and(|text| text.starts_with("## Pipeline"))
        })
        .expect("pipeline heading handle should be present");
    let identity = target_handle["identity"]
        .as_str()
        .expect("pipeline heading identity should be present");
    let expected_old_text = target_handle["text"]
        .as_str()
        .expect("pipeline heading text should be present");

    let replacement = "## Build Pipeline";
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
    let preview = &transform_response["files"][0]["operations"][0]["preview"];
    assert!(
        preview.get("old_text").is_none(),
        "compact preview should omit old_text by default"
    );
    let preview_old_hash = preview["old_hash"]
        .as_str()
        .expect("preview old_hash should be string");
    let preview_old_len = preview["old_len"]
        .as_u64()
        .expect("preview old_len should be number");
    assert!(
        preview_old_hash == identedit::changeset::hash_text(expected_old_text),
        "preview old_hash should match original heading text"
    );
    assert_eq!(
        preview_old_len as usize,
        expected_old_text.len(),
        "preview old_len should match original heading text length"
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
    assert!(
        modified.contains("\r\n"),
        "CRLF line endings should be preserved in markdown file"
    );
    assert!(modified.contains("## Build Pipeline"));
}
