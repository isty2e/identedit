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

fn run_select_with_files(arguments: &[&str], files: &[&Path]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_identedit"));
    command.env("IDENTEDIT_ALLOW_LEGACY", "1");
    command.arg("select");

    for argument in arguments {
        command.arg(argument);
    }

    for file in files {
        command.arg(file);
    }

    command.output().expect("failed to run identedit binary")
}

fn run_select_json_mode(request_json: &str) -> Output {
    run_select_json_mode_with_args(request_json, &[])
}

fn run_select_json_mode_with_args(request_json: &str, arguments: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_identedit"));
    command.env("IDENTEDIT_ALLOW_LEGACY", "1");
    command.arg("select").arg("--json");

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
#[test]
fn round1_exploit_cli_rejects_symlink_alias_duplicate_without_partial_handles() {
    use std::os::unix::fs::symlink;

    let workspace = tempdir().expect("tempdir should be created");
    let canonical_path = workspace.path().join("canonical.py");
    fs::write(&canonical_path, "def target():\n    return 1\n")
        .expect("fixture write should succeed");
    let symlink_path = workspace.path().join("alias.py");
    symlink(&canonical_path, &symlink_path).expect("symlink should be created");

    let output = run_select_with_files(
        &["--kind", "function_definition"],
        &[canonical_path.as_path(), symlink_path.as_path()],
    );
    assert!(
        !output.status.success(),
        "symlink aliases should be rejected as duplicate logical file entries"
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
        "error responses should not expose partial handles"
    );
}

#[cfg(unix)]
#[test]
fn round1_exploit_json_rejects_symlink_alias_duplicate_without_partial_handles() {
    use std::os::unix::fs::symlink;

    let workspace = tempdir().expect("tempdir should be created");
    let canonical_path = workspace.path().join("canonical.py");
    fs::write(&canonical_path, "def target():\n    return 1\n")
        .expect("fixture write should succeed");
    let symlink_path = workspace.path().join("alias.py");
    symlink(&canonical_path, &symlink_path).expect("symlink should be created");

    let request = json!({
        "command": "select",
        "files": [
            canonical_path.to_string_lossy().to_string(),
            symlink_path.to_string_lossy().to_string()
        ],
        "selector": {
            "kind": "function_definition",
            "name_pattern": null,
            "exclude_kinds": []
        }
    });

    let output = run_select_json_mode(&request.to_string());
    assert!(
        !output.status.success(),
        "symlink aliases should be rejected as duplicate logical file entries"
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
        "error responses should not expose partial handles"
    );
}

#[test]
fn round1_exploit_json_rejects_exact_duplicate_path_entries() {
    let fixture = fixture_path("example.py");
    let request = json!({
        "command": "select",
        "files": [
            fixture.to_string_lossy().to_string(),
            fixture.to_string_lossy().to_string()
        ],
        "selector": {
            "kind": "function_definition",
            "name_pattern": null,
            "exclude_kinds": []
        }
    });

    let output = run_select_json_mode(&request.to_string());
    assert!(
        !output.status.success(),
        "duplicate path entries should be rejected"
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
        "error responses should not expose partial handles"
    );
}

#[cfg(unix)]
#[test]
fn round1_exploit_cli_rejects_duplicate_alias_in_reverse_order() {
    use std::os::unix::fs::symlink;

    let workspace = tempdir().expect("tempdir should be created");
    let canonical_path = workspace.path().join("canonical.py");
    fs::write(&canonical_path, "def target():\n    return 1\n")
        .expect("fixture write should succeed");
    let symlink_path = workspace.path().join("alias.py");
    symlink(&canonical_path, &symlink_path).expect("symlink should be created");

    let output = run_select_with_files(
        &["--kind", "function_definition"],
        &[symlink_path.as_path(), canonical_path.as_path()],
    );
    assert!(
        !output.status.success(),
        "duplicate aliases should be rejected regardless of order"
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
        "error responses should not expose partial handles"
    );
}

#[test]
fn round1_explore_cli_summary_matches_handle_length_for_mixed_inputs() {
    let python_fixture = fixture_path("example.py");
    let json_fixture = fixture_path("example.json");

    let output = run_select_with_files(
        &["--kind", "function_definition"],
        &[python_fixture.as_path(), json_fixture.as_path()],
    );
    assert!(
        output.status.success(),
        "mixed inputs should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["summary"]["files_scanned"], 2);

    let handles = response["handles"]
        .as_array()
        .expect("handles should be an array");
    let matches = response["summary"]["matches"]
        .as_u64()
        .expect("summary.matches should be an integer");

    assert_eq!(matches, handles.len() as u64);
    assert!(
        handles
            .iter()
            .all(|handle| handle["kind"] == "function_definition"),
        "all handles should satisfy the requested kind filter"
    );
    assert!(
        handles
            .iter()
            .any(|handle| handle["file"] == python_fixture.to_string_lossy().to_string()),
        "expected at least one match from the python fixture"
    );
}

#[test]
fn round1_explore_json_single_file_field_reports_one_file_scanned() {
    let fixture = fixture_path("example.py");
    let request = json!({
        "command": "select",
        "file": fixture.to_string_lossy().to_string(),
        "selector": {
            "kind": "function_definition",
            "name_pattern": null,
            "exclude_kinds": []
        }
    });

    let output = run_select_json_mode(&request.to_string());
    assert!(
        output.status.success(),
        "single file request should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["summary"]["files_scanned"], 1);

    let handles = response["handles"]
        .as_array()
        .expect("handles should be an array");
    let matches = response["summary"]["matches"]
        .as_u64()
        .expect("summary.matches should be an integer");
    assert_eq!(matches, handles.len() as u64);
}

#[cfg(unix)]
#[test]
fn round2_exploit_cli_unreadable_file_returns_io_error_without_partial_handles() {
    use std::os::unix::fs::PermissionsExt;

    let workspace = tempdir().expect("tempdir should be created");
    let unreadable = workspace.path().join("locked.py");
    fs::write(&unreadable, "def target():\n    return 1\n").expect("fixture write should succeed");

    let mut permissions = fs::metadata(&unreadable)
        .expect("metadata should be readable")
        .permissions();
    permissions.set_mode(0o000);
    fs::set_permissions(&unreadable, permissions).expect("permissions update should succeed");

    let output = run_select_with_files(&["--kind", "function_definition"], &[unreadable.as_path()]);
    assert!(
        !output.status.success(),
        "unreadable file should fail with io_error"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "io_error");
    assert!(
        response.get("handles").is_none(),
        "error responses should not expose partial handles"
    );
}

#[cfg(unix)]
#[test]
fn round2_exploit_json_unreadable_file_returns_io_error_without_partial_handles() {
    use std::os::unix::fs::PermissionsExt;

    let workspace = tempdir().expect("tempdir should be created");
    let unreadable = workspace.path().join("locked.py");
    fs::write(&unreadable, "def target():\n    return 1\n").expect("fixture write should succeed");

    let mut permissions = fs::metadata(&unreadable)
        .expect("metadata should be readable")
        .permissions();
    permissions.set_mode(0o000);
    fs::set_permissions(&unreadable, permissions).expect("permissions update should succeed");

    let request = json!({
        "command": "select",
        "file": unreadable.to_string_lossy().to_string(),
        "selector": {
            "kind": "function_definition",
            "name_pattern": null,
            "exclude_kinds": []
        }
    });

    let output = run_select_json_mode(&request.to_string());
    assert!(
        !output.status.success(),
        "unreadable file should fail with io_error in json mode"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "io_error");
    assert!(
        response.get("handles").is_none(),
        "error responses should not expose partial handles"
    );
}

#[cfg(unix)]
#[test]
fn round2_exploit_cli_symlink_cycle_returns_io_error() {
    use std::os::unix::fs::symlink;

    let workspace = tempdir().expect("tempdir should be created");
    let loop_path = workspace.path().join("loop.py");
    symlink("loop.py", &loop_path).expect("self-referential symlink should be created");

    let output = run_select_with_files(&["--kind", "function_definition"], &[loop_path.as_path()]);
    assert!(
        !output.status.success(),
        "symlink cycle should fail with io_error"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "io_error");
    assert!(
        response.get("handles").is_none(),
        "error responses should not expose partial handles"
    );
}

#[cfg(unix)]
#[test]
fn round2_exploit_multi_file_first_failure_order_holds_for_unreadable_and_parse_errors() {
    use std::os::unix::fs::PermissionsExt;

    let workspace = tempdir().expect("tempdir should be created");
    let unreadable = workspace.path().join("locked.py");
    fs::write(&unreadable, "def target():\n    return 1\n").expect("fixture write should succeed");
    let mut permissions = fs::metadata(&unreadable)
        .expect("metadata should be readable")
        .permissions();
    permissions.set_mode(0o000);
    fs::set_permissions(&unreadable, permissions).expect("permissions update should succeed");

    let invalid = workspace.path().join("invalid.py");
    fs::write(&invalid, "def broken(\n").expect("fixture write should succeed");

    let io_first = run_select_with_files(
        &["--kind", "function_definition"],
        &[unreadable.as_path(), invalid.as_path()],
    );
    assert!(!io_first.status.success(), "input should fail");
    let io_response: Value =
        serde_json::from_slice(&io_first.stdout).expect("stdout should be valid JSON");
    assert_eq!(io_response["error"]["type"], "io_error");

    let parse_first = run_select_with_files(
        &["--kind", "function_definition"],
        &[invalid.as_path(), unreadable.as_path()],
    );
    assert!(!parse_first.status.success(), "input should fail");
    let parse_response: Value =
        serde_json::from_slice(&parse_first.stdout).expect("stdout should be valid JSON");
    assert_eq!(parse_response["error"]["type"], "parse_failure");
}

#[test]
fn round2_explore_cli_three_files_reports_consistent_summary() {
    let workspace = tempdir().expect("tempdir should be created");
    let python_file = workspace.path().join("generated.py");
    fs::write(
        &python_file,
        "def process_one(value):\n    return value + 1\n\ndef helper(value):\n    return value\n",
    )
    .expect("python fixture write should succeed");
    let json_file = fixture_path("example.json");
    let text_file = workspace.path().join("plain.txt");
    fs::write(&text_file, "nothing structured here").expect("text fixture write should succeed");

    let output = run_select_with_files(
        &["--kind", "function_definition", "--name", "process_*"],
        &[
            python_file.as_path(),
            json_file.as_path(),
            text_file.as_path(),
        ],
    );
    assert!(
        output.status.success(),
        "mixed three-file select should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["summary"]["files_scanned"], 3);
    assert_eq!(response["summary"]["matches"], 1);
    assert_eq!(response["handles"].as_array().map(Vec::len), Some(1));
    assert_eq!(response["handles"][0]["name"], "process_one");
}

#[test]
fn round2_explore_json_name_filter_applies_across_multiple_files() {
    let workspace = tempdir().expect("tempdir should be created");
    let generated = workspace.path().join("generated.py");
    fs::write(
        &generated,
        "def process_alpha(value):\n    return value\n\ndef helper(value):\n    return value\n",
    )
    .expect("python fixture write should succeed");

    let fixture = fixture_path("example.py");
    let request = json!({
        "command": "select",
        "files": [
            fixture.to_string_lossy().to_string(),
            generated.to_string_lossy().to_string()
        ],
        "selector": {
            "kind": "function_definition",
            "name_pattern": "process_*",
            "exclude_kinds": []
        }
    });

    let output = run_select_json_mode(&request.to_string());
    assert!(
        output.status.success(),
        "json multi-file select should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["summary"]["files_scanned"], 2);
    assert_eq!(response["summary"]["matches"], 2);
    let handles = response["handles"]
        .as_array()
        .expect("handles should be an array");
    assert!(
        handles.iter().all(|handle| handle["name"]
            .as_str()
            .is_some_and(|name| name.starts_with("process_"))),
        "all returned names should satisfy process_* glob"
    );
}

#[test]
fn round3_exploit_kind_flag_takes_precedence_over_invalid_json_payload() {
    let output = run_select_json_mode_with_args("{", &["--kind", "module"]);
    assert!(!output.status.success(), "request should fail");

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("--kind")),
        "expected selector-flag validation before JSON parsing"
    );
}

#[test]
fn round3_exploit_name_flag_takes_precedence_over_empty_stdin_payload() {
    let output = run_select_json_mode_with_args("", &["--name", "process_*"]);
    assert!(!output.status.success(), "request should fail");

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("--name")),
        "expected selector-flag validation before stdin JSON parsing"
    );
}

#[test]
fn round3_exploit_positional_file_rejection_precedes_exclude_kind_flag_message() {
    let fixture = fixture_path("example.py");
    let request = json!({
        "command": "select",
        "file": fixture.to_string_lossy().to_string(),
        "selector": {
            "kind": "function_definition",
            "name_pattern": null,
            "exclude_kinds": []
        }
    });
    let fixture_arg = fixture.to_str().expect("path should be valid UTF-8");

    let output = run_select_json_mode_with_args(
        &request.to_string(),
        &["--exclude-kind", "comment", fixture_arg],
    );
    assert!(!output.status.success(), "request should fail");

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("positional FILE arguments")),
        "positional FILE rejection should fire first"
    );
}

#[test]
fn round3_exploit_positional_file_rejection_precedes_kind_flag_message() {
    let fixture = fixture_path("example.py");
    let request = json!({
        "command": "select",
        "file": fixture.to_string_lossy().to_string(),
        "selector": {
            "kind": "function_definition",
            "name_pattern": null,
            "exclude_kinds": []
        }
    });
    let fixture_arg = fixture.to_str().expect("path should be valid UTF-8");

    let output =
        run_select_json_mode_with_args(&request.to_string(), &["--kind", "module", fixture_arg]);
    assert!(!output.status.success(), "request should fail");

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("positional FILE arguments")),
        "positional FILE rejection should fire first"
    );
}

#[test]
fn round3_explore_cli_exclude_kind_can_force_zero_matches_across_files() {
    let python_fixture = fixture_path("example.py");
    let mut generated_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp file should be created");
    generated_file
        .write_all(b"def process_beta(value):\n    return value\n")
        .expect("fixture write should succeed");
    let generated_path = generated_file.keep().expect("temp file should persist").1;

    let output = run_select_with_files(
        &[
            "--kind",
            "function_definition",
            "--exclude-kind",
            "function_definition",
        ],
        &[python_fixture.as_path(), generated_path.as_path()],
    );
    assert!(
        output.status.success(),
        "request should succeed with empty result: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["summary"]["files_scanned"], 2);
    assert_eq!(response["summary"]["matches"], 0);
    assert_eq!(response["handles"].as_array().map(Vec::len), Some(0));
}

#[test]
fn round3_explore_json_multi_file_handles_case_insensitive_extensions() {
    let mut python_upper = Builder::new()
        .suffix(".PY")
        .tempfile()
        .expect("temp python file should be created");
    python_upper
        .write_all(b"def process_case(value):\n    return value\n")
        .expect("python fixture write should succeed");
    let python_upper_path = python_upper.keep().expect("temp file should persist").1;

    let mut json_upper = Builder::new()
        .suffix(".JSON")
        .tempfile()
        .expect("temp json file should be created");
    json_upper
        .write_all(b"{\"config\": {\"enabled\": true}}\n")
        .expect("json fixture write should succeed");
    let json_upper_path = json_upper.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "select",
        "files": [
            python_upper_path.to_string_lossy().to_string(),
            json_upper_path.to_string_lossy().to_string()
        ],
        "selector": {
            "kind": "module",
            "name_pattern": null,
            "exclude_kinds": []
        }
    });

    let output = run_select_json_mode(&request.to_string());
    assert!(
        output.status.success(),
        "case-insensitive extension handling should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["summary"]["files_scanned"], 2);
    assert_eq!(response["summary"]["matches"], 1);
    assert_eq!(
        response["handles"][0]["file"],
        python_upper_path.to_string_lossy().to_string()
    );
}

#[test]
fn round4_exploit_cli_second_file_parse_failure_does_not_leak_first_file_handles() {
    let valid_fixture = fixture_path("example.py");
    let workspace = tempdir().expect("tempdir should be created");
    let invalid_file = workspace.path().join("invalid.py");
    fs::write(&invalid_file, "def broken(\n").expect("invalid fixture write should succeed");

    let output = run_select_with_files(
        &["--kind", "function_definition"],
        &[valid_fixture.as_path(), invalid_file.as_path()],
    );
    assert!(
        !output.status.success(),
        "second-file parse failure should fail whole request"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "parse_failure");
    assert!(
        response.get("handles").is_none(),
        "error responses should not expose partial handles from earlier files"
    );
}

#[test]
fn round4_exploit_json_second_file_parse_failure_does_not_leak_first_file_handles() {
    let valid_fixture = fixture_path("example.py");
    let workspace = tempdir().expect("tempdir should be created");
    let invalid_file = workspace.path().join("invalid.py");
    fs::write(&invalid_file, "def broken(\n").expect("invalid fixture write should succeed");

    let request = json!({
        "command": "select",
        "files": [
            valid_fixture.to_string_lossy().to_string(),
            invalid_file.to_string_lossy().to_string()
        ],
        "selector": {
            "kind": "function_definition",
            "name_pattern": null,
            "exclude_kinds": []
        }
    });

    let output = run_select_json_mode(&request.to_string());
    assert!(
        !output.status.success(),
        "second-file parse failure should fail whole request"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "parse_failure");
    assert!(
        response.get("handles").is_none(),
        "error responses should not expose partial handles from earlier files"
    );
}

#[cfg(unix)]
#[test]
fn round4_exploit_cli_second_file_duplicate_does_not_leak_first_file_handles() {
    use std::os::unix::fs::symlink;

    let workspace = tempdir().expect("tempdir should be created");
    let canonical_path = workspace.path().join("canonical.py");
    fs::write(&canonical_path, "def target():\n    return 1\n")
        .expect("fixture write should succeed");
    let symlink_path = workspace.path().join("alias.py");
    symlink(&canonical_path, &symlink_path).expect("symlink should be created");

    let output = run_select_with_files(
        &["--kind", "function_definition"],
        &[canonical_path.as_path(), symlink_path.as_path()],
    );
    assert!(
        !output.status.success(),
        "duplicate detection in later files should fail whole request"
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
        "error responses should not expose partial handles from earlier files"
    );
}

#[cfg(unix)]
#[test]
fn round4_exploit_json_second_file_duplicate_does_not_leak_first_file_handles() {
    use std::os::unix::fs::symlink;

    let workspace = tempdir().expect("tempdir should be created");
    let canonical_path = workspace.path().join("canonical.py");
    fs::write(&canonical_path, "def target():\n    return 1\n")
        .expect("fixture write should succeed");
    let symlink_path = workspace.path().join("alias.py");
    symlink(&canonical_path, &symlink_path).expect("symlink should be created");

    let request = json!({
        "command": "select",
        "files": [
            canonical_path.to_string_lossy().to_string(),
            symlink_path.to_string_lossy().to_string()
        ],
        "selector": {
            "kind": "function_definition",
            "name_pattern": null,
            "exclude_kinds": []
        }
    });

    let output = run_select_json_mode(&request.to_string());
    assert!(
        !output.status.success(),
        "duplicate detection in later files should fail whole request"
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
        "error responses should not expose partial handles from earlier files"
    );
}

#[test]
fn round4_explore_cli_name_filter_with_first_file_no_matches_second_file_matches() {
    let workspace = tempdir().expect("tempdir should be created");
    let generated = workspace.path().join("generated.py");
    fs::write(&generated, "def process_gamma(value):\n    return value\n")
        .expect("fixture write should succeed");

    let output = run_select_with_files(
        &["--kind", "function_definition", "--name", "process_*"],
        &[fixture_path("example.json").as_path(), generated.as_path()],
    );
    assert!(
        output.status.success(),
        "mixed no-match/match files should still succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["summary"]["files_scanned"], 2);
    assert_eq!(response["summary"]["matches"], 1);
    assert_eq!(response["handles"].as_array().map(Vec::len), Some(1));
    assert_eq!(response["handles"][0]["name"], "process_gamma");
}

#[test]
fn round4_explore_json_accepts_relative_dot_segment_paths_in_files_array() {
    let workspace = tempdir().expect("tempdir should be created");
    let generated = workspace.path().join("generated.py");
    fs::write(&generated, "def process_delta(value):\n    return value\n")
        .expect("fixture write should succeed");

    let request = json!({
        "command": "select",
        "files": [
            "./tests/fixtures/example.py",
            generated.to_string_lossy().to_string()
        ],
        "selector": {
            "kind": "function_definition",
            "name_pattern": "process_*",
            "exclude_kinds": []
        }
    });

    let output = run_select_json_mode(&request.to_string());
    assert!(
        output.status.success(),
        "relative dot-segment file paths should work in json mode: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["summary"]["files_scanned"], 2);
    assert_eq!(response["summary"]["matches"], 2);
}
