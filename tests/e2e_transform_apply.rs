use std::fs;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

use serde_json::{Value, json};
use tempfile::{Builder, tempdir};

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn copy_fixture_to_temp_python(name: &str) -> PathBuf {
    let source = fixture_path(name);
    let content = fs::read_to_string(&source).expect("fixture should be readable");
    let mut temp_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp python file should be created");
    temp_file
        .write_all(content.as_bytes())
        .expect("temp fixture write should succeed");
    temp_file.keep().expect("temp file should persist").1
}

fn copy_fixture_to_temp_json(name: &str) -> PathBuf {
    let source = fixture_path(name);
    let content = fs::read_to_string(&source).expect("fixture should be readable");
    let mut temp_file = Builder::new()
        .suffix(".json")
        .tempfile()
        .expect("temp json file should be created");
    temp_file
        .write_all(content.as_bytes())
        .expect("temp fixture write should succeed");
    temp_file.keep().expect("temp file should persist").1
}

fn run_identedit(arguments: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_identedit"));
    command.env("IDENTEDIT_ALLOW_LEGACY", "1");
    command.args(arguments);
    command.output().expect("failed to run identedit binary")
}

fn run_identedit_in_dir(directory: &Path, arguments: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_identedit"));
    command.env("IDENTEDIT_ALLOW_LEGACY", "1");
    command.current_dir(directory);
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

fn run_identedit_with_stdin_in_dir(directory: &Path, arguments: &[&str], input: &str) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_identedit"));
    command.env("IDENTEDIT_ALLOW_LEGACY", "1");
    command.current_dir(directory);
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

fn line_ref(source: &str, line: usize) -> String {
    let line_text = source
        .lines()
        .nth(line - 1)
        .expect("line should exist for anchor");
    format!(
        "{line}:{}",
        identedit::hashline::compute_line_hash(line_text)
    )
}

fn select_function_handle_by_name(file_path: &Path, name: &str) -> Value {
    let output = run_identedit(&[
        "select",
        "--verbose",
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
    response["handles"]
        .as_array()
        .expect("handles should be array")
        .iter()
        .find(|handle| handle["name"].as_str() == Some(name))
        .cloned()
        .expect("named handle should exist")
}

#[test]
fn select_transform_apply_pipeline_edits_file_end_to_end() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let select_output = run_identedit(&[
        "select",
        "--verbose",
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

    let replacement = "def process_data(value):\n    return value - 1";
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

    let apply_response: Value =
        serde_json::from_slice(&apply_output.stdout).expect("stdout should be valid JSON");
    assert_eq!(apply_response["summary"]["files_modified"], 1);
    assert_eq!(apply_response["summary"]["operations_applied"], 1);

    let modified_file = fs::read_to_string(&file_path).expect("file should be readable");
    assert!(modified_file.contains("return value - 1"));
}

#[test]
fn select_transform_apply_pipeline_edits_unknown_extension_via_fallback() {
    let mut temporary_file = Builder::new()
        .suffix(".txt")
        .tempfile()
        .expect("temp file should be created");
    temporary_file
        .write_all(b"def process_data(value):\n    return value + 1\n")
        .expect("fixture write should succeed");
    let file_path = temporary_file.keep().expect("temp file should persist").1;

    let select_output = run_identedit(&[
        "select",
        "--verbose",
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

    let replacement = "def process_data(value):\n    return value - 3\n";
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

    let apply_response: Value =
        serde_json::from_slice(&apply_output.stdout).expect("stdout should be valid JSON");
    assert_eq!(apply_response["summary"]["files_modified"], 1);
    assert_eq!(apply_response["summary"]["operations_applied"], 1);

    let modified_file = fs::read_to_string(&file_path).expect("file should be readable");
    assert!(modified_file.contains("return value - 3"));
}

#[test]
fn select_transform_apply_pipeline_edits_fallback_js_unicode_escape_name() {
    let mut temporary_file = Builder::new()
        .suffix(".txt")
        .tempfile()
        .expect("temp file should be created");
    temporary_file
        .write_all(b"function \\u0066oo(value) {\n  return value + 1;\n}\n")
        .expect("fixture write should succeed");
    let file_path = temporary_file.keep().expect("temp file should persist").1;

    let select_output = run_identedit(&[
        "select",
        "--verbose",
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
    assert_eq!(select_response["handles"][0]["name"], "\\u0066oo");
    let identity = select_response["handles"][0]["identity"]
        .as_str()
        .expect("identity should be present");

    let replacement = "function \\u0066oo(value) {\n  return value - 2;\n}\n";
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

    let apply_response: Value =
        serde_json::from_slice(&apply_output.stdout).expect("stdout should be valid JSON");
    assert_eq!(apply_response["summary"]["files_modified"], 1);
    assert_eq!(apply_response["summary"]["operations_applied"], 1);

    let modified_file = fs::read_to_string(&file_path).expect("file should be readable");
    assert!(modified_file.contains("function \\u0066oo(value)"));
    assert!(modified_file.contains("return value - 2;"));
}

#[test]
fn select_transform_apply_pipeline_supports_relative_dot_segment_paths() {
    let workspace = tempdir().expect("tempdir should be created");
    let nested = workspace.path().join("nested");
    fs::create_dir_all(&nested).expect("nested directory should be created");

    let target_path = workspace.path().join("target.py");
    let source = fixture_path("example.py");
    let content = fs::read_to_string(&source).expect("fixture should be readable");
    fs::write(&target_path, content).expect("target fixture write should succeed");

    let relative_path = "./nested/../target.py";
    let select_output = run_identedit_in_dir(
        workspace.path(),
        &[
            "select",
            "--verbose",
            "--kind",
            "function_definition",
            "--name",
            "process_*",
            relative_path,
        ],
    );
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

    let replacement = "def process_data(value):\n    return value * 3";
    let transform_output = run_identedit_in_dir(
        workspace.path(),
        &[
            "transform",
            "--identity",
            identity,
            "--replace",
            replacement,
            relative_path,
        ],
    );
    assert!(
        transform_output.status.success(),
        "transform failed: {}",
        String::from_utf8_lossy(&transform_output.stderr)
    );

    let transform_json =
        std::str::from_utf8(&transform_output.stdout).expect("transform output should be utf-8");
    let apply_output =
        run_identedit_with_stdin_in_dir(workspace.path(), &["apply"], transform_json);
    assert!(
        apply_output.status.success(),
        "apply failed: {}",
        String::from_utf8_lossy(&apply_output.stderr)
    );

    let apply_response: Value =
        serde_json::from_slice(&apply_output.stdout).expect("stdout should be valid JSON");
    assert_eq!(apply_response["summary"]["files_modified"], 1);
    assert_eq!(apply_response["summary"]["operations_applied"], 1);

    let modified_file = fs::read_to_string(&target_path).expect("file should be readable");
    assert!(modified_file.contains("return value * 3"));
}

#[test]
fn transform_apply_pipeline_supports_mixed_node_and_line_targets() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let source = fs::read_to_string(&file_path).expect("fixture should be readable");
    let helper_line = source
        .lines()
        .enumerate()
        .find_map(|(index, line)| line.contains("return \"helper\"").then_some(index + 1))
        .expect("helper return line should exist");

    let select_output = run_identedit(&[
        "select",
        "--verbose",
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
    let handle = &select_response["handles"][0];

    let transform_request = json!({
        "command": "transform",
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "identity": handle["identity"],
                "kind": handle["kind"],
                "span_hint": handle["span"],
                "expected_old_hash": identedit::changeset::hash_text(
                    handle["text"].as_str().expect("text should be present")
                ),
                "op": {
                    "type": "replace",
                    "new_text": "def process_data(value):\n    return value * 10"
                }
            },
            {
                "target": {
                    "type": "line",
                    "anchor": line_ref(&source, helper_line)
                },
                "op": {
                    "type": "set_line",
                    "new_text": "    return \"helper-updated\""
                }
            }
        ]
    });
    let transform_output =
        run_identedit_with_stdin(&["transform", "--json"], &transform_request.to_string());
    assert!(
        transform_output.status.success(),
        "transform with mixed targets failed: {}",
        String::from_utf8_lossy(&transform_output.stderr)
    );

    let transform_json =
        std::str::from_utf8(&transform_output.stdout).expect("transform output should be utf-8");
    let apply_output = run_identedit_with_stdin(&["apply"], transform_json);
    assert!(
        apply_output.status.success(),
        "apply for mixed node+line changeset failed: {}",
        String::from_utf8_lossy(&apply_output.stderr)
    );

    let apply_response: Value =
        serde_json::from_slice(&apply_output.stdout).expect("stdout should be valid JSON");
    assert_eq!(apply_response["summary"]["files_modified"], 1);
    assert_eq!(apply_response["summary"]["operations_applied"], 2);
    assert_eq!(apply_response["summary"]["operations_failed"], 0);

    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    assert!(modified.contains("return value * 10"));
    assert!(modified.contains("return \"helper-updated\""));
}

#[test]
fn select_transform_apply_pipeline_supports_parent_segment_paths() {
    let workspace = tempdir().expect("tempdir should be created");
    let nested = workspace.path().join("nested");
    fs::create_dir_all(&nested).expect("nested directory should be created");

    let target_path = workspace.path().join("target.py");
    let source = fixture_path("example.py");
    let content = fs::read_to_string(&source).expect("fixture should be readable");
    fs::write(&target_path, content).expect("target fixture write should succeed");

    let relative_path = "../target.py";
    let select_output = run_identedit_in_dir(
        &nested,
        &[
            "select",
            "--verbose",
            "--kind",
            "function_definition",
            "--name",
            "process_*",
            relative_path,
        ],
    );
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

    let replacement = "def process_data(value):\n    return value * 5";
    let transform_output = run_identedit_in_dir(
        &nested,
        &[
            "transform",
            "--identity",
            identity,
            "--replace",
            replacement,
            relative_path,
        ],
    );
    assert!(
        transform_output.status.success(),
        "transform failed: {}",
        String::from_utf8_lossy(&transform_output.stderr)
    );

    let transform_json =
        std::str::from_utf8(&transform_output.stdout).expect("transform output should be utf-8");
    let apply_output = run_identedit_with_stdin_in_dir(&nested, &["apply"], transform_json);
    assert!(
        apply_output.status.success(),
        "apply failed: {}",
        String::from_utf8_lossy(&apply_output.stderr)
    );

    let modified_file = fs::read_to_string(&target_path).expect("file should be readable");
    assert!(modified_file.contains("return value * 5"));
}

#[test]
fn select_transform_apply_pipeline_supports_delete_operation() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let select_output = run_identedit(&[
        "select",
        "--verbose",
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
    let original_text = select_response["handles"][0]["text"]
        .as_str()
        .expect("selected text should be present")
        .to_string();

    let transform_output = run_identedit(&[
        "transform",
        "--identity",
        identity,
        "--delete",
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

    let modified_file = fs::read_to_string(&file_path).expect("file should be readable");
    assert!(
        !modified_file.contains(&original_text),
        "delete pipeline should remove selected anchor text"
    );
    assert!(
        modified_file.contains("def helper():"),
        "delete pipeline should preserve unrelated function"
    );
}

#[test]
fn select_transform_apply_pipeline_supports_insert_before_and_after() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let select_output = run_identedit(&[
        "select",
        "--verbose",
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
    let handle = &select_response["handles"][0];
    let span_start = handle["span"]["start"].as_u64().expect("span start");
    let span_end = handle["span"]["end"].as_u64().expect("span end");
    let anchor_text = handle["text"]
        .as_str()
        .expect("anchor text should be present");

    let before_insert = "# e2e-before\n";
    let after_insert = "\n# e2e-after";
    let request = json!({
        "command": "transform",
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "identity": handle["identity"],
                "kind": handle["kind"],
                "span_hint": {
                    "start": span_start,
                    "end": span_end
                },
                "expected_old_hash": identedit::changeset::hash_text(anchor_text),
                "op": {
                    "type": "insert_before",
                    "new_text": before_insert
                }
            },
            {
                "identity": handle["identity"],
                "kind": handle["kind"],
                "span_hint": {
                    "start": span_start,
                    "end": span_end
                },
                "expected_old_hash": identedit::changeset::hash_text(anchor_text),
                "op": {
                    "type": "insert_after",
                    "new_text": after_insert
                }
            }
        ]
    });

    let transform_output = run_identedit_with_stdin(&["transform", "--json"], &request.to_string());
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

    let modified_file = fs::read_to_string(&file_path).expect("file should be readable");
    assert!(
        modified_file.contains("# e2e-before\ndef process_data"),
        "insert_before pipeline should insert text before anchor"
    );
    assert!(
        modified_file.contains("return result\n# e2e-after"),
        "insert_after pipeline should insert text after anchor"
    );
}

#[test]
fn select_transform_apply_pipeline_supports_same_file_move_before() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let source_before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let source_handle = select_function_handle_by_name(&file_path, "helper");
    let destination_handle = select_function_handle_by_name(&file_path, "process_data");

    let request = json!({
        "command": "transform",
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "identity": source_handle["identity"],
                    "kind": source_handle["kind"],
                    "span_hint": source_handle["span"],
                    "expected_old_hash": identedit::changeset::hash_text(
                        source_handle["text"].as_str().expect("source text should be present")
                    )
                },
                "op": {
                    "type": "move_before",
                    "destination": {
                        "identity": destination_handle["identity"],
                        "kind": destination_handle["kind"],
                        "span_hint": destination_handle["span"],
                        "expected_old_hash": identedit::changeset::hash_text(
                            destination_handle["text"]
                                .as_str()
                                .expect("destination text should be present")
                        )
                    }
                }
            }
        ]
    });
    let transform_output = run_identedit_with_stdin(&["transform", "--json"], &request.to_string());
    assert!(
        transform_output.status.success(),
        "transform same-file move_before failed: {}",
        String::from_utf8_lossy(&transform_output.stderr)
    );

    let transform_json =
        std::str::from_utf8(&transform_output.stdout).expect("transform output should be utf-8");
    let apply_output = run_identedit_with_stdin(&["apply"], transform_json);
    assert!(
        apply_output.status.success(),
        "apply same-file move_before failed: {}",
        String::from_utf8_lossy(&apply_output.stderr)
    );

    let apply_response: Value =
        serde_json::from_slice(&apply_output.stdout).expect("stdout should be valid JSON");
    assert_eq!(apply_response["summary"]["files_modified"], 1);
    assert_eq!(apply_response["summary"]["operations_applied"], 1);
    assert_eq!(apply_response["summary"]["operations_failed"], 0);

    let modified_file = fs::read_to_string(&file_path).expect("file should be readable");
    let helper_pos = modified_file
        .find("def helper(")
        .expect("helper definition should exist");
    let process_pos = modified_file
        .find("def process_data(")
        .expect("process_data definition should exist");
    assert!(
        helper_pos < process_pos,
        "move_before should place helper before process_data"
    );
    assert_eq!(modified_file.matches("def helper(").count(), 1);
    assert_eq!(modified_file.matches("def process_data(").count(), 1);
    assert_ne!(
        modified_file, source_before,
        "same-file move should mutate source ordering"
    );
}

#[test]
fn select_transform_apply_pipeline_supports_same_file_move_after() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let source_handle = select_function_handle_by_name(&file_path, "process_data");
    let destination_handle = select_function_handle_by_name(&file_path, "helper");

    let request = json!({
        "command": "transform",
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "identity": source_handle["identity"],
                    "kind": source_handle["kind"],
                    "span_hint": source_handle["span"],
                    "expected_old_hash": identedit::changeset::hash_text(
                        source_handle["text"].as_str().expect("source text should be present")
                    )
                },
                "op": {
                    "type": "move_after",
                    "destination": {
                        "identity": destination_handle["identity"],
                        "kind": destination_handle["kind"],
                        "span_hint": destination_handle["span"],
                        "expected_old_hash": identedit::changeset::hash_text(
                            destination_handle["text"]
                                .as_str()
                                .expect("destination text should be present")
                        )
                    }
                }
            }
        ]
    });
    let transform_output = run_identedit_with_stdin(&["transform", "--json"], &request.to_string());
    assert!(
        transform_output.status.success(),
        "transform same-file move_after failed: {}",
        String::from_utf8_lossy(&transform_output.stderr)
    );

    let transform_json =
        std::str::from_utf8(&transform_output.stdout).expect("transform output should be utf-8");
    let apply_output = run_identedit_with_stdin(&["apply"], transform_json);
    assert!(
        apply_output.status.success(),
        "apply same-file move_after failed: {}",
        String::from_utf8_lossy(&apply_output.stderr)
    );

    let modified_file = fs::read_to_string(&file_path).expect("file should be readable");
    let helper_pos = modified_file
        .find("def helper(")
        .expect("helper definition should exist");
    let process_pos = modified_file
        .find("def process_data(")
        .expect("process_data definition should exist");
    assert!(
        helper_pos < process_pos,
        "move_after(process_data -> helper) should place process_data after helper"
    );
    assert_eq!(modified_file.matches("def helper(").count(), 1);
    assert_eq!(modified_file.matches("def process_data(").count(), 1);
}

#[test]
fn select_transform_apply_pipeline_supports_cross_file_move_to_before() {
    let workspace = tempdir().expect("tempdir should be created");
    let source_file = workspace.path().join("source.py");
    let destination_file = workspace.path().join("destination.py");
    fs::write(
        &source_file,
        "def source_fn(value):\n    return value + 1\n\n\ndef keep_source(value):\n    return value + 2\n",
    )
    .expect("source fixture write should succeed");
    fs::write(
        &destination_file,
        "def destination_anchor(value):\n    return value * 2\n",
    )
    .expect("destination fixture write should succeed");

    let source_handle = select_function_handle_by_name(&source_file, "source_fn");
    let destination_handle =
        select_function_handle_by_name(&destination_file, "destination_anchor");

    let request = json!({
        "command": "transform",
        "file": source_file.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "identity": source_handle["identity"],
                    "kind": source_handle["kind"],
                    "span_hint": source_handle["span"],
                    "expected_old_hash": identedit::changeset::hash_text(
                        source_handle["text"].as_str().expect("source text should be present")
                    )
                },
                "op": {
                    "type": "move_to_before",
                    "destination_file": destination_file.to_string_lossy().to_string(),
                    "destination": {
                        "identity": destination_handle["identity"],
                        "kind": destination_handle["kind"],
                        "span_hint": destination_handle["span"],
                        "expected_old_hash": identedit::changeset::hash_text(
                            destination_handle["text"].as_str().expect("destination text should be present")
                        )
                    }
                }
            }
        ]
    });
    let transform_output = run_identedit_with_stdin(&["transform", "--json"], &request.to_string());
    assert!(
        transform_output.status.success(),
        "transform cross-file move_to_before failed: {}",
        String::from_utf8_lossy(&transform_output.stderr)
    );

    let transform_response: Value =
        serde_json::from_slice(&transform_output.stdout).expect("stdout should be valid JSON");
    assert_eq!(
        transform_response["files"].as_array().map(Vec::len),
        Some(2),
        "cross-file move should normalize to two file changes"
    );

    let transform_json =
        std::str::from_utf8(&transform_output.stdout).expect("transform output should be utf-8");
    let apply_output = run_identedit_with_stdin(&["apply"], transform_json);
    assert!(
        apply_output.status.success(),
        "apply cross-file move_to_before failed: {}",
        String::from_utf8_lossy(&apply_output.stderr)
    );

    let apply_response: Value =
        serde_json::from_slice(&apply_output.stdout).expect("stdout should be valid JSON");
    assert_eq!(apply_response["summary"]["files_modified"], 2);
    assert_eq!(apply_response["summary"]["operations_applied"], 2);
    assert_eq!(apply_response["summary"]["operations_failed"], 0);

    let source_after = fs::read_to_string(&source_file).expect("source should be readable");
    let destination_after =
        fs::read_to_string(&destination_file).expect("destination should be readable");
    assert!(
        !source_after.contains("def source_fn("),
        "source file should delete moved node"
    );
    assert!(
        source_after.contains("def keep_source("),
        "source file should preserve unrelated definitions"
    );
    let moved_pos = destination_after
        .find("def source_fn(")
        .expect("destination should contain moved source function");
    let anchor_pos = destination_after
        .find("def destination_anchor(")
        .expect("destination anchor should remain");
    assert!(
        moved_pos < anchor_pos,
        "moved node should be inserted before destination anchor"
    );
}

#[test]
fn select_transform_apply_pipeline_replaces_json_nested_object_without_side_effects() {
    let file_path = copy_fixture_to_temp_json("example.json");
    let select_output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "object",
        "--name",
        "config",
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

    let replacement =
        "{\n    \"enabled\": false,\n    \"retries\": 10,\n    \"mode\": \"safe\"\n  }";
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

    let updated_text = fs::read_to_string(&file_path).expect("file should be readable");
    let updated_json: Value = serde_json::from_str(&updated_text)
        .expect("updated JSON should remain syntactically valid");
    assert_eq!(updated_json["name"], "identedit");
    assert_eq!(updated_json["config"]["enabled"], false);
    assert_eq!(updated_json["config"]["retries"], 10);
    assert_eq!(updated_json["config"]["mode"], "safe");
    assert_eq!(updated_json["items"], json!([1, 2, 3]));
}

#[test]
fn select_transform_apply_pipeline_inserts_json_key_before_existing_key() {
    let file_path = copy_fixture_to_temp_json("example.json");
    let select_output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "key",
        "--name",
        "name",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        select_output.status.success(),
        "select failed: {}",
        String::from_utf8_lossy(&select_output.stderr)
    );

    let select_response: Value =
        serde_json::from_slice(&select_output.stdout).expect("stdout should be valid JSON");
    let handle = &select_response["handles"][0];
    let anchor_text = handle["text"]
        .as_str()
        .expect("anchor text should be present");
    let span_start = handle["span"]["start"].as_u64().expect("span start");
    let span_end = handle["span"]["end"].as_u64().expect("span end");

    let request = json!({
        "command": "transform",
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "identity": handle["identity"],
                "kind": handle["kind"],
                "span_hint": {"start": span_start, "end": span_end},
                "expected_old_hash": identedit::changeset::hash_text(anchor_text),
                "op": {
                    "type": "insert_before",
                    "new_text": "\"version\": 1,\n  "
                }
            }
        ]
    });

    let transform_output = run_identedit_with_stdin(&["transform", "--json"], &request.to_string());
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

    let updated_text = fs::read_to_string(&file_path).expect("file should be readable");
    let updated_json: Value = serde_json::from_str(&updated_text)
        .expect("updated JSON should remain syntactically valid");
    assert_eq!(updated_json["version"], 1);
    assert_eq!(updated_json["name"], "identedit");
    assert_eq!(updated_json["items"], json!([1, 2, 3]));
}

#[test]
fn stale_json_delete_changeset_returns_precondition_failed_after_mutation() {
    let file_path = copy_fixture_to_temp_json("example.json");
    let select_output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "object",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        select_output.status.success(),
        "select failed: {}",
        String::from_utf8_lossy(&select_output.stderr)
    );

    let select_response: Value =
        serde_json::from_slice(&select_output.stdout).expect("stdout should be valid JSON");
    let root_handle = select_response["handles"]
        .as_array()
        .expect("handles should be an array")
        .iter()
        .find(|handle| handle["name"].is_null())
        .expect("root object handle should exist");
    let identity = root_handle["identity"]
        .as_str()
        .expect("identity should be present");

    let transform_output = run_identedit(&[
        "transform",
        "--identity",
        identity,
        "--delete",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        transform_output.status.success(),
        "transform failed: {}",
        String::from_utf8_lossy(&transform_output.stderr)
    );

    let original_text = fs::read_to_string(&file_path).expect("file should be readable");
    let mutated_text = original_text.replacen("\"retries\": 3", "\"retries\": 4", 1);
    fs::write(&file_path, &mutated_text).expect("mutated fixture write should succeed");

    let transform_json =
        std::str::from_utf8(&transform_output.stdout).expect("transform output should be utf-8");
    let apply_output = run_identedit_with_stdin(&["apply"], transform_json);
    assert!(
        !apply_output.status.success(),
        "apply should fail for stale delete changeset"
    );

    let response: Value =
        serde_json::from_slice(&apply_output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "precondition_failed");

    let after = fs::read_to_string(&file_path).expect("file should be readable");
    assert_eq!(
        after, mutated_text,
        "failed apply must not modify target file"
    );
}
