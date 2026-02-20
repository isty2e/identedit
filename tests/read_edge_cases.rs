use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use serde_json::{Value, json};
use tempfile::{Builder, tempdir};

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn run_read(arguments: &[&str], file: Option<&PathBuf>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_identedit"));
    command.arg("read").arg("--mode").arg("ast").arg("--json");

    for argument in arguments {
        command.arg(argument);
    }

    if let Some(path) = file {
        command.arg(path);
    }

    command.output().expect("failed to run identedit binary")
}

fn run_read_with_files(arguments: &[&str], files: &[&Path]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_identedit"));
    command.arg("read").arg("--mode").arg("ast").arg("--json");

    for argument in arguments {
        command.arg(argument);
    }

    for file in files {
        command.arg(file);
    }

    command.output().expect("failed to run identedit binary")
}

fn run_read_json_mode(request_json: &str) -> Output {
    run_read_json_mode_with_args(request_json, &[])
}

fn run_read_json_mode_with_args(request_json: &str, arguments: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_identedit"));
    command
        .arg("read")
        .arg("--mode")
        .arg("ast")
        .arg("--json")
        .arg("--json");

    for argument in arguments {
        command.arg(argument);
    }

    command.stdin(Stdio::piped());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());

    let mut child = command.spawn().expect("failed to spawn identedit binary");
    let stdin = child.stdin.as_mut().expect("stdin should be available");
    stdin
        .write_all(request_json.as_bytes())
        .expect("failed to write request JSON");

    child
        .wait_with_output()
        .expect("failed to read process output")
}

#[cfg(unix)]
fn run_shell_script(script: &str, root: &Path) -> Output {
    Command::new("sh")
        .arg("-c")
        .arg(script)
        .env("IDENTEDIT_BIN", env!("CARGO_BIN_EXE_identedit"))
        .env("IDENTEDIT_ROOT", root)
        .output()
        .expect("failed to run shell command")
}

#[test]
fn accepts_valid_json_request_for_python_selection() {
    let fixture = fixture_path("example.py");
    let request = json!({
        "command": "read",
        "file": fixture.to_string_lossy().to_string(),
        "selector": {
            "kind": "function_definition",
            "name_pattern": "process_*",
            "exclude_kinds": []
        }
    });

    let output = run_read_json_mode(&request.to_string());

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["summary"]["matches"], 1);

    let handles = response["handles"]
        .as_array()
        .expect("handles should be an array");
    assert_eq!(handles.len(), 1);
    assert_eq!(handles[0]["name"], "process_data");
}

#[test]
fn cli_mode_supports_multiple_files_and_returns_flat_handles() {
    let fixture = fixture_path("example.py");
    let mut second_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp python file should be created");
    second_file
        .write_all(b"def generated_target():\n    return 1\n")
        .expect("temp python fixture write should succeed");
    let second_path = second_file.keep().expect("temp file should persist").1;

    let output = run_read_with_files(
        &["--kind", "function_definition"],
        &[fixture.as_path(), second_path.as_path()],
    );
    assert!(
        output.status.success(),
        "multi-file select should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["summary"]["files_scanned"], 2);
    assert!(response["summary"].get("provider").is_none());

    let handles = response["handles"]
        .as_array()
        .expect("handles should be an array");
    let matches = response["summary"]["matches"]
        .as_u64()
        .expect("matches should be an integer");
    assert_eq!(matches, handles.len() as u64);

    let fixture_path = fixture.to_string_lossy().to_string();
    let second_path = second_path.to_string_lossy().to_string();
    assert!(
        handles.iter().any(|handle| handle["file"] == fixture_path),
        "expected at least one handle from fixture path"
    );
    assert!(
        handles.iter().any(|handle| handle["file"] == second_path),
        "expected at least one handle from second path"
    );
}

#[test]
fn cli_mode_response_includes_expected_old_hash_for_each_handle() {
    let fixture = fixture_path("example.py");
    let output = run_read(
        &["--verbose", "--kind", "function_definition"],
        Some(&fixture),
    );
    assert!(
        output.status.success(),
        "select should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    let handles = response["handles"]
        .as_array()
        .expect("handles should be an array");
    assert!(
        !handles.is_empty(),
        "expected at least one function_definition handle"
    );

    for handle in handles {
        let text = handle["text"]
            .as_str()
            .expect("handle.text should be a string");
        let expected_old_hash = handle["expected_old_hash"]
            .as_str()
            .expect("select response must include expected_old_hash");
        assert_eq!(
            expected_old_hash.len(),
            identedit::changeset::HASH_HEX_LEN,
            "expected_old_hash should use fixed-length hex prefix"
        );
        assert_eq!(
            expected_old_hash,
            identedit::changeset::hash_text(text),
            "expected_old_hash should match blake3(text)"
        );
    }
}

#[test]
fn json_mode_supports_files_array_and_returns_flat_handles() {
    let python_fixture = fixture_path("example.py");
    let json_fixture = fixture_path("example.json");
    let request = json!({
        "command": "read",
        "files": [
            python_fixture.to_string_lossy().to_string(),
            json_fixture.to_string_lossy().to_string()
        ],
        "selector": {
            "kind": "module",
            "name_pattern": null,
            "exclude_kinds": []
        }
    });

    let output = run_read_json_mode(&request.to_string());
    assert!(
        output.status.success(),
        "multi-file JSON mode should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["summary"]["files_scanned"], 2);
    assert_eq!(response["summary"]["matches"], 1);
    assert!(response["summary"].get("provider").is_none());
    assert_eq!(
        response["handles"][0]["file"],
        python_fixture.to_string_lossy().to_string()
    );
}

#[test]
fn json_mode_response_includes_file_preconditions_for_all_scanned_files() {
    let python_fixture = fixture_path("example.py");
    let json_fixture = fixture_path("example.json");
    let request = json!({
        "command": "read",
        "files": [
            python_fixture.to_string_lossy().to_string(),
            json_fixture.to_string_lossy().to_string()
        ],
        "selector": {
            "kind": "module",
            "name_pattern": null,
            "exclude_kinds": []
        }
    });

    let output = run_read_json_mode(&request.to_string());
    assert!(
        output.status.success(),
        "select should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    let file_preconditions = response["file_preconditions"]
        .as_array()
        .expect("response.file_preconditions should be an array");
    assert_eq!(
        file_preconditions.len(),
        2,
        "expected one file precondition per scanned file"
    );

    let mut by_file = std::collections::BTreeMap::new();
    for precondition in file_preconditions {
        let file = precondition["file"]
            .as_str()
            .expect("file precondition must include file path");
        let hash = precondition["expected_file_hash"]
            .as_str()
            .expect("file precondition must include expected_file_hash");
        assert_eq!(
            hash.len(),
            identedit::changeset::HASH_HEX_LEN,
            "expected_file_hash should use fixed-length hex prefix"
        );
        by_file.insert(file.to_string(), hash.to_string());
    }

    for file in [python_fixture, json_fixture] {
        let source = fs::read_to_string(&file).expect("fixture should be readable");
        let expected_hash = identedit::changeset::hash_text(&source);
        let file_key = file.to_string_lossy().to_string();
        assert_eq!(
            by_file.get(&file_key),
            Some(&expected_hash),
            "file precondition hash should match current file contents for {file_key}"
        );
    }
}

#[test]
fn json_mode_rejects_mixed_file_and_files_fields() {
    let fixture = fixture_path("example.py");
    let request = json!({
        "command": "read",
        "file": fixture.to_string_lossy().to_string(),
        "files": [fixture.to_string_lossy().to_string()],
        "selector": {
            "kind": "function_definition",
            "name_pattern": null,
            "exclude_kinds": []
        }
    });

    let output = run_read_json_mode(&request.to_string());
    assert!(!output.status.success(), "mixed file and files should fail");

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let message = response["error"]["message"]
        .as_str()
        .expect("error message should be a string");
    assert!(
        message.contains("either 'file' or 'files'"),
        "message should explain mutually exclusive file fields: {message}"
    );
}

#[test]
fn json_mode_rejects_empty_files_array() {
    let request = json!({
        "command": "read",
        "files": [],
        "selector": {
            "kind": "function_definition",
            "name_pattern": null,
            "exclude_kinds": []
        }
    });

    let output = run_read_json_mode(&request.to_string());
    assert!(!output.status.success(), "empty files array should fail");

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let message = response["error"]["message"]
        .as_str()
        .expect("error message should be a string");
    assert!(
        message.contains("non-empty"),
        "message should explain that files must be non-empty: {message}"
    );
}

#[test]
fn json_mode_rejects_positional_file_arguments() {
    let fixture = fixture_path("example.py");
    let request = json!({
        "command": "read",
        "file": fixture.to_string_lossy().to_string(),
        "selector": {
            "kind": "function_definition",
            "name_pattern": null,
            "exclude_kinds": []
        }
    });
    let fixture_arg = fixture.to_str().expect("path should be valid UTF-8");

    let output = run_read_json_mode_with_args(&request.to_string(), &[fixture_arg]);
    assert!(
        !output.status.success(),
        "--json mode should reject positional FILE arguments"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("positional FILE arguments")),
        "expected explicit mixed-input validation error"
    );
}

#[test]
fn json_mode_rejects_kind_flag_argument() {
    let fixture = fixture_path("example.py");
    let request = json!({
        "command": "read",
        "file": fixture.to_string_lossy().to_string(),
        "selector": {
            "kind": "function_definition",
            "name_pattern": null,
            "exclude_kinds": []
        }
    });

    let output = run_read_json_mode_with_args(&request.to_string(), &["--kind", "module"]);
    assert!(
        !output.status.success(),
        "--json mode should reject --kind flag argument"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("--kind")),
        "expected mixed-input validation error mentioning --kind"
    );
}

#[test]
fn json_mode_rejects_name_and_exclude_kind_flag_arguments() {
    let fixture = fixture_path("example.py");
    let request = json!({
        "command": "read",
        "file": fixture.to_string_lossy().to_string(),
        "selector": {
            "kind": "function_definition",
            "name_pattern": null,
            "exclude_kinds": []
        }
    });

    let output = run_read_json_mode_with_args(
        &request.to_string(),
        &["--name", "process_*", "--exclude-kind", "comment"],
    );
    assert!(
        !output.status.success(),
        "--json mode should reject selector-related flag arguments"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("--name") || message.contains("--exclude-kind")),
        "expected mixed-input validation error mentioning selector flags"
    );
}

#[test]
fn cli_mode_rejects_duplicate_file_entries_by_canonical_path() {
    let mut temporary_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp file should be created");
    temporary_file
        .write_all(b"def target():\n    return 1\n")
        .expect("fixture write should succeed");
    let canonical_path = temporary_file.path().to_path_buf();
    let alias_path = canonical_path
        .parent()
        .expect("parent should exist")
        .join(".")
        .join(canonical_path.file_name().expect("file name should exist"));

    let output = run_read_with_files(
        &["--kind", "function_definition"],
        &[canonical_path.as_path(), alias_path.as_path()],
    );
    assert!(
        !output.status.success(),
        "duplicate canonical file entries should fail"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("Duplicate file entry")),
        "expected duplicate-file validation error"
    );
}

#[test]
fn json_mode_rejects_duplicate_file_entries_by_canonical_path() {
    let mut temporary_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp file should be created");
    temporary_file
        .write_all(b"def target():\n    return 1\n")
        .expect("fixture write should succeed");
    let canonical_path = temporary_file.path().to_path_buf();
    let alias_path = canonical_path
        .parent()
        .expect("parent should exist")
        .join(".")
        .join(canonical_path.file_name().expect("file name should exist"));

    let request = json!({
        "command": "read",
        "files": [
            canonical_path.to_string_lossy().to_string(),
            alias_path.to_string_lossy().to_string()
        ],
        "selector": {
            "kind": "function_definition",
            "name_pattern": null,
            "exclude_kinds": []
        }
    });

    let output = run_read_json_mode(&request.to_string());
    assert!(
        !output.status.success(),
        "duplicate canonical file entries should fail in JSON mode"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("Duplicate file entry")),
        "expected duplicate-file validation error"
    );
}

#[cfg(unix)]
#[test]
fn cli_mode_rejects_duplicate_file_entries_for_hardlink_alias() {
    let workspace = tempdir().expect("tempdir should be created");
    let canonical_path = workspace.path().join("canonical.py");
    fs::write(&canonical_path, "def target():\n    return 1\n")
        .expect("fixture write should succeed");
    let hardlink_path = workspace.path().join("hardlink.py");
    fs::hard_link(&canonical_path, &hardlink_path).expect("hardlink should be created");

    let output = run_read_with_files(
        &["--kind", "function_definition"],
        &[canonical_path.as_path(), hardlink_path.as_path()],
    );
    assert!(
        !output.status.success(),
        "hardlink aliases should be rejected as duplicate logical file entries"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("Duplicate file entry")),
        "expected duplicate-file validation error"
    );
}

#[cfg(unix)]
#[test]
fn json_mode_rejects_duplicate_file_entries_for_hardlink_alias() {
    let workspace = tempdir().expect("tempdir should be created");
    let canonical_path = workspace.path().join("canonical.py");
    fs::write(&canonical_path, "def target():\n    return 1\n")
        .expect("fixture write should succeed");
    let hardlink_path = workspace.path().join("hardlink.py");
    fs::hard_link(&canonical_path, &hardlink_path).expect("hardlink should be created");

    let request = json!({
        "command": "read",
        "files": [
            canonical_path.to_string_lossy().to_string(),
            hardlink_path.to_string_lossy().to_string()
        ],
        "selector": {
            "kind": "function_definition",
            "name_pattern": null,
            "exclude_kinds": []
        }
    });

    let output = run_read_json_mode(&request.to_string());
    assert!(
        !output.status.success(),
        "hardlink aliases should be rejected as duplicate logical file entries in JSON mode"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("Duplicate file entry")),
        "expected duplicate-file validation error"
    );
}

#[cfg(unix)]
#[test]
fn cli_mode_rejects_non_adjacent_hardlink_duplicates_with_middle_file() {
    let workspace = tempdir().expect("tempdir should be created");
    let fixture = "def target(value):\n    return value + 1\n";
    let canonical_path = workspace.path().join("a_canonical.py");
    let middle_path = workspace.path().join("m_middle.py");
    let hardlink_path = workspace.path().join("z_hardlink.py");
    fs::write(&canonical_path, fixture).expect("canonical fixture write should succeed");
    fs::write(&middle_path, fixture).expect("middle fixture write should succeed");
    fs::hard_link(&canonical_path, &hardlink_path).expect("hardlink should be created");

    let output = run_read_with_files(
        &["--kind", "function_definition"],
        &[
            canonical_path.as_path(),
            middle_path.as_path(),
            hardlink_path.as_path(),
        ],
    );
    assert!(
        !output.status.success(),
        "non-adjacent hardlink aliases should be rejected"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("Duplicate file entry")),
        "expected duplicate-file validation error"
    );
    assert!(
        response.get("handles").is_none(),
        "duplicate-entry error should not expose partial handles"
    );
}

#[cfg(unix)]
#[test]
fn json_mode_rejects_non_adjacent_hardlink_duplicates_with_middle_file() {
    let workspace = tempdir().expect("tempdir should be created");
    let fixture = "def target(value):\n    return value + 1\n";
    let canonical_path = workspace.path().join("a_canonical.py");
    let middle_path = workspace.path().join("m_middle.py");
    let hardlink_path = workspace.path().join("z_hardlink.py");
    fs::write(&canonical_path, fixture).expect("canonical fixture write should succeed");
    fs::write(&middle_path, fixture).expect("middle fixture write should succeed");
    fs::hard_link(&canonical_path, &hardlink_path).expect("hardlink should be created");

    let request = json!({
        "command": "read",
        "files": [
            canonical_path.to_string_lossy().to_string(),
            middle_path.to_string_lossy().to_string(),
            hardlink_path.to_string_lossy().to_string()
        ],
        "selector": {
            "kind": "function_definition",
            "name_pattern": null,
            "exclude_kinds": []
        }
    });

    let output = run_read_json_mode(&request.to_string());
    assert!(
        !output.status.success(),
        "json mode should reject non-adjacent hardlink aliases"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("Duplicate file entry")),
        "expected duplicate-file validation error"
    );
    assert!(
        response.get("handles").is_none(),
        "duplicate-entry error should not expose partial handles"
    );
}

#[cfg(unix)]
#[test]
fn cli_mode_rejects_three_hardlink_aliases_with_middle_file() {
    let workspace = tempdir().expect("tempdir should be created");
    let fixture = "def target(value):\n    return value + 1\n";
    let canonical_path = workspace.path().join("a_canonical.py");
    let middle_path = workspace.path().join("m_middle.py");
    let hardlink_b = workspace.path().join("b_hardlink.py");
    let hardlink_z = workspace.path().join("z_hardlink.py");
    fs::write(&canonical_path, fixture).expect("canonical fixture write should succeed");
    fs::write(&middle_path, fixture).expect("middle fixture write should succeed");
    fs::hard_link(&canonical_path, &hardlink_b).expect("hardlink_b should be created");
    fs::hard_link(&canonical_path, &hardlink_z).expect("hardlink_z should be created");

    let output = run_read_with_files(
        &["--kind", "function_definition"],
        &[
            hardlink_z.as_path(),
            middle_path.as_path(),
            hardlink_b.as_path(),
            canonical_path.as_path(),
        ],
    );
    assert!(
        !output.status.success(),
        "multiple hardlink aliases should be rejected"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("Duplicate file entry")),
        "expected duplicate-file validation error"
    );
    assert!(
        response.get("handles").is_none(),
        "duplicate-entry error should not expose partial handles"
    );
}

#[cfg(unix)]
#[test]
fn json_mode_rejects_three_hardlink_aliases_with_middle_file() {
    let workspace = tempdir().expect("tempdir should be created");
    let fixture = "def target(value):\n    return value + 1\n";
    let canonical_path = workspace.path().join("a_canonical.py");
    let middle_path = workspace.path().join("m_middle.py");
    let hardlink_b = workspace.path().join("b_hardlink.py");
    let hardlink_z = workspace.path().join("z_hardlink.py");
    fs::write(&canonical_path, fixture).expect("canonical fixture write should succeed");
    fs::write(&middle_path, fixture).expect("middle fixture write should succeed");
    fs::hard_link(&canonical_path, &hardlink_b).expect("hardlink_b should be created");
    fs::hard_link(&canonical_path, &hardlink_z).expect("hardlink_z should be created");

    let request = json!({
        "command": "read",
        "files": [
            hardlink_z.to_string_lossy().to_string(),
            middle_path.to_string_lossy().to_string(),
            hardlink_b.to_string_lossy().to_string(),
            canonical_path.to_string_lossy().to_string()
        ],
        "selector": {
            "kind": "function_definition",
            "name_pattern": null,
            "exclude_kinds": []
        }
    });

    let output = run_read_json_mode(&request.to_string());
    assert!(
        !output.status.success(),
        "json mode should reject multiple hardlink aliases"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("Duplicate file entry")),
        "expected duplicate-file validation error"
    );
    assert!(
        response.get("handles").is_none(),
        "duplicate-entry error should not expose partial handles"
    );
}

#[test]
fn cli_mode_file_preconditions_follow_input_order_for_multi_file_success() {
    let workspace = tempdir().expect("tempdir should be created");
    let file_c = workspace.path().join("c_file.py");
    let file_a = workspace.path().join("a_file.py");
    let file_b = workspace.path().join("b_file.py");
    fs::write(&file_c, "def c_target():\n    return 3\n").expect("file_c write should succeed");
    fs::write(&file_a, "def a_target():\n    return 1\n").expect("file_a write should succeed");
    fs::write(&file_b, "def b_target():\n    return 2\n").expect("file_b write should succeed");

    let output = run_read_with_files(
        &["--kind", "function_definition"],
        &[file_c.as_path(), file_a.as_path(), file_b.as_path()],
    );
    assert!(
        output.status.success(),
        "multi-file select should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    let file_preconditions = response["file_preconditions"]
        .as_array()
        .expect("response.file_preconditions should be an array");
    assert_eq!(file_preconditions.len(), 3);

    let expected_order = vec![
        file_c.to_string_lossy().to_string(),
        file_a.to_string_lossy().to_string(),
        file_b.to_string_lossy().to_string(),
    ];
    let actual_order: Vec<String> = file_preconditions
        .iter()
        .map(|entry| {
            entry["file"]
                .as_str()
                .expect("file precondition should include file path")
                .to_string()
        })
        .collect();
    assert_eq!(
        actual_order, expected_order,
        "file_preconditions should preserve input file order in CLI mode"
    );

    for (entry, file) in file_preconditions.iter().zip([file_c, file_a, file_b]) {
        let expected_hash = identedit::changeset::hash_text(
            &fs::read_to_string(&file).expect("fixture should be readable"),
        );
        assert_eq!(
            entry["expected_file_hash"].as_str(),
            Some(expected_hash.as_str()),
            "expected_file_hash should match current contents for {}",
            file.display()
        );
    }
}

#[test]
fn json_mode_file_preconditions_follow_input_order_for_multi_file_success() {
    let workspace = tempdir().expect("tempdir should be created");
    let file_c = workspace.path().join("c_file.py");
    let file_a = workspace.path().join("a_file.py");
    let file_b = workspace.path().join("b_file.py");
    fs::write(&file_c, "def c_target():\n    return 3\n").expect("file_c write should succeed");
    fs::write(&file_a, "def a_target():\n    return 1\n").expect("file_a write should succeed");
    fs::write(&file_b, "def b_target():\n    return 2\n").expect("file_b write should succeed");

    let request = json!({
        "command": "read",
        "files": [
            file_c.to_string_lossy().to_string(),
            file_a.to_string_lossy().to_string(),
            file_b.to_string_lossy().to_string()
        ],
        "selector": {
            "kind": "function_definition",
            "name_pattern": null,
            "exclude_kinds": []
        }
    });

    let output = run_read_json_mode(&request.to_string());
    assert!(
        output.status.success(),
        "multi-file select in JSON mode should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    let file_preconditions = response["file_preconditions"]
        .as_array()
        .expect("response.file_preconditions should be an array");
    assert_eq!(file_preconditions.len(), 3);

    let expected_order = vec![
        file_c.to_string_lossy().to_string(),
        file_a.to_string_lossy().to_string(),
        file_b.to_string_lossy().to_string(),
    ];
    let actual_order: Vec<String> = file_preconditions
        .iter()
        .map(|entry| {
            entry["file"]
                .as_str()
                .expect("file precondition should include file path")
                .to_string()
        })
        .collect();
    assert_eq!(
        actual_order, expected_order,
        "file_preconditions should preserve input file order in JSON mode"
    );

    for (entry, file) in file_preconditions.iter().zip([file_c, file_a, file_b]) {
        let expected_hash = identedit::changeset::hash_text(
            &fs::read_to_string(&file).expect("fixture should be readable"),
        );
        assert_eq!(
            entry["expected_file_hash"].as_str(),
            Some(expected_hash.as_str()),
            "expected_file_hash should match current contents for {}",
            file.display()
        );
    }
}

#[test]
fn cli_mode_multi_file_failure_follows_input_order_without_partial_handles() {
    let workspace = tempdir().expect("tempdir should be created");
    let invalid_python = workspace.path().join("invalid.py");
    fs::write(&invalid_python, "def broken(\n").expect("invalid fixture write should succeed");
    let missing_python = workspace.path().join("missing.py");

    let io_first_output = run_read_with_files(
        &["--kind", "function_definition"],
        &[missing_python.as_path(), invalid_python.as_path()],
    );
    assert!(
        !io_first_output.status.success(),
        "mixed failure inputs should fail"
    );
    let io_response: Value =
        serde_json::from_slice(&io_first_output.stdout).expect("stdout should be valid JSON");
    assert_eq!(io_response["error"]["type"], "io_error");
    assert!(
        io_response.get("handles").is_none(),
        "error responses should not expose partial handles"
    );

    let parse_first_output = run_read_with_files(
        &["--kind", "function_definition"],
        &[invalid_python.as_path(), missing_python.as_path()],
    );
    assert!(
        !parse_first_output.status.success(),
        "mixed failure inputs should fail"
    );
    let parse_response: Value =
        serde_json::from_slice(&parse_first_output.stdout).expect("stdout should be valid JSON");
    assert_eq!(parse_response["error"]["type"], "parse_failure");
    assert!(
        parse_response.get("handles").is_none(),
        "error responses should not expose partial handles"
    );
}

#[test]
fn json_mode_multi_file_failure_follows_input_order_without_partial_handles() {
    let workspace = tempdir().expect("tempdir should be created");
    let invalid_python = workspace.path().join("invalid.py");
    fs::write(&invalid_python, "def broken(\n").expect("invalid fixture write should succeed");
    let missing_python = workspace.path().join("missing.py");

    let io_first_request = json!({
        "command": "read",
        "files": [
            missing_python.to_string_lossy().to_string(),
            invalid_python.to_string_lossy().to_string()
        ],
        "selector": {
            "kind": "function_definition",
            "name_pattern": null,
            "exclude_kinds": []
        }
    });
    let io_first_output = run_read_json_mode(&io_first_request.to_string());
    assert!(
        !io_first_output.status.success(),
        "mixed failure inputs should fail in JSON mode"
    );
    let io_response: Value =
        serde_json::from_slice(&io_first_output.stdout).expect("stdout should be valid JSON");
    assert_eq!(io_response["error"]["type"], "io_error");
    assert!(
        io_response.get("handles").is_none(),
        "error responses should not expose partial handles"
    );

    let parse_first_request = json!({
        "command": "read",
        "files": [
            invalid_python.to_string_lossy().to_string(),
            missing_python.to_string_lossy().to_string()
        ],
        "selector": {
            "kind": "function_definition",
            "name_pattern": null,
            "exclude_kinds": []
        }
    });
    let parse_first_output = run_read_json_mode(&parse_first_request.to_string());
    assert!(
        !parse_first_output.status.success(),
        "mixed failure inputs should fail in JSON mode"
    );
    let parse_response: Value =
        serde_json::from_slice(&parse_first_output.stdout).expect("stdout should be valid JSON");
    assert_eq!(parse_response["error"]["type"], "parse_failure");
    assert!(
        parse_response.get("handles").is_none(),
        "error responses should not expose partial handles"
    );
}

#[test]
fn accepts_valid_json_request_for_json_selection() {
    let fixture = fixture_path("example.json");
    let request = json!({
        "command": "read",
        "file": fixture.to_string_lossy().to_string(),
        "selector": {
            "kind": "key",
            "name_pattern": "config",
            "exclude_kinds": []
        }
    });

    let output = run_read_json_mode(&request.to_string());

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["summary"]["matches"], 1);

    let handles = response["handles"]
        .as_array()
        .expect("handles should be an array");
    assert_eq!(handles.len(), 1);
    assert_eq!(handles[0]["kind"], "key");
    assert_eq!(handles[0]["name"], "config");
}

#[test]
fn json_mode_python_selector_with_no_matches_returns_success_and_empty_handles() {
    let fixture = fixture_path("example.py");
    let request = json!({
        "command": "read",
        "file": fixture.to_string_lossy().to_string(),
        "selector": {
            "kind": "function_definition",
            "name_pattern": "does_not_exist_*",
            "exclude_kinds": []
        }
    });

    let output = run_read_json_mode(&request.to_string());
    assert!(
        output.status.success(),
        "no-match selector should still succeed in JSON mode: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["summary"]["matches"], 0);
    assert_eq!(response["handles"].as_array().map(Vec::len), Some(0));
}

#[test]
fn json_mode_json_selector_with_no_matches_returns_success_and_empty_handles() {
    let fixture = fixture_path("example.json");
    let request = json!({
        "command": "read",
        "file": fixture.to_string_lossy().to_string(),
        "selector": {
            "kind": "key",
            "name_pattern": "does_not_exist",
            "exclude_kinds": []
        }
    });

    let output = run_read_json_mode(&request.to_string());
    assert!(
        output.status.success(),
        "no-match selector should still succeed in JSON mode: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["summary"]["matches"], 0);
    assert_eq!(response["handles"].as_array().map(Vec::len), Some(0));
}

#[test]
fn python_selector_with_no_matches_returns_success_and_empty_handles() {
    let fixture = fixture_path("example.py");
    let output = run_read(
        &[
            "--kind",
            "function_definition",
            "--name",
            "does_not_exist_*",
        ],
        Some(&fixture),
    );
    assert!(
        output.status.success(),
        "no-match selector should still succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["summary"]["matches"], 0);
    assert_eq!(response["handles"].as_array().map(Vec::len), Some(0));
}

#[test]
fn json_selector_with_no_matches_returns_success_and_empty_handles() {
    let fixture = fixture_path("example.json");
    let output = run_read(
        &["--kind", "key", "--name", "does_not_exist"],
        Some(&fixture),
    );
    assert!(
        output.status.success(),
        "no-match selector should still succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["summary"]["matches"], 0);
    assert_eq!(response["handles"].as_array().map(Vec::len), Some(0));
}

#[test]
fn empty_python_file_returns_structured_module_handle() {
    let mut temp_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp python file should be created");
    temp_file
        .write_all(b"")
        .expect("empty fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let output = run_read(&["--kind", "module"], Some(&file_path));
    assert!(
        output.status.success(),
        "select should succeed for empty python file: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["summary"]["matches"], 1);
    assert_eq!(response["handles"][0]["kind"], "module");
    assert_eq!(response["handles"][0]["span"]["start"], 0);
    assert_eq!(response["handles"][0]["span"]["end"], 0);
}

#[test]
fn empty_json_file_returns_structured_empty_result() {
    let mut temp_file = Builder::new()
        .suffix(".json")
        .tempfile()
        .expect("temp json file should be created");
    temp_file
        .write_all(b"")
        .expect("empty fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let output = run_read(&["--kind", "object"], Some(&file_path));
    assert!(
        output.status.success(),
        "select should succeed for empty json file: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["summary"]["matches"], 0);
    assert_eq!(
        response["handles"].as_array().map(Vec::len),
        Some(0),
        "empty JSON selection should return zero handles"
    );
}

#[test]
fn returns_error_when_kind_flag_is_missing() {
    let fixture = fixture_path("example.py");
    let output = run_read(&[], Some(&fixture));

    assert!(
        output.status.success(),
        "missing --kind should still succeed with unconstrained read: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert!(
        response["summary"]["matches"]
            .as_u64()
            .is_some_and(|matches| matches > 0),
        "unconstrained read should return at least one AST handle"
    );
}

#[test]
fn returns_error_for_invalid_selector_glob_in_flag_mode() {
    let fixture = fixture_path("example.py");
    let output = run_read(
        &["--kind", "function_definition", "--name", "["],
        Some(&fixture),
    );

    assert!(!output.status.success(), "invalid glob should fail");
    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_selector");
}

#[test]
fn returns_error_for_invalid_selector_glob_in_json_mode() {
    let fixture = fixture_path("example.py");
    let request = json!({
        "command": "read",
        "file": fixture.to_string_lossy().to_string(),
        "selector": {
            "kind": "function_definition",
            "name_pattern": "[",
            "exclude_kinds": []
        }
    });

    let output = run_read_json_mode(&request.to_string());
    assert!(
        !output.status.success(),
        "invalid selector glob should fail in JSON mode"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_selector");
}

#[test]
fn returns_error_for_whitespace_only_kind_in_json_mode() {
    let fixture = fixture_path("example.py");
    let request = json!({
        "command": "read",
        "file": fixture.to_string_lossy().to_string(),
        "selector": {
            "kind": "   ",
            "name_pattern": null,
            "exclude_kinds": []
        }
    });

    let output = run_read_json_mode(&request.to_string());
    assert!(
        !output.status.success(),
        "whitespace-only selector.kind should fail in JSON mode"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("selector.kind")),
        "expected selector.kind validation message"
    );
}

#[test]
fn returns_error_for_invalid_json_payload_in_json_mode() {
    let output = run_read_json_mode("{");

    assert!(!output.status.success(), "invalid JSON payload should fail");
    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}

#[test]
fn returns_error_when_selector_is_missing_in_json_mode() {
    let output = run_read_json_mode(
        r#"{
  "command": "read",
  "file": "tests/fixtures/example.py"
}"#,
    );

    assert!(
        !output.status.success(),
        "missing selector payload should fail"
    );
    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}

#[test]
fn returns_error_when_file_is_missing_in_json_mode() {
    let output = run_read_json_mode(
        r#"{
  "command": "read",
  "selector": {
    "kind": "function_definition",
    "name_pattern": null,
    "exclude_kinds": []
  }
}"#,
    );

    assert!(!output.status.success(), "missing file field should fail");
    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}

#[test]
fn returns_error_when_json_request_has_unknown_top_level_field() {
    let output = run_read_json_mode(
        r#"{
  "command": "read",
  "file": "tests/fixtures/example.py",
  "selector": {
    "kind": "function_definition",
    "name_pattern": null,
    "exclude_kinds": []
  },
  "unexpected": true
}"#,
    );

    assert!(
        !output.status.success(),
        "unknown top-level fields should fail in strict JSON mode"
    );
    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("unknown field `unexpected`")),
        "expected unknown top-level field message"
    );
}

#[test]
fn returns_error_when_selector_has_unknown_field_in_json_mode() {
    let output = run_read_json_mode(
        r#"{
  "command": "read",
  "file": "tests/fixtures/example.py",
  "selector": {
    "kind": "function_definition",
    "name_pattern": null,
    "exclude_kinds": [],
    "unexpected_selector_field": "extra"
  }
}"#,
    );

    assert!(
        !output.status.success(),
        "unknown selector fields should fail in strict JSON mode"
    );
    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("unknown field `unexpected_selector_field`")),
        "expected unknown selector field message"
    );
}

#[test]
fn json_mode_treats_env_token_in_file_path_as_literal_string() {
    let request = json!({
        "command": "read",
        "file": format!("${{IDENTEDIT_SELECT_JSON_PATH_{}}}/example.py", std::process::id()),
        "selector": {
            "kind": "function_definition",
            "name_pattern": null,
            "exclude_kinds": []
        }
    });

    let output = run_read_json_mode(&request.to_string());
    assert!(
        !output.status.success(),
        "json-mode file paths should not expand env tokens"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "io_error");
}

#[test]
fn returns_error_when_selector_exclude_kinds_has_wrong_type_in_json_mode() {
    let output = run_read_json_mode(
        r#"{
  "command": "read",
  "file": "tests/fixtures/example.py",
  "selector": {
    "kind": "function_definition",
    "name_pattern": null,
    "exclude_kinds": "comment"
  }
}"#,
    );

    assert!(
        !output.status.success(),
        "selector.exclude_kinds type mismatch should fail"
    );
    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}

#[test]
fn returns_error_when_selector_kind_has_wrong_type_in_json_mode() {
    let output = run_read_json_mode(
        r#"{
  "command": "read",
  "file": "tests/fixtures/example.py",
  "selector": {
    "kind": 123,
    "name_pattern": null,
    "exclude_kinds": []
  }
}"#,
    );

    assert!(
        !output.status.success(),
        "selector.kind type mismatch should fail"
    );
    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}

#[test]
fn returns_error_for_non_read_command_in_json_mode() {
    let output = run_read_json_mode(
        r#"{
  "command": "verify",
  "file": "tests/fixtures/example.py",
  "selector": {
    "kind": "function_definition",
    "name_pattern": null,
    "exclude_kinds": []
  }
}"#,
    );

    assert!(!output.status.success(), "unsupported command should fail");
    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("expected 'read'")),
        "expected unsupported command message"
    );
}

#[test]
fn returns_error_for_legacy_select_command_in_json_mode() {
    let output = run_read_json_mode(
        r#"{
  "command": "select",
  "file": "tests/fixtures/example.py",
  "selector": {
    "kind": "function_definition",
    "name_pattern": null,
    "exclude_kinds": []
  }
}"#,
    );

    assert!(!output.status.success(), "legacy command should fail");
    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("expected 'read'")),
        "expected legacy-command rejection message"
    );
}

#[test]
fn returns_error_when_json_mode_payload_contains_file_and_files() {
    let output = run_read_json_mode(
        r#"{
  "command": "read",
  "file": "tests/fixtures/example.py",
  "files": ["tests/fixtures/example.py"],
  "selector": {
    "kind": "function_definition",
    "name_pattern": null,
    "exclude_kinds": []
  }
}"#,
    );

    assert!(
        !output.status.success(),
        "payload mixing file/files should fail"
    );
    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"].as_str().is_some_and(|message| {
            message.contains("either 'file' or 'files'") && message.contains("not both")
        }),
        "expected shape-conflict message"
    );
}

#[test]
fn returns_error_for_homoglyph_command_token_in_json_mode() {
    let output = run_read_json_mode(
        r#"{
  "command": "reаd",
  "file": "tests/fixtures/example.py",
  "selector": {
    "kind": "function_definition",
    "name_pattern": null,
    "exclude_kinds": []
  }
}"#,
    );

    assert!(!output.status.success(), "homoglyph command should fail");
    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("expected 'read'")),
        "expected command-token rejection message"
    );
}

#[cfg(unix)]
#[test]
fn supports_shell_variable_expanded_path_in_flag_mode() {
    let workspace = tempdir().expect("tempdir should be created");
    let file_path = workspace.path().join("example.py");
    let source =
        fs::read_to_string(fixture_path("example.py")).expect("fixture should be readable");
    fs::write(&file_path, source).expect("fixture write should succeed");

    let output = run_shell_script(
        "\"$IDENTEDIT_BIN\" read --mode ast --json --kind function_definition \"${IDENTEDIT_ROOT}/example.py\"",
        workspace.path(),
    );
    assert!(
        output.status.success(),
        "read via shell-expanded path should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let _response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
}

#[cfg(unix)]
#[test]
fn single_quoted_env_token_path_remains_literal_in_flag_mode() {
    let workspace = tempdir().expect("tempdir should be created");
    let file_path = workspace.path().join("example.py");
    let source =
        fs::read_to_string(fixture_path("example.py")).expect("fixture should be readable");
    fs::write(&file_path, source).expect("fixture write should succeed");

    let output = run_shell_script(
        "\"$IDENTEDIT_BIN\" read --mode ast --json --kind function_definition '${IDENTEDIT_ROOT}/example.py'",
        workspace.path(),
    );
    assert!(
        !output.status.success(),
        "single-quoted env token path should remain literal and fail"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "io_error");
}

#[test]
fn returns_error_for_command_token_with_trailing_whitespace_in_json_mode() {
    let output = run_read_json_mode(
        r#"{
  "command": "read ",
  "file": "tests/fixtures/example.py",
  "selector": {
    "kind": "function_definition",
    "name_pattern": null,
    "exclude_kinds": []
  }
}"#,
    );

    assert!(
        !output.status.success(),
        "command token with trailing whitespace should fail"
    );
    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}

#[test]
fn returns_error_for_uppercase_command_token_in_json_mode() {
    let output = run_read_json_mode(
        r#"{
  "command": "READ",
  "file": "tests/fixtures/example.py",
  "selector": {
    "kind": "function_definition",
    "name_pattern": null,
    "exclude_kinds": []
  }
}"#,
    );

    assert!(
        !output.status.success(),
        "uppercase command token should fail"
    );
    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}

#[test]
fn supports_case_insensitive_python_extension() {
    let fixture_contents =
        std::fs::read_to_string(fixture_path("example.py")).expect("fixture should be readable");
    let mut temporary_file = Builder::new()
        .suffix(".PY")
        .tempfile()
        .expect("temp file should be created");
    temporary_file
        .write_all(fixture_contents.as_bytes())
        .expect("temp fixture write should succeed");

    let temp_path = temporary_file.path().to_path_buf();
    let output = run_read(&["--kind", "function_definition"], Some(&temp_path));

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let _response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
}

#[test]
fn supports_case_insensitive_json_extension() {
    let fixture_contents =
        std::fs::read_to_string(fixture_path("example.json")).expect("fixture should be readable");
    let mut temporary_file = Builder::new()
        .suffix(".JSON")
        .tempfile()
        .expect("temp file should be created");
    temporary_file
        .write_all(fixture_contents.as_bytes())
        .expect("temp fixture write should succeed");

    let temp_path = temporary_file.path().to_path_buf();
    let output = run_read(&["--kind", "object"], Some(&temp_path));

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let _response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
}

#[test]
fn extensionless_file_routes_to_fallback_provider() {
    let mut temporary_file = Builder::new()
        .tempfile()
        .expect("temp file should be created");
    temporary_file
        .write_all(b"def process_data(value):\n    return value + 1\n")
        .expect("fixture write should succeed");
    let temp_path = temporary_file.path().to_path_buf();

    let output = run_read(&["--kind", "function_definition"], Some(&temp_path));
    assert!(
        output.status.success(),
        "select should use fallback for extensionless files: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["summary"]["matches"], 1);
}

#[test]
fn hidden_dotfile_without_basename_routes_to_fallback_provider() {
    let directory = tempdir().expect("tempdir should be created");
    let dotfile_path = directory.path().join(".json");
    fs::write(
        &dotfile_path,
        "def process_data(value):\n    return value + 1\n",
    )
    .expect("dotfile write should succeed");

    let output = run_read(&["--kind", "function_definition"], Some(&dotfile_path));
    assert!(
        output.status.success(),
        "select should use fallback for hidden dotfile without basename: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["summary"]["matches"], 1);
}

#[test]
fn fallback_select_ignores_python_def_inside_triple_quoted_string() {
    let mut temporary_file = Builder::new()
        .suffix(".txt")
        .tempfile()
        .expect("temp file should be created");
    temporary_file
        .write_all(
            b"text = \"\"\"\ndef fake_inside_docstring():\n    return 0\n\"\"\"\n\ndef real_function():\n    return 1\n",
        )
        .expect("fixture write should succeed");
    let temp_path = temporary_file.path().to_path_buf();

    let output = run_read(&["--kind", "function_definition"], Some(&temp_path));
    assert!(
        output.status.success(),
        "select should succeed via fallback: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["summary"]["matches"], 1);
    let names: Vec<&str> = response["handles"]
        .as_array()
        .expect("handles should be an array")
        .iter()
        .filter_map(|handle| handle["name"].as_str())
        .collect();
    assert_eq!(names, vec!["real_function"]);
}

#[test]
fn fallback_select_name_pattern_matches_backslash_bearing_symbol_name() {
    let mut temporary_file = Builder::new()
        .suffix(".txt")
        .tempfile()
        .expect("temp file should be created");
    temporary_file
        .write_all(b"function \\u0066oo(value) {\n  return value + 1;\n}\n")
        .expect("fixture write should succeed");
    let temp_path = temporary_file.path().to_path_buf();

    let output = run_read(
        &["--kind", "function_definition", "--name", "\\u0066*"],
        Some(&temp_path),
    );
    assert!(
        output.status.success(),
        "select should succeed for fallback unicode-escape symbol: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["summary"]["matches"], 1);

    let handles = response["handles"]
        .as_array()
        .expect("handles should be an array");
    assert_eq!(handles.len(), 1, "exactly one fallback handle should match");
    assert_eq!(handles[0]["name"], "\\u0066oo");
}

#[test]
fn select_handles_use_utf8_boundary_spans_across_providers() {
    let mut fallback_file = Builder::new()
        .suffix(".txt")
        .tempfile()
        .expect("temp fallback file should be created");
    fallback_file
        .write_all("function café(value) {\n  return value + 1;\n}\n".as_bytes())
        .expect("fallback fixture write should succeed");
    let fallback_path = fallback_file.path().to_path_buf();

    let cases = vec![
        (fixture_path("example.py"), "function_definition"),
        (fixture_path("example.json"), "key"),
        (fixture_path("example.js"), "function_declaration"),
        (fixture_path("example.ts"), "function_declaration"),
        (fixture_path("example.tsx"), "function_declaration"),
        (fixture_path("example.rs"), "function_item"),
        (fixture_path("example.go"), "function_declaration"),
        (fallback_path, "function_definition"),
    ];

    for (file, kind) in cases {
        let source = fs::read_to_string(&file).expect("fixture should be readable");
        let output = run_read(&["--kind", kind], Some(&file));
        assert!(
            output.status.success(),
            "select should succeed for {} with kind {}: {}",
            file.display(),
            kind,
            String::from_utf8_lossy(&output.stderr)
        );

        let response: Value =
            serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
        let handles = response["handles"]
            .as_array()
            .expect("handles should be an array");
        assert!(
            !handles.is_empty(),
            "expected at least one handle for {} with kind {}",
            file.display(),
            kind
        );

        for handle in handles {
            let span = handle["span"]
                .as_object()
                .expect("span should be an object");
            let start = span["start"].as_u64().expect("span.start should be u64") as usize;
            let end = span["end"].as_u64().expect("span.end should be u64") as usize;
            assert!(
                start <= end && end <= source.len(),
                "span [{start}, {end}) should stay within utf-8 source bounds for {}",
                file.display()
            );
            assert!(
                source.is_char_boundary(start),
                "span.start should be utf-8 boundary for {}",
                file.display()
            );
            assert!(
                source.is_char_boundary(end),
                "span.end should be utf-8 boundary for {}",
                file.display()
            );
        }
    }
}

#[test]
fn returns_io_error_when_file_argument_is_directory() {
    let directory = tempdir().expect("tempdir should be created");
    let directory_path = directory.path().to_path_buf();

    let output = run_read(&["--kind", "function_definition"], Some(&directory_path));
    assert!(
        !output.status.success(),
        "select should fail when FILE points to a directory"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "io_error");
}

#[test]
fn supports_cr_only_python_files() {
    let mut temporary_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp file should be created");
    temporary_file
        .write_all(b"def process_data(value):\r    return value + 1\r\rdef helper():\r    return \"helper\"\r")
        .expect("cr-only fixture write should succeed");
    let temp_path = temporary_file.path().to_path_buf();

    let output = run_read(&["--kind", "function_definition"], Some(&temp_path));
    assert!(
        output.status.success(),
        "select should succeed for CR-only python input: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(
        response["summary"]["matches"], 2,
        "fixture should expose two function_definition handles"
    );
}

#[test]
fn supports_utf8_bom_prefixed_python_files() {
    let mut temporary_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp file should be created");
    temporary_file
        .write_all(b"\xEF\xBB\xBFdef process_data(value):\n    return value + 1\n")
        .expect("bom python fixture write should succeed");
    let temp_path = temporary_file.path().to_path_buf();

    let output = run_read(&["--kind", "function_definition"], Some(&temp_path));
    assert!(
        output.status.success(),
        "select should succeed for UTF-8 BOM python input: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["summary"]["matches"], 1);
    assert_eq!(response["handles"][0]["span"]["start"], 3);
}

#[test]
fn bom_only_python_file_returns_empty_module_handle_at_eof() {
    let mut temporary_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp file should be created");
    temporary_file
        .write_all(b"\xEF\xBB\xBF")
        .expect("bom-only python fixture write should succeed");
    let temp_path = temporary_file.path().to_path_buf();

    let output = run_read(&["--verbose", "--kind", "module"], Some(&temp_path));
    assert!(
        output.status.success(),
        "select should succeed for BOM-only python input: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["summary"]["matches"], 1);
    assert_eq!(response["handles"][0]["kind"], "module");
    assert_eq!(response["handles"][0]["span"]["start"], 3);
    assert_eq!(response["handles"][0]["span"]["end"], 3);
    assert_eq!(response["handles"][0]["text"], "");
}

#[test]
fn supports_utf8_bom_prefixed_json_files() {
    let mut temporary_file = Builder::new()
        .suffix(".json")
        .tempfile()
        .expect("temp file should be created");
    temporary_file
        .write_all(b"\xEF\xBB\xBF{\"enabled\": true}\n")
        .expect("bom json fixture write should succeed");
    let temp_path = temporary_file.path().to_path_buf();

    let output = run_read(&["--kind", "object"], Some(&temp_path));
    assert!(
        output.status.success(),
        "select should succeed for UTF-8 BOM json input: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["summary"]["matches"], 1);
    assert_eq!(response["handles"][0]["span"]["start"], 3);
}

#[test]
fn bom_only_json_file_returns_structured_empty_result() {
    let mut temporary_file = Builder::new()
        .suffix(".json")
        .tempfile()
        .expect("temp file should be created");
    temporary_file
        .write_all(b"\xEF\xBB\xBF")
        .expect("bom-only json fixture write should succeed");
    let temp_path = temporary_file.path().to_path_buf();

    let output = run_read(&["--kind", "object"], Some(&temp_path));
    assert!(
        output.status.success(),
        "select should succeed for BOM-only json input: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["summary"]["matches"], 0);
    assert_eq!(response["handles"].as_array().map(Vec::len), Some(0));
}

#[test]
fn returns_parse_failure_for_syntax_invalid_python_file() {
    let mut temporary_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp file should be created");
    temporary_file
        .write_all(b"def broken(:\n    pass\n")
        .expect("invalid python fixture write should succeed");
    let temp_path = temporary_file.path().to_path_buf();

    let output = run_read(&["--kind", "function_definition"], Some(&temp_path));
    assert!(
        !output.status.success(),
        "select should fail for syntax-invalid python input"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "parse_failure");
}

#[test]
fn returns_parse_failure_for_partially_binary_python_file() {
    let mut temporary_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp file should be created");
    temporary_file
        .write_all(b"def process_data(value):\n    return value + 1\n\xff\n")
        .expect("binary-like python fixture write should succeed");
    let temp_path = temporary_file.path().to_path_buf();

    let output = run_read(&["--kind", "function_definition"], Some(&temp_path));
    assert!(
        !output.status.success(),
        "select should fail for partially-binary python payload"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "parse_failure");
}

#[test]
fn returns_parse_failure_for_nul_in_python_source() {
    let mut temporary_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp file should be created");
    temporary_file
        .write_all(b"def process_data(value):\n    return value + 1\n\x00\n")
        .expect("nul python fixture write should succeed");
    let temp_path = temporary_file.path().to_path_buf();

    let output = run_read(&["--kind", "function_definition"], Some(&temp_path));
    assert!(
        !output.status.success(),
        "select should fail for python source containing embedded NUL"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "parse_failure");
}

#[test]
fn returns_parse_failure_for_bom_plus_nul_python_source() {
    let mut temporary_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp file should be created");
    temporary_file
        .write_all(b"\xEF\xBB\xBFdef process_data(value):\n    return value + 1\n\x00\n")
        .expect("bom+nul python fixture write should succeed");
    let temp_path = temporary_file.path().to_path_buf();

    let output = run_read(&["--kind", "function_definition"], Some(&temp_path));
    assert!(
        !output.status.success(),
        "select should fail for python source containing BOM and embedded NUL"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "parse_failure");
}

#[test]
fn returns_parse_failure_for_syntax_invalid_json_file() {
    let mut temporary_file = Builder::new()
        .suffix(".json")
        .tempfile()
        .expect("temp file should be created");
    temporary_file
        .write_all(br#"{ "name": "identedit",, "enabled": true }"#)
        .expect("invalid json fixture write should succeed");
    let temp_path = temporary_file.path().to_path_buf();

    let output = run_read(&["--kind", "object"], Some(&temp_path));
    assert!(
        !output.status.success(),
        "select should fail for syntax-invalid json input"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "parse_failure");
}

#[cfg(unix)]
#[test]
fn non_utf8_path_argument_returns_io_error_without_panicking() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let mut command = Command::new(env!("CARGO_BIN_EXE_identedit"));
    command.arg("read").arg("--mode").arg("ast").arg("--json");
    command.arg("--kind");
    command.arg("function_definition");
    command.arg(OsString::from_vec(vec![0xFF, 0x2E, 0x70, 0x79]));

    let output = command.output().expect("failed to run identedit binary");
    assert!(
        !output.status.success(),
        "select should fail for non-UTF8 path arguments"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "io_error");
}
