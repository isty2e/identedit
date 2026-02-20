use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

use serde_json::Value;
use tempfile::Builder;

struct StaleCase {
    suffix: &'static str,
    source: &'static str,
    replacement: &'static str,
}

fn stale_cases() -> Vec<StaleCase> {
    vec![
        StaleCase {
            suffix: ".js",
            source: "function processData(value) {\n  return value + 1;\n}\n\nfunction helper(value) {\n  return value + 2;\n}\n",
            replacement: "function processData(value) {\n  return value - 1;\n}",
        },
        StaleCase {
            suffix: ".ts",
            source: "function processData(value: number): number {\n  return value + 1;\n}\n\nfunction helper(value: number): number {\n  return value + 2;\n}\n",
            replacement: "function processData(value: number): number {\n  return value - 1;\n}",
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

fn build_changeset_json(path: &str, replacement: &str) -> String {
    let select_output = run_identedit(&[
        "read",
        "--json",
        "--kind",
        "function_declaration",
        "--name",
        "process*",
        path,
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

    let transform_output = run_identedit(&[
        "edit",
        "--identity",
        identity,
        "--replace",
        replacement,
        path,
    ]);
    assert!(
        transform_output.status.success(),
        "transform failed: {}",
        String::from_utf8_lossy(&transform_output.stderr)
    );

    std::str::from_utf8(&transform_output.stdout)
        .expect("transform output should be utf-8")
        .to_string()
}

fn prepend_comment_header(original: &str) -> String {
    format!("// shifted header\n{original}")
}

fn prepend_blank_line(original: &str) -> String {
    format!("\n{original}")
}

#[test]
fn apply_after_offset_shift_mutation_recovers_via_kind_hash_for_jsts() {
    let shift_mutations: [fn(&str) -> String; 2] = [prepend_comment_header, prepend_blank_line];

    for case in stale_cases() {
        for mutate in shift_mutations {
            let file_path = write_temp_source(case.suffix, case.source);
            let path = file_path.to_str().expect("path should be utf-8");
            let changeset_json = build_changeset_json(path, case.replacement);

            let original = fs::read_to_string(&file_path).expect("file should be readable");
            let mutated = mutate(&original);
            let prefix_len = mutated.len() - original.len();
            let prefix = mutated[..prefix_len].to_string();
            fs::write(&file_path, mutated).expect("file mutation should succeed");

            let apply_output = run_identedit_with_stdin(&["apply"], &changeset_json);
            assert!(
                apply_output.status.success(),
                "apply should recover via unique kind+hash after offset-shift mutation: {}",
                String::from_utf8_lossy(&apply_output.stderr)
            );

            let updated = fs::read_to_string(&file_path).expect("file should remain readable");
            assert!(
                updated.starts_with(&prefix),
                "header/blank-line prefix should be preserved after recovery"
            );
            assert!(
                updated.contains("return value - 1;"),
                "replacement should still apply after offset-shift recovery"
            );
        }
    }
}

#[test]
fn apply_after_same_span_text_change_returns_deterministic_precondition_failed_for_jsts() {
    for case in stale_cases() {
        let file_path = write_temp_source(case.suffix, case.source);
        let path = file_path.to_str().expect("path should be utf-8");
        let changeset_json = build_changeset_json(path, case.replacement);

        let original = fs::read_to_string(&file_path).expect("file should be readable");
        let mutated = original.replacen("return value + 1;", "return value + 2;", 1);
        fs::write(&file_path, mutated).expect("file mutation should succeed");

        let apply_output = run_identedit_with_stdin(&["apply"], &changeset_json);
        assert!(
            !apply_output.status.success(),
            "apply should fail after same-span stale edit"
        );
        let response: Value =
            serde_json::from_slice(&apply_output.stdout).expect("stdout should be valid JSON");
        assert_eq!(response["error"]["type"], "precondition_failed");
    }
}
