use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

use serde_json::Value;
use tempfile::Builder;

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

fn write_temp_text(suffix: &str, text: &str) -> PathBuf {
    write_temp_bytes(suffix, text.as_bytes())
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

#[test]
fn select_span_starts_after_utf8_bom_for_javascript_function() {
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(b"function processData(value) {\n  return value + 1;\n}\n");
    let file_path = write_temp_bytes(".js", &bytes);

    let output = run_identedit(&[
        "select",
        "--kind",
        "function_declaration",
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
    let function = handles
        .iter()
        .find(|handle| handle["name"] == "processData")
        .expect("function handle should exist");
    assert_eq!(function["span"]["start"], 3);
}

#[test]
fn transform_apply_preserves_crlf_segments_for_typescript() {
    let file_path = write_temp_text(
        ".ts",
        "function processData(value: number): number {\r\n  return value + 1;\r\n}\r\n\r\nfunction untouched(value: number): number {\r\n  return value + 2;\r\n}\r\n",
    );
    let path = file_path.to_str().expect("path should be utf-8");

    let select_output = run_identedit(&[
        "select",
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

    let replacement = "function processData(value: number): number {\r\n  return value - 1;\r\n}";
    let transform_output = run_identedit(&[
        "transform",
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
    assert!(modified.contains("\r\n"), "CRLF line endings should remain");
    assert!(
        modified
            .contains("function untouched(value: number): number {\r\n  return value + 2;\r\n}"),
        "untouched section should preserve CRLF bytes"
    );
}

#[test]
fn transform_apply_preserves_cr_only_segments_for_tsx() {
    let file_path = write_temp_text(
        ".tsx",
        "export function View(): JSX.Element {\r  return <div>Hello</div>;\r}\r\rconst untouched = () => <span>ok</span>;\r",
    );
    let path = file_path.to_str().expect("path should be utf-8");

    let select_output = run_identedit(&[
        "select",
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

    let replacement = "export function View(): JSX.Element {\r  return <main>Updated</main>;\r}";
    let transform_output = run_identedit(&[
        "transform",
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
    assert!(
        modified.contains('\r'),
        "CR-only separators should remain in output"
    );
    assert!(
        !modified.contains('\n'),
        "CR-only source should not be normalized to LF/CRLF"
    );
    assert!(
        modified.contains("const untouched = () => <span>ok</span>;\r"),
        "untouched section should preserve CR-only separator"
    );
}
