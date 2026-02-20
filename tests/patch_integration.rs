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
        .write_all(
            b"service:\n  # keep-this-comment\n  retries: 2\n  name: identedit\n",
        )
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
        .write_all(
            b"# keep-this-comment\n[server]\nhost = \"127.0.0.1\"\n",
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
}

#[test]
fn patch_json_config_path_set_create_missing_rejects_yaml_comment_preserving_fallback() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(
            b"service:\n  # keep-this-comment\n  name: identedit\n",
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
        .write_all(
            b"service:\n  # keep-this-comment\n  retries: 2\n",
        )
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
    assert!(
        !output.status.success(),
        "array OOB should fail"
    );

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
        .write_all(
            b"service:\n  # keep-this-comment\n  name: identedit\n",
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
    assert_eq!(before, after, "yaml rejection must not mutate file");
}

#[test]
fn patch_json_config_path_toml_comment_rejection_does_not_mutate_file() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(
            b"# keep-this-comment\n[server]\nhost = \"127.0.0.1\"\n",
        )
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
    assert_eq!(before, after, "multi-document rejection must not mutate file");
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
        .write_all(
            b"service:\n  # keep-this-comment\n  name: identedit\n",
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
        .write_all(
            b"# keep-this-comment\n[server]\nhost = \"127.0.0.1\"\n",
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
        .write_all(
            b"service:\n  # keep-this-comment\n  name: identedit\n",
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
        .write_all(
            b"# keep-this-comment\n[server]\nhost = \"127.0.0.1\"\n",
        )
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
    assert!(!output.status.success(), "array OOB should fail in flag mode");

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
        .write_all(
            b"service:\n  # keep-this-comment\n  retries: 2\n",
        )
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
        .write_all(
            b"service:\n  # keep-this-comment\n  name: identedit\n",
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
        .write_all(
            b"# keep-this-comment\n[server]\nhost = \"127.0.0.1\"\n",
        )
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
        .write_all(
            b"service:\n  # keep-this-comment\n  name: identedit\n",
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
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("YAML comments")),
        "error should mention YAML comments limitation"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "rejected flag-mode operation must not mutate file");
}

#[test]
fn patch_flag_toml_comment_rejection_keeps_file_unchanged() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(
            b"# keep-this-comment\n[server]\nhost = \"127.0.0.1\"\n",
        )
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
    assert_eq!(before, after, "rejected flag-mode operation must not mutate file");
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
    assert_eq!(before, after, "rejected flag-mode operation must not mutate file");
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
    assert_eq!(before, after, "rejected flag-mode operation must not mutate file");
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
    assert_eq!(before, after, "rejected flag-mode operation must not mutate file");
}

#[test]
fn patch_json_create_missing_existing_toml_path_with_hash_precondition_preserves_comment() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(
            b"# keep-this-comment\n[server]\nport = 8080\nhost = \"127.0.0.1\"\n",
        )
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
        .write_all(
            b"service:\n  # keep-this-comment\n  retries: 2\n",
        )
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
        .write_all(
            b"service:\n  # keep-this-comment\n  retries: 2\n",
        )
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
        .write_all(
            b"# keep-this-comment\n[server]\nport = 8080\nhost = \"127.0.0.1\"\n",
        )
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
        .write_all(
            b"service:\n  # keep-this-comment\n  name: identedit\n",
        )
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
        .write_all(
            b"# keep-this-comment\n[server]\nhost = \"127.0.0.1\"\n",
        )
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
    assert!(!output.status.success(), "delete should reject create_missing field");
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
    assert!(!output.status.success(), "delete should reject create_missing field");
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
    assert!(!output.status.success(), "non-boolean create_missing should be rejected");
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
    assert!(!output.status.success(), "unknown payload field should be rejected");
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
    assert!(!output.status.success(), "append on non-array path must fail");
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
    assert_eq!(before, after, "stale-hash append failure must not mutate file");
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
    assert!(!output.status.success(), "append should fail for missing path");
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("was not found")),
        "missing path should report deterministic diagnostic"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "missing-path append failure must not mutate file");
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
