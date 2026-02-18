use std::io::Write;
use std::path::Path;

use serde_json::{Value, json};
use tempfile::Builder;

mod common;

fn run_identedit(arguments: &[&str]) -> std::process::Output {
    common::run_identedit(arguments)
}

fn select_first_handle(file: &Path, kind: &str, name_pattern: Option<&str>) -> Value {
    common::select_first_handle(file, kind, name_pattern)
}

fn write_json_file(value: &Value) -> tempfile::NamedTempFile {
    let mut file = Builder::new()
        .suffix(".json")
        .tempfile()
        .expect("temp JSON should be created");
    file.write_all(value.to_string().as_bytes())
        .expect("temp JSON should be writable");
    file
}

fn build_replace_changeset(file: &Path, identity: &str, replacement: &str) -> Value {
    let output = run_identedit(&[
        "transform",
        "--identity",
        identity,
        "--replace",
        replacement,
        file.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "transform should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("transform output should be valid JSON")
}

#[test]
fn changeset_merge_combines_two_files_without_manual_stitching() {
    let file_a = common::copy_fixture_to_temp_python("example.py");
    let file_b = common::copy_fixture_to_temp_python("example.py");
    let handle_a = select_first_handle(&file_a, "function_definition", Some("process_*"));
    let handle_b = select_first_handle(&file_b, "function_definition", Some("process_*"));
    let identity_a = handle_a["identity"]
        .as_str()
        .expect("identity should be present");
    let identity_b = handle_b["identity"]
        .as_str()
        .expect("identity should be present");

    let change_a = build_replace_changeset(
        &file_a,
        identity_a,
        "def process_data(value):\n    return value * 2",
    );
    let change_b = build_replace_changeset(
        &file_b,
        identity_b,
        "def process_data(value):\n    return value * 3",
    );
    let file_a_json = write_json_file(&change_a);
    let file_b_json = write_json_file(&change_b);

    let output = run_identedit(&[
        "changeset",
        "merge",
        file_a_json.path().to_str().expect("path should be utf-8"),
        file_b_json.path().to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "changeset merge should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let merged: Value =
        serde_json::from_slice(&output.stdout).expect("merged output should be JSON");
    assert_eq!(
        merged["files"].as_array().map(std::vec::Vec::len),
        Some(2),
        "merge should emit both files in one changeset payload"
    );
}

#[test]
fn changeset_merge_allows_non_overlapping_operations_on_same_file() {
    let file = common::copy_fixture_to_temp_python("example.py");
    let process = select_first_handle(&file, "function_definition", Some("process_*"));
    let helper = select_first_handle(&file, "function_definition", Some("helper"));
    let process_identity = process["identity"]
        .as_str()
        .expect("process identity should exist");
    let helper_identity = helper["identity"]
        .as_str()
        .expect("helper identity should exist");

    let process_change = build_replace_changeset(
        &file,
        process_identity,
        "def process_data(value):\n    return value * 10",
    );
    let helper_change = build_replace_changeset(
        &file,
        helper_identity,
        "def helper():\n    return \"changed\"",
    );
    let process_json = write_json_file(&process_change);
    let helper_json = write_json_file(&helper_change);

    let output = run_identedit(&[
        "changeset",
        "merge",
        process_json.path().to_str().expect("path should be utf-8"),
        helper_json.path().to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "merge should allow non-overlapping same-file edits: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let merged: Value =
        serde_json::from_slice(&output.stdout).expect("merged output should be JSON");
    assert_eq!(
        merged["files"].as_array().map(std::vec::Vec::len),
        Some(1),
        "same-file merge should collapse into one file entry"
    );
    assert_eq!(
        merged["files"][0]["operations"]
            .as_array()
            .map(std::vec::Vec::len),
        Some(2),
        "merged file entry should contain both operations"
    );
}

#[test]
fn changeset_merge_rejects_conflicting_same_file_operations() {
    let file = common::copy_fixture_to_temp_python("example.py");
    let process = select_first_handle(&file, "function_definition", Some("process_*"));
    let process_identity = process["identity"]
        .as_str()
        .expect("process identity should exist");

    let change_one = build_replace_changeset(
        &file,
        process_identity,
        "def process_data(value):\n    return value * 20",
    );
    let change_two = build_replace_changeset(
        &file,
        process_identity,
        "def process_data(value):\n    return value * 21",
    );
    let change_one_json = write_json_file(&change_one);
    let change_two_json = write_json_file(&change_two);

    let output = run_identedit(&[
        "changeset",
        "merge",
        change_one_json
            .path()
            .to_str()
            .expect("path should be utf-8"),
        change_two_json
            .path()
            .to_str()
            .expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "merge should reject conflicting operations on the same span"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("error output should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("conflicting operations")),
        "expected strict conflict diagnostics"
    );
}

#[test]
fn changeset_merge_rejects_move_with_content_edit_on_same_file() {
    let file = common::copy_fixture_to_temp_python("example.py");
    let destination = file.with_extension("renamed.py");
    let process = select_first_handle(&file, "function_definition", Some("process_*"));
    let process_identity = process["identity"]
        .as_str()
        .expect("process identity should exist");

    let edit_change = build_replace_changeset(
        &file,
        process_identity,
        "def process_data(value):\n    return value * 30",
    );

    let move_change = json!({
        "files": [
            {
                "file": file.to_string_lossy().to_string(),
                "operations": [
                    {
                        "target": {
                            "type": "node",
                            "identity": "move-placeholder",
                            "kind": "file",
                            "expected_old_hash": "move-placeholder"
                        },
                        "op": {
                            "type": "move",
                            "to": destination.to_string_lossy().to_string()
                        },
                        "preview": {
                            "old_text": "",
                            "new_text": "",
                            "matched_span": {
                                "start": 0,
                                "end": 0
                            }
                        }
                    }
                ]
            }
        ],
        "transaction": {
            "mode": "all_or_nothing"
        }
    });

    let edit_json = write_json_file(&edit_change);
    let move_json = write_json_file(&move_change);
    let output = run_identedit(&[
        "changeset",
        "merge",
        edit_json.path().to_str().expect("path should be utf-8"),
        move_json.path().to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "merge should reject move + content edit on same file"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("error output should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("move cannot be merged")),
        "expected strict move/edit diagnostic"
    );
}
