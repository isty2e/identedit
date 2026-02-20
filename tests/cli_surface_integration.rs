use std::fs;
use std::path::Path;
use std::path::PathBuf;

use serde_json::{Value, json};

mod common;

fn run_identedit(args: &[&str]) -> std::process::Output {
    common::run_identedit(args)
}

fn run_identedit_with_stdin(args: &[&str], input: &str) -> std::process::Output {
    common::run_identedit_with_stdin(args, input)
}

fn copy_fixture_to_temp_python(name: &str) -> PathBuf {
    common::copy_fixture_to_temp_python(name)
}

fn read_json(file: &Path) -> Value {
    let output = run_identedit(&[
        "read",
        "--mode",
        "ast",
        "--kind",
        "function_definition",
        "--verbose",
        "--json",
        file.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "read should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON")
}

#[test]
fn top_level_help_exposes_new_command_surface() {
    let output = run_identedit(&["--help"]);
    assert!(
        output.status.success(),
        "help should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).expect("stdout should be utf-8");
    assert!(text.contains("read"));
    assert!(text.contains("edit"));
    assert!(text.contains("apply"));
    assert!(text.contains("merge"));
    assert!(text.contains("grammar"));
    assert!(text.contains("patch"));
    assert!(!text.contains("hashline"));
    assert!(!text.contains("transform"));
    assert!(!text.contains("changeset"));
    assert!(!text.contains("select"));
}

#[test]
fn read_line_mode_outputs_line_hash_anchors() {
    let file = copy_fixture_to_temp_python("example.py");
    let read_output = run_identedit(&[
        "read",
        "--mode",
        "line",
        file.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        read_output.status.success(),
        "read --mode line should succeed: {}",
        String::from_utf8_lossy(&read_output.stderr)
    );
    let text = String::from_utf8(read_output.stdout).expect("stdout should be utf-8");
    assert!(text.contains("1:"));
    assert!(text.contains("|"));
}

#[test]
fn patch_supports_at_node_identity_and_file_end_insert() {
    let file = copy_fixture_to_temp_python("example.py");
    let read_response = read_json(&file);
    let handle = read_response["handles"][0].clone();
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present")
        .to_string();

    let patch_node = run_identedit(&[
        "patch",
        "--at",
        &identity,
        "--replace",
        "def process_data(value):\n    return value - 11",
        file.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        patch_node.status.success(),
        "node patch should succeed: {}",
        String::from_utf8_lossy(&patch_node.stderr)
    );

    let patch_file_end = run_identedit(&[
        "patch",
        "--at",
        "file-end",
        "--insert",
        "\n# appended-by-patch\n",
        file.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        patch_file_end.status.success(),
        "file-end patch should succeed: {}",
        String::from_utf8_lossy(&patch_file_end.stderr)
    );

    let updated = fs::read_to_string(&file).expect("file should be readable");
    assert!(updated.contains("return value - 11"));
    assert!(updated.contains("# appended-by-patch"));
}

#[test]
fn patch_supports_at_line_anchor() {
    let file = copy_fixture_to_temp_python("example.py");
    let output = run_identedit(&[
        "read",
        "--mode",
        "line",
        "--json",
        file.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "line read should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be json");
    let anchor = response["handles"][1]["anchor"]
        .as_str()
        .expect("line anchor should be present")
        .to_string();

    let patch = run_identedit(&[
        "patch",
        "--at",
        &anchor,
        "--set-line",
        "    result = value + 99",
        file.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        patch.status.success(),
        "line patch should succeed: {}",
        String::from_utf8_lossy(&patch.stderr)
    );

    let updated = fs::read_to_string(&file).expect("file should be readable");
    assert!(updated.contains("result = value + 99"));
}

#[test]
fn apply_dry_run_previews_without_writing() {
    let file = copy_fixture_to_temp_python("example.py");
    let before = fs::read_to_string(&file).expect("file should be readable");
    let read_response = read_json(&file);
    let handle = read_response["handles"][0].clone();
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");

    let edit_output = run_identedit(&[
        "edit",
        "--identity",
        identity,
        "--replace",
        "def process_data(value):\n    return value - 5",
        file.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        edit_output.status.success(),
        "edit should succeed: {}",
        String::from_utf8_lossy(&edit_output.stderr)
    );
    let changeset = String::from_utf8(edit_output.stdout).expect("stdout should be utf-8");

    let dry_run = run_identedit_with_stdin(&["apply", "--dry-run"], &changeset);
    assert!(
        dry_run.status.success(),
        "apply --dry-run should succeed: {}",
        String::from_utf8_lossy(&dry_run.stderr)
    );
    let dry_run_response: Value =
        serde_json::from_slice(&dry_run.stdout).expect("stdout should be json");
    assert_eq!(dry_run_response["transaction"]["status"], "dry_run");

    let after = fs::read_to_string(&file).expect("file should be readable");
    assert_eq!(before, after, "dry-run must not modify files");
}

#[test]
fn apply_repair_remaps_stale_line_anchors() {
    let file = copy_fixture_to_temp_python("example.py");
    let line_read = run_identedit(&[
        "read",
        "--mode",
        "line",
        "--json",
        file.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        line_read.status.success(),
        "line read should succeed: {}",
        String::from_utf8_lossy(&line_read.stderr)
    );
    let line_response: Value =
        serde_json::from_slice(&line_read.stdout).expect("stdout should be json");
    let anchor = line_response["handles"][1]["anchor"]
        .as_str()
        .expect("anchor should exist")
        .to_string();

    let edit_request = json!({
        "command": "edit",
        "file": file.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "type": "line",
                    "anchor": anchor,
                },
                "op": {
                    "type": "replace",
                    "new_text": "    result = value + 123\n"
                }
            }
        ]
    });
    let edit_output = run_identedit_with_stdin(&["edit", "--json"], &edit_request.to_string());
    assert!(
        edit_output.status.success(),
        "edit json should succeed: {}",
        String::from_utf8_lossy(&edit_output.stderr)
    );
    let changeset = String::from_utf8(edit_output.stdout).expect("stdout should be utf-8");

    let original = fs::read_to_string(&file).expect("file should be readable");
    fs::write(&file, format!("# header\n{original}")).expect("file rewrite should succeed");

    let strict_apply = run_identedit_with_stdin(&["apply"], &changeset);
    assert!(
        !strict_apply.status.success(),
        "strict apply should fail with stale anchor"
    );

    let repaired_apply = run_identedit_with_stdin(&["apply", "--repair"], &changeset);
    assert!(
        repaired_apply.status.success(),
        "apply --repair should remap stale line anchor: {}",
        String::from_utf8_lossy(&repaired_apply.stderr)
    );

    let updated = fs::read_to_string(&file).expect("file should be readable");
    assert!(updated.contains("result = value + 123"));
}

#[test]
fn legacy_subcommands_are_no_longer_available() {
    let output = run_identedit(&["transform", "--json"]);
    assert!(
        !output.status.success(),
        "legacy transform command should fail"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unrecognized subcommand 'transform'"));

    let output = run_identedit(&["hashline", "show", "tests/fixtures/example.py"]);
    assert!(
        !output.status.success(),
        "legacy hashline command should fail"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unrecognized subcommand 'hashline'"));
}
