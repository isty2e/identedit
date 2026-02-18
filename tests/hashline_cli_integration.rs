use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

use serde_json::{Value, json};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use tempfile::Builder;

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
    child
        .stdin
        .as_mut()
        .expect("stdin should be available")
        .write_all(input.as_bytes())
        .expect("stdin write should succeed");
    child
        .wait_with_output()
        .expect("failed to collect process output")
}

fn write_temp_source(suffix: &str, content: &str) -> PathBuf {
    let mut file = Builder::new()
        .suffix(suffix)
        .tempfile()
        .expect("temp source should be created");
    file.write_all(content.as_bytes())
        .expect("temp source write should succeed");
    file.keep().expect("temp source should persist").1
}

fn write_temp_json(content: &str) -> PathBuf {
    write_temp_source(".json", content)
}

fn line_ref(source: &str, line: usize) -> String {
    let line_text = source
        .lines()
        .nth(line - 1)
        .expect("line should exist for anchor");
    let hash = identedit::hashline::compute_line_hash(line_text);
    format!("{line}:{hash}")
}

fn parse_stdout_json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON")
}

fn error_body(output: &Output) -> Value {
    let response = parse_stdout_json(output);
    response
        .get("error")
        .cloned()
        .expect("error response should include 'error' object")
}

fn parse_embedded_check_from_error_message(message: &str) -> Value {
    let (_, json_payload) = message
        .split_once('\n')
        .expect("precondition error should include embedded JSON payload on the next line");
    serde_json::from_str(json_payload).expect("embedded check payload should be valid JSON")
}

#[test]
fn hashline_show_defaults_to_plain_text_output() {
    let file = write_temp_source(".txt", "alpha\nbeta");
    let output = run_identedit(&[
        "hashline",
        "show",
        file.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "show failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf-8");
    assert!(
        stdout
            .lines()
            .all(|line| line.contains('|') && line.contains(':')),
        "plain hashline output should contain LINE:HASH|CONTENT format; got:\n{stdout}"
    );
    assert!(stdout.contains("|alpha"));
    assert!(stdout.contains("|beta"));
}

#[test]
fn hashline_show_json_returns_structured_lines() {
    let file = write_temp_source(".txt", "alpha\nbeta");
    let output = run_identedit(&[
        "hashline",
        "show",
        "--json",
        file.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "show --json failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["command"], "show");
    assert_eq!(response["summary"]["total_lines"], 2);
    assert_eq!(response["lines"][0]["line"], 1);
    assert_eq!(response["lines"][0]["content"], "alpha");
    assert_eq!(response["lines"][1]["line"], 2);
    assert_eq!(response["lines"][1]["content"], "beta");
}

#[test]
fn hashline_check_accepts_edits_from_file() {
    let source = "a\nb\nc\n";
    let file = write_temp_source(".txt", source);
    let edits = json!([
      { "set_line": { "anchor": line_ref(source, 2), "new_text": "B" } }
    ]);
    let edits_path = write_temp_json(
        &serde_json::to_string_pretty(&edits).expect("edits JSON should serialize"),
    );

    let output = run_identedit(&[
        "hashline",
        "check",
        file.to_str().expect("path should be utf-8"),
        "--edits",
        edits_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "check failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["command"], "check");
    assert_eq!(response["check"]["ok"], true);
    assert_eq!(response["check"]["summary"]["matched"], 1);
    assert!(
        response["check"].get("mismatches").is_none(),
        "compact check output should omit mismatch details on success"
    );
}

#[test]
fn hashline_check_accepts_stdin_wrapper_payload() {
    let source = "a\nb\nc\n";
    let file = write_temp_source(".txt", source);
    let payload = json!({
      "command": "hashline",
      "edits": [
        { "set_line": { "anchor": line_ref(source, 2), "new_text": "B" } }
      ]
    });

    let output = run_identedit_with_stdin(
        &[
            "hashline",
            "check",
            file.to_str().expect("path should be utf-8"),
            "--edits",
            "-",
        ],
        &serde_json::to_string_pretty(&payload).expect("payload should serialize"),
    );
    assert!(
        output.status.success(),
        "check stdin failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["command"], "check");
    assert_eq!(response["check"]["ok"], true);
}

#[test]
fn hashline_check_verbose_includes_mismatches_field_on_success() {
    let source = "a\nb\n";
    let file = write_temp_source(".txt", source);
    let payload = json!({
      "command": "hashline",
      "edits": [
        { "set_line": { "anchor": line_ref(source, 2), "new_text": "B" } }
      ]
    });

    let output = run_identedit_with_stdin(
        &[
            "hashline",
            "check",
            file.to_str().expect("path should be utf-8"),
            "--edits",
            "-",
            "--verbose",
        ],
        &serde_json::to_string_pretty(&payload).expect("payload should serialize"),
    );
    assert!(
        output.status.success(),
        "check --verbose should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert!(
        response["check"]["mismatches"].is_array(),
        "verbose check output should include mismatch array"
    );
}

#[test]
fn hashline_apply_dry_run_default_omits_content_and_returns_output_fingerprint() {
    let source = "a\nb\n";
    let file = write_temp_source(".txt", source);
    let edits = json!([
      { "set_line": { "anchor": line_ref(source, 2), "new_text": "B" } }
    ]);
    let edits_path = write_temp_json(
        &serde_json::to_string_pretty(&edits).expect("edits JSON should serialize"),
    );

    let output = run_identedit(&[
        "hashline",
        "apply",
        file.to_str().expect("path should be utf-8"),
        "--edits",
        edits_path.to_str().expect("path should be utf-8"),
        "--dry-run",
    ]);
    assert!(
        output.status.success(),
        "apply --dry-run failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["command"], "apply");
    assert_eq!(response["dry_run"], true);
    assert_eq!(response["changed"], true);
    assert!(response.get("content").is_none());
    assert_eq!(
        response["output_hash"],
        identedit::hash::hash_text("a\nB\n")
    );
    assert_eq!(response["output_bytes"], "a\nB\n".len());

    let after = fs::read_to_string(&file).expect("file should still be readable");
    assert_eq!(after, source, "dry-run must not mutate source file");
}

#[test]
fn hashline_apply_dry_run_include_content_returns_content() {
    let source = "a\nb\n";
    let file = write_temp_source(".txt", source);
    let edits = json!([
      { "set_line": { "anchor": line_ref(source, 2), "new_text": "B" } }
    ]);
    let edits_path = write_temp_json(
        &serde_json::to_string_pretty(&edits).expect("edits JSON should serialize"),
    );

    let output = run_identedit(&[
        "hashline",
        "apply",
        file.to_str().expect("path should be utf-8"),
        "--edits",
        edits_path.to_str().expect("path should be utf-8"),
        "--dry-run",
        "--include-content",
    ]);
    assert!(
        output.status.success(),
        "apply --dry-run --include-content failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["content"], "a\nB\n");
    assert!(
        response.get("output_hash").is_none(),
        "include-content mode should omit compact output fingerprint"
    );
    assert!(
        response.get("output_bytes").is_none(),
        "include-content mode should omit compact output fingerprint"
    );
}

#[test]
fn hashline_apply_writes_file_when_not_dry_run() {
    let source = "a\nb\n";
    let file = write_temp_source(".txt", source);
    let edits = json!([
      { "set_line": { "anchor": line_ref(source, 2), "new_text": "B" } }
    ]);
    let edits_path = write_temp_json(
        &serde_json::to_string_pretty(&edits).expect("edits JSON should serialize"),
    );

    let output = run_identedit(&[
        "hashline",
        "apply",
        file.to_str().expect("path should be utf-8"),
        "--edits",
        edits_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "apply failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["command"], "apply");
    assert_eq!(response["dry_run"], false);
    assert_eq!(response["changed"], true);
    assert!(response["content"].is_null());

    let after = fs::read_to_string(&file).expect("file should still be readable");
    assert_eq!(after, "a\nB\n");
}

#[test]
fn hashline_apply_repair_remaps_unique_stale_anchor() {
    let source = "a\nb\na\n";
    let file = write_temp_source(".txt", source);
    let stale_anchor = format!("1:{}", identedit::hashline::compute_line_hash("b"));
    let edits = json!([
      { "set_line": { "anchor": stale_anchor, "new_text": "B" } }
    ]);
    let edits_text = serde_json::to_string_pretty(&edits).expect("edits JSON should serialize");

    let strict_output = run_identedit_with_stdin(
        &[
            "hashline",
            "apply",
            file.to_str().expect("path should be utf-8"),
            "--edits",
            "-",
        ],
        &edits_text,
    );
    assert!(
        !strict_output.status.success(),
        "strict mode should reject stale anchor"
    );

    let repair_output = run_identedit_with_stdin(
        &[
            "hashline",
            "apply",
            file.to_str().expect("path should be utf-8"),
            "--edits",
            "-",
            "--repair",
        ],
        &edits_text,
    );
    assert!(
        repair_output.status.success(),
        "repair mode should remap stale anchor: {}",
        String::from_utf8_lossy(&repair_output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&repair_output.stdout).expect("stdout should be JSON");
    assert_eq!(response["command"], "apply");
    assert_eq!(response["mode"], "repair");

    let after = fs::read_to_string(&file).expect("file should still be readable");
    assert_eq!(after, "a\nB\na\n");
}

#[test]
fn hashline_check_stdin_wrapper_with_unknown_field_reports_wrapper_shape_error() {
    let source = "a\nb\n";
    let file = write_temp_source(".txt", source);
    let payload = json!({
      "command": "hashline",
      "edits": [
        { "set_line": { "anchor": line_ref(source, 2), "new_text": "B" } }
      ],
      "metadata": { "ci": true }
    });

    let output = run_identedit_with_stdin(
        &[
            "hashline",
            "check",
            file.to_str().expect("path should be utf-8"),
            "--edits",
            "-",
        ],
        &serde_json::to_string_pretty(&payload).expect("payload should serialize"),
    );
    assert!(
        !output.status.success(),
        "unknown wrapper fields should fail"
    );

    let error = error_body(&output);
    assert_eq!(error["type"], "invalid_request");
    let message = error["message"]
        .as_str()
        .expect("error message should be a string");
    assert!(
        message.contains("Invalid hashline stdin request"),
        "expected wrapper-specific message, got: {message}"
    );
    assert!(
        message.contains("metadata"),
        "expected offending field name in message, got: {message}"
    );
}

#[test]
fn hashline_check_stdin_wrapper_missing_edits_reports_wrapper_shape_error() {
    let source = "a\nb\n";
    let file = write_temp_source(".txt", source);
    let payload = json!({
      "command": "hashline"
    });

    let output = run_identedit_with_stdin(
        &[
            "hashline",
            "check",
            file.to_str().expect("path should be utf-8"),
            "--edits",
            "-",
        ],
        &serde_json::to_string_pretty(&payload).expect("payload should serialize"),
    );
    assert!(
        !output.status.success(),
        "missing edits in wrapper should fail"
    );

    let error = error_body(&output);
    assert_eq!(error["type"], "invalid_request");
    let message = error["message"]
        .as_str()
        .expect("error message should be a string");
    assert!(
        message.contains("Invalid hashline stdin request"),
        "expected wrapper-specific message, got: {message}"
    );
    assert!(
        message.contains("missing field"),
        "expected missing field detail, got: {message}"
    );
}

#[test]
fn hashline_check_file_and_stdin_payload_produce_same_summary() {
    let source = "a\nb\nc\n";
    let file = write_temp_source(".txt", source);
    let edits = json!([
      { "set_line": { "anchor": line_ref(source, 2), "new_text": "B" } }
    ]);
    let edits_path = write_temp_json(
        &serde_json::to_string_pretty(&edits).expect("edits JSON should serialize"),
    );
    let wrapped = json!({
      "command": "hashline",
      "edits": edits
    });

    let file_output = run_identedit(&[
        "hashline",
        "check",
        file.to_str().expect("path should be utf-8"),
        "--edits",
        edits_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        file_output.status.success(),
        "file-based check should succeed: {}",
        String::from_utf8_lossy(&file_output.stderr)
    );
    let stdin_output = run_identedit_with_stdin(
        &[
            "hashline",
            "check",
            file.to_str().expect("path should be utf-8"),
            "--edits",
            "-",
        ],
        &serde_json::to_string_pretty(&wrapped).expect("payload should serialize"),
    );
    assert!(
        stdin_output.status.success(),
        "stdin-based check should succeed: {}",
        String::from_utf8_lossy(&stdin_output.stderr)
    );

    let file_json = parse_stdout_json(&file_output);
    let stdin_json = parse_stdout_json(&stdin_output);
    assert_eq!(file_json["check"], stdin_json["check"]);
}

#[test]
fn hashline_apply_stdin_wrapper_with_wrong_command_is_rejected() {
    let source = "a\nb\n";
    let file = write_temp_source(".txt", source);
    let payload = json!({
      "command": "hashline_apply",
      "edits": [
        { "set_line": { "anchor": line_ref(source, 2), "new_text": "B" } }
      ]
    });

    let output = run_identedit_with_stdin(
        &[
            "hashline",
            "apply",
            file.to_str().expect("path should be utf-8"),
            "--edits",
            "-",
        ],
        &serde_json::to_string_pretty(&payload).expect("payload should serialize"),
    );
    assert!(
        !output.status.success(),
        "unexpected command should be rejected"
    );

    let error = error_body(&output);
    assert_eq!(error["type"], "invalid_request");
    let message = error["message"]
        .as_str()
        .expect("error message should be a string");
    assert!(
        message.contains("Unsupported command"),
        "expected unsupported command message, got: {message}"
    );
}

#[test]
fn hashline_check_stdin_trailing_garbage_is_invalid_json() {
    let source = "a\nb\n";
    let file = write_temp_source(".txt", source);
    let payload = format!(
        "[{{\"set_line\":{{\"anchor\":\"{}\",\"new_text\":\"B\"}}}}]\nnot-json",
        line_ref(source, 2)
    );

    let output = run_identedit_with_stdin(
        &[
            "hashline",
            "check",
            file.to_str().expect("path should be utf-8"),
            "--edits",
            "-",
        ],
        &payload,
    );
    assert!(
        !output.status.success(),
        "stdin with trailing garbage should fail"
    );

    let error = error_body(&output);
    assert_eq!(error["type"], "invalid_request");
    let message = error["message"]
        .as_str()
        .expect("error message should be a string");
    assert!(
        message.contains("Failed to parse stdin JSON request"),
        "expected JSON parse failure message, got: {message}"
    );
}

#[cfg(unix)]
#[test]
fn hashline_apply_dry_run_succeeds_when_file_is_read_only() {
    let source = "a\nb\n";
    let file = write_temp_source(".txt", source);
    let edits = json!([
      { "set_line": { "anchor": line_ref(source, 2), "new_text": "B" } }
    ]);
    let edits_path = write_temp_json(
        &serde_json::to_string_pretty(&edits).expect("edits JSON should serialize"),
    );

    let mut permissions = fs::metadata(&file)
        .expect("file metadata should be readable")
        .permissions();
    permissions.set_mode(0o444);
    fs::set_permissions(&file, permissions).expect("file should become read-only");

    let dry_run_output = run_identedit(&[
        "hashline",
        "apply",
        file.to_str().expect("path should be utf-8"),
        "--edits",
        edits_path.to_str().expect("path should be utf-8"),
        "--dry-run",
    ]);
    assert!(
        dry_run_output.status.success(),
        "dry-run should not require write permission: {}",
        String::from_utf8_lossy(&dry_run_output.stderr)
    );

    let write_output = run_identedit(&[
        "hashline",
        "apply",
        file.to_str().expect("path should be utf-8"),
        "--edits",
        edits_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !write_output.status.success(),
        "non-dry-run should fail on read-only files"
    );

    let error = error_body(&write_output);
    assert_eq!(error["type"], "io_error");
}

#[test]
fn hashline_apply_stdin_wrapper_missing_command_reports_wrapper_shape_error() {
    let source = "a\nb\n";
    let file = write_temp_source(".txt", source);
    let payload = json!({
      "edits": [
        { "set_line": { "anchor": line_ref(source, 2), "new_text": "B" } }
      ]
    });

    let output = run_identedit_with_stdin(
        &[
            "hashline",
            "apply",
            file.to_str().expect("path should be utf-8"),
            "--edits",
            "-",
        ],
        &serde_json::to_string_pretty(&payload).expect("payload should serialize"),
    );
    assert!(
        !output.status.success(),
        "wrapper without command should fail"
    );

    let error = error_body(&output);
    assert_eq!(error["type"], "invalid_request");
    let message = error["message"]
        .as_str()
        .expect("error message should be a string");
    assert!(
        message.contains("Invalid hashline stdin request"),
        "expected wrapper-specific message, got: {message}"
    );
    assert!(
        message.contains("missing field"),
        "expected missing field detail, got: {message}"
    );
}

#[test]
fn hashline_check_stdin_object_without_wrapper_keys_is_rejected() {
    let source = "a\nb\n";
    let file = write_temp_source(".txt", source);
    let payload = json!({
      "set_line": { "anchor": line_ref(source, 2), "new_text": "B" }
    });

    let output = run_identedit_with_stdin(
        &[
            "hashline",
            "check",
            file.to_str().expect("path should be utf-8"),
            "--edits",
            "-",
        ],
        &serde_json::to_string_pretty(&payload).expect("payload should serialize"),
    );
    assert!(
        !output.status.success(),
        "object payload without wrapper keys should fail"
    );

    let error = error_body(&output);
    assert_eq!(error["type"], "invalid_request");
    let message = error["message"]
        .as_str()
        .expect("error message should be a string");
    assert!(
        message.contains("unknown field"),
        "expected unknown field guidance, got: {message}"
    );
}

#[test]
fn hashline_check_stdin_scalar_payload_is_rejected() {
    let source = "a\nb\n";
    let file = write_temp_source(".txt", source);

    let output = run_identedit_with_stdin(
        &[
            "hashline",
            "check",
            file.to_str().expect("path should be utf-8"),
            "--edits",
            "-",
        ],
        "\"not-an-edit-array\"",
    );
    assert!(
        !output.status.success(),
        "scalar payload should fail shape validation"
    );

    let error = error_body(&output);
    assert_eq!(error["type"], "invalid_request");
    let message = error["message"]
        .as_str()
        .expect("error message should be a string");
    assert!(
        message.contains("expected either an edit array"),
        "expected shape guidance, got: {message}"
    );
}

#[test]
fn hashline_apply_repair_dry_run_default_omits_content_and_returns_output_fingerprint() {
    let source = "a\nb\na\n";
    let file = write_temp_source(".txt", source);
    let stale_anchor = format!("1:{}", identedit::hashline::compute_line_hash("b"));
    let edits = json!([
      { "set_line": { "anchor": stale_anchor, "new_text": "B" } }
    ]);

    let output = run_identedit_with_stdin(
        &[
            "hashline",
            "apply",
            file.to_str().expect("path should be utf-8"),
            "--edits",
            "-",
            "--repair",
            "--dry-run",
        ],
        &serde_json::to_string_pretty(&edits).expect("edits should serialize"),
    );
    assert!(
        output.status.success(),
        "repair+dry-run should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response = parse_stdout_json(&output);
    assert_eq!(response["command"], "apply");
    assert_eq!(response["mode"], "repair");
    assert_eq!(response["dry_run"], true);
    assert_eq!(response["changed"], true);
    assert!(response.get("content").is_none());
    assert_eq!(
        response["output_hash"],
        identedit::hash::hash_text("a\nB\na\n")
    );
    assert_eq!(response["output_bytes"], "a\nB\na\n".len());

    let after = fs::read_to_string(&file).expect("file should be readable");
    assert_eq!(after, source, "dry-run must not mutate source file");
}

#[cfg(unix)]
#[test]
fn hashline_apply_noop_on_read_only_file_succeeds_without_dry_run() {
    let source = "a\nb\n";
    let file = write_temp_source(".txt", source);
    let edits = json!([
      { "set_line": { "anchor": line_ref(source, 2), "new_text": "b" } }
    ]);
    let edits_path = write_temp_json(
        &serde_json::to_string_pretty(&edits).expect("edits JSON should serialize"),
    );

    let mut permissions = fs::metadata(&file)
        .expect("file metadata should be readable")
        .permissions();
    permissions.set_mode(0o444);
    fs::set_permissions(&file, permissions).expect("file should become read-only");

    let output = run_identedit(&[
        "hashline",
        "apply",
        file.to_str().expect("path should be utf-8"),
        "--edits",
        edits_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "no-op apply should avoid write path and succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response = parse_stdout_json(&output);
    assert_eq!(response["changed"], false);
    assert_eq!(response["dry_run"], false);

    let after = fs::read_to_string(&file).expect("file should still be readable");
    assert_eq!(after, source);
}

#[test]
fn hashline_check_and_apply_invalid_json_fail_with_same_error_type() {
    let source = "a\nb\n";
    let file = write_temp_source(".txt", source);
    let invalid = "{";

    let check_output = run_identedit_with_stdin(
        &[
            "hashline",
            "check",
            file.to_str().expect("path should be utf-8"),
            "--edits",
            "-",
        ],
        invalid,
    );
    let apply_output = run_identedit_with_stdin(
        &[
            "hashline",
            "apply",
            file.to_str().expect("path should be utf-8"),
            "--edits",
            "-",
        ],
        invalid,
    );

    assert!(!check_output.status.success());
    assert!(!apply_output.status.success());

    let check_error = error_body(&check_output);
    let apply_error = error_body(&apply_output);
    assert_eq!(check_error["type"], "invalid_request");
    assert_eq!(apply_error["type"], "invalid_request");
}

#[test]
fn hashline_apply_repair_non_remappable_stale_anchor_fails_without_mutation() {
    let source = "a\nb\n";
    let file = write_temp_source(".txt", source);
    let stale_anchor = format!("1:{}", identedit::hashline::compute_line_hash("missing"));
    let edits = json!([
      { "set_line": { "anchor": stale_anchor, "new_text": "B" } }
    ]);

    let output = run_identedit_with_stdin(
        &[
            "hashline",
            "apply",
            file.to_str().expect("path should be utf-8"),
            "--edits",
            "-",
            "--repair",
        ],
        &serde_json::to_string_pretty(&edits).expect("edits should serialize"),
    );
    assert!(
        !output.status.success(),
        "repair should fail when no remap candidate exists"
    );

    let error = error_body(&output);
    assert_eq!(error["type"], "invalid_request");
    let message = error["message"]
        .as_str()
        .expect("error message should be a string");
    assert!(
        message.contains("preconditions failed"),
        "expected precondition context, got: {message}"
    );

    let after = fs::read_to_string(&file).expect("file should still be readable");
    assert_eq!(after, source, "failed apply must not mutate file");
}

#[test]
fn hashline_apply_stdin_wrapper_with_non_array_edits_is_rejected() {
    let source = "a\nb\n";
    let file = write_temp_source(".txt", source);
    let payload = json!({
      "command": "hashline",
      "edits": { "set_line": { "anchor": line_ref(source, 2), "new_text": "B" } }
    });

    let output = run_identedit_with_stdin(
        &[
            "hashline",
            "apply",
            file.to_str().expect("path should be utf-8"),
            "--edits",
            "-",
        ],
        &serde_json::to_string_pretty(&payload).expect("payload should serialize"),
    );
    assert!(
        !output.status.success(),
        "wrapper with non-array edits should fail"
    );

    let error = error_body(&output);
    assert_eq!(error["type"], "invalid_request");
    let message = error["message"]
        .as_str()
        .expect("error message should be a string");
    assert!(
        message.contains("Invalid hashline stdin request"),
        "expected wrapper-specific message, got: {message}"
    );
    assert!(
        message.contains("must be an array"),
        "expected array type mismatch detail, got: {message}"
    );
}

#[test]
fn hashline_check_array_edit_with_unknown_fields_fails_validation() {
    let source = "a\nb\n";
    let file = write_temp_source(".txt", source);
    let payload = json!([
      {
        "set_line": {
          "anchor": line_ref(source, 2),
          "new_text": "B",
          "extra": true
        }
      }
    ]);

    let output = run_identedit_with_stdin(
        &[
            "hashline",
            "check",
            file.to_str().expect("path should be utf-8"),
            "--edits",
            "-",
        ],
        &serde_json::to_string_pretty(&payload).expect("payload should serialize"),
    );
    assert!(
        !output.status.success(),
        "unknown edit fields should fail validation"
    );

    let error = error_body(&output);
    assert_eq!(error["type"], "invalid_request");
    let message = error["message"]
        .as_str()
        .expect("error message should be a string");
    assert!(
        message.contains("unknown field"),
        "expected unknown field detail, got: {message}"
    );
}

#[test]
fn hashline_check_with_zero_line_anchor_reports_invalid_anchor() {
    let source = "a\nb\n";
    let file = write_temp_source(".txt", source);
    let edits = json!([
      { "set_line": { "anchor": "0:aaaaaaaaaaaa", "new_text": "B" } }
    ]);

    let output = run_identedit_with_stdin(
        &[
            "hashline",
            "check",
            file.to_str().expect("path should be utf-8"),
            "--edits",
            "-",
        ],
        &serde_json::to_string_pretty(&edits).expect("payload should serialize"),
    );
    assert!(!output.status.success(), "line 0 anchor should fail");

    let error = error_body(&output);
    assert_eq!(error["type"], "invalid_request");
    let message = error["message"]
        .as_str()
        .expect("error message should be a string");
    assert!(
        message.contains("line number must be >= 1"),
        "expected anchor validation message, got: {message}"
    );
}

#[test]
fn hashline_apply_strict_and_repair_match_for_fresh_anchors() {
    let source = "a\nb\n";
    let strict_file = write_temp_source(".txt", source);
    let repair_file = write_temp_source(".txt", source);
    let edits = json!([
      { "set_line": { "anchor": line_ref(source, 2), "new_text": "B" } }
    ]);
    let edits_path = write_temp_json(
        &serde_json::to_string_pretty(&edits).expect("edits JSON should serialize"),
    );

    let strict_output = run_identedit(&[
        "hashline",
        "apply",
        strict_file.to_str().expect("path should be utf-8"),
        "--edits",
        edits_path.to_str().expect("path should be utf-8"),
    ]);
    let repair_output = run_identedit(&[
        "hashline",
        "apply",
        repair_file.to_str().expect("path should be utf-8"),
        "--edits",
        edits_path.to_str().expect("path should be utf-8"),
        "--repair",
    ]);

    assert!(strict_output.status.success());
    assert!(repair_output.status.success());

    let strict_after = fs::read_to_string(&strict_file).expect("strict file should be readable");
    let repair_after = fs::read_to_string(&repair_file).expect("repair file should be readable");
    assert_eq!(strict_after, repair_after);

    let strict_response = parse_stdout_json(&strict_output);
    let repair_response = parse_stdout_json(&repair_output);
    assert_eq!(
        strict_response["operations_total"],
        repair_response["operations_total"]
    );
    assert_eq!(
        strict_response["operations_applied"],
        repair_response["operations_applied"]
    );
}

#[test]
fn hashline_apply_with_empty_edits_is_noop() {
    let source = "a\nb\n";
    let file = write_temp_source(".txt", source);
    let edits_path =
        write_temp_json(&serde_json::to_string_pretty(&json!([])).expect("JSON should serialize"));

    let output = run_identedit(&[
        "hashline",
        "apply",
        file.to_str().expect("path should be utf-8"),
        "--edits",
        edits_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(output.status.success(), "empty edits should be allowed");

    let response = parse_stdout_json(&output);
    assert_eq!(response["changed"], false);
    assert_eq!(response["operations_total"], 0);
    assert_eq!(response["operations_applied"], 0);

    let after = fs::read_to_string(&file).expect("file should still be readable");
    assert_eq!(after, source);
}

#[test]
fn hashline_check_stdin_wrapper_requires_string_command() {
    let source = "a\nb\n";
    let file = write_temp_source(".txt", source);
    let payload = json!({
      "command": 42,
      "edits": [
        { "set_line": { "anchor": line_ref(source, 2), "new_text": "B" } }
      ]
    });

    let output = run_identedit_with_stdin(
        &[
            "hashline",
            "check",
            file.to_str().expect("path should be utf-8"),
            "--edits",
            "-",
        ],
        &serde_json::to_string_pretty(&payload).expect("payload should serialize"),
    );
    assert!(!output.status.success(), "wrapper command must be a string");

    let error = error_body(&output);
    assert_eq!(error["type"], "invalid_request");
    let message = error["message"]
        .as_str()
        .expect("error message should be a string");
    assert!(
        message.contains("field 'command' must be a string"),
        "expected command type guidance, got: {message}"
    );
}

#[test]
fn hashline_apply_array_rejects_non_object_edit_entries() {
    let source = "a\nb\n";
    let file = write_temp_source(".txt", source);
    let payload = json!(["not-an-edit-object"]);

    let output = run_identedit_with_stdin(
        &[
            "hashline",
            "apply",
            file.to_str().expect("path should be utf-8"),
            "--edits",
            "-",
        ],
        &serde_json::to_string_pretty(&payload).expect("payload should serialize"),
    );
    assert!(
        !output.status.success(),
        "non-object edit entry should fail"
    );

    let error = error_body(&output);
    assert_eq!(error["type"], "invalid_request");
    let message = error["message"]
        .as_str()
        .expect("error message should be a string");
    assert!(
        message.contains("index 0"),
        "expected edit index in message, got: {message}"
    );
    assert!(
        message.contains("expected an object"),
        "expected object type guidance, got: {message}"
    );
}

#[test]
fn hashline_apply_array_rejects_multi_operation_edit_object() {
    let source = "a\nb\n";
    let file = write_temp_source(".txt", source);
    let payload = json!([
      {
        "set_line": { "anchor": line_ref(source, 2), "new_text": "B" },
        "insert_after": { "anchor": line_ref(source, 2), "text": "C" }
      }
    ]);

    let output = run_identedit_with_stdin(
        &[
            "hashline",
            "apply",
            file.to_str().expect("path should be utf-8"),
            "--edits",
            "-",
        ],
        &serde_json::to_string_pretty(&payload).expect("payload should serialize"),
    );
    assert!(
        !output.status.success(),
        "multi-operation edit object should fail"
    );

    let error = error_body(&output);
    assert_eq!(error["type"], "invalid_request");
    let message = error["message"]
        .as_str()
        .expect("error message should be a string");
    assert!(
        message.contains("exactly one operation key"),
        "expected single-op guidance, got: {message}"
    );
}

#[test]
fn hashline_apply_array_rejects_unknown_operation_key() {
    let source = "a\nb\n";
    let file = write_temp_source(".txt", source);
    let payload = json!([
      {
        "delete_line": { "anchor": line_ref(source, 2) }
      }
    ]);

    let output = run_identedit_with_stdin(
        &[
            "hashline",
            "apply",
            file.to_str().expect("path should be utf-8"),
            "--edits",
            "-",
        ],
        &serde_json::to_string_pretty(&payload).expect("payload should serialize"),
    );
    assert!(!output.status.success(), "unknown operation should fail");

    let error = error_body(&output);
    assert_eq!(error["type"], "invalid_request");
    let message = error["message"]
        .as_str()
        .expect("error message should be a string");
    assert!(
        message.contains("unknown operation key 'delete_line'"),
        "expected unknown-op guidance, got: {message}"
    );
}

#[test]
fn hashline_check_accepts_wrapper_payload_from_file() {
    let source = "a\nb\n";
    let file = write_temp_source(".txt", source);
    let payload = json!({
      "command": "hashline",
      "edits": [
        { "set_line": { "anchor": line_ref(source, 2), "new_text": "B" } }
      ]
    });
    let edits_path =
        write_temp_json(&serde_json::to_string_pretty(&payload).expect("payload should serialize"));

    let output = run_identedit(&[
        "hashline",
        "check",
        file.to_str().expect("path should be utf-8"),
        "--edits",
        edits_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "wrapper payload from file should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response = parse_stdout_json(&output);
    assert_eq!(response["check"]["ok"], true);
}

#[test]
fn hashline_apply_accepts_wrapper_payload_from_file() {
    let source = "a\nb\n";
    let file = write_temp_source(".txt", source);
    let payload = json!({
      "command": "hashline",
      "edits": [
        { "set_line": { "anchor": line_ref(source, 2), "new_text": "B" } }
      ]
    });
    let edits_path =
        write_temp_json(&serde_json::to_string_pretty(&payload).expect("payload should serialize"));

    let output = run_identedit(&[
        "hashline",
        "apply",
        file.to_str().expect("path should be utf-8"),
        "--edits",
        edits_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "wrapper payload from file should apply: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let after = fs::read_to_string(&file).expect("file should still be readable");
    assert_eq!(after, "a\nB\n");
}

#[test]
fn hashline_show_missing_file_returns_io_error() {
    let missing = write_temp_source(".txt", "temporary")
        .to_str()
        .expect("path should be utf-8")
        .to_string();
    fs::remove_file(&missing).expect("temp file should be removable");

    let output = run_identedit(&["hashline", "show", &missing]);
    assert!(!output.status.success(), "missing file should fail");

    let error = error_body(&output);
    assert_eq!(error["type"], "io_error");
}

#[test]
fn hashline_check_missing_edits_file_returns_io_error() {
    let source = "a\nb\n";
    let file = write_temp_source(".txt", source);
    let missing_edits = write_temp_source(".json", "[]");
    fs::remove_file(&missing_edits).expect("temp edits file should be removable");

    let output = run_identedit(&[
        "hashline",
        "check",
        file.to_str().expect("path should be utf-8"),
        "--edits",
        missing_edits.to_str().expect("path should be utf-8"),
    ]);
    assert!(!output.status.success(), "missing edits file should fail");

    let error = error_body(&output);
    assert_eq!(error["type"], "io_error");
}

#[test]
fn hashline_apply_non_utf8_source_returns_io_error() {
    let path = write_temp_source(".txt", "");
    fs::write(&path, [0xff, 0xfe]).expect("invalid utf-8 payload should be written");
    let edits_path =
        write_temp_json(&serde_json::to_string_pretty(&json!([])).expect("JSON should serialize"));

    let output = run_identedit(&[
        "hashline",
        "apply",
        path.to_str().expect("path should be utf-8"),
        "--edits",
        edits_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(!output.status.success(), "non-utf8 source should fail");

    let error = error_body(&output);
    assert_eq!(error["type"], "io_error");
}

#[test]
fn hashline_show_plain_text_preserves_special_characters_in_content() {
    let file = write_temp_source(".txt", "a:b|c\n  spaced | value  ");
    let output = run_identedit(&[
        "hashline",
        "show",
        file.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "show should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf-8");
    assert!(stdout.contains("|a:b|c"));
    assert!(stdout.contains("|  spaced | value  "));
}

#[test]
fn hashline_show_json_empty_file_returns_zero_lines() {
    let file = write_temp_source(".txt", "");
    let output = run_identedit(&[
        "hashline",
        "show",
        "--json",
        file.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "show --json should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response = parse_stdout_json(&output);
    assert_eq!(response["summary"]["total_lines"], 0);
    assert_eq!(
        response["lines"]
            .as_array()
            .expect("lines should be an array")
            .len(),
        0
    );
}

#[test]
fn hashline_apply_noop_non_dry_run_has_null_content_field() {
    let source = "a\nb\n";
    let file = write_temp_source(".txt", source);
    let edits = json!([
      { "set_line": { "anchor": line_ref(source, 2), "new_text": "b" } }
    ]);
    let edits_path = write_temp_json(
        &serde_json::to_string_pretty(&edits).expect("edits JSON should serialize"),
    );

    let output = run_identedit(&[
        "hashline",
        "apply",
        file.to_str().expect("path should be utf-8"),
        "--edits",
        edits_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(output.status.success(), "no-op apply should succeed");

    let response = parse_stdout_json(&output);
    assert_eq!(response["changed"], false);
    assert_eq!(response["dry_run"], false);
    assert!(response["content"].is_null());
}

#[test]
fn hashline_apply_dry_run_preserves_crlf_content() {
    let source = "a\r\nb\r\n";
    let file = write_temp_source(".txt", source);
    let edits = json!([
      { "set_line": { "anchor": line_ref(source, 2), "new_text": "B" } }
    ]);
    let edits_path = write_temp_json(
        &serde_json::to_string_pretty(&edits).expect("edits JSON should serialize"),
    );

    let output = run_identedit(&[
        "hashline",
        "apply",
        file.to_str().expect("path should be utf-8"),
        "--edits",
        edits_path.to_str().expect("path should be utf-8"),
        "--dry-run",
    ]);
    assert!(
        output.status.success(),
        "dry-run should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response = parse_stdout_json(&output);
    assert!(response.get("content").is_none());
    assert_eq!(
        response["output_hash"],
        identedit::hash::hash_text("a\r\nB\r\n")
    );
    assert_eq!(response["output_bytes"], "a\r\nB\r\n".len());
}

#[test]
fn hashline_apply_dry_run_preserves_cr_only_content() {
    let source = "a\rb\r";
    let file = write_temp_source(".txt", source);
    let second_line_anchor = format!("2:{}", identedit::hashline::compute_line_hash("b"));
    let edits = json!([
      { "set_line": { "anchor": second_line_anchor, "new_text": "B" } }
    ]);
    let edits_path = write_temp_json(
        &serde_json::to_string_pretty(&edits).expect("edits JSON should serialize"),
    );

    let output = run_identedit(&[
        "hashline",
        "apply",
        file.to_str().expect("path should be utf-8"),
        "--edits",
        edits_path.to_str().expect("path should be utf-8"),
        "--dry-run",
    ]);
    assert!(
        output.status.success(),
        "dry-run should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response = parse_stdout_json(&output);
    assert!(response.get("content").is_none());
    assert_eq!(
        response["output_hash"],
        identedit::hash::hash_text("a\rB\r")
    );
    assert_eq!(response["output_bytes"], "a\rB\r".len());
}

#[test]
fn hashline_apply_reports_overlap_with_edit_indices() {
    let source = "a\nb\nc\n";
    let file = write_temp_source(".txt", source);
    let edits = json!([
      { "set_line": { "anchor": line_ref(source, 2), "new_text": "B1" } },
      { "set_line": { "anchor": line_ref(source, 2), "new_text": "B2" } }
    ]);

    let output = run_identedit_with_stdin(
        &[
            "hashline",
            "apply",
            file.to_str().expect("path should be utf-8"),
            "--edits",
            "-",
        ],
        &serde_json::to_string_pretty(&edits).expect("edits should serialize"),
    );
    assert!(!output.status.success(), "overlapping edits should fail");

    let error = error_body(&output);
    assert_eq!(error["type"], "invalid_request");
    let message = error["message"]
        .as_str()
        .expect("error message should be a string");
    assert!(
        message.contains("Overlapping hashline edits"),
        "expected overlap message, got: {message}"
    );
    assert!(
        message.contains("edit #0") && message.contains("edit #1"),
        "expected both edit indices in message, got: {message}"
    );
}

#[test]
fn hashline_patch_applies_fresh_anchors_in_strict_mode() {
    let source = "a\nb\n";
    let file = write_temp_source(".txt", source);
    let edits = json!([
      { "set_line": { "anchor": line_ref(source, 2), "new_text": "B" } }
    ]);
    let edits_path = write_temp_json(
        &serde_json::to_string_pretty(&edits).expect("edits JSON should serialize"),
    );

    let output = run_identedit(&[
        "hashline",
        "patch",
        file.to_str().expect("path should be utf-8"),
        "--edits",
        edits_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "hashline patch should succeed for fresh anchors: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response = parse_stdout_json(&output);
    assert_eq!(response["command"], "patch");
    assert_eq!(response["applied_mode"], "strict");
    assert_eq!(response["strict_check"]["ok"], true);
    assert_eq!(response["operations_applied"], 1);

    let after = fs::read_to_string(&file).expect("file should still be readable");
    assert_eq!(after, "a\nB\n");
}

#[test]
fn hashline_patch_without_auto_repair_fails_precondition_and_keeps_file() {
    let source = "a\nb\na\n";
    let file = write_temp_source(".txt", source);
    let stale_anchor = format!("1:{}", identedit::hashline::compute_line_hash("b"));
    let edits = json!([
      { "set_line": { "anchor": stale_anchor, "new_text": "B" } }
    ]);

    let output = run_identedit_with_stdin(
        &[
            "hashline",
            "patch",
            file.to_str().expect("path should be utf-8"),
            "--edits",
            "-",
        ],
        &serde_json::to_string_pretty(&edits).expect("edits should serialize"),
    );
    assert!(
        !output.status.success(),
        "hashline patch should fail strict precondition without auto-repair"
    );

    let error = error_body(&output);
    assert_eq!(error["type"], "invalid_request");
    let message = error["message"]
        .as_str()
        .expect("error message should be a string");
    let embedded_check = parse_embedded_check_from_error_message(message);
    assert_eq!(embedded_check["ok"], false);

    let after = fs::read_to_string(&file).expect("file should still be readable");
    assert_eq!(after, source, "failed patch must not mutate source file");
}

#[test]
fn hashline_patch_auto_repair_remaps_once_when_deterministic() {
    let source = "a\nb\na\n";
    let file = write_temp_source(".txt", source);
    let stale_anchor = format!("1:{}", identedit::hashline::compute_line_hash("b"));
    let edits = json!([
      { "set_line": { "anchor": stale_anchor, "new_text": "B" } }
    ]);

    let output = run_identedit_with_stdin(
        &[
            "hashline",
            "patch",
            file.to_str().expect("path should be utf-8"),
            "--edits",
            "-",
            "--auto-repair",
        ],
        &serde_json::to_string_pretty(&edits).expect("edits should serialize"),
    );
    assert!(
        output.status.success(),
        "hashline patch auto-repair should succeed on deterministic remap: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response = parse_stdout_json(&output);
    assert_eq!(response["command"], "patch");
    assert_eq!(response["applied_mode"], "repair");
    assert_eq!(response["strict_check"]["ok"], false);
    assert_eq!(response["operations_applied"], 1);

    let after = fs::read_to_string(&file).expect("file should still be readable");
    assert_eq!(after, "a\nB\na\n");
}

#[test]
fn hashline_patch_auto_repair_does_not_retry_ambiguous_remaps() {
    let source = "b\nb\na\n";
    let file = write_temp_source(".txt", source);
    let stale_anchor = format!("3:{}", identedit::hashline::compute_line_hash("b"));
    let edits = json!([
      { "set_line": { "anchor": stale_anchor, "new_text": "B" } }
    ]);

    let output = run_identedit_with_stdin(
        &[
            "hashline",
            "patch",
            file.to_str().expect("path should be utf-8"),
            "--edits",
            "-",
            "--auto-repair",
        ],
        &serde_json::to_string_pretty(&edits).expect("edits should serialize"),
    );
    assert!(
        !output.status.success(),
        "ambiguous remaps should not be auto-retried"
    );

    let error = error_body(&output);
    assert_eq!(error["type"], "invalid_request");
    let message = error["message"]
        .as_str()
        .expect("error message should be a string");
    let embedded_check = parse_embedded_check_from_error_message(message);
    assert_eq!(embedded_check["ok"], false);
    assert!(
        embedded_check["summary"]["ambiguous"]
            .as_u64()
            .expect("ambiguous count should be u64")
            > 0,
        "embedded diagnostics should report ambiguous remap candidates"
    );

    let after = fs::read_to_string(&file).expect("file should still be readable");
    assert_eq!(after, source, "failed patch must not mutate source file");
}

#[test]
fn hashline_patch_auto_repair_keeps_strict_mode_when_not_needed() {
    let source = "a\nb\n";
    let file = write_temp_source(".txt", source);
    let edits = json!([
      { "set_line": { "anchor": line_ref(source, 2), "new_text": "B" } }
    ]);

    let output = run_identedit_with_stdin(
        &[
            "hashline",
            "patch",
            file.to_str().expect("path should be utf-8"),
            "--edits",
            "-",
            "--auto-repair",
        ],
        &serde_json::to_string_pretty(&edits).expect("edits should serialize"),
    );
    assert!(
        output.status.success(),
        "patch should succeed with fresh anchors even when auto-repair is enabled"
    );

    let response = parse_stdout_json(&output);
    assert_eq!(response["applied_mode"], "strict");
    assert_eq!(response["strict_check"]["ok"], true);
}

#[test]
fn hashline_apply_precondition_failure_includes_check_payload() {
    let source = "a\nb\n";
    let file = write_temp_source(".txt", source);
    let edits = json!([
      { "set_line": { "anchor": "1:aaaaaaaaaaaa", "new_text": "A" } }
    ]);

    let output = run_identedit_with_stdin(
        &[
            "hashline",
            "apply",
            file.to_str().expect("path should be utf-8"),
            "--edits",
            "-",
        ],
        &serde_json::to_string_pretty(&edits).expect("edits should serialize"),
    );
    assert!(
        !output.status.success(),
        "stale anchor should fail preconditions"
    );

    let error = error_body(&output);
    assert_eq!(error["type"], "invalid_request");
    let message = error["message"]
        .as_str()
        .expect("error message should be a string");
    assert!(
        message.contains("Hashline preconditions failed"),
        "expected precondition header, got: {message}"
    );
    assert!(
        message.contains("\"ok\": false") && message.contains("\"mismatches\""),
        "expected embedded check payload, got: {message}"
    );
}

#[test]
fn hashline_apply_precondition_payload_uses_canonical_mismatch_order() {
    let source = "x\na\nx\nb\n";
    let file = write_temp_source(".txt", source);
    let stale_x = format!("10:{}", identedit::hashline::compute_line_hash("x"));
    let stale_b = format!("9:{}", identedit::hashline::compute_line_hash("b"));
    let edits = json!([
      {
        "replace_lines": {
          "start_anchor": stale_x,
          "end_anchor": stale_b,
          "new_text": "X"
        }
      }
    ]);

    let output = run_identedit_with_stdin(
        &[
            "hashline",
            "apply",
            file.to_str().expect("path should be utf-8"),
            "--edits",
            "-",
        ],
        &serde_json::to_string_pretty(&edits).expect("edits should serialize"),
    );
    assert!(
        !output.status.success(),
        "stale anchors should fail preconditions"
    );

    let error = error_body(&output);
    assert_eq!(error["type"], "invalid_request");
    let message = error["message"]
        .as_str()
        .expect("error message should be a string");
    let check = parse_embedded_check_from_error_message(message);
    let mismatches = check["mismatches"]
        .as_array()
        .expect("mismatches should be an array");
    assert_eq!(mismatches.len(), 2);
    assert_eq!(mismatches[0]["line"], 9);
    assert_eq!(mismatches[1]["line"], 10);

    let remaps = mismatches[1]["remaps"]
        .as_array()
        .expect("ambiguous remap candidates should be present");
    assert_eq!(remaps[0]["line"], 1);
    assert_eq!(remaps[1]["line"], 3);
}

#[test]
fn hashline_check_accepts_empty_wrapper_edits() {
    let source = "a\nb\n";
    let file = write_temp_source(".txt", source);
    let payload = json!({
      "command": "hashline",
      "edits": []
    });

    let output = run_identedit_with_stdin(
        &[
            "hashline",
            "check",
            file.to_str().expect("path should be utf-8"),
            "--edits",
            "-",
        ],
        &serde_json::to_string_pretty(&payload).expect("payload should serialize"),
    );
    assert!(
        output.status.success(),
        "empty wrapper edits should be valid: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response = parse_stdout_json(&output);
    assert_eq!(response["check"]["ok"], true);
    assert_eq!(response["check"]["summary"]["total"], 0);
}

#[test]
fn hashline_apply_accepts_empty_wrapper_edits_as_noop() {
    let source = "a\nb\n";
    let file = write_temp_source(".txt", source);
    let payload = json!({
      "command": "hashline",
      "edits": []
    });
    let edits_path =
        write_temp_json(&serde_json::to_string_pretty(&payload).expect("payload should serialize"));

    let output = run_identedit(&[
        "hashline",
        "apply",
        file.to_str().expect("path should be utf-8"),
        "--edits",
        edits_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "empty wrapper edits should be noop: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response = parse_stdout_json(&output);
    assert_eq!(response["changed"], false);
    assert_eq!(response["operations_total"], 0);
    assert_eq!(response["operations_applied"], 0);

    let after = fs::read_to_string(&file).expect("file should still be readable");
    assert_eq!(after, source);
}

#[test]
fn hashline_show_plain_empty_file_prints_single_newline() {
    let file = write_temp_source(".txt", "");
    let output = run_identedit(&[
        "hashline",
        "show",
        file.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "show should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf-8");
    assert_eq!(stdout, "\n");
}

#[test]
fn hashline_show_plain_non_empty_output_has_single_trailing_newline() {
    let file = write_temp_source(".txt", "alpha\nbeta");
    let output = run_identedit(&[
        "hashline",
        "show",
        file.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "show should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf-8");
    assert!(
        stdout.ends_with('\n'),
        "plain output should end with one newline"
    );
    assert!(
        !stdout.ends_with("\n\n"),
        "plain output should not have a double trailing newline"
    );
}

#[test]
fn hashline_check_stdin_wrapper_rejects_null_edits() {
    let source = "a\nb\n";
    let file = write_temp_source(".txt", source);
    let payload = json!({
      "command": "hashline",
      "edits": null
    });

    let output = run_identedit_with_stdin(
        &[
            "hashline",
            "check",
            file.to_str().expect("path should be utf-8"),
            "--edits",
            "-",
        ],
        &serde_json::to_string_pretty(&payload).expect("payload should serialize"),
    );
    assert!(!output.status.success(), "null edits should fail");

    let error = error_body(&output);
    assert_eq!(error["type"], "invalid_request");
    let message = error["message"]
        .as_str()
        .expect("error message should be a string");
    assert!(
        message.contains("field 'edits' must be an array"),
        "expected edits type guidance, got: {message}"
    );
}

#[test]
fn hashline_check_stdin_wrapper_rejects_null_command() {
    let source = "a\nb\n";
    let file = write_temp_source(".txt", source);
    let payload = json!({
      "command": null,
      "edits": []
    });

    let output = run_identedit_with_stdin(
        &[
            "hashline",
            "check",
            file.to_str().expect("path should be utf-8"),
            "--edits",
            "-",
        ],
        &serde_json::to_string_pretty(&payload).expect("payload should serialize"),
    );
    assert!(!output.status.success(), "null command should fail");

    let error = error_body(&output);
    assert_eq!(error["type"], "invalid_request");
    let message = error["message"]
        .as_str()
        .expect("error message should be a string");
    assert!(
        message.contains("field 'command' must be a string"),
        "expected command type guidance, got: {message}"
    );
}

#[test]
fn hashline_apply_array_rejects_null_operation_payload() {
    let source = "a\nb\n";
    let file = write_temp_source(".txt", source);
    let payload = json!([
      { "set_line": null }
    ]);

    let output = run_identedit_with_stdin(
        &[
            "hashline",
            "apply",
            file.to_str().expect("path should be utf-8"),
            "--edits",
            "-",
        ],
        &serde_json::to_string_pretty(&payload).expect("payload should serialize"),
    );
    assert!(
        !output.status.success(),
        "null operation payload should fail"
    );

    let error = error_body(&output);
    assert_eq!(error["type"], "invalid_request");
    let message = error["message"]
        .as_str()
        .expect("error message should be a string");
    assert!(
        message.contains("index 0"),
        "expected edit index in message, got: {message}"
    );
}

#[test]
fn hashline_check_array_rejects_empty_edit_object() {
    let source = "a\nb\n";
    let file = write_temp_source(".txt", source);
    let payload = json!([{}]);

    let output = run_identedit_with_stdin(
        &[
            "hashline",
            "check",
            file.to_str().expect("path should be utf-8"),
            "--edits",
            "-",
        ],
        &serde_json::to_string_pretty(&payload).expect("payload should serialize"),
    );
    assert!(!output.status.success(), "empty edit object should fail");

    let error = error_body(&output);
    assert_eq!(error["type"], "invalid_request");
    let message = error["message"]
        .as_str()
        .expect("error message should be a string");
    assert!(
        message.contains("exactly one operation key"),
        "expected single-op guidance, got: {message}"
    );
}

#[test]
fn hashline_apply_repair_ambiguous_remap_fails_with_ambiguous_hint() {
    let source = "a\nx\na\n";
    let file = write_temp_source(".txt", source);
    let stale_anchor = format!("2:{}", identedit::hashline::compute_line_hash("a"));
    let edits = json!([
      { "set_line": { "anchor": stale_anchor, "new_text": "X" } }
    ]);

    let output = run_identedit_with_stdin(
        &[
            "hashline",
            "apply",
            file.to_str().expect("path should be utf-8"),
            "--edits",
            "-",
            "--repair",
        ],
        &serde_json::to_string_pretty(&edits).expect("edits should serialize"),
    );
    assert!(
        !output.status.success(),
        "ambiguous remap in repair mode should fail"
    );

    let error = error_body(&output);
    assert_eq!(error["type"], "invalid_request");
    let message = error["message"]
        .as_str()
        .expect("error message should be a string");
    assert!(
        message.contains("\"ambiguous\""),
        "expected ambiguous mismatch detail, got: {message}"
    );
}

#[test]
fn hashline_apply_strict_stale_anchor_fails_without_mutation() {
    let source = "a\nb\n";
    let file = write_temp_source(".txt", source);
    let stale_anchor = format!("2:{}", identedit::hashline::compute_line_hash("c"));
    let edits = json!([
      { "set_line": { "anchor": stale_anchor, "new_text": "B" } }
    ]);

    let output = run_identedit_with_stdin(
        &[
            "hashline",
            "apply",
            file.to_str().expect("path should be utf-8"),
            "--edits",
            "-",
        ],
        &serde_json::to_string_pretty(&edits).expect("edits should serialize"),
    );
    assert!(
        !output.status.success(),
        "strict mode should reject stale anchors"
    );

    let after = fs::read_to_string(&file).expect("file should still be readable");
    assert_eq!(after, source, "strict precondition failure must not mutate");
}

#[test]
fn hashline_check_reports_ambiguous_status_for_duplicate_remap_candidates() {
    let source = "a\nx\na\n";
    let file = write_temp_source(".txt", source);
    let stale_anchor = format!("2:{}", identedit::hashline::compute_line_hash("a"));
    let edits = json!([
      { "set_line": { "anchor": stale_anchor, "new_text": "X" } }
    ]);

    let output = run_identedit_with_stdin(
        &[
            "hashline",
            "check",
            file.to_str().expect("path should be utf-8"),
            "--edits",
            "-",
        ],
        &serde_json::to_string_pretty(&edits).expect("edits should serialize"),
    );
    assert!(output.status.success(), "check should succeed");

    let response = parse_stdout_json(&output);
    assert_eq!(response["check"]["ok"], false);
    assert_eq!(response["check"]["summary"]["ambiguous"], 1);
}

#[test]
fn hashline_apply_with_empty_stdin_edits_fails_json_parse() {
    let source = "a\nb\n";
    let file = write_temp_source(".txt", source);

    let output = run_identedit_with_stdin(
        &[
            "hashline",
            "apply",
            file.to_str().expect("path should be utf-8"),
            "--edits",
            "-",
        ],
        "",
    );
    assert!(!output.status.success(), "empty stdin should fail");

    let error = error_body(&output);
    assert_eq!(error["type"], "invalid_request");
    let message = error["message"]
        .as_str()
        .expect("error message should be a string");
    assert!(
        message.contains("Failed to parse stdin JSON request"),
        "expected parse error message, got: {message}"
    );
}

#[test]
fn hashline_check_stdin_wrapper_command_with_whitespace_is_rejected() {
    let source = "a\nb\n";
    let file = write_temp_source(".txt", source);
    let payload = json!({
      "command": " hashline ",
      "edits": []
    });

    let output = run_identedit_with_stdin(
        &[
            "hashline",
            "check",
            file.to_str().expect("path should be utf-8"),
            "--edits",
            "-",
        ],
        &serde_json::to_string_pretty(&payload).expect("payload should serialize"),
    );
    assert!(
        !output.status.success(),
        "whitespace-padded command should be rejected"
    );

    let error = error_body(&output);
    assert_eq!(error["type"], "invalid_request");
    let message = error["message"]
        .as_str()
        .expect("error message should be a string");
    assert!(
        message.contains("Unsupported command"),
        "expected unsupported command message, got: {message}"
    );
}

#[test]
fn hashline_show_json_echoes_requested_file_path() {
    let source = "a\n";
    let file = write_temp_source(".txt", source);
    let expected_path = file.to_string_lossy().to_string();

    let output = run_identedit(&[
        "hashline",
        "show",
        "--json",
        file.to_str().expect("path should be utf-8"),
    ]);
    assert!(output.status.success(), "show --json should succeed");

    let response = parse_stdout_json(&output);
    assert_eq!(
        response["file"]
            .as_str()
            .expect("file should be serialized"),
        expected_path
    );
}

#[test]
fn hashline_check_replace_lines_counts_both_anchors() {
    let source = "a\nb\nc\n";
    let file = write_temp_source(".txt", source);
    let edits = json!([
      {
        "replace_lines": {
          "start_anchor": line_ref(source, 1),
          "end_anchor": line_ref(source, 3),
          "new_text": "A\nB\nC"
        }
      }
    ]);

    let output = run_identedit_with_stdin(
        &[
            "hashline",
            "check",
            file.to_str().expect("path should be utf-8"),
            "--edits",
            "-",
        ],
        &serde_json::to_string_pretty(&edits).expect("edits should serialize"),
    );
    assert!(output.status.success(), "check should succeed");

    let response = parse_stdout_json(&output);
    assert_eq!(response["check"]["ok"], true);
    assert_eq!(response["check"]["summary"]["total"], 2);
    assert_eq!(response["check"]["summary"]["matched"], 2);
}

#[test]
fn hashline_apply_replace_lines_same_start_end_updates_one_line() {
    let source = "a\nb\nc\n";
    let file = write_temp_source(".txt", source);
    let edits = json!([
      {
        "replace_lines": {
          "start_anchor": line_ref(source, 2),
          "end_anchor": line_ref(source, 2),
          "new_text": "B"
        }
      }
    ]);

    let output = run_identedit_with_stdin(
        &[
            "hashline",
            "apply",
            file.to_str().expect("path should be utf-8"),
            "--edits",
            "-",
        ],
        &serde_json::to_string_pretty(&edits).expect("edits should serialize"),
    );
    assert!(output.status.success(), "apply should succeed");

    let after = fs::read_to_string(&file).expect("file should still be readable");
    assert_eq!(after, "a\nB\nc\n");
}

#[test]
fn hashline_apply_replace_lines_without_end_anchor_updates_start_line_only() {
    let source = "a\nb\nc\n";
    let file = write_temp_source(".txt", source);
    let edits = json!([
      {
        "replace_lines": {
          "start_anchor": line_ref(source, 2),
          "new_text": "B"
        }
      }
    ]);

    let output = run_identedit_with_stdin(
        &[
            "hashline",
            "apply",
            file.to_str().expect("path should be utf-8"),
            "--edits",
            "-",
        ],
        &serde_json::to_string_pretty(&edits).expect("edits should serialize"),
    );
    assert!(output.status.success(), "apply should succeed");

    let after = fs::read_to_string(&file).expect("file should still be readable");
    assert_eq!(after, "a\nB\nc\n");
}

#[test]
fn hashline_apply_insert_after_rejects_empty_text() {
    let source = "a\nb\n";
    let file = write_temp_source(".txt", source);
    let edits = json!([
      {
        "insert_after": {
          "anchor": line_ref(source, 1),
          "text": ""
        }
      }
    ]);

    let output = run_identedit_with_stdin(
        &[
            "hashline",
            "apply",
            file.to_str().expect("path should be utf-8"),
            "--edits",
            "-",
        ],
        &serde_json::to_string_pretty(&edits).expect("edits should serialize"),
    );
    assert!(
        !output.status.success(),
        "insert_after with empty text should fail"
    );

    let error = error_body(&output);
    assert_eq!(error["type"], "invalid_request");
}

#[test]
fn hashline_check_summary_total_matches_all_anchor_count() {
    let source = "a\nb\nc\n";
    let file = write_temp_source(".txt", source);
    let edits = json!([
      { "set_line": { "anchor": line_ref(source, 1), "new_text": "A" } },
      {
        "replace_lines": {
          "start_anchor": line_ref(source, 2),
          "end_anchor": line_ref(source, 3),
          "new_text": "B\nC"
        }
      }
    ]);

    let output = run_identedit_with_stdin(
        &[
            "hashline",
            "check",
            file.to_str().expect("path should be utf-8"),
            "--edits",
            "-",
        ],
        &serde_json::to_string_pretty(&edits).expect("edits should serialize"),
    );
    assert!(output.status.success(), "check should succeed");

    let response = parse_stdout_json(&output);
    assert_eq!(response["check"]["summary"]["total"], 3);
    assert_eq!(response["check"]["summary"]["matched"], 3);
}

#[test]
fn hashline_apply_reports_operation_counts_for_mixed_edits() {
    let source = "a\nb\n";
    let file = write_temp_source(".txt", source);
    let edits = json!([
      { "set_line": { "anchor": line_ref(source, 1), "new_text": "A" } },
      { "insert_after": { "anchor": line_ref(source, 2), "text": "c\nd" } }
    ]);

    let output = run_identedit_with_stdin(
        &[
            "hashline",
            "apply",
            file.to_str().expect("path should be utf-8"),
            "--edits",
            "-",
            "--dry-run",
        ],
        &serde_json::to_string_pretty(&edits).expect("edits should serialize"),
    );
    assert!(
        output.status.success(),
        "mixed edits should apply in dry-run: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response = parse_stdout_json(&output);
    assert_eq!(response["operations_total"], 2);
    assert_eq!(response["operations_applied"], 2);
    assert_eq!(response["changed"], true);
}

#[test]
fn hashline_check_supports_anchor_refs_with_anchor_table() {
    let source = "a\nb\nc\n";
    let file = write_temp_source(".txt", source);
    let payload = json!({
      "command": "hashline",
      "anchors": {
        "a1": line_ref(source, 1),
        "a2": line_ref(source, 2),
        "a3": line_ref(source, 3)
      },
      "edits": [
        { "set_line": { "anchor_ref": "a1", "new_text": "A" } },
        {
          "replace_lines": {
            "start_anchor_ref": "a2",
            "end_anchor_ref": "a3",
            "new_text": "B\nC"
          }
        }
      ]
    });

    let output = run_identedit_with_stdin(
        &[
            "hashline",
            "check",
            file.to_str().expect("path should be utf-8"),
            "--edits",
            "-",
        ],
        &serde_json::to_string_pretty(&payload).expect("payload should serialize"),
    );
    assert!(
        output.status.success(),
        "anchor_ref check should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response = parse_stdout_json(&output);
    assert_eq!(response["check"]["ok"], true);
    assert_eq!(response["check"]["summary"]["total"], 3);
}

#[test]
fn hashline_apply_supports_anchor_refs_with_anchor_table() {
    let source = "a\nb\n";
    let file = write_temp_source(".txt", source);
    let payload = json!({
      "command": "hashline",
      "anchors": {
        "a1": line_ref(source, 1),
        "a2": line_ref(source, 2)
      },
      "edits": [
        { "set_line": { "anchor_ref": "a1", "new_text": "A" } },
        { "insert_after": { "anchor_ref": "a2", "text": "c\nd" } }
      ]
    });

    let output = run_identedit_with_stdin(
        &[
            "hashline",
            "apply",
            file.to_str().expect("path should be utf-8"),
            "--edits",
            "-",
            "--dry-run",
        ],
        &serde_json::to_string_pretty(&payload).expect("payload should serialize"),
    );
    assert!(
        output.status.success(),
        "anchor_ref apply should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response = parse_stdout_json(&output);
    assert_eq!(response["operations_total"], 2);
    assert_eq!(response["operations_applied"], 2);
    assert_eq!(response["changed"], true);
}

#[test]
fn hashline_check_rejects_unknown_anchor_ref() {
    let source = "a\nb\n";
    let file = write_temp_source(".txt", source);
    let payload = json!({
      "command": "hashline",
      "anchors": {
        "a1": line_ref(source, 1)
      },
      "edits": [
        { "set_line": { "anchor_ref": "missing", "new_text": "A" } }
      ]
    });

    let output = run_identedit_with_stdin(
        &[
            "hashline",
            "check",
            file.to_str().expect("path should be utf-8"),
            "--edits",
            "-",
        ],
        &serde_json::to_string_pretty(&payload).expect("payload should serialize"),
    );
    assert!(!output.status.success(), "unknown anchor_ref should fail");

    let error = error_body(&output);
    assert_eq!(error["type"], "invalid_request");
    let message = error["message"]
        .as_str()
        .expect("error message should be string");
    assert!(
        message.contains("unknown anchor_ref"),
        "expected unknown anchor_ref diagnostic, got: {message}"
    );
}

#[test]
fn hashline_check_rejects_anchor_and_anchor_ref_together() {
    let source = "a\nb\n";
    let file = write_temp_source(".txt", source);
    let payload = json!({
      "command": "hashline",
      "anchors": {
        "a1": line_ref(source, 1)
      },
      "edits": [
        {
          "set_line": {
            "anchor": line_ref(source, 1),
            "anchor_ref": "a1",
            "new_text": "A"
          }
        }
      ]
    });

    let output = run_identedit_with_stdin(
        &[
            "hashline",
            "check",
            file.to_str().expect("path should be utf-8"),
            "--edits",
            "-",
        ],
        &serde_json::to_string_pretty(&payload).expect("payload should serialize"),
    );
    assert!(
        !output.status.success(),
        "anchor + anchor_ref mixed edit should fail"
    );

    let error = error_body(&output);
    assert_eq!(error["type"], "invalid_request");
    let message = error["message"]
        .as_str()
        .expect("error message should be string");
    assert!(
        message.contains("cannot contain both"),
        "expected mixed-anchor diagnostic, got: {message}"
    );
}

#[test]
fn hashline_check_rejects_anchor_ref_without_anchor_table() {
    let source = "a\nb\n";
    let file = write_temp_source(".txt", source);
    let payload = json!({
      "command": "hashline",
      "edits": [
        { "set_line": { "anchor_ref": "a1", "new_text": "A" } }
      ]
    });

    let output = run_identedit_with_stdin(
        &[
            "hashline",
            "check",
            file.to_str().expect("path should be utf-8"),
            "--edits",
            "-",
        ],
        &serde_json::to_string_pretty(&payload).expect("payload should serialize"),
    );
    assert!(
        !output.status.success(),
        "anchor_ref without anchors table should fail"
    );

    let error = error_body(&output);
    assert_eq!(error["type"], "invalid_request");
    let message = error["message"]
        .as_str()
        .expect("error message should be string");
    assert!(
        message.contains("requires top-level 'anchors'"),
        "expected missing anchors diagnostic, got: {message}"
    );
}
