use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::Output;

use serde_json::{Value, json};
use tempfile::Builder;

mod common;

fn run_identedit(args: &[&str]) -> Output {
    common::run_identedit(args)
}

fn run_identedit_with_stdin(args: &[&str], input: &str) -> Output {
    common::run_identedit_with_stdin(args, input)
}

fn copy_fixture_to_temp_python(name: &str) -> std::path::PathBuf {
    common::copy_fixture_to_temp_python(name)
}

fn copy_fixture_to_temp_json(name: &str) -> std::path::PathBuf {
    common::copy_fixture_to_temp_json(name)
}

fn copy_fixture_to_temp_with_suffix(name: &str, suffix: &str) -> std::path::PathBuf {
    let source = common::fixture_path(name);
    let content = fs::read_to_string(&source).expect("fixture should be readable");
    let mut temp_file = Builder::new()
        .suffix(suffix)
        .tempfile()
        .expect("temp file should be created");
    temp_file
        .write_all(content.as_bytes())
        .expect("temp fixture write should succeed");
    temp_file.keep().expect("temp file should persist").1
}

fn select_named_function_handle(file: &Path, pattern: &str) -> Value {
    common::select_first_handle(file, "function_definition", Some(pattern))
}

fn create_scoped_regex_fixture() -> std::path::PathBuf {
    let mut temp_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp python file should be created");
    temp_file
        .write_all(
            b"def process_data(value):\n    return value + 1\n\n\ndef helper(value):\n    return value + 2\n",
        )
        .expect("fixture write should succeed");
    temp_file.keep().expect("temp file should persist").1
}

fn create_temp_python_source(content: &str) -> std::path::PathBuf {
    let mut temp_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp python file should be created");
    temp_file
        .write_all(content.as_bytes())
        .expect("temp python fixture write should succeed");
    temp_file.keep().expect("temp file should persist").1
}

fn create_temp_text_file(content: &str) -> std::path::PathBuf {
    let mut temp_file = Builder::new()
        .suffix(".txt")
        .tempfile()
        .expect("temp text file should be created");
    temp_file
        .write_all(content.as_bytes())
        .expect("temp text file write should succeed");
    temp_file.keep().expect("temp text file should persist").1
}

fn create_temp_binary_file(bytes: &[u8]) -> std::path::PathBuf {
    let mut temp_file = Builder::new()
        .suffix(".bin")
        .tempfile()
        .expect("temp binary file should be created");
    temp_file
        .write_all(bytes)
        .expect("temp binary file write should succeed");
    temp_file.keep().expect("temp binary file should persist").1
}

fn line_ref(source: &str, line: usize) -> String {
    let line_text = source
        .lines()
        .nth(line - 1)
        .expect("line should exist for anchor");
    let hash = identedit::hashline::compute_line_hash(line_text);
    format!("{line}:{hash}")
}

#[test]
fn patch_replace_applies_change_in_single_command() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_function_handle(&file_path, "process_*");
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");
    let replacement = "def process_data(value):\n    return value * 9";

    let output = run_identedit(&[
        "patch",
        "--identity",
        identity,
        "--replace",
        replacement,
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "patch replace failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["summary"]["files_modified"], 1);
    assert_eq!(response["summary"]["operations_applied"], 1);
    assert_eq!(response["summary"]["operations_failed"], 0);

    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    assert!(
        modified.contains("return value * 9"),
        "replacement text should be written"
    );
}

#[test]
fn patch_kind_name_replace_applies_change_without_read_step() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let replacement = "def process_data(value):\n    return value * 11";

    let output = run_identedit(&[
        "patch",
        "--kind",
        "function_definition",
        "--name",
        "process_*",
        "--replace",
        replacement,
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "kind/name patch replace failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["summary"]["files_modified"], 1);
    assert_eq!(response["summary"]["operations_applied"], 1);

    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    assert!(
        modified.contains("return value * 11"),
        "replacement text should be written through kind/name targeting"
    );
}

#[test]
fn patch_replace_accepts_text_file_payload() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_function_handle(&file_path, "process_*");
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");
    let payload_path = create_temp_text_file("def process_data(value):\n    return value * 21");

    let output = run_identedit(&[
        "patch",
        "--identity",
        identity,
        "--replace",
        "--text-file",
        payload_path.to_str().expect("payload path should be utf-8"),
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "patch replace with text file failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    assert!(
        modified.contains("return value * 21"),
        "replacement text from file should be written"
    );
}

#[test]
fn patch_line_replace_range_accepts_stdin_text_payload() {
    let file_path = create_temp_text_file("alpha\nbeta\ngamma\ndelta\n");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let start_anchor = line_ref(&before, 2);
    let end_anchor = line_ref(&before, 3);

    let output = run_identedit_with_stdin(
        &[
            "patch",
            "--anchor",
            start_anchor.as_str(),
            "--replace-range",
            "--end-anchor",
            end_anchor.as_str(),
            "--stdin-text",
            file_path.to_str().expect("path should be utf-8"),
        ],
        "BETA\nGAMMA",
    );

    assert!(
        output.status.success(),
        "patch replace-range with stdin text failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    assert_eq!(modified, "alpha\nBETA\nGAMMA\ndelta\n");
}

#[test]
fn patch_flag_rejects_inline_text_and_text_file_together() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_function_handle(&file_path, "process_*");
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");
    let payload_path = create_temp_text_file("def process_data(value):\n    return value * 21");

    let output = run_identedit(&[
        "patch",
        "--identity",
        identity,
        "--replace",
        "def process_data(value):\n    return value * 22",
        "--text-file",
        payload_path.to_str().expect("payload path should be utf-8"),
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        !output.status.success(),
        "inline and file payload should conflict"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let message = response["error"]["message"]
        .as_str()
        .expect("error message should be present");
    assert!(
        message.contains("--replace")
            && message.contains("--text-file")
            && message.contains("--stdin-text"),
        "error should explain text source conflict, got: {message}"
    );
}

#[test]
fn patch_file_start_insert_accepts_text_file_payload() {
    let file_path = create_temp_text_file("body\n");
    let payload_path = create_temp_text_file("# generated header\n");

    let output = run_identedit(&[
        "patch",
        "--at",
        "file-start",
        "--insert",
        "--text-file",
        payload_path.to_str().expect("payload path should be utf-8"),
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "file-start insert with text file failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    assert_eq!(modified, "# generated header\nbody\n");
}

#[test]
fn patch_flag_config_path_set_value_accepts_stdin_text_payload() {
    let file_path = copy_fixture_to_temp_json("example.json");

    let output = run_identedit_with_stdin(
        &[
            "patch",
            "--config-path",
            "config.retries",
            "--set-value",
            "--stdin-text",
            file_path.to_str().expect("path should be utf-8"),
        ],
        "7",
    );

    assert!(
        output.status.success(),
        "config path set-value with stdin text failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let modified: Value =
        serde_json::from_str(&fs::read_to_string(&file_path).expect("modified file should read"))
            .expect("modified JSON should parse");
    assert_eq!(modified["config"]["retries"], 7);
}

#[test]
fn patch_flag_rejects_text_file_and_stdin_text_together() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_function_handle(&file_path, "process_*");
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");
    let payload_path = create_temp_text_file("def process_data(value):\n    return value * 21");

    let output = run_identedit_with_stdin(
        &[
            "patch",
            "--identity",
            identity,
            "--replace",
            "--text-file",
            payload_path.to_str().expect("payload path should be utf-8"),
            "--stdin-text",
            file_path.to_str().expect("path should be utf-8"),
        ],
        "def process_data(value):\n    return value * 22",
    );

    assert!(
        !output.status.success(),
        "multiple external text sources should conflict"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let message = response["error"]["message"]
        .as_str()
        .expect("error message should be present");
    assert!(
        message.contains("--text-file") && message.contains("--stdin-text"),
        "error should explain external text source conflict, got: {message}"
    );
}

#[test]
fn patch_json_mode_rejects_flag_text_source_options() {
    let request = json!({
        "command": "patch",
        "file": "/tmp/example.py",
        "target": {
            "type": "line",
            "anchor": "1:aaaaaaaaaaaa"
        },
        "op": {
            "type": "set_line",
            "new_text": "value"
        }
    });

    let output =
        run_identedit_with_stdin(&["patch", "--json", "--stdin-text"], &request.to_string());

    assert!(
        !output.status.success(),
        "json mode should reject flag text source options"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let message = response["error"]["message"]
        .as_str()
        .expect("error message should be present");
    assert!(
        message.contains("--stdin-text") && message.contains("flag mode"),
        "error should explain json/text-source incompatibility, got: {message}"
    );
}

#[test]
fn patch_scoped_regex_accepts_stdin_text_replacement() {
    let file_path = create_scoped_regex_fixture();
    let handle = select_named_function_handle(&file_path, "process_*");
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");

    let output = run_identedit_with_stdin(
        &[
            "patch",
            "--identity",
            identity,
            "--scoped-regex",
            "value",
            "--scoped-replacement",
            "--stdin-text",
            file_path.to_str().expect("path should be utf-8"),
        ],
        "payload",
    );

    assert!(
        output.status.success(),
        "scoped regex with stdin replacement failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    assert!(modified.contains("def process_data(payload):"));
    assert!(modified.contains("return payload + 1"));
    assert!(modified.contains("def helper(value):"));
}

#[test]
fn patch_delete_rejects_external_text_source() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_function_handle(&file_path, "process_*");
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");
    let payload_path = create_temp_text_file("unused");

    let output = run_identedit(&[
        "patch",
        "--identity",
        identity,
        "--delete",
        "--text-file",
        payload_path.to_str().expect("payload path should be utf-8"),
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        !output.status.success(),
        "delete should reject external text source"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let message = response["error"]["message"]
        .as_str()
        .expect("error message should be present");
    assert!(
        message.contains("--text-file") && message.contains("--stdin-text"),
        "error should explain that delete cannot consume external text, got: {message}"
    );
}

#[test]
fn patch_line_set_line_text_file_preserves_crlf() {
    let file_path = create_temp_text_file("alpha\r\nbeta\r\ngamma\r\n");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let anchor = line_ref(&before, 2);
    let payload_path = create_temp_text_file("BETA");

    let output = run_identedit(&[
        "patch",
        "--anchor",
        anchor.as_str(),
        "--set-line",
        "--text-file",
        payload_path.to_str().expect("payload path should be utf-8"),
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "set-line with text file failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    assert_eq!(modified, "alpha\r\nBETA\r\ngamma\r\n");
}

#[test]
fn patch_config_append_accepts_stdin_text_payload() {
    let file_path = copy_fixture_to_temp_json("example.json");

    let output = run_identedit_with_stdin(
        &[
            "patch",
            "--config-path",
            "items",
            "--append-value",
            "--stdin-text",
            file_path.to_str().expect("path should be utf-8"),
        ],
        "4",
    );

    assert!(
        output.status.success(),
        "config append with stdin text failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let modified: Value =
        serde_json::from_str(&fs::read_to_string(&file_path).expect("modified file should read"))
            .expect("modified JSON should parse");
    assert_eq!(modified["items"], json!([1, 2, 3, 4]));
}

#[test]
fn patch_replace_text_file_non_utf8_returns_io_error_without_mutation() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let handle = select_named_function_handle(&file_path, "process_*");
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");
    let payload_path = create_temp_binary_file(&[0x66, 0x6f, 0x80]);

    let output = run_identedit(&[
        "patch",
        "--identity",
        identity,
        "--replace",
        "--text-file",
        payload_path.to_str().expect("payload path should be utf-8"),
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(!output.status.success(), "non-utf8 text file should fail");
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "io_error");
    let after = fs::read_to_string(&file_path).expect("file should still be readable");
    assert_eq!(after, before);
}

#[test]
fn patch_file_end_insert_stdin_text_preserves_trailing_newline() {
    let file_path = create_temp_text_file("body\n");

    let output = run_identedit_with_stdin(
        &[
            "patch",
            "--at",
            "file-end",
            "--insert",
            "--stdin-text",
            file_path.to_str().expect("path should be utf-8"),
        ],
        "tail\n",
    );

    assert!(
        output.status.success(),
        "file-end insert with stdin text failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    assert_eq!(modified, "body\ntail\n");
}

#[test]
fn patch_replace_stdin_text_dry_run_does_not_modify_file() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let handle = select_named_function_handle(&file_path, "process_*");
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");

    let output = run_identedit_with_stdin(
        &[
            "patch",
            "--identity",
            identity,
            "--replace",
            "--stdin-text",
            "--dry-run",
            file_path.to_str().expect("path should be utf-8"),
        ],
        "def process_data(value):\n    return value * 33",
    );

    assert!(
        output.status.success(),
        "replace with stdin dry-run failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let after = fs::read_to_string(&file_path).expect("file should remain readable");
    assert_eq!(after, before);
}

#[test]
fn patch_line_replace_range_empty_stdin_deletes_range() {
    let file_path = create_temp_text_file("alpha\nbeta\ngamma\ndelta\n");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let start_anchor = line_ref(&before, 2);
    let end_anchor = line_ref(&before, 3);

    let output = run_identedit_with_stdin(
        &[
            "patch",
            "--anchor",
            start_anchor.as_str(),
            "--replace-range",
            "--end-anchor",
            end_anchor.as_str(),
            "--stdin-text",
            file_path.to_str().expect("path should be utf-8"),
        ],
        "",
    );

    assert!(
        output.status.success(),
        "replace-range with empty stdin failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    assert_eq!(modified, "alpha\ndelta\n");
}

#[test]
fn patch_config_append_stdin_dry_run_does_not_modify_file() {
    let file_path = copy_fixture_to_temp_json("example.json");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let output = run_identedit_with_stdin(
        &[
            "patch",
            "--config-path",
            "items",
            "--append-value",
            "--stdin-text",
            "--dry-run",
            file_path.to_str().expect("path should be utf-8"),
        ],
        "4",
    );

    assert!(
        output.status.success(),
        "append with stdin dry-run failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let after = fs::read_to_string(&file_path).expect("file should remain readable");
    assert_eq!(after, before);
}

#[test]
fn patch_scoped_regex_text_file_dry_run_does_not_modify_file() {
    let file_path = create_scoped_regex_fixture();
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let handle = select_named_function_handle(&file_path, "process_*");
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");
    let payload_path = create_temp_text_file("payload");

    let output = run_identedit(&[
        "patch",
        "--identity",
        identity,
        "--scoped-regex",
        "value",
        "--scoped-replacement",
        "--text-file",
        payload_path.to_str().expect("payload path should be utf-8"),
        "--dry-run",
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "scoped regex with text-file dry-run failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["regex_replacements"], 2);

    let after = fs::read_to_string(&file_path).expect("file should remain readable");
    assert_eq!(after, before);
}

#[test]
fn patch_file_end_insert_text_file_with_utf8_bom_preserves_payload_bytes() {
    let file_path = create_temp_text_file("body\n");
    let payload_path = create_temp_text_file("\u{FEFF}tail\n");

    let output = run_identedit(&[
        "patch",
        "--at",
        "file-end",
        "--insert",
        "--text-file",
        payload_path.to_str().expect("payload path should be utf-8"),
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "file-end insert with BOM payload failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    assert_eq!(modified, format!("body\n{}\n", "\u{FEFF}tail"));
}

#[test]
fn patch_line_set_line_empty_stdin_preserves_crlf_line_endings() {
    let file_path = create_temp_text_file("alpha\r\nbeta\r\ngamma\r\n");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let anchor = line_ref(&before, 2);

    let output = run_identedit_with_stdin(
        &[
            "patch",
            "--anchor",
            anchor.as_str(),
            "--set-line",
            "--stdin-text",
            file_path.to_str().expect("path should be utf-8"),
        ],
        "",
    );

    assert!(
        output.status.success(),
        "set-line with empty stdin failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    assert_eq!(modified, "alpha\r\n\r\ngamma\r\n");
}

#[test]
fn patch_missing_operation_with_stdin_text_reports_invalid_request() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_function_handle(&file_path, "process_*");
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");

    let output = run_identedit_with_stdin(
        &[
            "patch",
            "--identity",
            identity,
            "--stdin-text",
            file_path.to_str().expect("path should be utf-8"),
        ],
        "unused",
    );

    assert!(
        !output.status.success(),
        "missing operation should fail even when stdin text is present"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let message = response["error"]["message"]
        .as_str()
        .expect("error message should be present");
    assert!(
        message.contains("Choose exactly one node operation"),
        "error should prioritize missing operation, got: {message}"
    );
}

#[test]
fn patch_scoped_replacement_stdin_without_pattern_reports_pairing_error() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_function_handle(&file_path, "process_*");
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");

    let output = run_identedit_with_stdin(
        &[
            "patch",
            "--identity",
            identity,
            "--scoped-replacement",
            "--stdin-text",
            file_path.to_str().expect("path should be utf-8"),
        ],
        "payload",
    );

    assert!(
        !output.status.success(),
        "scoped replacement without pattern should fail"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let message = response["error"]["message"]
        .as_str()
        .expect("error message should be present");
    assert!(
        message.contains("--scoped-regex") && message.contains("--scoped-replacement"),
        "error should preserve scoped pairing guidance, got: {message}"
    );
}

#[test]
fn patch_line_insert_after_line_text_file_multiline_preserves_crlf() {
    let file_path = create_temp_text_file("alpha\r\nbeta\r\n");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let anchor = line_ref(&before, 1);
    let payload_path = create_temp_text_file("middle\r\ntail");

    let output = run_identedit(&[
        "patch",
        "--anchor",
        anchor.as_str(),
        "--insert-after-line",
        "--text-file",
        payload_path.to_str().expect("payload path should be utf-8"),
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "insert-after-line with text file failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    assert_eq!(modified, "alpha\r\nmiddle\r\ntail\r\nbeta\r\n");
}

#[test]
fn patch_config_set_value_text_file_invalid_json_does_not_mutate_file() {
    let file_path = copy_fixture_to_temp_json("example.json");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let payload_path = create_temp_text_file("{invalid-json");

    let output = run_identedit(&[
        "patch",
        "--config-path",
        "config",
        "--set-value",
        "--text-file",
        payload_path.to_str().expect("payload path should be utf-8"),
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(!output.status.success(), "invalid JSON payload should fail");
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let after = fs::read_to_string(&file_path).expect("file should remain readable");
    assert_eq!(after, before);
}

#[test]
fn patch_replace_text_file_directory_returns_io_error_without_mutation() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let handle = select_named_function_handle(&file_path, "process_*");
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");
    let payload_dir = Builder::new()
        .prefix("identedit-payload-dir")
        .tempdir()
        .expect("temp dir should be created");

    let output = run_identedit(&[
        "patch",
        "--identity",
        identity,
        "--replace",
        "--text-file",
        payload_dir
            .path()
            .to_str()
            .expect("payload path should be utf-8"),
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(!output.status.success(), "directory payload should fail");
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "io_error");
    let after = fs::read_to_string(&file_path).expect("file should remain readable");
    assert_eq!(after, before);
}

#[test]
fn patch_set_line_stdin_text_preserves_literal_dash_payload() {
    let file_path = create_temp_text_file("alpha\nbeta\n");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let anchor = line_ref(&before, 2);

    let output = run_identedit_with_stdin(
        &[
            "patch",
            "--anchor",
            anchor.as_str(),
            "--set-line",
            "--stdin-text",
            file_path.to_str().expect("path should be utf-8"),
        ],
        "--help",
    );

    assert!(
        output.status.success(),
        "set-line with literal dash payload failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    assert_eq!(modified, "alpha\n--help\n");
}

#[test]
fn patch_config_set_value_stdin_invalid_json_does_not_mutate_file() {
    let file_path = copy_fixture_to_temp_json("example.json");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let output = run_identedit_with_stdin(
        &[
            "patch",
            "--config-path",
            "config",
            "--set-value",
            "--stdin-text",
            file_path.to_str().expect("path should be utf-8"),
        ],
        "{invalid-json",
    );

    assert!(
        !output.status.success(),
        "invalid JSON stdin payload should fail"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let after = fs::read_to_string(&file_path).expect("file should remain readable");
    assert_eq!(after, before);
}

#[test]
fn patch_config_set_value_text_file_invalid_yaml_does_not_mutate_file() {
    let file_path = copy_fixture_to_temp_with_suffix("example.yaml", ".yaml");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let payload_path = create_temp_text_file("name: [unterminated");

    let output = run_identedit(&[
        "patch",
        "--config-path",
        "service",
        "--set-value",
        "--text-file",
        payload_path.to_str().expect("payload path should be utf-8"),
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(!output.status.success(), "invalid YAML payload should fail");
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let after = fs::read_to_string(&file_path).expect("file should remain readable");
    assert_eq!(after, before);
}

#[test]
fn patch_config_set_value_text_file_invalid_toml_does_not_mutate_file() {
    let file_path = copy_fixture_to_temp_with_suffix("example.toml", ".toml");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let payload_path = create_temp_text_file("{ invalid = }");

    let output = run_identedit(&[
        "patch",
        "--config-path",
        "server",
        "--set-value",
        "--text-file",
        payload_path.to_str().expect("payload path should be utf-8"),
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(!output.status.success(), "invalid TOML payload should fail");
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let after = fs::read_to_string(&file_path).expect("file should remain readable");
    assert_eq!(after, before);
}

#[test]
fn patch_config_append_text_file_invalid_json_does_not_mutate_file() {
    let file_path = copy_fixture_to_temp_json("example.json");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let payload_path = create_temp_text_file("[invalid");

    let output = run_identedit(&[
        "patch",
        "--config-path",
        "items",
        "--append-value",
        "--text-file",
        payload_path.to_str().expect("payload path should be utf-8"),
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        !output.status.success(),
        "invalid append payload should fail"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let after = fs::read_to_string(&file_path).expect("file should remain readable");
    assert_eq!(after, before);
}

#[test]
fn patch_config_set_value_text_file_valid_yaml_with_trailing_newline_applies() {
    let file_path = copy_fixture_to_temp_with_suffix("example.yaml", ".yaml");
    let payload_path = create_temp_text_file("5\n");

    let output = run_identedit(&[
        "patch",
        "--config-path",
        "service.retries",
        "--set-value",
        "--text-file",
        payload_path.to_str().expect("payload path should be utf-8"),
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "valid YAML payload with trailing newline should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    assert!(modified.contains("retries: 5"));
}

#[test]
fn patch_config_set_value_text_file_valid_toml_with_trailing_newline_applies() {
    let file_path = copy_fixture_to_temp_with_suffix("example.toml", ".toml");
    let payload_path = create_temp_text_file("9090\n");

    let output = run_identedit(&[
        "patch",
        "--config-path",
        "server.port",
        "--set-value",
        "--text-file",
        payload_path.to_str().expect("payload path should be utf-8"),
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "valid TOML payload with trailing newline should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    assert!(modified.contains("port = 9090"));
}

#[test]
fn patch_node_replace_stdin_text_with_line_only_flag_reports_node_guidance() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_function_handle(&file_path, "process_*");
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");

    let output = run_identedit_with_stdin(
        &[
            "patch",
            "--identity",
            identity,
            "--replace",
            "--stdin-text",
            "--end-anchor",
            "1:aaaaaaaaaaaa",
            file_path.to_str().expect("path should be utf-8"),
        ],
        "def process_data(value):\n    return value * 44",
    );

    assert!(
        !output.status.success(),
        "line-only flags should be rejected before node patch runs"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let message = response["error"]["message"]
        .as_str()
        .expect("error message should be present");
    assert!(
        message.contains("--replace") && message.contains("--insert-before"),
        "error should keep node guidance, got: {message}"
    );
}

#[test]
fn patch_file_insert_text_file_directory_returns_io_error_without_mutation() {
    let file_path = create_temp_text_file("body\n");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let payload_dir = Builder::new()
        .prefix("identedit-file-insert-dir")
        .tempdir()
        .expect("temp dir should be created");

    let output = run_identedit(&[
        "patch",
        "--at",
        "file-end",
        "--insert",
        "--text-file",
        payload_dir
            .path()
            .to_str()
            .expect("payload path should be utf-8"),
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        !output.status.success(),
        "directory payload should fail for file insert"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "io_error");
    let after = fs::read_to_string(&file_path).expect("file should remain readable");
    assert_eq!(after, before);
}

#[test]
fn patch_line_set_line_text_file_directory_returns_io_error_without_mutation() {
    let file_path = create_temp_text_file("alpha\nbeta\n");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let anchor = line_ref(&before, 2);
    let payload_dir = Builder::new()
        .prefix("identedit-line-payload-dir")
        .tempdir()
        .expect("temp dir should be created");

    let output = run_identedit(&[
        "patch",
        "--anchor",
        anchor.as_str(),
        "--set-line",
        "--text-file",
        payload_dir
            .path()
            .to_str()
            .expect("payload path should be utf-8"),
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        !output.status.success(),
        "directory payload should fail for line set"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "io_error");
    let after = fs::read_to_string(&file_path).expect("file should remain readable");
    assert_eq!(after, before);
}

#[test]
fn patch_config_append_text_file_directory_returns_io_error_without_mutation() {
    let file_path = copy_fixture_to_temp_json("example.json");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let payload_dir = Builder::new()
        .prefix("identedit-config-payload-dir")
        .tempdir()
        .expect("temp dir should be created");

    let output = run_identedit(&[
        "patch",
        "--config-path",
        "items",
        "--append-value",
        "--text-file",
        payload_dir
            .path()
            .to_str()
            .expect("payload path should be utf-8"),
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        !output.status.success(),
        "directory payload should fail for config append"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "io_error");
    let after = fs::read_to_string(&file_path).expect("file should remain readable");
    assert_eq!(after, before);
}

#[test]
fn patch_replace_text_file_path_with_spaces_applies() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_function_handle(&file_path, "process_*");
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");
    let payload_dir = Builder::new()
        .prefix("identedit payload dir ")
        .tempdir()
        .expect("temp dir should be created");
    let payload_path = payload_dir.path().join("replacement body.txt");
    fs::write(
        &payload_path,
        "def process_data(value):\n    return value * 55",
    )
    .expect("payload file should be written");

    let output = run_identedit(&[
        "patch",
        "--identity",
        identity,
        "--replace",
        "--text-file",
        payload_path.to_str().expect("payload path should be utf-8"),
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "text-file path with spaces should work: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    assert!(modified.contains("return value * 55"));
}

#[test]
fn patch_config_set_value_text_file_path_with_spaces_applies() {
    let file_path = copy_fixture_to_temp_json("example.json");
    let payload_dir = Builder::new()
        .prefix("identedit config payload ")
        .tempdir()
        .expect("temp dir should be created");
    let payload_path = payload_dir.path().join("value payload.txt");
    fs::write(&payload_path, "11").expect("payload file should be written");

    let output = run_identedit(&[
        "patch",
        "--config-path",
        "config.retries",
        "--set-value",
        "--text-file",
        payload_path.to_str().expect("payload path should be utf-8"),
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "config text-file path with spaces should work: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let modified: Value =
        serde_json::from_str(&fs::read_to_string(&file_path).expect("modified file should read"))
            .expect("modified JSON should parse");
    assert_eq!(modified["config"]["retries"], 11);
}

#[test]
fn patch_file_insert_stdin_text_with_node_flag_reports_file_guidance() {
    let file_path = create_temp_text_file("body\n");

    let output = run_identedit_with_stdin(
        &[
            "patch",
            "--at",
            "file-end",
            "--insert",
            "--stdin-text",
            "--delete",
            file_path.to_str().expect("path should be utf-8"),
        ],
        "tail\n",
    );

    assert!(
        !output.status.success(),
        "file mode should reject node-only flags before patching"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let message = response["error"]["message"]
        .as_str()
        .expect("error message should be present");
    assert!(
        message.contains("File target mode supports only --insert"),
        "error should keep file guidance, got: {message}"
    );
}

#[test]
fn patch_line_insert_after_line_stdin_dry_run_does_not_modify_file() {
    let file_path = create_temp_text_file("alpha\nbeta\n");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let anchor = line_ref(&before, 1);

    let output = run_identedit_with_stdin(
        &[
            "patch",
            "--anchor",
            anchor.as_str(),
            "--insert-after-line",
            "--stdin-text",
            "--dry-run",
            file_path.to_str().expect("path should be utf-8"),
        ],
        "middle\nline",
    );

    assert!(
        output.status.success(),
        "insert-after-line stdin dry-run failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let after = fs::read_to_string(&file_path).expect("file should remain readable");
    assert_eq!(after, before);
}

#[test]
fn patch_config_delete_stdin_text_rejects_unused_source() {
    let file_path = copy_fixture_to_temp_json("example.json");

    let output = run_identedit_with_stdin(
        &[
            "patch",
            "--config-path",
            "config.retries",
            "--delete",
            "--stdin-text",
            file_path.to_str().expect("path should be utf-8"),
        ],
        "unused",
    );

    assert!(
        !output.status.success(),
        "config delete should reject unused stdin source"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let message = response["error"]["message"]
        .as_str()
        .expect("error message should be present");
    assert!(
        message.contains("--text-file") && message.contains("--stdin-text"),
        "error should explain unused external source, got: {message}"
    );
}

#[test]
fn patch_scoped_regex_text_file_path_with_spaces_applies() {
    let file_path = create_scoped_regex_fixture();
    let handle = select_named_function_handle(&file_path, "process_*");
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");
    let payload_dir = Builder::new()
        .prefix("identedit scoped payload ")
        .tempdir()
        .expect("temp dir should be created");
    let payload_path = payload_dir.path().join("replacement text.txt");
    fs::write(&payload_path, "payload").expect("payload file should be written");

    let output = run_identedit(&[
        "patch",
        "--identity",
        identity,
        "--scoped-regex",
        "value",
        "--scoped-replacement",
        "--text-file",
        payload_path.to_str().expect("payload path should be utf-8"),
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "scoped regex text-file path with spaces should work: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    assert!(modified.contains("def process_data(payload):"));
    assert!(modified.contains("return payload + 1"));
}

#[test]
fn patch_set_line_stdin_utf8_bom_payload_preserves_bytes() {
    let file_path = create_temp_text_file("alpha\nbeta\n");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let anchor = line_ref(&before, 2);

    let output = run_identedit_with_stdin(
        &[
            "patch",
            "--anchor",
            anchor.as_str(),
            "--set-line",
            "--stdin-text",
            file_path.to_str().expect("path should be utf-8"),
        ],
        "\u{FEFF}beta",
    );

    assert!(
        output.status.success(),
        "set-line with BOM stdin payload failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    assert_eq!(modified, "alpha\n\u{FEFF}beta\n");
}

#[test]
fn patch_config_set_value_stdin_json_string_with_leading_dash_applies() {
    let file_path = copy_fixture_to_temp_json("example.json");

    let output = run_identedit_with_stdin(
        &[
            "patch",
            "--config-path",
            "name",
            "--set-value",
            "--stdin-text",
            file_path.to_str().expect("path should be utf-8"),
        ],
        "\"--help\"",
    );

    assert!(
        output.status.success(),
        "JSON string payload with leading dash should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let modified: Value =
        serde_json::from_str(&fs::read_to_string(&file_path).expect("modified file should read"))
            .expect("modified JSON should parse");
    assert_eq!(modified["name"], "--help");
}

#[test]
fn patch_symbol_replaces_unique_symbol_without_kind_name_flags() {
    let file_path = copy_fixture_to_temp_python("example.py");

    let output = run_identedit(&[
        "patch",
        "--symbol",
        "helper",
        "--replace",
        "def helper():\n    return \"patched\"",
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "symbol patch should infer the unique helper node: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    assert!(modified.contains("return \"patched\""));
    assert!(modified.contains("def process_data(value):"));
}

#[test]
fn patch_symbol_qualified_name_targets_nested_method() {
    let source = "def process_data(value):\n    return value + 1\n\n\nclass Processor:\n    def process_data(self, value):\n        return value + 2\n\n\nclass Other:\n    def process_data(self, value):\n        return value + 3\n";
    let file_path = create_temp_python_source(source);

    let output = run_identedit(&[
        "patch",
        "--symbol",
        "Processor.process_data",
        "--replace",
        "def process_data(self, value):\n        return value * 7",
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "qualified symbol patch should target exactly one method: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    assert!(modified.contains(
        "class Processor:\n    def process_data(self, value):\n        return value * 7"
    ));
    assert!(modified.contains("def process_data(value):\n    return value + 1"));
    assert!(
        modified
            .contains("class Other:\n    def process_data(self, value):\n        return value + 3")
    );
}

#[test]
fn patch_symbol_unqualified_duplicate_reports_ambiguous_target() {
    let source = "def process_data(value):\n    return value + 1\n\n\nclass Processor:\n    def process_data(self, value):\n        return value + 2\n";
    let file_path = create_temp_python_source(source);

    let output = run_identedit(&[
        "patch",
        "--symbol",
        "process_data",
        "--replace",
        "def process_data(value):\n    return 0",
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        !output.status.success(),
        "unqualified duplicate symbol should fail instead of guessing"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "ambiguous_target");
    let message = response["error"]["message"]
        .as_str()
        .expect("error message should be present");
    assert!(message.contains("symbol='process_data'"));
    let candidates = response["error"]["candidates"]
        .as_array()
        .expect("ambiguous symbol response should include candidate contexts");
    assert_eq!(candidates.len(), 2);
    assert_eq!(candidates[0]["name"], "process_data");
    assert_eq!(candidates[0]["qualified_name"], "process_data");
    assert_eq!(candidates[0]["line"], 1);
    assert_eq!(candidates[0]["preview"], "def process_data(value):");
    assert_eq!(candidates[1]["name"], "process_data");
    assert_eq!(candidates[1]["qualified_name"], "Processor.process_data");
    assert_eq!(candidates[1]["line"], 6);
    assert_eq!(candidates[1]["preview"], "def process_data(self, value):");
    assert!(candidates.iter().all(|candidate| {
        candidate["identity"]
            .as_str()
            .is_some_and(|value| value.len() == 16)
    }));
    assert_eq!(
        fs::read_to_string(&file_path).expect("file should be readable"),
        source,
        "ambiguous symbol patch must not mutate the source"
    );
}

#[test]
fn patch_symbol_duplicate_qualified_name_reports_ambiguous_target() {
    let source = "class Processor:\n    def process_data(self, value):\n        return value + 2\n\n\nclass Processor:\n    def process_data(self, value):\n        return value + 3\n";
    let file_path = create_temp_python_source(source);

    let output = run_identedit(&[
        "patch",
        "--symbol",
        "Processor.process_data",
        "--replace",
        "def process_data(self, value):\n        return 0",
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        !output.status.success(),
        "duplicate qualified symbol should fail instead of choosing the first match"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "ambiguous_target");
    let message = response["error"]["message"]
        .as_str()
        .expect("error message should be present");
    assert!(message.contains("symbol='Processor.process_data'"));
    let candidates = response["error"]["candidates"]
        .as_array()
        .expect("ambiguous qualified symbol response should include candidate contexts");
    assert_eq!(candidates.len(), 2);
    assert!(
        candidates
            .iter()
            .all(|candidate| candidate["qualified_name"] == "Processor.process_data")
    );
    assert_eq!(candidates[0]["line"], 2);
    assert_eq!(candidates[1]["line"], 7);
    assert_eq!(
        fs::read_to_string(&file_path).expect("file should be readable"),
        source,
        "ambiguous qualified symbol patch must not mutate the source"
    );
}

#[test]
fn patch_symbol_missing_symbol_reports_target_missing_without_mutation() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let before = fs::read_to_string(&file_path).expect("file should be readable");

    let output = run_identedit(&[
        "patch",
        "--symbol",
        "Processor.process_data",
        "--replace",
        "def process_data(self, value):\n        return 0",
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        !output.status.success(),
        "missing symbol should return target_missing"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "target_missing");
    let message = response["error"]["message"]
        .as_str()
        .expect("error message should be present");
    assert!(message.contains("symbol='Processor.process_data'"));
    assert_eq!(
        fs::read_to_string(&file_path).expect("file should be readable"),
        before,
        "missing symbol patch must not mutate the source"
    );
}

#[test]
fn patch_symbol_rejects_mixed_kind_name_selector() {
    let file_path = copy_fixture_to_temp_python("example.py");

    let output = run_identedit(&[
        "patch",
        "--symbol",
        "helper",
        "--kind",
        "function_definition",
        "--name",
        "helper",
        "--replace",
        "def helper():\n    return \"patched\"",
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        !output.status.success(),
        "symbol selector should not mix with kind/name selector"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let message = response["error"]["message"]
        .as_str()
        .expect("error message should be present");
    assert!(message.contains("--symbol") && message.contains("--kind"));
}

#[test]
fn patch_symbol_rejects_empty_symbol() {
    let file_path = copy_fixture_to_temp_python("example.py");

    let output = run_identedit(&[
        "patch",
        "--symbol",
        "   ",
        "--replace",
        "def helper():\n    return \"patched\"",
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        !output.status.success(),
        "empty symbol selector should fail"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let message = response["error"]["message"]
        .as_str()
        .expect("error message should be present");
    assert!(message.contains("--symbol") && message.contains("empty"));
}

#[test]
fn patch_kind_name_replace_dry_run_previews_without_writing() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let before = fs::read_to_string(&file_path).expect("file should be readable");
    let replacement = "def process_data(value):\n    return value * 12";

    let output = run_identedit(&[
        "patch",
        "--kind",
        "function_definition",
        "--name",
        "process_*",
        "--replace",
        replacement,
        "--dry-run",
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "kind/name patch dry-run failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["transaction"]["status"], "dry_run");
    assert_eq!(response["summary"]["operations_applied"], 1);

    let after = fs::read_to_string(&file_path).expect("file should be readable");
    assert_eq!(before, after, "dry-run must not modify the source file");
}

#[test]
fn patch_kind_name_replace_dry_run_diff_outputs_unified_diff_without_writing() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let before = fs::read_to_string(&file_path).expect("file should be readable");

    let output = run_identedit(&[
        "patch",
        "--kind",
        "function_definition",
        "--name",
        "process_*",
        "--replace",
        "def process_data(value):\n    return value * 13",
        "--dry-run",
        "--diff",
        "--color",
        "never",
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "kind/name patch dry-run diff failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let diff = String::from_utf8(output.stdout).expect("stdout should be utf-8");
    assert!(diff.contains("--- "));
    assert!(diff.contains("+++ "));
    assert!(diff.contains("@@ -1,3 +1,2 @@"));
    assert!(diff.contains("-def process_data(value):"));
    assert!(diff.contains("-    result = value + 1"));
    assert!(diff.contains("+def process_data(value):"));
    assert!(diff.contains("+    return value * 13"));
    assert!(
        serde_json::from_str::<Value>(&diff).is_err(),
        "diff output must not be JSON"
    );
    assert_eq!(
        fs::read_to_string(&file_path).expect("file should be readable"),
        before,
        "dry-run diff must not modify the source file"
    );
}

#[test]
fn patch_kind_name_replace_dry_run_diff_can_force_color() {
    let file_path = copy_fixture_to_temp_python("example.py");

    let output = run_identedit(&[
        "patch",
        "--kind",
        "function_definition",
        "--name",
        "process_*",
        "--replace",
        "def process_data(value):\n    return value * 17",
        "--dry-run",
        "--diff",
        "--color",
        "always",
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "colored dry-run diff failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let diff = String::from_utf8(output.stdout).expect("stdout should be utf-8");
    assert!(diff.contains("\u{1b}[31m-"));
    assert!(diff.contains("\u{1b}[32m+"));
    assert!(diff.contains("\u{1b}[36m@@"));
}

#[test]
fn patch_diff_without_dry_run_reports_invalid_request() {
    let file_path = copy_fixture_to_temp_python("example.py");

    let output = run_identedit(&[
        "patch",
        "--at",
        "file-end",
        "--insert",
        "\n# epilogue\n",
        "--diff",
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        !output.status.success(),
        "--diff without --dry-run should fail"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let message = response["error"]["message"]
        .as_str()
        .expect("error message should be present");
    assert!(
        message.contains("--diff") && message.contains("--dry-run"),
        "error should explain that diff output is dry-run only"
    );
}

#[test]
fn patch_color_without_diff_reports_invalid_request() {
    let file_path = copy_fixture_to_temp_python("example.py");

    let output = run_identedit(&[
        "patch",
        "--at",
        "file-end",
        "--insert",
        "\n# epilogue\n",
        "--dry-run",
        "--color",
        "never",
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        !output.status.success(),
        "--color without --diff should fail"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let message = response["error"]["message"]
        .as_str()
        .expect("error message should be present");
    assert!(
        message.contains("--color") && message.contains("--diff"),
        "error should explain that color only affects diff output"
    );
}

#[test]
fn patch_json_mode_rejects_diff_output() {
    let output = run_identedit_with_stdin(&["patch", "--json", "--diff"], "{}");

    assert!(
        !output.status.success(),
        "--json --diff should fail before interpreting stdin"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let message = response["error"]["message"]
        .as_str()
        .expect("error message should be present");
    assert!(
        message.contains("--diff") && message.contains("JSON"),
        "error should explain that JSON mode always returns JSON"
    );
}

#[test]
fn patch_kind_name_requires_both_flags() {
    let file_path = copy_fixture_to_temp_python("example.py");

    let output = run_identedit(&[
        "patch",
        "--kind",
        "function_definition",
        "--replace",
        "def process_data(value):\n    return value * 2",
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        !output.status.success(),
        "patch should reject selector mode when --name is missing"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let message = response["error"]["message"]
        .as_str()
        .expect("error message should be present");
    assert!(
        message.contains("--kind") && message.contains("--name") && message.contains("Example"),
        "selector mode error should mention both required flags and show the direct fix"
    );
}

#[test]
fn patch_kind_name_reports_target_missing_for_unmatched_symbol() {
    let file_path = copy_fixture_to_temp_python("example.py");

    let output = run_identedit(&[
        "patch",
        "--kind",
        "function_definition",
        "--name",
        "does_not_exist",
        "--replace",
        "def does_not_exist():\n    return 0",
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        !output.status.success(),
        "patch should fail when kind/name selector matches no symbol"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "target_missing");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| {
                message.contains("function_definition") && message.contains("does_not_exist")
            }),
        "target-missing message should describe the selector"
    );
}

#[test]
fn patch_kind_name_reports_ambiguous_target_for_duplicate_symbol() {
    let file_path = copy_fixture_to_temp_python("ambiguous.py");

    let output = run_identedit(&[
        "patch",
        "--kind",
        "function_definition",
        "--name",
        "duplicate",
        "--replace",
        "def duplicate():\n    return 2",
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        !output.status.success(),
        "patch should fail when kind/name selector matches multiple symbols"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "ambiguous_target");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| {
                message.contains("function_definition") && message.contains("duplicate")
            }),
        "ambiguous-target message should describe the selector"
    );
    let candidates = response["error"]["candidates"]
        .as_array()
        .expect("ambiguous kind/name response should include candidate contexts");
    assert_eq!(candidates.len(), 2);
    assert_eq!(candidates[0]["kind"], "function_definition");
    assert_eq!(candidates[0]["name"], "duplicate");
    assert_eq!(candidates[0]["qualified_name"], "duplicate");
    assert_eq!(candidates[0]["line"], 1);
    assert_eq!(candidates[0]["preview"], "def duplicate():");
    assert_eq!(candidates[1]["line"], 5);
}

#[test]
fn patch_kind_name_rejects_mixed_with_identity_target() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_function_handle(&file_path, "process_*");
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");

    let output = run_identedit(&[
        "patch",
        "--identity",
        identity,
        "--kind",
        "function_definition",
        "--name",
        "process_*",
        "--replace",
        "def process_data(value):\n    return value * 13",
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        !output.status.success(),
        "patch should reject mixing selector targeting with identity targeting"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let message = response["error"]["message"]
        .as_str()
        .expect("error message should be present");
    assert!(
        message.contains("Choose exactly one target selector")
            && message.contains("--identity")
            && message.contains("--kind"),
        "mixed target error should explain the valid selector families"
    );
}

#[test]
fn patch_kind_name_rejects_mixed_with_at_target() {
    let file_path = copy_fixture_to_temp_python("example.py");

    let output = run_identedit(&[
        "patch",
        "--at",
        "file-end",
        "--kind",
        "function_definition",
        "--name",
        "process_*",
        "--replace",
        "def process_data(value):\n    return value * 13",
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        !output.status.success(),
        "patch should reject mixing selector targeting with --at"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}

#[test]
fn patch_kind_name_scoped_regex_rewrites_only_selected_symbol() {
    let file_path = create_scoped_regex_fixture();

    let output = run_identedit(&[
        "patch",
        "--kind",
        "function_definition",
        "--name",
        "process_*",
        "--scoped-regex",
        "value",
        "--scoped-replacement",
        "item",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "selector scoped regex failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["summary"]["operations_applied"], 1);
    assert_eq!(response["regex_replacements"], 2);

    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    assert!(modified.contains("def process_data(item):"));
    assert!(modified.contains("return item + 1"));
    assert!(
        modified.contains("def helper(value):\n    return value + 2"),
        "selector scoped regex must not rewrite outside selected target span"
    );
}

#[test]
fn patch_kind_name_scoped_regex_dry_run_does_not_modify_file() {
    let file_path = create_scoped_regex_fixture();
    let before = fs::read_to_string(&file_path).expect("file should be readable");

    let output = run_identedit(&[
        "patch",
        "--kind",
        "function_definition",
        "--name",
        "process_*",
        "--scoped-regex",
        "value",
        "--scoped-replacement",
        "item",
        "--dry-run",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "selector scoped regex dry-run failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["transaction"]["status"], "dry_run");
    assert_eq!(response["summary"]["operations_applied"], 1);
    assert_eq!(
        fs::read_to_string(&file_path).expect("file should be readable"),
        before,
        "dry-run must not modify source text"
    );
}

#[test]
fn patch_kind_name_invalid_glob_reports_invalid_selector() {
    let file_path = copy_fixture_to_temp_python("example.py");

    let output = run_identedit(&[
        "patch",
        "--kind",
        "function_definition",
        "--name",
        "[",
        "--replace",
        "def process_data(value):\n    return value * 2",
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        !output.status.success(),
        "patch should reject invalid selector glob"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_selector");
}

#[test]
fn patch_kind_name_empty_kind_reports_invalid_request() {
    let file_path = copy_fixture_to_temp_python("example.py");

    let output = run_identedit(&[
        "patch",
        "--kind",
        "",
        "--name",
        "process_*",
        "--replace",
        "def process_data(value):\n    return value * 2",
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        !output.status.success(),
        "empty selector kind should be rejected"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}

#[test]
fn patch_scoped_regex_flag_mode_rewrites_only_inside_target_span() {
    let file_path = create_scoped_regex_fixture();
    let handle = select_named_function_handle(&file_path, "process_*");
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");

    let output = run_identedit(&[
        "patch",
        "--identity",
        identity,
        "--scoped-regex",
        "value",
        "--scoped-replacement",
        "item",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "patch scoped regex failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["summary"]["operations_applied"], 1);
    assert_eq!(response["regex_replacements"], 2);

    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    assert!(modified.contains("def process_data(item):"));
    assert!(modified.contains("return item + 1"));
    assert!(
        modified.contains("def helper(value):\n    return value + 2"),
        "scoped regex must not rewrite outside selected target span"
    );
}

#[test]
fn patch_scoped_regex_flag_mode_rejects_zero_matches() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_function_handle(&file_path, "process_*");
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");

    let output = run_identedit(&[
        "patch",
        "--identity",
        identity,
        "--scoped-regex",
        "does_not_exist",
        "--scoped-replacement",
        "x",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "patch scoped regex should fail when pattern has zero matches"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("matched 0 occurrences")),
        "expected deterministic zero-match diagnostic"
    );
}

#[test]
fn patch_delete_removes_target_node() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_function_handle(&file_path, "helper");
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");

    let output = run_identedit(&[
        "patch",
        "--identity",
        identity,
        "--delete",
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "patch delete failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["summary"]["operations_applied"], 1);

    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    assert!(
        !modified.contains("def helper():"),
        "target function should be deleted"
    );
}

#[test]
fn patch_insert_before_writes_at_anchor_start() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_function_handle(&file_path, "helper");
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");

    let output = run_identedit(&[
        "patch",
        "--identity",
        identity,
        "--insert-before",
        "# inserted-before-helper\n",
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "patch insert-before failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    assert!(
        modified.contains("# inserted-before-helper\ndef helper():"),
        "insert-before text should appear immediately before helper definition"
    );
}

#[test]
fn patch_insert_after_writes_at_anchor_end() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_function_handle(&file_path, "helper");
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");

    let output = run_identedit(&[
        "patch",
        "--identity",
        identity,
        "--insert-after",
        "\n# inserted-after-helper\n",
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "patch insert-after failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    let helper_offset = modified
        .find("def helper():")
        .expect("helper function should still exist");
    let marker_offset = modified
        .find("# inserted-after-helper")
        .expect("insert-after marker should exist");
    assert!(
        marker_offset > helper_offset,
        "insert-after marker should appear after helper definition"
    );
}

#[test]
fn patch_rejects_multiple_operations_in_single_request() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_function_handle(&file_path, "process_*");
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");

    let output = run_identedit(&[
        "patch",
        "--identity",
        identity,
        "--replace",
        "def process_data(value):\n    return value + 1",
        "--delete",
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        !output.status.success(),
        "patch should reject multiple operations"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}

#[test]
fn patch_returns_ambiguous_target_error_for_duplicate_identity() {
    let file_path = copy_fixture_to_temp_python("ambiguous.py");
    let handle = select_named_function_handle(&file_path, "duplicate");
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");

    let output = run_identedit(&[
        "patch",
        "--identity",
        identity,
        "--replace",
        "def duplicate():\n    return 2",
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        !output.status.success(),
        "patch should fail when identity is ambiguous"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "ambiguous_target");
    let candidates = response["error"]["candidates"]
        .as_array()
        .expect("ambiguous identity response should include candidate contexts");
    assert_eq!(candidates.len(), 2);
    assert!(
        candidates
            .iter()
            .all(|candidate| candidate["identity"] == identity)
    );
    assert_eq!(candidates[0]["line"], 1);
    assert_eq!(candidates[1]["line"], 5);
}

#[test]
fn patch_verbose_includes_applied_file_results() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_function_handle(&file_path, "process_*");
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");

    let output = run_identedit(&[
        "patch",
        "--identity",
        identity,
        "--replace",
        "def process_data(value):\n    return value * 5",
        "--verbose",
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "patch verbose failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["summary"]["operations_applied"], 1);
    let applied = response["applied"]
        .as_array()
        .expect("verbose patch response should include applied array");
    assert_eq!(applied.len(), 1);
    assert_eq!(applied[0]["operations_applied"], 1);
}

#[test]
fn patch_without_operation_flag_returns_invalid_request() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_function_handle(&file_path, "process_*");
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");

    let output = run_identedit(&[
        "patch",
        "--identity",
        identity,
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        !output.status.success(),
        "patch should reject requests with no operation selected"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}

#[test]
fn patch_ambiguous_target_failure_keeps_source_file_unchanged() {
    let file_path = copy_fixture_to_temp_python("ambiguous.py");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let handle = select_named_function_handle(&file_path, "duplicate");
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");

    let output = run_identedit(&[
        "patch",
        "--identity",
        identity,
        "--replace",
        "def duplicate():\n    return 999",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "patch should fail for ambiguous identity"
    );

    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(
        before, after,
        "source file must remain unchanged when patch fails"
    );
}

#[test]
fn patch_reports_io_error_for_missing_file() {
    let output = run_identedit(&[
        "patch",
        "--identity",
        "deadbeefdeadbeef",
        "--replace",
        "def process_data(value):\n    return value",
        "/tmp/identedit-missing-file-should-not-exist.py",
    ]);
    assert!(
        !output.status.success(),
        "patch should fail for missing file path"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "io_error");
}

#[test]
fn patch_insert_before_preserves_utf8_bom() {
    let mut temp_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp python file should be created");
    temp_file
        .write_all(
            b"\xEF\xBB\xBFdef process_data(value):\n    return value + 1\n\ndef helper():\n    return value + 2\n",
        )
        .expect("bom fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let handle = select_named_function_handle(&file_path, "helper");
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");

    let output = run_identedit(&[
        "patch",
        "--identity",
        identity,
        "--insert-before",
        "# before helper\n",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "patch insert-before should support BOM files: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let bytes = fs::read(&file_path).expect("modified file should be readable");
    assert!(
        bytes.starts_with(&[0xEF, 0xBB, 0xBF]),
        "UTF-8 BOM prefix must remain intact after patch"
    );
}

#[test]
fn patch_replace_supports_crlf_files() {
    let mut temp_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp python file should be created");
    let source = "def process_data(value):\r\n    return value + 1\r\n\r\ndef helper():\r\n    return value + 2\r\n";
    temp_file
        .write_all(source.as_bytes())
        .expect("crlf fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let handle = select_named_function_handle(&file_path, "process_*");
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");

    let output = run_identedit(&[
        "patch",
        "--identity",
        identity,
        "--replace",
        "def process_data(value):\r\n    return value * 10\r\n",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "patch replace should support CRLF source: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    assert!(
        modified.contains("return value * 10\r\n"),
        "replacement should preserve CRLF sequence"
    );
    assert!(
        modified.contains("def helper():\r\n"),
        "non-target sections should keep CRLF endings"
    );
}

#[test]
fn patch_file_start_dry_run_does_not_modify_file() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let before = fs::read_to_string(&file_path).expect("file should be readable");

    let output = run_identedit(&[
        "patch",
        "--at",
        "file-start",
        "--insert",
        "# preamble\n",
        "--dry-run",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "file-start dry-run should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["transaction"]["status"], "dry_run");
    assert_eq!(
        fs::read_to_string(&file_path).expect("file should be readable"),
        before,
        "file-start dry-run must not modify the file"
    );
}

#[test]
fn patch_file_target_rejects_node_operation_with_actionable_message() {
    let file_path = copy_fixture_to_temp_python("example.py");

    let output = run_identedit(&[
        "patch",
        "--at",
        "file-end",
        "--replace",
        "x",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "file target should reject node operations"
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let message = response["error"]["message"]
        .as_str()
        .expect("error message should be present");
    assert!(
        message.contains("--insert")
            && message.contains("file-start")
            && message.contains("file-end"),
        "file-mode error should explain that file targets only support insert"
    );
}

#[test]
fn patch_file_end_dry_run_does_not_modify_file() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let before = fs::read_to_string(&file_path).expect("file should be readable");

    let output = run_identedit(&[
        "patch",
        "--at",
        "file-end",
        "--insert",
        "\n# epilogue\n",
        "--dry-run",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "file-end dry-run should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["transaction"]["status"], "dry_run");
    assert_eq!(
        fs::read_to_string(&file_path).expect("file should be readable"),
        before,
        "file-end dry-run must not modify the file"
    );
}

#[test]
fn patch_file_end_dry_run_diff_outputs_insert_preview_without_writing() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let before = fs::read_to_string(&file_path).expect("file should be readable");

    let output = run_identedit(&[
        "patch",
        "--at",
        "file-end",
        "--insert",
        "\n# epilogue\n",
        "--dry-run",
        "--diff",
        "--color",
        "never",
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "file-end dry-run diff should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let diff = String::from_utf8(output.stdout).expect("stdout should be utf-8");
    assert!(diff.contains("@@ -8,0 +8,2 @@"));
    assert!(diff.contains("+# epilogue"));
    assert_eq!(
        fs::read_to_string(&file_path).expect("file should be readable"),
        before,
        "file-end dry-run diff must not modify the file"
    );
}

#[test]
fn patch_line_flag_set_line_applies_change() {
    let source = "a\nb\n";
    let mut temp_file = Builder::new()
        .suffix(".txt")
        .tempfile()
        .expect("temp text file should be created");
    temp_file
        .write_all(source.as_bytes())
        .expect("fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let anchor = line_ref(source, 2);

    let output = run_identedit(&[
        "patch",
        "--anchor",
        &anchor,
        "--set-line",
        "B",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "patch line flag set-line failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["applied_mode"], "strict");
    assert_eq!(response["operations_applied"], 1);

    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    assert_eq!(modified, "a\nB\n");
}

#[test]
fn patch_line_flag_set_line_dry_run_previews_without_writing() {
    let source = "a\nb\n";
    let mut temp_file = Builder::new()
        .suffix(".txt")
        .tempfile()
        .expect("temp text file should be created");
    temp_file
        .write_all(source.as_bytes())
        .expect("fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let anchor = line_ref(source, 2);

    let output = run_identedit(&[
        "patch",
        "--anchor",
        &anchor,
        "--set-line",
        "B",
        "--dry-run",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "patch line flag dry-run failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["dry_run"], true);
    assert_eq!(response["operations_applied"], 1);
    assert_eq!(response["changed"], true);

    let after = fs::read_to_string(&file_path).expect("file should be readable");
    assert_eq!(after, source, "line-mode dry-run must not modify the file");
}

#[test]
fn patch_line_flag_set_line_dry_run_diff_outputs_file_diff_without_writing() {
    let source = "a\nb\n";
    let mut temp_file = Builder::new()
        .suffix(".txt")
        .tempfile()
        .expect("temp text file should be created");
    temp_file
        .write_all(source.as_bytes())
        .expect("fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let anchor = line_ref(source, 2);

    let output = run_identedit(&[
        "patch",
        "--anchor",
        &anchor,
        "--set-line",
        "B",
        "--dry-run",
        "--diff",
        "--color",
        "never",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "line dry-run diff failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let diff = String::from_utf8(output.stdout).expect("stdout should be utf-8");
    assert!(diff.contains("@@ -1,2 +1,2 @@"));
    assert!(diff.contains("-b"));
    assert!(diff.contains("+B"));
    assert_eq!(
        fs::read_to_string(&file_path).expect("file should be readable"),
        source,
        "line dry-run diff must not modify the file"
    );
}

#[test]
fn patch_line_flag_replace_range_supports_end_anchor() {
    let source = "a\nb\nc\n";
    let mut temp_file = Builder::new()
        .suffix(".txt")
        .tempfile()
        .expect("temp text file should be created");
    temp_file
        .write_all(source.as_bytes())
        .expect("fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let anchor = line_ref(source, 2);
    let end_anchor = line_ref(source, 3);

    let output = run_identedit(&[
        "patch",
        "--anchor",
        &anchor,
        "--end-anchor",
        &end_anchor,
        "--replace-range",
        "x\ny",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "patch line flag replace-range failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    assert_eq!(modified, "a\nx\ny\n");
}

#[test]
fn patch_line_flag_insert_after_line_applies_change() {
    let source = "a\nb\n";
    let mut temp_file = Builder::new()
        .suffix(".txt")
        .tempfile()
        .expect("temp text file should be created");
    temp_file
        .write_all(source.as_bytes())
        .expect("fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let anchor = line_ref(source, 1);

    let output = run_identedit(&[
        "patch",
        "--anchor",
        &anchor,
        "--insert-after-line",
        "x",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "patch line flag insert-after-line failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    assert_eq!(modified, "a\nx\nb\n");
}

#[test]
fn patch_line_flag_supports_auto_repair() {
    let source = "a\nb\na\n";
    let mut temp_file = Builder::new()
        .suffix(".txt")
        .tempfile()
        .expect("temp text file should be created");
    temp_file
        .write_all(source.as_bytes())
        .expect("fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let stale_anchor = format!("1:{}", identedit::hashline::compute_line_hash("b"));

    let output = run_identedit(&[
        "patch",
        "--anchor",
        &stale_anchor,
        "--set-line",
        "B",
        "--auto-repair",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "patch line flag auto-repair failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["applied_mode"], "repair");
    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    assert_eq!(modified, "a\nB\na\n");
}

#[test]
fn patch_line_flag_auto_repair_dry_run_does_not_modify_file() {
    let source = "a\nb\na\n";
    let mut temp_file = Builder::new()
        .suffix(".txt")
        .tempfile()
        .expect("temp text file should be created");
    temp_file
        .write_all(source.as_bytes())
        .expect("fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let stale_anchor = format!("1:{}", identedit::hashline::compute_line_hash("b"));

    let output = run_identedit(&[
        "patch",
        "--anchor",
        &stale_anchor,
        "--set-line",
        "B",
        "--auto-repair",
        "--dry-run",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "patch line flag auto-repair dry-run failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["applied_mode"], "repair");
    assert_eq!(response["dry_run"], true);
    assert_eq!(
        fs::read_to_string(&file_path).expect("file should be readable"),
        source,
        "dry-run repair must not modify the file"
    );
}

#[test]
fn patch_flag_rejects_identity_and_anchor_together() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_function_handle(&file_path, "process_*");
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");
    let anchor = line_ref("a\nb\n", 1);

    let output = run_identedit(&[
        "patch",
        "--identity",
        identity,
        "--anchor",
        &anchor,
        "--replace",
        "def process_data(value):\n    return value * 9",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "patch should reject mixed target selection"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let message = response["error"]["message"]
        .as_str()
        .expect("error message should be present");
    assert!(
        message.contains("Choose exactly one target selector")
            && message.contains("--identity")
            && message.contains("--anchor"),
        "mixed target error should list the valid selector families"
    );
}

#[test]
fn patch_flag_rejects_line_target_with_node_operation() {
    let source = "a\nb\n";
    let mut temp_file = Builder::new()
        .suffix(".txt")
        .tempfile()
        .expect("temp text file should be created");
    temp_file
        .write_all(source.as_bytes())
        .expect("fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let anchor = line_ref(source, 1);

    let output = run_identedit(&[
        "patch",
        "--anchor",
        &anchor,
        "--replace",
        "x",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "line target should reject node operation flags"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let message = response["error"]["message"]
        .as_str()
        .expect("error message should be present");
    assert!(
        message.contains("--set-line")
            && message.contains("--replace-range")
            && message.contains("--insert-after-line")
            && message.contains("--identity"),
        "line-mode error should list valid line flags and point back to node targeting"
    );
}

#[test]
fn patch_flag_rejects_node_target_with_line_operation() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_function_handle(&file_path, "process_*");
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");

    let output = run_identedit(&[
        "patch",
        "--identity",
        identity,
        "--set-line",
        "x",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "node target should reject line operation flags"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let message = response["error"]["message"]
        .as_str()
        .expect("error message should be present");
    assert!(
        message.contains("--replace")
            && message.contains("--delete")
            && message.contains("--insert-before")
            && message.contains("--anchor"),
        "node-mode error should list valid node flags and point to line targeting"
    );
}

#[test]
fn patch_flag_rejects_end_anchor_without_replace_range() {
    let source = "a\nb\n";
    let mut temp_file = Builder::new()
        .suffix(".txt")
        .tempfile()
        .expect("temp text file should be created");
    temp_file
        .write_all(source.as_bytes())
        .expect("fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let anchor = line_ref(source, 1);
    let end_anchor = line_ref(source, 2);

    let output = run_identedit(&[
        "patch",
        "--anchor",
        &anchor,
        "--end-anchor",
        &end_anchor,
        "--set-line",
        "x",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "--end-anchor should be rejected when --replace-range is not selected"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}

#[test]
fn patch_flag_rejects_multiple_line_operations() {
    let source = "a\nb\n";
    let mut temp_file = Builder::new()
        .suffix(".txt")
        .tempfile()
        .expect("temp text file should be created");
    temp_file
        .write_all(source.as_bytes())
        .expect("fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let anchor = line_ref(source, 1);

    let output = run_identedit(&[
        "patch",
        "--anchor",
        &anchor,
        "--set-line",
        "x",
        "--replace-range",
        "y",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "patch should reject multiple line operations in one request"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}

#[test]
fn patch_json_node_target_replace_applies_change() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_function_handle(&file_path, "process_*");
    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "node",
            "identity": handle["identity"],
            "kind": handle["kind"],
            "span_hint": {
                "start": handle["span"]["start"],
                "end": handle["span"]["end"]
            },
            "expected_old_hash": identedit::changeset::hash_text(
                handle["text"].as_str().expect("text should be string")
            )
        },
        "op": {
            "type": "replace",
            "new_text": "def process_data(value):\n    return value * 11"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "patch --json node replace failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["summary"]["operations_applied"], 1);

    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    assert!(modified.contains("return value * 11"));
}

#[test]
fn patch_json_node_target_replace_options_dry_run_does_not_modify_file() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let before = fs::read_to_string(&file_path).expect("file should be readable");
    let handle = select_named_function_handle(&file_path, "process_*");
    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "node",
            "identity": handle["identity"],
            "kind": handle["kind"],
            "span_hint": handle["span"],
            "expected_old_hash": handle["expected_old_hash"]
        },
        "op": {
            "type": "replace",
            "new_text": "def process_data(value):\n    return value * 17"
        },
        "options": {
            "dry_run": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "json node dry-run should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["transaction"]["status"], "dry_run");
    assert_eq!(
        fs::read_to_string(&file_path).expect("file should be readable"),
        before,
        "json node dry-run must not modify the file"
    );
}

#[test]
fn patch_json_cli_dry_run_overrides_node_request_and_does_not_modify_file() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let before = fs::read_to_string(&file_path).expect("file should be readable");
    let handle = select_named_function_handle(&file_path, "process_*");
    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "node",
            "identity": handle["identity"],
            "kind": handle["kind"],
            "span_hint": handle["span"],
            "expected_old_hash": handle["expected_old_hash"]
        },
        "op": {
            "type": "replace",
            "new_text": "def process_data(value):\n    return value * 19"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json", "--dry-run"], &request.to_string());
    assert!(
        output.status.success(),
        "json cli dry-run should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["transaction"]["status"], "dry_run");
    assert_eq!(
        fs::read_to_string(&file_path).expect("file should be readable"),
        before,
        "cli dry-run in json mode must not modify node target files"
    );
}

#[test]
fn patch_json_node_target_scoped_regex_applies_change_and_reports_count() {
    let file_path = create_scoped_regex_fixture();
    let handle = select_named_function_handle(&file_path, "process_*");
    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "node",
            "identity": handle["identity"],
            "kind": handle["kind"],
            "span_hint": handle["span"],
            "expected_old_hash": identedit::changeset::hash_text(
                handle["text"].as_str().expect("text should be string")
            )
        },
        "op": {
            "type": "scoped_regex",
            "pattern": "value",
            "replacement": "item"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "patch --json scoped regex failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["summary"]["operations_applied"], 1);
    assert_eq!(response["regex_replacements"], 2);

    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    assert!(modified.contains("def process_data(item):"));
    assert!(modified.contains("return item + 1"));
    assert!(
        modified.contains("def helper(value):\n    return value + 2"),
        "scoped regex must not rewrite outside selected target span"
    );
}

#[test]
fn patch_json_node_target_scoped_regex_rejects_invalid_pattern() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_function_handle(&file_path, "process_*");
    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "node",
            "identity": handle["identity"],
            "kind": handle["kind"],
            "span_hint": handle["span"],
            "expected_old_hash": identedit::changeset::hash_text(
                handle["text"].as_str().expect("text should be string")
            )
        },
        "op": {
            "type": "scoped_regex",
            "pattern": "(unterminated",
            "replacement": "x"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "invalid scoped regex pattern must be rejected"
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("Invalid scoped regex pattern")),
        "expected deterministic invalid-pattern diagnostic"
    );
}

#[test]
fn patch_json_node_target_scoped_regex_rejects_zero_matches() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_function_handle(&file_path, "process_*");
    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "node",
            "identity": handle["identity"],
            "kind": handle["kind"],
            "span_hint": handle["span"],
            "expected_old_hash": identedit::changeset::hash_text(
                handle["text"].as_str().expect("text should be string")
            )
        },
        "op": {
            "type": "scoped_regex",
            "pattern": "does_not_exist",
            "replacement": "x"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "scoped regex should fail when pattern has zero matches"
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("matched 0 occurrences")),
        "expected deterministic zero-match diagnostic"
    );
}

#[test]
fn patch_json_node_target_scoped_regex_preserves_stale_precondition_behavior() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_function_handle(&file_path, "process_*");
    fs::write(
        &file_path,
        "def process_data(value):\n    return value + 100\n\n\ndef helper():\n    return \"helper\"\n",
    )
    .expect("fixture mutation should succeed");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "node",
            "identity": handle["identity"],
            "kind": handle["kind"],
            "span_hint": handle["span"],
            "expected_old_hash": identedit::changeset::hash_text(
                handle["text"].as_str().expect("text should be string")
            )
        },
        "op": {
            "type": "scoped_regex",
            "pattern": "value",
            "replacement": "item"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "scoped regex should preserve stale precondition behavior"
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    let error_type = response["error"]["type"]
        .as_str()
        .expect("error type should be present");
    assert!(
        matches!(error_type, "precondition_failed" | "target_missing"),
        "expected stale target diagnostic, got: {error_type}"
    );
}

#[test]
fn patch_json_line_target_set_line_applies_change() {
    let source = "a\nb\n";
    let mut temp_file = Builder::new()
        .suffix(".txt")
        .tempfile()
        .expect("temp text file should be created");
    temp_file
        .write_all(source.as_bytes())
        .expect("fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "line",
            "anchor": line_ref(source, 2)
        },
        "op": {
            "type": "set_line",
            "new_text": "B"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "patch --json line set_line failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["applied_mode"], "strict");
    assert_eq!(response["operations_applied"], 1);

    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    assert_eq!(modified, "a\nB\n");
}

#[test]
fn patch_json_line_target_options_dry_run_does_not_modify_file() {
    let source = "a\nb\n";
    let mut temp_file = Builder::new()
        .suffix(".txt")
        .tempfile()
        .expect("temp text file should be created");
    temp_file
        .write_all(source.as_bytes())
        .expect("fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let anchor = line_ref(source, 2);
    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "line",
            "anchor": anchor
        },
        "op": {
            "type": "set_line",
            "new_text": "B"
        },
        "options": {
            "dry_run": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "json line dry-run should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["dry_run"], true);
    assert_eq!(
        fs::read_to_string(&file_path).expect("file should be readable"),
        source,
        "json line dry-run must not modify the file"
    );
}

#[test]
fn patch_json_cli_dry_run_overrides_line_request_and_does_not_modify_file() {
    let source = "a\nb\n";
    let mut temp_file = Builder::new()
        .suffix(".txt")
        .tempfile()
        .expect("temp text file should be created");
    temp_file
        .write_all(source.as_bytes())
        .expect("fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "line",
            "anchor": line_ref(source, 2)
        },
        "op": {
            "type": "set_line",
            "new_text": "B"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json", "--dry-run"], &request.to_string());
    assert!(
        output.status.success(),
        "json cli line dry-run should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["dry_run"], true);
    assert_eq!(
        fs::read_to_string(&file_path).expect("file should be readable"),
        source,
        "cli dry-run in json mode must not modify line target files"
    );
}

#[test]
fn patch_json_line_target_replace_lines_supports_end_anchor() {
    let source = "a\nb\nc\n";
    let mut temp_file = Builder::new()
        .suffix(".txt")
        .tempfile()
        .expect("temp text file should be created");
    temp_file
        .write_all(source.as_bytes())
        .expect("fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "line",
            "anchor": line_ref(source, 2),
            "end_anchor": line_ref(source, 3)
        },
        "op": {
            "type": "replace_lines",
            "new_text": "x\ny"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "patch --json line replace_lines failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    assert_eq!(modified, "a\nx\ny\n");
}

#[test]
fn patch_json_line_target_can_auto_repair() {
    let source = "a\nb\na\n";
    let mut temp_file = Builder::new()
        .suffix(".txt")
        .tempfile()
        .expect("temp text file should be created");
    temp_file
        .write_all(source.as_bytes())
        .expect("fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let stale_anchor = format!("1:{}", identedit::hashline::compute_line_hash("b"));
    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "line",
            "anchor": stale_anchor
        },
        "op": {
            "type": "set_line",
            "new_text": "B"
        },
        "options": {
            "auto_repair": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "patch --json line auto-repair failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["applied_mode"], "repair");

    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    assert_eq!(modified, "a\nB\na\n");
}

#[test]
fn patch_json_line_target_auto_repair_dry_run_ambiguous_keeps_file_unchanged() {
    let source = "a\nb\na\n";
    let mut temp_file = Builder::new()
        .suffix(".txt")
        .tempfile()
        .expect("temp text file should be created");
    temp_file
        .write_all(source.as_bytes())
        .expect("fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let stale_anchor = format!("2:{}", identedit::hashline::compute_line_hash("z"));
    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "line",
            "anchor": stale_anchor
        },
        "op": {
            "type": "set_line",
            "new_text": "B"
        },
        "options": {
            "dry_run": true,
            "auto_repair": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "mismatched repair dry-run should still fail"
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert_eq!(
        fs::read_to_string(&file_path).expect("file should be readable"),
        source,
        "failing json line dry-run must not modify the file"
    );
}

#[test]
fn patch_json_rejects_node_target_with_line_only_op() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_function_handle(&file_path, "process_*");
    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "node",
            "identity": handle["identity"],
            "kind": handle["kind"],
            "expected_old_hash": identedit::changeset::hash_text(
                handle["text"].as_str().expect("text should be string")
            )
        },
        "op": {
            "type": "set_line",
            "new_text": "x"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "node target should reject line-only operation payload"
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}

#[test]
fn patch_json_rejects_line_target_with_node_only_op() {
    let source = "a\nb\n";
    let mut temp_file = Builder::new()
        .suffix(".txt")
        .tempfile()
        .expect("temp text file should be created");
    temp_file
        .write_all(source.as_bytes())
        .expect("fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "line",
            "anchor": line_ref(source, 2)
        },
        "op": {
            "type": "delete"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "line target should reject node-only operation payload"
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}

#[test]
fn patch_json_config_path_set_updates_json_value() {
    let file_path = copy_fixture_to_temp_with_suffix("example.json", ".json");
    let original = fs::read_to_string(&file_path).expect("fixture should be readable");
    let expected_file_hash = identedit::hash::hash_text(&original);

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "config.retries",
            "expected_file_hash": expected_file_hash
        },
        "op": {
            "type": "set",
            "new_text": "10"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "config path set should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["summary"]["operations_applied"], 1);

    let updated = fs::read_to_string(&file_path).expect("updated file should be readable");
    assert!(updated.contains("\"retries\": 10"));
}

#[test]
fn patch_json_config_path_set_options_dry_run_does_not_modify_json() {
    let file_path = copy_fixture_to_temp_with_suffix("example.json", ".json");
    let before = fs::read_to_string(&file_path).expect("json should be readable");
    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "config.enabled"
        },
        "op": {
            "type": "set",
            "new_text": "false"
        },
        "options": {
            "dry_run": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "json config path dry-run should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["transaction"]["status"], "dry_run");
    assert_eq!(
        fs::read_to_string(&file_path).expect("json should be readable"),
        before,
        "json config-path dry-run must not mutate the file"
    );
}

#[test]
fn patch_json_config_path_delete_removes_json_key_and_keeps_valid_document() {
    let file_path = copy_fixture_to_temp_with_suffix("example.json", ".json");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "config.enabled"
        },
        "op": {
            "type": "delete"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "config path delete should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let updated = fs::read_to_string(&file_path).expect("updated file should be readable");
    let parsed: Value = serde_json::from_str(&updated).expect("updated JSON should stay valid");
    assert!(
        parsed["config"].get("enabled").is_none(),
        "deleted key should not exist in config object"
    );
}

#[test]
fn patch_json_config_path_set_updates_yaml_value() {
    let file_path = copy_fixture_to_temp_with_suffix("example.yaml", ".yaml");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.retries"
        },
        "op": {
            "type": "set",
            "new_text": "5"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "yaml config path set should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(
        updated.contains("retries: 5"),
        "yaml value should be updated in-place"
    );
}

#[test]
fn patch_json_config_path_delete_removes_toml_key() {
    let file_path = copy_fixture_to_temp_with_suffix("example.toml", ".toml");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "database.settings.enabled"
        },
        "op": {
            "type": "delete"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "toml config path delete should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(!updated.contains("enabled = true"));
    assert!(updated.contains("max_connections = 32"));
}

#[test]
fn patch_json_config_path_reports_missing_path() {
    let file_path = copy_fixture_to_temp_with_suffix("example.yaml", ".yaml");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.not_found"
        },
        "op": {
            "type": "set",
            "new_text": "9"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(!output.status.success(), "missing config path should fail");

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("was not found")),
        "expected missing-path diagnostic"
    );
}

#[test]
fn patch_flag_config_path_set_value_updates_json() {
    let file_path = copy_fixture_to_temp_with_suffix("example.json", ".json");

    let output = run_identedit(&[
        "patch",
        "--config-path",
        "config.enabled",
        "--set-value",
        "false",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "flag config path set should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let updated = fs::read_to_string(&file_path).expect("updated JSON should be readable");
    let parsed: Value = serde_json::from_str(&updated).expect("updated JSON should stay valid");
    assert_eq!(parsed["config"]["enabled"], Value::Bool(false));
}

#[test]
fn patch_flag_config_path_set_value_dry_run_does_not_modify_json() {
    let file_path = copy_fixture_to_temp_with_suffix("example.json", ".json");
    let before = fs::read_to_string(&file_path).expect("json should be readable");

    let output = run_identedit(&[
        "patch",
        "--config-path",
        "config.enabled",
        "--set-value",
        "false",
        "--dry-run",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "flag config path dry-run should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["transaction"]["status"], "dry_run");
    assert_eq!(
        fs::read_to_string(&file_path).expect("json should be readable"),
        before,
        "config dry-run must not mutate the source file"
    );
}

#[test]
fn patch_flag_config_path_create_missing_dry_run_does_not_modify_json() {
    let file_path = copy_fixture_to_temp_with_suffix("example.json", ".json");
    let before = fs::read_to_string(&file_path).expect("json should be readable");

    let output = run_identedit(&[
        "patch",
        "--config-path",
        "config.sidecar.host",
        "--set-value",
        "\"127.0.0.1\"",
        "--create-missing",
        "--dry-run",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "flag config path create-missing dry-run should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["transaction"]["status"], "dry_run");
    assert_eq!(
        fs::read_to_string(&file_path).expect("json should be readable"),
        before,
        "config create-missing dry-run must not mutate the source file"
    );
}

#[test]
fn patch_flag_config_path_set_value_create_missing_sets_nested_json_keys() {
    let file_path = copy_fixture_to_temp_with_suffix("example.json", ".json");

    let output = run_identedit(&[
        "patch",
        "--config-path",
        "config.sidecar.host",
        "--set-value",
        "\"127.0.0.1\"",
        "--create-missing",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "flag config path create-missing should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let updated = fs::read_to_string(&file_path).expect("updated JSON should be readable");
    let parsed: Value = serde_json::from_str(&updated).expect("updated JSON should stay valid");
    assert_eq!(
        parsed["config"]["sidecar"]["host"],
        Value::String("127.0.0.1".to_string())
    );
}

#[test]
fn patch_json_config_path_set_create_missing_updates_yaml_value() {
    let file_path = copy_fixture_to_temp_with_suffix("example.yaml", ".yaml");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.sidecar.port"
        },
        "op": {
            "type": "set",
            "new_text": "9000",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "yaml config path create-missing should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed["service"]["sidecar"]["port"].as_i64(), Some(9000));
}

#[test]
fn patch_json_config_path_set_create_missing_updates_toml_value() {
    let file_path = copy_fixture_to_temp_with_suffix("example.toml", ".toml");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "database.settings.sidecar.port"
        },
        "op": {
            "type": "set",
            "new_text": "9100",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "toml config path create-missing should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(
        parsed["database"]["settings"]["sidecar"]["port"].as_integer(),
        Some(9100)
    );
}

#[test]
fn patch_flag_config_path_delete_rejects_create_missing() {
    let file_path = copy_fixture_to_temp_with_suffix("example.json", ".json");

    let output = run_identedit(&[
        "patch",
        "--config-path",
        "config.enabled",
        "--delete",
        "--create-missing",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "config path delete should reject --create-missing"
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("--create-missing")),
        "error should mention create-missing restriction"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_rejects_array_oob_with_append_hint() {
    let file_path = copy_fixture_to_temp_with_suffix("example.json", ".json");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "items[9]"
        },
        "op": {
            "type": "set",
            "new_text": "10",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "out-of-bounds array index should remain an error"
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("append operation")),
        "array OOB error should include append-operation hint"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_preserves_crlf_yaml_newlines() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\r\n  name: identedit\r\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.sidecar.port"
        },
        "op": {
            "type": "set",
            "new_text": "9000",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "yaml create-missing should succeed on CRLF source: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let updated = fs::read(&file_path).expect("updated YAML should be readable");
    assert!(
        updated.windows(2).any(|pair| pair == b"\r\n"),
        "updated YAML should retain CRLF line endings"
    );
    for (index, byte) in updated.iter().enumerate() {
        if *byte == b'\n' {
            assert!(
                index > 0 && updated[index - 1] == b'\r',
                "every newline should be CRLF, found lone LF at byte {index}"
            );
        }
    }
}

#[test]
fn patch_json_config_path_set_create_missing_rejects_yaml_multi_document() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"---\nservice:\n  name: identedit\n---\nmetadata:\n  owner: team\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.sidecar.port"
        },
        "op": {
            "type": "set",
            "new_text": "9000",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "yaml multi-document create-missing should be rejected"
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("multiple YAML documents")),
        "error should explain multi-document create-missing limitation"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_rejects_yaml_anchor_alias() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(
            b"defaults: &defaults\n  retries: 2\nservice:\n  <<: *defaults\n  name: identedit\n",
        )
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.sidecar.port"
        },
        "op": {
            "type": "set",
            "new_text": "9000",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "yaml anchor/alias create-missing should be rejected"
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("anchor/alias")),
        "error should explain anchor/alias create-missing limitation"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_existing_path_preserves_yaml_comments() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  # keep-this-comment\n  retries: 2\n  name: identedit\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.retries"
        },
        "op": {
            "type": "set",
            "new_text": "5",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "yaml existing-path create-missing should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(
        updated.contains("# keep-this-comment"),
        "existing-path create-missing should not drop nearby comments"
    );
    assert!(
        updated.contains("retries: 5"),
        "targeted value should be updated"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_bootstraps_empty_json_root() {
    let mut temp_file = Builder::new()
        .suffix(".json")
        .tempfile()
        .expect("temp json file should be created");
    temp_file
        .write_all(b"")
        .expect("json fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.enabled"
        },
        "op": {
            "type": "set",
            "new_text": "true",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "empty-json create-missing should bootstrap root object: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let updated = fs::read_to_string(&file_path).expect("updated JSON should be readable");
    let parsed: Value = serde_json::from_str(&updated).expect("updated JSON should stay valid");
    assert_eq!(parsed["service"]["enabled"], Value::Bool(true));
}

#[test]
fn patch_json_config_path_set_create_missing_nested_array_oob_has_append_hint() {
    let file_path = copy_fixture_to_temp_with_suffix("example.json", ".json");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "config.targets[0].name"
        },
        "op": {
            "type": "set",
            "new_text": "\"api\"",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "nested array slot creation should remain out-of-bounds error"
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("append operation")),
        "nested array OOB error should include append-operation hint"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_existing_toml_path_preserves_comments() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(
            b"# keep-this-comment\n[server]\nport = 8080 # trailing-comment\nhost = \"127.0.0.1\"\n",
        )
        .expect("toml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "server.port"
        },
        "op": {
            "type": "set",
            "new_text": "9090",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "existing TOML path create-missing should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.contains("# keep-this-comment"),
        "existing-path create-missing should keep TOML comments"
    );
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(
        parsed["server"]["port"].as_integer(),
        Some(9090),
        "targeted TOML value should be updated"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_rejects_toml_comment_preserving_fallback() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\n[server]\nhost = \"127.0.0.1\"\n")
        .expect("toml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "server.port"
        },
        "op": {
            "type": "set",
            "new_text": "9090",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "missing-path TOML create-missing with comments should be rejected for safety"
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("TOML comments")),
        "error should explain comment-preservation limitation"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_with_stale_expected_file_hash_fails_precondition() {
    let file_path = copy_fixture_to_temp_with_suffix("example.json", ".json");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "config.sidecar.port",
            "expected_file_hash": "deadbeefdeadbeef"
        },
        "op": {
            "type": "set",
            "new_text": "9000",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "stale expected_file_hash should fail before create-missing"
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "precondition_failed");
}

#[test]
fn patch_json_config_path_delete_rejects_create_missing_payload_field() {
    let file_path = copy_fixture_to_temp_with_suffix("example.json", ".json");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "config.enabled"
        },
        "op": {
            "type": "delete",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "delete payload should reject create_missing field"
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}

#[test]
fn patch_json_config_path_set_create_missing_root_array_oob_has_append_hint() {
    let mut temp_file = Builder::new()
        .suffix(".json")
        .tempfile()
        .expect("temp json file should be created");
    temp_file
        .write_all(b"[]")
        .expect("json fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "[0].name"
        },
        "op": {
            "type": "set",
            "new_text": "\"api\"",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "root array out-of-bounds should remain an error"
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("append operation")),
        "root array OOB should include append-operation hint"
    );
}

#[test]
fn patch_flag_config_path_create_missing_rejects_unrelated_line_flags() {
    let file_path = copy_fixture_to_temp_with_suffix("example.json", ".json");
    let anchor = line_ref("a\nb\n", 1);

    let output = run_identedit(&[
        "patch",
        "--config-path",
        "config.sidecar.port",
        "--set-value",
        "9000",
        "--create-missing",
        "--anchor",
        &anchor,
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "config-path create-missing should reject unrelated line-target flags"
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let message = response["error"]["message"]
        .as_str()
        .expect("error message should be present");
    assert!(
        message.contains("--set-value")
            && message.contains("--append-value")
            && message.contains("--delete"),
        "config-mode error should list valid config path operations"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_rejects_yaml_comment_preserving_fallback() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  # keep-this-comment\n  name: identedit\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.sidecar.port"
        },
        "op": {
            "type": "set",
            "new_text": "9000",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "missing-path YAML create-missing with comments should be rejected for safety"
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("YAML comments")),
        "error should explain YAML comment-preservation limitation"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_existing_yaml_path_preserves_comment() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  # keep-this-comment\n  retries: 2\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.retries"
        },
        "op": {
            "type": "set",
            "new_text": "5",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "existing YAML path create-missing should still succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(updated.contains("# keep-this-comment"));
    assert!(updated.contains("retries: 5"));
}

#[test]
fn patch_json_config_path_set_create_missing_preserves_cr_only_toml_newlines() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"[server]\rhost = \"127.0.0.1\"\r")
        .expect("toml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "server.port"
        },
        "op": {
            "type": "set",
            "new_text": "9090",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "CR-only TOML create-missing should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let updated = fs::read(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.contains(&b'\r'),
        "updated TOML should retain CR-only line endings"
    );
    for (index, byte) in updated.iter().enumerate() {
        if *byte == b'\n' {
            assert!(
                index > 0 && updated[index - 1] == b'\r',
                "every newline should be CRLF or CR-only compatible; found lone LF at byte {index}"
            );
        }
    }
}

#[test]
fn patch_json_config_path_set_create_missing_still_rejects_invalid_path_characters() {
    let file_path = copy_fixture_to_temp_with_suffix("example.json", ".json");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service bad.path"
        },
        "op": {
            "type": "set",
            "new_text": "1",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "invalid path characters should still be rejected"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}

#[test]
fn patch_json_config_path_set_create_missing_empty_json_with_stale_hash_fails_precondition() {
    let mut temp_file = Builder::new()
        .suffix(".json")
        .tempfile()
        .expect("temp json file should be created");
    temp_file
        .write_all(b"")
        .expect("json fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.enabled",
            "expected_file_hash": "deadbeefdeadbeef"
        },
        "op": {
            "type": "set",
            "new_text": "true",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "stale hash should fail even when bootstrapping empty json"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "precondition_failed");
}

#[test]
fn patch_json_config_path_set_create_missing_array_oob_does_not_mutate_file() {
    let file_path = copy_fixture_to_temp_with_suffix("example.json", ".json");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "items[99]"
        },
        "op": {
            "type": "set",
            "new_text": "10",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(!output.status.success(), "array OOB should fail");

    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(
        before, after,
        "failed create-missing operation must not mutate file"
    );
}

#[test]
fn patch_json_config_path_yaml_comment_rejection_does_not_mutate_file() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  # keep-this-comment\n  name: identedit\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.sidecar.port"
        },
        "op": {
            "type": "set",
            "new_text": "9000",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(!output.status.success(), "operation should fail");
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "yaml rejection must not mutate file");
}

#[test]
fn patch_json_config_path_toml_comment_rejection_does_not_mutate_file() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\n[server]\nhost = \"127.0.0.1\"\n")
        .expect("toml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "server.port"
        },
        "op": {
            "type": "set",
            "new_text": "9090",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(!output.status.success(), "operation should fail");
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "toml rejection must not mutate file");
}

#[test]
fn patch_json_config_path_yaml_multi_document_rejection_does_not_mutate_file() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"---\nservice:\n  name: identedit\n---\nmetadata:\n  owner: team\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.sidecar.port"
        },
        "op": {
            "type": "set",
            "new_text": "9000",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(!output.status.success(), "operation should fail");
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(
        before, after,
        "multi-document rejection must not mutate file"
    );
}

#[test]
fn patch_json_config_path_yaml_anchor_alias_rejection_does_not_mutate_file() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(
            b"defaults: &defaults\n  retries: 2\nservice:\n  <<: *defaults\n  name: identedit\n",
        )
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.sidecar.port"
        },
        "op": {
            "type": "set",
            "new_text": "9000",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(!output.status.success(), "operation should fail");
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "anchor/alias rejection must not mutate file");
}

#[test]
fn patch_json_config_path_set_create_missing_empty_json_with_exact_hash_succeeds() {
    let mut temp_file = Builder::new()
        .suffix(".json")
        .tempfile()
        .expect("temp json file should be created");
    temp_file
        .write_all(b"")
        .expect("json fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.enabled",
            "expected_file_hash": identedit::hash::hash_text("")
        },
        "op": {
            "type": "set",
            "new_text": "true",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "exact hash should allow empty-json bootstrap: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn patch_json_config_path_missing_path_without_create_missing_bypasses_yaml_comment_guard() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  # keep-this-comment\n  name: identedit\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.sidecar.port"
        },
        "op": {
            "type": "set",
            "new_text": "9000"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(!output.status.success(), "operation should fail");
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("was not found")),
        "missing path without create-missing should keep strict missing-path diagnostic"
    );
}

#[test]
fn patch_json_config_path_missing_path_without_create_missing_bypasses_toml_comment_guard() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\n[server]\nhost = \"127.0.0.1\"\n")
        .expect("toml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "server.port"
        },
        "op": {
            "type": "set",
            "new_text": "9090"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(!output.status.success(), "operation should fail");
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("was not found")),
        "missing path without create-missing should keep strict missing-path diagnostic"
    );
}

#[test]
fn patch_json_config_path_create_missing_existing_anchor_path_rejects_with_no_mutation() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(
            b"defaults: &defaults\n  retries: 2\nservice:\n  <<: *defaults\n  name: identedit\n",
        )
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "defaults.retries"
        },
        "op": {
            "type": "set",
            "new_text": "5",
            "create_missing": true
        }
    });

    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "anchor/alias yaml should be rejected in create-missing mode"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "rejected operation must not mutate file");
}

#[test]
fn patch_json_config_path_create_missing_existing_multi_document_path_preserves_both_docs() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"---\nservice:\n  retries: 2\n---\nmetadata:\n  owner: team\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.retries"
        },
        "op": {
            "type": "set",
            "new_text": "5",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "existing multi-doc path should still use strict edit path: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(updated.contains("---\nservice:"));
    assert!(updated.contains("---\nmetadata:"));
    assert!(updated.contains("retries: 5"));
}

#[test]
fn patch_json_config_path_create_missing_invalid_json_value_does_not_mutate_file() {
    let file_path = copy_fixture_to_temp_with_suffix("example.json", ".json");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "config.sidecar.port"
        },
        "op": {
            "type": "set",
            "new_text": "{invalid-json",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(!output.status.success(), "invalid value should fail");
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "failure must not mutate JSON source");
}

#[test]
fn patch_json_config_path_create_missing_invalid_toml_value_does_not_mutate_file() {
    let file_path = copy_fixture_to_temp_with_suffix("example.toml", ".toml");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "database.settings.sidecar.port"
        },
        "op": {
            "type": "set",
            "new_text": "{invalid-toml",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(!output.status.success(), "invalid value should fail");
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "failure must not mutate TOML source");
}

#[test]
fn patch_flag_config_path_create_missing_rejects_yaml_comment_fallback() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  # keep-this-comment\n  name: identedit\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let output = run_identedit(&[
        "patch",
        "--config-path",
        "service.sidecar.port",
        "--set-value",
        "9000",
        "--create-missing",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "flag-mode YAML fallback with comments should be rejected"
    );
}

#[test]
fn patch_flag_config_path_create_missing_rejects_toml_comment_fallback() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\n[server]\nhost = \"127.0.0.1\"\n")
        .expect("toml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let output = run_identedit(&[
        "patch",
        "--config-path",
        "server.port",
        "--set-value",
        "9090",
        "--create-missing",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "flag-mode TOML fallback with comments should be rejected"
    );
}

#[test]
fn patch_flag_config_path_create_missing_rejects_yaml_multi_document() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"---\nservice:\n  name: identedit\n---\nmetadata:\n  owner: team\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let output = run_identedit(&[
        "patch",
        "--config-path",
        "service.sidecar.port",
        "--set-value",
        "9000",
        "--create-missing",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "flag-mode YAML multi-document fallback should be rejected"
    );
}

#[test]
fn patch_flag_config_path_create_missing_rejects_yaml_anchor_alias() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(
            b"defaults: &defaults\n  retries: 2\nservice:\n  <<: *defaults\n  name: identedit\n",
        )
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let output = run_identedit(&[
        "patch",
        "--config-path",
        "service.sidecar.port",
        "--set-value",
        "9000",
        "--create-missing",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "flag-mode YAML anchor/alias fallback should be rejected"
    );
}

#[test]
fn patch_flag_config_path_create_missing_array_oob_does_not_mutate_file() {
    let file_path = copy_fixture_to_temp_with_suffix("example.json", ".json");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let output = run_identedit(&[
        "patch",
        "--config-path",
        "items[99]",
        "--set-value",
        "10",
        "--create-missing",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "array OOB should fail in flag mode"
    );

    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "flag-mode failure must not mutate file");
}

#[test]
fn patch_flag_config_path_create_missing_existing_yaml_path_preserves_comment() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  # keep-this-comment\n  retries: 2\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let output = run_identedit(&[
        "patch",
        "--config-path",
        "service.retries",
        "--set-value",
        "5",
        "--create-missing",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "existing YAML path should still use strict rewrite path: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(updated.contains("# keep-this-comment"));
    assert!(updated.contains("retries: 5"));
}

#[test]
fn patch_flag_config_path_create_missing_yaml_comment_error_mentions_comment() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  # keep-this-comment\n  name: identedit\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let output = run_identedit(&[
        "patch",
        "--config-path",
        "service.sidecar.port",
        "--set-value",
        "9000",
        "--create-missing",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(!output.status.success(), "operation should fail");

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("YAML comments")),
        "error should mention YAML comments limitation"
    );
}

#[test]
fn patch_flag_config_path_create_missing_toml_comment_error_mentions_comment() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\n[server]\nhost = \"127.0.0.1\"\n")
        .expect("toml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let output = run_identedit(&[
        "patch",
        "--config-path",
        "server.port",
        "--set-value",
        "9090",
        "--create-missing",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(!output.status.success(), "operation should fail");

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("TOML comments")),
        "error should mention TOML comments limitation"
    );
}

#[test]
fn patch_flag_config_path_create_missing_array_oob_error_mentions_append() {
    let file_path = copy_fixture_to_temp_with_suffix("example.json", ".json");

    let output = run_identedit(&[
        "patch",
        "--config-path",
        "items[99]",
        "--set-value",
        "10",
        "--create-missing",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(!output.status.success(), "operation should fail");

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("append operation")),
        "array OOB error should include append-operation hint"
    );
}

#[test]
fn patch_json_config_path_create_missing_stale_hash_reports_empty_actual_hash() {
    let mut temp_file = Builder::new()
        .suffix(".json")
        .tempfile()
        .expect("temp json file should be created");
    temp_file
        .write_all(b"")
        .expect("json fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.enabled",
            "expected_file_hash": "deadbeefdeadbeef"
        },
        "op": {
            "type": "set",
            "new_text": "true",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(!output.status.success(), "operation should fail");
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "precondition_failed");
}

#[test]
fn patch_json_config_path_bootstrap_empty_json_writes_valid_object_document() {
    let mut temp_file = Builder::new()
        .suffix(".json")
        .tempfile()
        .expect("temp json file should be created");
    temp_file
        .write_all(b"")
        .expect("json fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.enabled"
        },
        "op": {
            "type": "set",
            "new_text": "true",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(output.status.success(), "operation should succeed");
    let updated = fs::read_to_string(&file_path).expect("updated file should be readable");
    let parsed: Value = serde_json::from_str(&updated).expect("updated file should be valid JSON");
    assert_eq!(parsed["service"]["enabled"], Value::Bool(true));
    assert!(
        updated.trim_start().starts_with('{'),
        "bootstrapped document should be object-shaped JSON text"
    );
}

#[test]
fn patch_json_config_path_create_missing_existing_multi_document_path_with_hash_precondition() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"---\nservice:\n  retries: 2\n---\nmetadata:\n  owner: team\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.retries",
            "expected_file_hash": identedit::hash::hash_text(&before)
        },
        "op": {
            "type": "set",
            "new_text": "5",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(output.status.success(), "operation should succeed");
    let updated = fs::read_to_string(&file_path).expect("updated file should be readable");
    assert!(updated.contains("retries: 5"));
    assert!(updated.contains("---\nmetadata:"));
}

#[test]
fn patch_flag_yaml_comment_rejection_keeps_file_unchanged() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  # keep-this-comment\n  name: identedit\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let output = run_identedit(&[
        "patch",
        "--config-path",
        "service.sidecar.port",
        "--set-value",
        "9000",
        "--create-missing",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(!output.status.success(), "operation should fail");
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("YAML comments")),
        "error should mention YAML comments limitation"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(
        before, after,
        "rejected flag-mode operation must not mutate file"
    );
}

#[test]
fn patch_flag_toml_comment_rejection_keeps_file_unchanged() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\n[server]\nhost = \"127.0.0.1\"\n")
        .expect("toml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let output = run_identedit(&[
        "patch",
        "--config-path",
        "server.port",
        "--set-value",
        "9090",
        "--create-missing",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(!output.status.success(), "operation should fail");
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("TOML comments")),
        "error should mention TOML comments limitation"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(
        before, after,
        "rejected flag-mode operation must not mutate file"
    );
}

#[test]
fn patch_flag_yaml_multidoc_rejection_keeps_file_unchanged() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"---\nservice:\n  name: identedit\n---\nmetadata:\n  owner: team\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let output = run_identedit(&[
        "patch",
        "--config-path",
        "service.sidecar.port",
        "--set-value",
        "9000",
        "--create-missing",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(!output.status.success(), "operation should fail");
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(
        before, after,
        "rejected flag-mode operation must not mutate file"
    );
}

#[test]
fn patch_flag_yaml_anchor_alias_rejection_keeps_file_unchanged() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(
            b"defaults: &defaults\n  retries: 2\nservice:\n  <<: *defaults\n  name: identedit\n",
        )
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let output = run_identedit(&[
        "patch",
        "--config-path",
        "service.sidecar.port",
        "--set-value",
        "9000",
        "--create-missing",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(!output.status.success(), "operation should fail");
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(
        before, after,
        "rejected flag-mode operation must not mutate file"
    );
}

#[test]
fn patch_flag_array_oob_rejection_reports_append_and_keeps_file() {
    let file_path = copy_fixture_to_temp_with_suffix("example.json", ".json");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let output = run_identedit(&[
        "patch",
        "--config-path",
        "items[99]",
        "--set-value",
        "10",
        "--create-missing",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(!output.status.success(), "operation should fail");
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("append operation")),
        "error should include append-operation hint"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(
        before, after,
        "rejected flag-mode operation must not mutate file"
    );
}

#[test]
fn patch_json_create_missing_existing_toml_path_with_hash_precondition_preserves_comment() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\n[server]\nport = 8080\nhost = \"127.0.0.1\"\n")
        .expect("toml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "server.port",
            "expected_file_hash": identedit::hash::hash_text(&before)
        },
        "op": {
            "type": "set",
            "new_text": "9090",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(output.status.success(), "operation should succeed");
    let updated = fs::read_to_string(&file_path).expect("updated file should be readable");
    assert!(updated.contains("# keep-this-comment"));
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["server"]["port"].as_integer(), Some(9090));
}

#[test]
fn patch_json_create_missing_existing_yaml_path_with_hash_precondition_preserves_comment() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  # keep-this-comment\n  retries: 2\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.retries",
            "expected_file_hash": identedit::hash::hash_text(&before)
        },
        "op": {
            "type": "set",
            "new_text": "5",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(output.status.success(), "operation should succeed");
    let updated = fs::read_to_string(&file_path).expect("updated file should be readable");
    assert!(updated.contains("# keep-this-comment"));
    assert!(updated.contains("retries: 5"));
}

#[test]
fn patch_json_create_missing_existing_yaml_path_stale_hash_fails_precondition_no_mutation() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  # keep-this-comment\n  retries: 2\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.retries",
            "expected_file_hash": "deadbeefdeadbeef"
        },
        "op": {
            "type": "set",
            "new_text": "5",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(!output.status.success(), "operation should fail");
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "precondition_failed");
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "stale-hash failure must not mutate file");
}

#[test]
fn patch_json_create_missing_existing_toml_path_stale_hash_fails_precondition_no_mutation() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\n[server]\nport = 8080\nhost = \"127.0.0.1\"\n")
        .expect("toml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "server.port",
            "expected_file_hash": "deadbeefdeadbeef"
        },
        "op": {
            "type": "set",
            "new_text": "9090",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(!output.status.success(), "operation should fail");
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "precondition_failed");
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "stale-hash failure must not mutate file");
}

#[test]
fn patch_json_create_missing_yaml_comment_fallback_with_stale_hash_fails_precondition_first() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  # keep-this-comment\n  name: identedit\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.sidecar.port",
            "expected_file_hash": "deadbeefdeadbeef"
        },
        "op": {
            "type": "set",
            "new_text": "9000",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(!output.status.success(), "operation should fail");
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "precondition_failed");
}

#[test]
fn patch_json_create_missing_toml_comment_fallback_with_stale_hash_fails_precondition_first() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\n[server]\nhost = \"127.0.0.1\"\n")
        .expect("toml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "server.port",
            "expected_file_hash": "deadbeefdeadbeef"
        },
        "op": {
            "type": "set",
            "new_text": "9090",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(!output.status.success(), "operation should fail");
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "precondition_failed");
}

#[test]
fn patch_json_config_delete_rejects_create_missing_false_field() {
    let file_path = copy_fixture_to_temp_with_suffix("example.json", ".json");
    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "config.enabled"
        },
        "op": {
            "type": "delete",
            "create_missing": false
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "delete should reject create_missing field"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}

#[test]
fn patch_json_config_delete_rejects_create_missing_null_field() {
    let file_path = copy_fixture_to_temp_with_suffix("example.json", ".json");
    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "config.enabled"
        },
        "op": {
            "type": "delete",
            "create_missing": null
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "delete should reject create_missing field"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}

#[test]
fn patch_json_config_set_rejects_non_boolean_create_missing_type() {
    let file_path = copy_fixture_to_temp_with_suffix("example.json", ".json");
    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "config.sidecar.port"
        },
        "op": {
            "type": "set",
            "new_text": "9000",
            "create_missing": "yes"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "non-boolean create_missing should be rejected"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}

#[test]
fn patch_json_config_set_omitted_create_missing_keeps_strict_missing_path_behavior() {
    let file_path = copy_fixture_to_temp_with_suffix("example.json", ".json");
    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "config.sidecar.port"
        },
        "op": {
            "type": "set",
            "new_text": "9000"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "omitted create_missing should keep strict missing-path behavior"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("was not found")),
        "strict mode should report missing path"
    );
}

#[test]
fn patch_json_config_set_explicit_false_create_missing_keeps_strict_missing_path_behavior() {
    let file_path = copy_fixture_to_temp_with_suffix("example.json", ".json");
    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "config.sidecar.port"
        },
        "op": {
            "type": "set",
            "new_text": "9000",
            "create_missing": false
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "create_missing=false should keep strict missing-path behavior"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("was not found")),
        "strict mode should report missing path"
    );
}

#[test]
fn patch_json_config_set_with_create_missing_rejects_unknown_payload_field() {
    let file_path = copy_fixture_to_temp_with_suffix("example.json", ".json");
    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "config.sidecar.port"
        },
        "op": {
            "type": "set",
            "new_text": "9000",
            "create_missing": true,
            "unexpected": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "unknown payload field should be rejected"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}

#[test]
fn patch_json_config_path_append_appends_json_array() {
    let file_path = copy_fixture_to_temp_with_suffix("example.json", ".json");
    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "items"
        },
        "op": {
            "type": "append",
            "new_text": "4"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "json config append should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let updated = fs::read_to_string(&file_path).expect("updated JSON should be readable");
    let parsed: Value = serde_json::from_str(&updated).expect("updated JSON should stay valid");
    assert_eq!(parsed["items"], json!([1, 2, 3, 4]));
}

#[test]
fn patch_flag_config_path_append_value_appends_json_array() {
    let file_path = copy_fixture_to_temp_with_suffix("example.json", ".json");
    let output = run_identedit(&[
        "patch",
        "--config-path",
        "items",
        "--append-value",
        "4",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "flag config append should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let updated = fs::read_to_string(&file_path).expect("updated JSON should be readable");
    let parsed: Value = serde_json::from_str(&updated).expect("updated JSON should stay valid");
    assert_eq!(parsed["items"], json!([1, 2, 3, 4]));
}

#[test]
fn patch_json_config_path_append_appends_toml_array() {
    let file_path = copy_fixture_to_temp_with_suffix("example.toml", ".toml");
    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "pipelines[0].steps"
        },
        "op": {
            "type": "append",
            "new_text": "\"qa\""
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "toml config append should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(
        parsed["pipelines"][0]["steps"],
        toml::Value::Array(vec![
            toml::Value::String("fmt".to_string()),
            toml::Value::String("clippy".to_string()),
            toml::Value::String("qa".to_string())
        ])
    );
}

#[test]
fn patch_json_config_path_append_rejects_non_array_target() {
    let file_path = copy_fixture_to_temp_with_suffix("example.json", ".json");
    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "config.retries"
        },
        "op": {
            "type": "append",
            "new_text": "4"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "append on non-array path must fail"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("append") && message.contains("array")),
        "non-array append should return a clear array-target diagnostic"
    );
}

#[test]
fn patch_json_config_path_append_rejects_create_missing_payload_field() {
    let file_path = copy_fixture_to_temp_with_suffix("example.json", ".json");
    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "items"
        },
        "op": {
            "type": "append",
            "new_text": "4",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "append payload should reject create_missing field"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}

#[test]
fn patch_flag_config_path_append_rejects_create_missing() {
    let file_path = copy_fixture_to_temp_with_suffix("example.json", ".json");
    let output = run_identedit(&[
        "patch",
        "--config-path",
        "items",
        "--append-value",
        "4",
        "--create-missing",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "append should reject create-missing"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("--create-missing")),
        "error should mention create-missing restriction"
    );
}

#[test]
fn patch_json_config_path_append_with_stale_hash_fails_without_mutation() {
    let file_path = copy_fixture_to_temp_with_suffix("example.json", ".json");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "items",
            "expected_file_hash": "deadbeefdeadbeef"
        },
        "op": {
            "type": "append",
            "new_text": "4"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "stale hash must fail for append operation"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "precondition_failed");
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(
        before, after,
        "stale-hash append failure must not mutate file"
    );
}

#[test]
fn patch_json_config_path_append_appends_yaml_block_sequence() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  tags:\n    - api\n    - worker\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.tags"
        },
        "op": {
            "type": "append",
            "new_text": "batch"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "yaml block-sequence append should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(
        parsed["service"]["tags"],
        serde_yaml::to_value(vec!["api", "worker", "batch"]).expect("yaml list should serialize")
    );
}

#[test]
fn patch_json_config_path_append_appends_yaml_flow_sequence() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service: { tags: [api, worker] }\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.tags"
        },
        "op": {
            "type": "append",
            "new_text": "batch"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "yaml flow-sequence append should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(
        parsed["service"]["tags"],
        serde_yaml::to_value(vec!["api", "worker", "batch"]).expect("yaml list should serialize")
    );
}

#[test]
fn patch_json_config_path_append_rejects_missing_path() {
    let file_path = copy_fixture_to_temp_with_suffix("example.json", ".json");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "config.not_found"
        },
        "op": {
            "type": "append",
            "new_text": "4"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "append should fail for missing path"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("was not found")),
        "missing path should report deterministic diagnostic"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(
        before, after,
        "missing-path append failure must not mutate file"
    );
}

#[test]
fn patch_json_config_path_append_supports_index_targeting_nested_array() {
    let mut temp_file = Builder::new()
        .suffix(".json")
        .tempfile()
        .expect("temp json file should be created");
    temp_file
        .write_all(br#"{"matrix":[[1,2],[3]]}"#)
        .expect("json fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "matrix[1]"
        },
        "op": {
            "type": "append",
            "new_text": "4"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "append should allow index path that resolves to nested array: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let updated = fs::read_to_string(&file_path).expect("updated JSON should be readable");
    let parsed: Value = serde_json::from_str(&updated).expect("updated JSON should stay valid");
    assert_eq!(parsed["matrix"], json!([[1, 2], [3, 4]]));
}

#[test]
fn patch_json_config_path_append_rejects_index_targeting_scalar() {
    let file_path = copy_fixture_to_temp_with_suffix("example.json", ".json");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "items[0]"
        },
        "op": {
            "type": "append",
            "new_text": "4"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "append should fail when index resolves to scalar value"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("array/sequence")),
        "scalar append should report array-target diagnostic"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "failed scalar append must not mutate file");
}
