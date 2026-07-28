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

fn run_shared_intent(command: &str, intent_args: &[&str], file: &Path) -> std::process::Output {
    let mut args = Vec::with_capacity(intent_args.len() + 3);
    args.push(command);
    args.extend_from_slice(intent_args);
    if command == "patch" {
        args.push("--dry-run");
    }
    args.push(file.to_str().expect("path should be utf-8"));
    run_identedit(&args)
}

fn assert_shared_intent_is_plannable_and_dry_runnable(intent_args: &[&str], file: &Path) {
    let before = fs::read_to_string(file).expect("file should be readable");

    let edit_output = run_shared_intent("edit", intent_args, file);
    assert!(
        edit_output.status.success(),
        "edit should accept shared intent: {}",
        String::from_utf8_lossy(&edit_output.stderr)
    );
    let edit_response: Value =
        serde_json::from_slice(&edit_output.stdout).expect("edit stdout should be JSON");
    assert_eq!(
        edit_response["files"][0]["operations"]
            .as_array()
            .map(Vec::len),
        Some(1)
    );
    assert_eq!(
        fs::read_to_string(file).expect("file should remain readable"),
        before,
        "edit must only plan the shared intent"
    );

    let patch_output = run_shared_intent("patch", intent_args, file);
    assert!(
        patch_output.status.success(),
        "patch --dry-run should accept shared intent: {}",
        String::from_utf8_lossy(&patch_output.stderr)
    );
    serde_json::from_slice::<Value>(&patch_output.stdout).expect("patch stdout should be JSON");
    assert_eq!(
        fs::read_to_string(file).expect("file should remain readable"),
        before,
        "patch --dry-run must not apply the shared intent"
    );
}

#[test]
fn package_exposes_only_the_identedit_binary() {
    let manifest_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    let manifest_text = fs::read_to_string(manifest_path).expect("Cargo.toml should be readable");
    let manifest: toml::Value =
        toml::from_str(&manifest_text).expect("Cargo.toml should be valid TOML");

    assert_eq!(manifest["package"]["autolib"].as_bool(), Some(false));
    assert!(
        !Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/lib.rs")
            .exists(),
        "the CLI-only package must not expose an implicit Rust library target"
    );
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
fn top_level_version_reports_package_version() {
    let output = run_identedit(&["--version"]);
    assert!(
        output.status.success(),
        "version should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).expect("stdout should be utf-8");
    assert_eq!(
        text.trim(),
        format!("identedit {}", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn edit_and_patch_expose_the_same_single_target_intent_options() {
    let edit_output = run_identedit(&["edit", "--help"]);
    let patch_output = run_identedit(&["patch", "--help"]);
    assert!(edit_output.status.success());
    assert!(patch_output.status.success());

    let edit_help = String::from_utf8(edit_output.stdout).expect("edit help should be utf-8");
    let patch_help = String::from_utf8(patch_output.stdout).expect("patch help should be utf-8");
    let shared_options = [
        "--at",
        "--end-anchor",
        "--config-path",
        "--document-index",
        "--kind",
        "--name",
        "--symbol",
        "--replace",
        "--text-file",
        "--stdin-text",
        "--set-value",
        "--append-value",
        "--create-missing",
        "--insert",
        "--scoped-regex",
        "--scoped-replacement",
        "--delete",
        "--insert-before",
        "--insert-after",
        "--set-line",
        "--replace-range",
        "--insert-after-line",
    ];

    for option in shared_options {
        assert!(
            edit_help.contains(option),
            "edit help is missing shared intent option {option}"
        );
        assert!(
            patch_help.contains(option),
            "patch help is missing shared intent option {option}"
        );
    }
    assert!(
        !edit_help.contains("--identity"),
        "edit should not expose the removed --identity alias"
    );
}

#[test]
fn edit_and_patch_accept_representative_shared_intents() {
    let file = copy_fixture_to_temp_python("example.py");

    assert_shared_intent_is_plannable_and_dry_runnable(
        &[
            "--symbol",
            "process_data",
            "--replace",
            "def process_data(value):\n    return value - 21",
        ],
        &file,
    );

    let line_output = run_identedit(&[
        "read",
        "--mode",
        "line",
        "--json",
        file.to_str().expect("path should be utf-8"),
    ]);
    assert!(line_output.status.success());
    let line_response: Value =
        serde_json::from_slice(&line_output.stdout).expect("line read stdout should be JSON");
    let anchor = line_response["handles"][1]["anchor"]
        .as_str()
        .expect("line anchor should be present");
    assert_shared_intent_is_plannable_and_dry_runnable(
        &["--at", anchor, "--set-line", "    result = value + 21"],
        &file,
    );

    assert_shared_intent_is_plannable_and_dry_runnable(
        &["--at", "file-end", "--insert", "\n# shared intent\n"],
        &file,
    );
}

#[test]
fn edit_and_patch_report_the_same_invalid_intent_diagnostic() {
    let file = copy_fixture_to_temp_python("example.py");
    let invalid_intent = ["--at", "file-end", "--replace", "invalid"];

    let edit_output = run_shared_intent("edit", &invalid_intent, &file);
    let patch_output = run_shared_intent("patch", &invalid_intent, &file);
    assert!(!edit_output.status.success());
    assert!(!patch_output.status.success());

    let edit_response: Value =
        serde_json::from_slice(&edit_output.stdout).expect("edit stdout should be JSON");
    let patch_response: Value =
        serde_json::from_slice(&patch_output.stdout).expect("patch stdout should be JSON");
    assert_eq!(edit_response["error"], patch_response["error"]);
}

#[test]
fn patch_help_exposes_symbol_selector() {
    let output = run_identedit(&["patch", "--help"]);
    assert!(
        output.status.success(),
        "patch help should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).expect("stdout should be utf-8");
    assert!(text.contains("--symbol"));
    assert!(text.contains("Class.method"));
}

#[test]
fn read_help_exposes_plain_json_flag() {
    let output = run_identedit(&["read", "--help"]);
    assert!(
        output.status.success(),
        "read help should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).expect("stdout should be utf-8");
    assert!(text.contains("--json"));
    assert!(
        !text.contains("--json..."),
        "read --json should not be exposed as a count flag"
    );
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
        "--at",
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
fn apply_dry_run_reports_compact_multi_file_summary() {
    let first_file = copy_fixture_to_temp_python("example.py");
    let second_file = copy_fixture_to_temp_python("example.py");
    let first_before = fs::read_to_string(&first_file).expect("first file should be readable");
    let second_before = fs::read_to_string(&second_file).expect("second file should be readable");

    let first_read = read_json(&first_file);
    let second_read = read_json(&second_file);
    let first_identity = first_read["handles"][0]["identity"]
        .as_str()
        .expect("first identity should be present");
    let second_identity = second_read["handles"][0]["identity"]
        .as_str()
        .expect("second identity should be present");

    let first_edit = run_identedit(&[
        "edit",
        "--at",
        first_identity,
        "--replace",
        "def process_data(value):\n    return value - 10",
        first_file.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        first_edit.status.success(),
        "first edit should succeed: {}",
        String::from_utf8_lossy(&first_edit.stderr)
    );
    let second_edit = run_identedit(&[
        "edit",
        "--at",
        second_identity,
        "--replace",
        "def process_data(value):\n    return value + 10",
        second_file.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        second_edit.status.success(),
        "second edit should succeed: {}",
        String::from_utf8_lossy(&second_edit.stderr)
    );

    let first_changeset: Value =
        serde_json::from_slice(&first_edit.stdout).expect("first edit stdout should be json");
    let second_changeset: Value =
        serde_json::from_slice(&second_edit.stdout).expect("second edit stdout should be json");
    let combined = json!({
        "files": [
            first_changeset["files"][0].clone(),
            second_changeset["files"][0].clone(),
        ],
        "transaction": { "mode": "all_or_nothing" },
    });

    let dry_run = run_identedit_with_stdin(&["apply", "--dry-run"], &combined.to_string());
    assert!(
        dry_run.status.success(),
        "apply --dry-run should succeed: {}",
        String::from_utf8_lossy(&dry_run.stderr)
    );
    let response: Value = serde_json::from_slice(&dry_run.stdout).expect("stdout should be json");

    assert_eq!(response["summary"]["files_modified"], 2);
    assert_eq!(response["summary"]["operations_applied"], 2);
    assert_eq!(response["summary"]["operations_failed"], 0);
    assert_eq!(response["transaction"]["mode"], "all_or_nothing");
    assert_eq!(response["transaction"]["status"], "dry_run");
    assert_eq!(response["dry_run"]["status"], "ready");
    assert_eq!(response["dry_run"]["files_checked"], 2);
    assert_eq!(response["dry_run"]["would_modify_files"], 2);
    assert_eq!(response["dry_run"]["would_apply_operations"], 2);
    assert_eq!(response["dry_run"]["preconditions"], "passed");
    assert_eq!(response["dry_run"]["ambiguous_targets"], 0);
    assert_eq!(response["dry_run"]["stale_targets"], 0);
    assert_eq!(
        response["dry_run"]["files"]
            .as_array()
            .expect("dry-run files should be an array")
            .len(),
        2
    );
    assert!(
        response.get("applied").is_none(),
        "compact dry-run should not include verbose applied entries"
    );

    assert_eq!(
        fs::read_to_string(&first_file).expect("first file should remain readable"),
        first_before,
        "dry-run must not modify first file"
    );
    assert_eq!(
        fs::read_to_string(&second_file).expect("second file should remain readable"),
        second_before,
        "dry-run must not modify second file"
    );
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
