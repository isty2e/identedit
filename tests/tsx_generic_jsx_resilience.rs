use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

use serde_json::Value;
use tempfile::Builder;

fn write_temp_tsx(source: &str) -> PathBuf {
    let mut temp_file = Builder::new()
        .suffix(".tsx")
        .tempfile()
        .expect("temp tsx file should be created");
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

#[test]
fn tsx_generic_and_jsx_mixture_select_is_deterministic() {
    let file_path = write_temp_tsx(
        "type Box<T> = { value: T };\n\nconst identity = <T,>(value: T): T => value;\n\nexport function View<T extends { id: number }>(props: T): JSX.Element {\n  return <section>{identity<Box<number>>({ value: props.id }).value}</section>;\n}\n",
    );
    let path = file_path.to_str().expect("path should be utf-8");

    let first = run_identedit(&[
        "read",
        "--json",
        "--mode",
        "ast",
        "--kind",
        "arrow_function",
        path,
    ]);
    let second = run_identedit(&[
        "read",
        "--json",
        "--mode",
        "ast",
        "--kind",
        "arrow_function",
        path,
    ]);
    assert!(
        first.status.success(),
        "first select failed: {}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert!(
        second.status.success(),
        "second select failed: {}",
        String::from_utf8_lossy(&second.stderr)
    );

    let first_response: Value =
        serde_json::from_slice(&first.stdout).expect("stdout should be valid JSON");
    let second_response: Value =
        serde_json::from_slice(&second.stdout).expect("stdout should be valid JSON");
    assert_eq!(
        first_response, second_response,
        "repeated select output should be deterministic for generic/JSX boundary input"
    );

    let function_output = run_identedit(&[
        "read",
        "--json",
        "--kind",
        "function_declaration",
        "--name",
        "View",
        path,
    ]);
    assert!(
        function_output.status.success(),
        "function select failed: {}",
        String::from_utf8_lossy(&function_output.stderr)
    );
    let function_response: Value =
        serde_json::from_slice(&function_output.stdout).expect("stdout should be valid JSON");
    assert_eq!(
        function_response["handles"]
            .as_array()
            .expect("handles should be array")
            .len(),
        1
    );
}

#[test]
fn tsx_generic_function_can_be_transformed_and_applied_without_parser_drift() {
    let file_path = write_temp_tsx(
        "type Box<T> = { value: T };\n\nconst identity = <T,>(value: T): T => value;\n\nexport function View<T extends { id: number }>(props: T): JSX.Element {\n  return <section>{identity<Box<number>>({ value: props.id }).value}</section>;\n}\n",
    );
    let path = file_path.to_str().expect("path should be utf-8");

    let select_output = run_identedit(&[
        "read",
        "--json",
        "--kind",
        "function_declaration",
        "--name",
        "View",
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

    let replacement = "export function View<T extends { id: number }>(props: T): JSX.Element {\n  return <article>{identity<Box<number>>({ value: props.id }).value}</article>;\n}";
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

    let transform_json =
        std::str::from_utf8(&transform_output.stdout).expect("transform output should be utf-8");
    let apply_output = run_identedit_with_stdin(&["apply"], transform_json);
    assert!(
        apply_output.status.success(),
        "apply failed: {}",
        String::from_utf8_lossy(&apply_output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("file should be readable");
    assert!(modified.contains("<article>"));
    assert!(modified.contains("</article>"));
}
