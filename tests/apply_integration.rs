#[cfg(unix)]
use std::ffi::OsString;
use std::fs;
use std::fs::FileTimes;
use std::fs::OpenOptions;
use std::io::Write;
#[cfg(unix)]
use std::os::unix::ffi::OsStringExt;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use fs2::FileExt;
use serde_json::{Value, json};
use tempfile::{Builder, NamedTempFile, tempdir};

mod common;

fn fixture_path(name: &str) -> PathBuf {
    common::fixture_path(name)
}

fn copy_fixture_to_temp_python(name: &str) -> PathBuf {
    common::copy_fixture_to_temp_python(name)
}

fn copy_fixture_to_temp_json(name: &str) -> PathBuf {
    common::copy_fixture_to_temp_json(name)
}

fn run_identedit(arguments: &[&str]) -> Output {
    common::run_identedit(arguments)
}

fn run_identedit_in_dir(directory: &Path, arguments: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_identedit"));
    command.current_dir(directory);
    command.args(arguments);
    command.output().expect("failed to run identedit binary")
}

fn run_identedit_with_stdin(arguments: &[&str], input: &str) -> Output {
    let normalized_input = normalize_apply_input_payload(arguments, input);
    common::run_identedit_with_stdin(arguments, &normalized_input)
}

fn run_identedit_with_raw_stdin(arguments: &[&str], input: &[u8]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_identedit"));
    command.args(arguments);
    command.stdin(Stdio::piped());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());

    let mut child = command.spawn().expect("failed to spawn identedit binary");
    child
        .stdin
        .as_mut()
        .expect("stdin should be available")
        .write_all(input)
        .expect("stdin write should succeed");
    child
        .wait_with_output()
        .expect("failed to read process output")
}

fn run_identedit_with_raw_stdin_and_env(
    arguments: &[&str],
    input: &[u8],
    envs: &[(&str, &str)],
) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_identedit"));
    command.args(arguments);
    for (key, value) in envs {
        command.env(key, value);
    }
    command.stdin(Stdio::piped());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());

    let mut child = command.spawn().expect("failed to spawn identedit binary");
    child
        .stdin
        .as_mut()
        .expect("stdin should be available")
        .write_all(input)
        .expect("stdin write should succeed");
    child
        .wait_with_output()
        .expect("failed to read process output")
}

#[cfg(unix)]
fn run_shell_script(script: &str, root: &Path) -> Output {
    common::run_shell_script(script, root, None)
}

fn run_identedit_with_stdin_in_dir(directory: &Path, arguments: &[&str], input: &str) -> Output {
    let normalized_input = normalize_apply_input_payload(arguments, input);
    let mut command = Command::new(env!("CARGO_BIN_EXE_identedit"));
    command.current_dir(directory);
    command.args(arguments);
    command.stdin(Stdio::piped());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());

    let mut child = command.spawn().expect("failed to spawn identedit binary");
    child
        .stdin
        .as_mut()
        .expect("stdin should be available")
        .write_all(normalized_input.as_bytes())
        .expect("stdin write should succeed");
    child
        .wait_with_output()
        .expect("failed to read process output")
}

fn select_named_handle(file_path: &Path, name_pattern: &str) -> Value {
    let output = run_identedit(&[
        "read",
        "--json",
        "--verbose",
        "--kind",
        "function_definition",
        "--name",
        name_pattern,
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "select failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    response["handles"][0].clone()
}

fn select_first_handle(file_path: &Path, kind: &str, name_pattern: Option<&str>) -> Value {
    common::select_first_handle(file_path, kind, name_pattern)
}

fn select_root_json_object_handle(file_path: &Path) -> Value {
    let output = run_identedit(&[
        "read",
        "--json",
        "--verbose",
        "--kind",
        "object",
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
        .expect("handles should be an array")
        .iter()
        .find(|handle| handle["name"].is_null())
        .cloned()
        .expect("root JSON object handle should exist")
}

fn write_changeset_json(content: &str) -> NamedTempFile {
    let normalized = normalize_changeset_document(content);
    let mut file = Builder::new()
        .suffix(".json")
        .tempfile()
        .expect("changeset temp file should be created");
    file.write_all(normalized.as_bytes())
        .expect("changeset write should succeed");
    file
}

fn write_raw_changeset_json(content: &str) -> NamedTempFile {
    let mut file = Builder::new()
        .suffix(".json")
        .tempfile()
        .expect("changeset temp file should be created");
    file.write_all(content.as_bytes())
        .expect("changeset write should succeed");
    file
}

fn normalize_apply_input_payload(arguments: &[&str], input: &str) -> String {
    if arguments.first() != Some(&"apply") {
        return input.to_string();
    }

    let Ok(mut payload) = serde_json::from_str::<Value>(input) else {
        return input.to_string();
    };

    if is_legacy_file_change(&payload) {
        return wrap_legacy_file_change(payload).to_string();
    }

    if let Some(changeset) = payload.get_mut("changeset")
        && is_legacy_file_change(changeset)
    {
        let wrapped = wrap_legacy_file_change(changeset.clone());
        *changeset = wrapped;
        return payload.to_string();
    }

    input.to_string()
}

fn normalize_changeset_document(content: &str) -> String {
    let Ok(payload) = serde_json::from_str::<Value>(content) else {
        return content.to_string();
    };

    if is_legacy_file_change(&payload) {
        return wrap_legacy_file_change(payload).to_string();
    }

    content.to_string()
}

fn is_legacy_file_change(payload: &Value) -> bool {
    payload.get("file").is_some()
        && payload.get("operations").is_some()
        && payload.get("files").is_none()
}

fn wrap_legacy_file_change(payload: Value) -> Value {
    json!({
        "files": [payload],
        "transaction": {
            "mode": "all_or_nothing"
        }
    })
}

fn json_string_literal(path: &Path) -> String {
    path.to_str()
        .expect("path should be utf-8")
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
}

fn create_large_python_file(function_count: usize) -> PathBuf {
    common::create_large_python_file(function_count)
}

#[path = "apply_integration/scenario_01_core_and_line_endings.rs"]
mod scenario_01_core_and_line_endings;
#[path = "apply_integration/scenario_02_file_target_and_alias_conflicts.rs"]
mod scenario_02_file_target_and_alias_conflicts;
#[path = "apply_integration/scenario_03_operation_conflicts_and_preview_guard.rs"]
mod scenario_03_operation_conflicts_and_preview_guard;
#[path = "apply_integration/scenario_04_payload_validation.rs"]
mod scenario_04_payload_validation;
#[path = "apply_integration/scenario_05_move_and_transactions.rs"]
mod scenario_05_move_and_transactions;
#[path = "apply_integration/scenario_06_permissions_and_fs_edges.rs"]
mod scenario_06_permissions_and_fs_edges;
