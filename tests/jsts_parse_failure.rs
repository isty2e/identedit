use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

use serde_json::{Value, json};
use tempfile::Builder;

struct SyntaxFailureCase {
    suffix: &'static str,
    source: &'static str,
    provider: &'static str,
    syntax_message: &'static str,
}

fn invalid_syntax_cases() -> Vec<SyntaxFailureCase> {
    vec![
        SyntaxFailureCase {
            suffix: ".js",
            source: "function broken( {\n  return 1;\n}\n",
            provider: "tree-sitter-javascript",
            syntax_message: "Syntax errors detected in JavaScript source",
        },
        SyntaxFailureCase {
            suffix: ".jsx",
            source: "function View() {\n  return <div>;\n}\n",
            provider: "tree-sitter-javascript",
            syntax_message: "Syntax errors detected in JavaScript source",
        },
        SyntaxFailureCase {
            suffix: ".ts",
            source: "function broken(value: number {\n  return value;\n}\n",
            provider: "tree-sitter-typescript",
            syntax_message: "Syntax errors detected in TypeScript source",
        },
        SyntaxFailureCase {
            suffix: ".tsx",
            source: "export function View(): JSX.Element {\n  return <div>;\n}\n",
            provider: "tree-sitter-tsx",
            syntax_message: "Syntax errors detected in TSX source",
        },
    ]
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

fn assert_parse_failure(output: Output, expected_provider: &str, expected_syntax_message: &str) {
    assert!(
        !output.status.success(),
        "command should fail for syntax-invalid source"
    );
    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "parse_failure");
    let message = response["error"]["message"]
        .as_str()
        .expect("error.message should be a string");
    assert!(
        message.contains(expected_provider),
        "message should mention provider '{expected_provider}', got: {message}"
    );
    assert!(
        message.contains(expected_syntax_message),
        "message should contain syntax detail '{expected_syntax_message}', got: {message}"
    );
}

#[test]
fn select_reports_parse_failure_for_syntax_invalid_jsts_variants() {
    for case in invalid_syntax_cases() {
        let file_path = write_temp_source(case.suffix, case.source);
        let output = run_identedit(&[
            "read",
            "--json",
            "--kind",
            "function_declaration",
            file_path.to_str().expect("path should be utf-8"),
        ]);

        assert_parse_failure(output, case.provider, case.syntax_message);
        fs::remove_file(&file_path).expect("temp file cleanup should succeed");
    }
}

#[test]
fn transform_reports_parse_failure_for_syntax_invalid_jsts_variants() {
    for case in invalid_syntax_cases() {
        let file_path = write_temp_source(case.suffix, case.source);
        let output = run_identedit(&[
            "edit",
            "--identity",
            "deadbeef",
            "--replace",
            "function replacement() { return 0; }",
            file_path.to_str().expect("path should be utf-8"),
        ]);

        assert_parse_failure(output, case.provider, case.syntax_message);
        fs::remove_file(&file_path).expect("temp file cleanup should succeed");
    }
}

#[test]
fn apply_reports_parse_failure_for_syntax_invalid_jsts_variants() {
    for case in invalid_syntax_cases() {
        let file_path = write_temp_source(case.suffix, case.source);
        let request = json!({
            "files": [{
                "file": file_path.to_str().expect("path should be utf-8"),
                "operations": [{
                    "target": {
                        "identity": "deadbeef",
                        "kind": "function_declaration",
                        "span_hint": { "start": 0, "end": 1 },
                        "expected_old_hash": "00"
                    },
                    "op": { "type": "replace", "new_text": "function replacement() { return 0; }" },
                    "preview": {
                        "old_text": "x",
                        "new_text": "function replacement() { return 0; }",
                        "matched_span": { "start": 0, "end": 1 }
                    }
                }]
            }],
            "transaction": { "mode": "all_or_nothing" }
        });

        let output = run_identedit_with_stdin(&["apply"], &request.to_string());
        assert_parse_failure(output, case.provider, case.syntax_message);
        fs::remove_file(&file_path).expect("temp file cleanup should succeed");
    }
}
