use super::*;

#[test]
fn patch_json_mode_rejects_flag_text_source_options() {
    let request = json!({
        "command": "patch",
        "file": "/tmp/example.py",
        "target": {
            "type": "line",
            "anchor": "1:aaaaaaaaaaaa"
        },
        "op": {
            "type": "set_line",
            "new_text": "value"
        }
    });

    let output =
        run_identedit_with_stdin(&["patch", "--json", "--stdin-text"], &request.to_string());

    assert!(
        !output.status.success(),
        "json mode should reject flag text source options"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let message = response["error"]["message"]
        .as_str()
        .expect("error message should be present");
    assert!(
        message.contains("--stdin-text") && message.contains("flag mode"),
        "error should explain json/text-source incompatibility, got: {message}"
    );
}

#[test]
fn patch_json_mode_rejects_diff_output() {
    let output = run_identedit_with_stdin(&["patch", "--json", "--diff"], "{}");

    assert!(
        !output.status.success(),
        "--json --diff should fail before interpreting stdin"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let message = response["error"]["message"]
        .as_str()
        .expect("error message should be present");
    assert!(
        message.contains("--diff") && message.contains("JSON"),
        "error should explain that JSON mode always returns JSON"
    );
}

#[test]
fn patch_json_node_target_replace_applies_change() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_function_handle(&file_path, "process_*");
    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "node",
            "identity": handle["identity"],
            "kind": handle["kind"],
            "span_hint": {
                "start": handle["span"]["start"],
                "end": handle["span"]["end"]
            },
            "expected_old_hash": identedit::changeset::hash_text(
                handle["text"].as_str().expect("text should be string")
            )
        },
        "op": {
            "type": "replace",
            "new_text": "def process_data(value):\n    return value * 11"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "patch --json node replace failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["summary"]["operations_applied"], 1);

    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    assert!(modified.contains("return value * 11"));
}

#[test]
fn patch_json_node_target_replace_options_dry_run_does_not_modify_file() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let before = fs::read_to_string(&file_path).expect("file should be readable");
    let handle = select_named_function_handle(&file_path, "process_*");
    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "node",
            "identity": handle["identity"],
            "kind": handle["kind"],
            "span_hint": handle["span"],
            "expected_old_hash": handle["expected_old_hash"]
        },
        "op": {
            "type": "replace",
            "new_text": "def process_data(value):\n    return value * 17"
        },
        "options": {
            "dry_run": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "json node dry-run should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["transaction"]["status"], "dry_run");
    assert_eq!(
        fs::read_to_string(&file_path).expect("file should be readable"),
        before,
        "json node dry-run must not modify the file"
    );
}

#[test]
fn patch_json_cli_dry_run_overrides_node_request_and_does_not_modify_file() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let before = fs::read_to_string(&file_path).expect("file should be readable");
    let handle = select_named_function_handle(&file_path, "process_*");
    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "node",
            "identity": handle["identity"],
            "kind": handle["kind"],
            "span_hint": handle["span"],
            "expected_old_hash": handle["expected_old_hash"]
        },
        "op": {
            "type": "replace",
            "new_text": "def process_data(value):\n    return value * 19"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json", "--dry-run"], &request.to_string());
    assert!(
        output.status.success(),
        "json cli dry-run should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["transaction"]["status"], "dry_run");
    assert_eq!(
        fs::read_to_string(&file_path).expect("file should be readable"),
        before,
        "cli dry-run in json mode must not modify node target files"
    );
}
