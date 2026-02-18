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

    if let Some(name) = expected_name {
        assert!(
            handles.iter().any(|handle| handle["name"] == name),
            "expected handle with name '{name}' for kind {kind}"
        );
    }
}

fn find_statement_handle_with_text<'a>(handles: &'a [Value], snippet: &str) -> &'a Value {
    handles
        .iter()
        .find(|handle| {
            handle["kind"] == "statement"
                && handle["text"]
                    .as_str()
                    .is_some_and(|text| text.contains(snippet))
        })
        .expect("statement handle with expected text should be present")
}

#[test]
fn select_covers_sql_kinds_and_provider() {
    let sql_file = fixture_path("example.sql");

    assert_select_kind_and_optional_name(&sql_file, "program", None);
    assert_select_kind_and_optional_name(&sql_file, "statement", None);
    assert_select_kind_and_optional_name(&sql_file, "create_table", None);
}

#[test]
fn select_supports_case_insensitive_sql_extension() {
    let file_path = copy_fixture_to_temp("example.sql", ".SQL");
    assert_select_kind_and_optional_name(&file_path, "statement", None);
}

#[test]
fn select_supports_utf8_bom_prefixed_sql_files() {
    let fixture = fs::read(fixture_path("example.sql")).expect("fixture should be readable");
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(&fixture);
    let file_path = write_temp_bytes(".sql", &bytes);

    assert_select_kind_and_optional_name(&file_path, "statement", None);
}

#[test]
fn select_reports_parse_failure_for_syntax_invalid_sql() {
    let file_path = write_temp_source(
        ".sql",
        "CREATE TABLE users (\n  id INTEGER PRIMARY KEY,\n  name TEXT\n",
    );
    let output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "statement",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "syntax-invalid sql should fail under the sql provider"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "parse_failure");
    let message = response["error"]["message"]
        .as_str()
        .expect("error.message should be a string");
    assert!(message.contains("tree-sitter-sql"));
    assert!(message.contains("Syntax errors detected in SQL source"));
}

#[test]
fn transform_replace_and_apply_support_sql_statement() {
    let file_path = copy_fixture_to_temp("example.sql", ".sql");
    let select_output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "statement",
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
    let target = find_statement_handle_with_text(handles, "INSERT INTO users");
    let identity = target["identity"]
        .as_str()
        .expect("identity should be present");

    let replacement = "INSERT INTO users (id, name) VALUES (1, 'pilot');";
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
    assert!(modified.contains("VALUES (1, 'pilot');"));
}

#[test]
fn transform_reports_ambiguous_target_for_duplicate_sql_statement_identity() {
    let source = "SELECT id FROM users WHERE id = 1;\nSELECT id FROM users WHERE id = 1;\n";
    let file_path = write_temp_source(".sql", source);
    let select_output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "statement",
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
        .expect("fixture should include duplicate statement identity");

    let output = run_identedit(&[
        "transform",
        "--identity",
        duplicate_identity,
        "--replace",
        "SELECT id FROM users WHERE id = 2;",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "transform should fail for ambiguous duplicate SQL statement identity"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "ambiguous_target");
}

#[test]
fn transform_json_span_hint_disambiguates_duplicate_sql_statement_identity() {
    let source = "SELECT id FROM users WHERE id = 1;\nSELECT id FROM users WHERE id = 1;\n";
    let file_path = write_temp_source(".sql", source);
    let select_output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "statement",
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
        .expect("fixture should contain at least one statement handle");
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
        "command": "transform",
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
                    "new_text": "SELECT id FROM users WHERE id = 2;"
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["transform", "--json"], &request.to_string());
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
    assert_eq!(preview["new_text"], "SELECT id FROM users WHERE id = 2;");
}

#[test]
fn transform_json_duplicate_sql_identity_with_missed_span_hint_returns_ambiguous_target() {
    let source = "SELECT id FROM users WHERE id = 1;\nSELECT id FROM users WHERE id = 1;\n";
    let file_path = write_temp_source(".sql", source);
    let select_output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "statement",
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
        .expect("fixture should contain at least one statement handle");
    let identity = target["identity"]
        .as_str()
        .expect("identity should be present");
    let kind = target["kind"].as_str().expect("kind should be present");
    let expected_old_hash = target["expected_old_hash"]
        .as_str()
        .expect("expected_old_hash should be present");

    let request = json!({
        "command": "transform",
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
                    "new_text": "SELECT id FROM users WHERE id = 2;"
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["transform", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "missed span_hint should fall back to ambiguous target"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "ambiguous_target");
}

#[test]
fn apply_reports_precondition_failed_after_sql_source_mutation() {
    let file_path = copy_fixture_to_temp("example.sql", ".sql");

    let select_output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "statement",
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
    let target = find_statement_handle_with_text(handles, "INSERT INTO users");
    let identity = target["identity"]
        .as_str()
        .expect("identity should be present");

    let transform_output = run_identedit(&[
        "transform",
        "--identity",
        identity,
        "--replace",
        "INSERT INTO users (id, name) VALUES (2, 'pilot');",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        transform_output.status.success(),
        "transform failed: {}",
        String::from_utf8_lossy(&transform_output.stderr)
    );

    let original = fs::read_to_string(&file_path).expect("file should be readable");
    // Keep token width stable so span_hint still identifies the same node span.
    let mutated = original.replacen("identedit", "othername", 1);
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

#[test]
fn select_reports_parse_failure_for_nul_in_sql_source() {
    let file_path = write_temp_bytes(".sql", b"SELECT 1;\0\n");
    let output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "statement",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "select should fail for NUL SQL source"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "parse_failure");
    let message = response["error"]["message"]
        .as_str()
        .expect("error.message should be a string");
    assert!(message.contains("tree-sitter-sql"));
}

#[test]
fn transform_reports_parse_failure_for_nul_in_sql_source() {
    let file_path = write_temp_bytes(".sql", b"SELECT 1;\0\n");
    let output = run_identedit(&[
        "transform",
        "--identity",
        "deadbeefdeadbeef",
        "--replace",
        "SELECT 2;",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "transform should fail for NUL SQL source"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "parse_failure");
    let message = response["error"]["message"]
        .as_str()
        .expect("error.message should be a string");
    assert!(message.contains("tree-sitter-sql"));
}

#[test]
fn apply_reports_parse_failure_for_nul_in_sql_source() {
    let file_path = write_temp_bytes(".sql", b"SELECT 1;\0\n");
    let request = json!({
        "files": [{
            "file": file_path.to_string_lossy(),
            "operations": []
        }],
        "transaction": {"mode": "all_or_nothing"}
    });
    let output = run_identedit_with_stdin(
        &["apply"],
        &serde_json::to_string(&request).expect("request should serialize"),
    );
    assert!(
        !output.status.success(),
        "apply should fail for NUL SQL source"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "parse_failure");
    let message = response["error"]["message"]
        .as_str()
        .expect("error.message should be a string");
    assert!(message.contains("tree-sitter-sql"));
}

#[test]
fn select_reports_parse_failure_for_unterminated_block_comment_in_sql() {
    let file_path = write_temp_source(".sql", "/* broken comment\nSELECT 1;\n");
    let output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "statement",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "unterminated SQL block comment should fail parse"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "parse_failure");
}

#[test]
fn transform_replace_and_apply_preserve_crlf_sql_source_segments() {
    let source = "CREATE TABLE users (\r\n    id INTEGER PRIMARY KEY,\r\n    name TEXT NOT NULL\r\n);\r\n\r\nINSERT INTO users (id, name) VALUES (1, 'identedit');\r\nSELECT id, name FROM users WHERE id = 1;\r\n";
    let file_path = write_temp_source(".sql", source);
    let select_output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "statement",
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
    let target = find_statement_handle_with_text(handles, "INSERT INTO users");
    let identity = target["identity"]
        .as_str()
        .expect("identity should be present");

    let transform_output = run_identedit(&[
        "transform",
        "--identity",
        identity,
        "--replace",
        "INSERT INTO users (id, name) VALUES (2, 'pilot');",
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
    assert!(
        modified.contains("\r\nSELECT id, name FROM users WHERE id = 1;\r\n"),
        "untouched SQL segments should preserve original CRLF endings"
    );
}

#[test]
fn select_keeps_semicolons_inside_sql_string_literals_within_single_statement() {
    let source = "INSERT INTO logs(message) VALUES ('alpha;beta');\nSELECT count(*) FROM logs;\n";
    let file_path = write_temp_source(".sql", source);
    let output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "statement",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "select failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    let statements = response["handles"]
        .as_array()
        .expect("handles should be an array");
    assert_eq!(statements.len(), 2, "expected exactly two SQL statements");

    assert!(
        statements.iter().any(|handle| handle["text"]
            .as_str()
            .is_some_and(|text| { text.contains("VALUES ('alpha;beta')") })),
        "semicolon inside string literal should remain in a single statement"
    );
}

#[test]
fn select_supports_cr_only_sql_files() {
    let source = "CREATE TABLE users (\rid INTEGER PRIMARY KEY,\rname TEXT NOT NULL\r);\rSELECT id FROM users;\r";
    let file_path = write_temp_source(".sql", source);
    let output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "statement",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "select should support CR-only SQL files: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    let statements = response["handles"]
        .as_array()
        .expect("handles should be an array");
    assert!(
        statements.len() >= 2,
        "expected multiple statements for CR-only SQL source"
    );
}

#[test]
fn select_reports_parse_failure_for_bom_plus_nul_sql_source() {
    let file_path = write_temp_bytes(".sql", b"\xEF\xBB\xBFSELECT 1;\0\n");
    let output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "statement",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "select should fail for BOM+NUL SQL source"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "parse_failure");
}

#[test]
fn transform_reports_parse_failure_for_syntax_invalid_sql_source() {
    let file_path = write_temp_source(".sql", "CREATE TABLE users (\n  id INTEGER,\n");
    let output = run_identedit(&[
        "transform",
        "--identity",
        "deadbeefdeadbeef",
        "--replace",
        "SELECT 2;",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "transform should fail for syntax-invalid SQL source"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "parse_failure");
    let message = response["error"]["message"]
        .as_str()
        .expect("error.message should be a string");
    assert!(message.contains("tree-sitter-sql"));
}

#[test]
fn apply_reports_parse_failure_for_syntax_invalid_sql_source() {
    let file_path = write_temp_source(".sql", "CREATE TABLE users (\n  id INTEGER,\n");
    let request = json!({
        "files": [{
            "file": file_path.to_string_lossy(),
            "operations": []
        }],
        "transaction": {"mode": "all_or_nothing"}
    });
    let output = run_identedit_with_stdin(
        &["apply"],
        &serde_json::to_string(&request).expect("request should serialize"),
    );
    assert!(
        !output.status.success(),
        "apply should fail for syntax-invalid SQL source"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "parse_failure");
    let message = response["error"]["message"]
        .as_str()
        .expect("error.message should be a string");
    assert!(message.contains("tree-sitter-sql"));
}

#[test]
fn select_exposes_both_outer_and_inner_statements_for_cte_window_sql() {
    let source = "WITH ranked AS (\n  SELECT id, ROW_NUMBER() OVER (ORDER BY id) AS rn FROM users\n)\nSELECT id FROM ranked WHERE rn = 1;\n";
    let file_path = write_temp_source(".sql", source);
    let output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "statement",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "CTE/window SQL should parse: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    let statements = response["handles"]
        .as_array()
        .expect("handles should be an array");
    assert_eq!(
        statements.len(),
        2,
        "CTE should expose outer and inner statements"
    );
    assert!(
        statements.iter().any(|handle| {
            handle["text"]
                .as_str()
                .is_some_and(|text| text.starts_with("WITH ranked AS"))
        }),
        "outer CTE statement should be present"
    );
    assert!(
        statements.iter().any(|handle| {
            handle["text"].as_str().is_some_and(|text| {
                text == "SELECT id, ROW_NUMBER() OVER (ORDER BY id) AS rn FROM users"
            })
        }),
        "inner SELECT statement should be present"
    );
}

#[test]
fn select_parses_quoted_identifiers_and_unicode_sql_literals() {
    let source = "SELECT \"Display Name\", '한글😀' AS label FROM \"User Events\";\n";
    let file_path = write_temp_source(".sql", source);
    let output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "statement",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "quoted-identifier SQL should parse: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    let statements = response["handles"]
        .as_array()
        .expect("handles should be an array");
    assert_eq!(statements.len(), 1);
    assert!(
        statements[0]["text"]
            .as_str()
            .is_some_and(|text| text.contains("'한글😀'")),
        "unicode literal should remain in statement text"
    );
}

#[test]
fn transform_json_file_end_insert_and_apply_support_sql() {
    let file_path = copy_fixture_to_temp("example.sql", ".sql");
    let select_output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "statement",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        select_output.status.success(),
        "select failed: {}",
        String::from_utf8_lossy(&select_output.stderr)
    );

    let select_response: Value =
        serde_json::from_slice(&select_output.stdout).expect("stdout should be valid JSON");
    let expected_file_hash = select_response["file_preconditions"][0]["expected_file_hash"]
        .as_str()
        .expect("expected_file_hash should be present");

    let request = json!({
        "command": "transform",
        "file": file_path,
        "operations": [{
            "target": {
                "type": "file_end",
                "expected_file_hash": expected_file_hash
            },
            "op": {
                "type": "insert",
                "new_text": "\nSELECT count(*) FROM users;"
            }
        }]
    });

    let transform_output = run_identedit_with_stdin(&["transform", "--json"], &request.to_string());
    assert!(
        transform_output.status.success(),
        "transform file_end insert failed: {}",
        String::from_utf8_lossy(&transform_output.stderr)
    );

    let transform_json =
        std::str::from_utf8(&transform_output.stdout).expect("transform output should be utf-8");
    let apply_output = run_identedit_with_stdin(&["apply"], transform_json);
    assert!(
        apply_output.status.success(),
        "apply file_end insert failed: {}",
        String::from_utf8_lossy(&apply_output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("file should be readable");
    assert!(modified.contains("SELECT count(*) FROM users;"));
}

#[test]
fn transform_json_file_start_insert_and_apply_support_sql() {
    let file_path = copy_fixture_to_temp("example.sql", ".sql");
    let select_output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "statement",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        select_output.status.success(),
        "select failed: {}",
        String::from_utf8_lossy(&select_output.stderr)
    );

    let select_response: Value =
        serde_json::from_slice(&select_output.stdout).expect("stdout should be valid JSON");
    let expected_file_hash = select_response["file_preconditions"][0]["expected_file_hash"]
        .as_str()
        .expect("expected_file_hash should be present");

    let request = json!({
        "command": "transform",
        "file": file_path,
        "operations": [{
            "target": {
                "type": "file_start",
                "expected_file_hash": expected_file_hash
            },
            "op": {
                "type": "insert",
                "new_text": "-- generated by edge hunt\n"
            }
        }]
    });

    let transform_output = run_identedit_with_stdin(&["transform", "--json"], &request.to_string());
    assert!(
        transform_output.status.success(),
        "transform file_start insert failed: {}",
        String::from_utf8_lossy(&transform_output.stderr)
    );

    let transform_json =
        std::str::from_utf8(&transform_output.stdout).expect("transform output should be utf-8");
    let apply_output = run_identedit_with_stdin(&["apply"], transform_json);
    assert!(
        apply_output.status.success(),
        "apply file_start insert failed: {}",
        String::from_utf8_lossy(&apply_output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("file should be readable");
    assert!(
        modified.starts_with("-- generated by edge hunt\n"),
        "file_start insert should prepend comment"
    );
}

#[test]
fn transform_json_rejects_file_end_insert_with_stale_hash_sql() {
    let file_path = copy_fixture_to_temp("example.sql", ".sql");
    let request = json!({
        "command": "transform",
        "file": file_path,
        "operations": [{
            "target": {
                "type": "file_end",
                "expected_file_hash": "deadbeefdeadbeef"
            },
            "op": {
                "type": "insert",
                "new_text": "\nSELECT 42;"
            }
        }]
    });
    let output = run_identedit_with_stdin(&["transform", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "transform should reject stale file_end hash for SQL"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "precondition_failed");
}

#[test]
fn transform_json_rejects_file_start_insert_with_stale_hash_sql() {
    let file_path = copy_fixture_to_temp("example.sql", ".sql");
    let request = json!({
        "command": "transform",
        "file": file_path,
        "operations": [{
            "target": {
                "type": "file_start",
                "expected_file_hash": "deadbeefdeadbeef"
            },
            "op": {
                "type": "insert",
                "new_text": "-- stale\n"
            }
        }]
    });
    let output = run_identedit_with_stdin(&["transform", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "transform should reject stale file_start hash for SQL"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "precondition_failed");
}

#[test]
fn select_ignores_semicolons_inside_sql_line_comments_for_statement_count() {
    let source = "SELECT 1; -- ; ; ;\nSELECT 2;\n";
    let file_path = write_temp_source(".sql", source);
    let output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "statement",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "select failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    let statements = response["handles"]
        .as_array()
        .expect("handles should be an array");
    assert_eq!(statements.len(), 2, "expected two SQL statements");
}

#[test]
fn select_statement_handles_are_sorted_by_start_span_for_nested_sql() {
    let source = "WITH ranked AS (\n  SELECT id, ROW_NUMBER() OVER (ORDER BY id) AS rn FROM users\n)\nSELECT id FROM ranked WHERE rn = 1;\n";
    let file_path = write_temp_source(".sql", source);
    let output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "statement",
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

    let starts = handles
        .iter()
        .map(|handle| {
            handle["span"]["start"]
                .as_u64()
                .expect("span.start should be present")
        })
        .collect::<Vec<_>>();
    let mut sorted = starts.clone();
    sorted.sort_unstable();
    assert_eq!(
        starts, sorted,
        "statement handles should be ordered by start"
    );
}
