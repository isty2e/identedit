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

fn assert_select_kind_and_optional_name(file: &Path, kind: &str, expected_name: Option<&str>) {
    let output = run_identedit(&[
        "select",
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

    if let Some(name) = expected_name {
        assert!(
            handles.iter().any(|handle| handle["name"] == name),
            "expected handle with name '{name}' for {kind}"
        );
    }
}

#[test]
fn select_covers_php_kinds_and_provider() {
    let php_file = fixture_path("example.php");

    assert_select_kind_and_optional_name(&php_file, "function_definition", Some("process_data"));
    assert_select_kind_and_optional_name(&php_file, "class_declaration", Some("ExampleService"));
    assert_select_kind_and_optional_name(&php_file, "method_declaration", Some("ProcessData"));
}

#[test]
fn select_supports_case_insensitive_php_extension() {
    let file_path = copy_fixture_to_temp("example.php", ".PHP");
    assert_select_kind_and_optional_name(&file_path, "function_definition", Some("process_data"));
}

#[test]
fn transform_replace_and_apply_support_php_function_definition() {
    let file_path = copy_fixture_to_temp("example.php", ".php");
    let select_output = run_identedit(&[
        "select",
        "--kind",
        "function_definition",
        "--name",
        "process_*",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        select_output.status.success(),
        "select failed: {}",
        String::from_utf8_lossy(&select_output.stderr)
    );

    let select_response: Value =
        serde_json::from_slice(&select_output.stdout).expect("stdout should be valid JSON");
    let identity = select_response["handles"][0]["identity"]
        .as_str()
        .expect("identity should be present");

    let replacement = "function process_data(int $value): int\n{\n    return $value + 2;\n}";
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
    assert!(modified.contains("$value + 2"));
}

#[test]
fn select_reports_parse_failure_for_syntax_invalid_php() {
    let file_path = write_temp_source(
        ".php",
        "<?php\nfunction broken(int $value): int\n{\n    return $value + 1;\n",
    );
    let output = run_identedit(&[
        "select",
        "--kind",
        "function_definition",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "syntax-invalid php should fail under the php provider"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "parse_failure");
    let message = response["error"]["message"]
        .as_str()
        .expect("error.message should be a string");
    assert!(message.contains("tree-sitter-php"));
    assert!(message.contains("Syntax errors detected in PHP source"));
}

#[test]
fn transform_reports_ambiguous_target_for_duplicate_php_function_identity() {
    let source = "<?php\nfunction configure(int $value): int\n{\n    return $value + 1;\n}\n\nfunction configure(int $value): int\n{\n    return $value + 1;\n}\n";
    let file_path = write_temp_source(".php", source);
    let select_output = run_identedit(&[
        "select",
        "--kind",
        "function_definition",
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
        .filter(|handle| handle["name"] == "configure")
        .map(|handle| {
            handle["identity"]
                .as_str()
                .expect("identity should be string")
        })
        .find(|identity| {
            handles
                .iter()
                .filter(|h| h["identity"] == *identity && h["name"] == "configure")
                .count()
                >= 2
        })
        .expect("fixture should include duplicate configure function identity");

    let output = run_identedit(&[
        "transform",
        "--identity",
        duplicate_identity,
        "--replace",
        "function configure(int $value): int\n{\n    return $value + 2;\n}",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "transform should fail for ambiguous duplicate PHP function identity"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "ambiguous_target");
}

#[test]
fn transform_json_span_hint_disambiguates_duplicate_php_function_identity() {
    let source = "<?php\nfunction configure(int $value): int\n{\n    return $value + 1;\n}\n\nfunction configure(int $value): int\n{\n    return $value + 1;\n}\n";
    let file_path = write_temp_source(".php", source);
    let select_output = run_identedit(&[
        "select",
        "--kind",
        "function_definition",
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
        .filter(|handle| handle["name"] == "configure")
        .collect::<Vec<_>>();
    assert!(
        duplicate_handles.len() >= 2,
        "fixture should include at least two configure functions"
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
                "new_text": "function configure(int $value): int\n{\n    return $value + 2;\n}"
            }
        }]
    });
    let request_body = serde_json::to_string(&request).expect("request should serialize");

    let transform_output = run_identedit_with_stdin(&["transform", "--json"], &request_body);
    assert!(
        transform_output.status.success(),
        "transform --json should disambiguate duplicate PHP function identity: {}",
        String::from_utf8_lossy(&transform_output.stderr)
    );

    let transform_json =
        std::str::from_utf8(&transform_output.stdout).expect("transform output should be utf-8");
    let apply_output = run_identedit_with_stdin(&["apply"], transform_json);
    assert!(
        apply_output.status.success(),
        "apply failed after PHP span_hint disambiguation: {}",
        String::from_utf8_lossy(&apply_output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("file should be readable");
    assert_eq!(modified.matches("$value + 2").count(), 1);
    assert_eq!(modified.matches("$value + 1").count(), 1);
}

#[test]
fn select_reports_parse_failure_for_nul_in_php_source() {
    let file_path = write_temp_source(".php", "<?php\nfunction run(): int {\n    return 1;\n}\0");
    let output = run_identedit(&[
        "select",
        "--kind",
        "function_definition",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "select should fail for NUL PHP source"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "parse_failure");
    let message = response["error"]["message"]
        .as_str()
        .expect("error.message should be string");
    assert!(message.contains("tree-sitter-php"));
}

#[test]
fn apply_reports_parse_failure_for_nul_in_php_source() {
    let file_path = write_temp_bytes(
        ".php",
        b"<?php\nfunction run(): int {\n    return 1;\n}\0\n",
    );
    let request = json!({
        "files": [{
            "file": file_path.to_string_lossy(),
            "operations": [{
                "target": {
                    "type": "node",
                    "identity": "deadbeef",
                    "kind": "function_definition",
                    "expected_old_hash": "00",
                    "span_hint": {"start": 0, "end": 1}
                },
                "op": {"type": "replace", "new_text": "function run(): int\n{\n    return 2;\n}"},
                "preview": {
                    "old_text": "x",
                    "new_text": "function run(): int\n{\n    return 2;\n}",
                    "matched_span": {"start": 0, "end": 1}
                }
            }]
        }],
        "transaction": {"mode": "all_or_nothing"}
    });
    let request_body = serde_json::to_string(&request).expect("request should serialize");

    let output = run_identedit_with_stdin(&["apply"], &request_body);
    assert!(
        !output.status.success(),
        "apply should fail for NUL PHP source"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "parse_failure");
    let message = response["error"]["message"]
        .as_str()
        .expect("error.message should be string");
    assert!(message.contains("tree-sitter-php"));
}

#[test]
fn select_supports_mixed_html_php_template_function_definition() {
    let source = "<html><body><?php\nfunction render_title(string $text): string\n{\n    return strtoupper($text);\n}\n?></body></html>\n";
    let file_path = write_temp_source(".php", source);
    assert_select_kind_and_optional_name(&file_path, "function_definition", Some("render_title"));
}

#[test]
fn select_ignores_function_like_text_inside_php_heredoc() {
    let source = "<?php\n$template = <<<HTML\nfunction fake(int $x): int { return $x; }\nHTML;\n\nfunction real_function(int $value): int\n{\n    return $value + 1;\n}\n";
    let file_path = write_temp_source(".php", source);
    let output = run_identedit(&[
        "select",
        "--kind",
        "function_definition",
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
            .any(|handle| handle["name"] == "real_function"),
        "expected real function to be selected"
    );
    assert!(
        !handles.iter().any(|handle| handle["name"] == "fake"),
        "function-like heredoc text should not produce function_definition handles"
    );
}
