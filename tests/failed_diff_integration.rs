use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Output;

use serde_json::Value;
use tempfile::Builder;

mod common;

fn create_temp_source(content: &str) -> PathBuf {
    create_temp_file(content, ".txt")
}

fn create_temp_diff(content: &str) -> PathBuf {
    create_temp_file(content, ".diff")
}

fn create_temp_file(content: &str, suffix: &str) -> PathBuf {
    let mut file = Builder::new()
        .suffix(suffix)
        .tempfile()
        .expect("temporary file should be created");
    file.write_all(content.as_bytes())
        .expect("temporary file should be written");
    file.keep().expect("temporary file should persist").1
}

fn run_from_diff(command: &str, diff: &Path, source: Option<&Path>) -> Output {
    let diff = diff.to_string_lossy().into_owned();
    let source = source.map(|path| path.to_string_lossy().into_owned());
    let mut args = vec![command, "--from-diff", diff.as_str()];
    if let Some(source) = source.as_deref() {
        args.push(source);
    }
    common::run_identedit(&args)
}

fn response(output: &Output) -> Value {
    assert!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    serde_json::from_slice(&output.stdout).expect("stdout should contain JSON")
}

fn error(output: &Output) -> Value {
    assert!(
        !output.status.success(),
        "command unexpectedly succeeded: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    serde_json::from_slice(&output.stdout).expect("stdout should contain JSON error")
}

fn assert_source_unchanged(path: &Path, expected: &str) {
    assert_eq!(
        fs::read_to_string(path).expect("source should be readable"),
        expected
    );
}

fn serialized_anchor_line(candidate: &Value, field: &str) -> usize {
    candidate["target"][field]
        .as_str()
        .expect("candidate anchor should be a string")
        .split_once(':')
        .expect("candidate anchor should contain ':'")
        .0
        .parse()
        .expect("candidate anchor line should be numeric")
}

fn apply_handoff_candidate(handoff: &Value, change_index: usize, candidate_index: usize) -> Output {
    let candidate = &handoff["changes"][change_index]["candidates"][candidate_index];
    let request = serde_json::json!({
        "command": "patch",
        "file": handoff["file"],
        "target": candidate["target"],
        "op": candidate["op"]
    });
    common::run_identedit_with_stdin(&["patch", "--json"], &request.to_string())
}

#[test]
fn edit_and_patch_return_the_same_unique_preview_without_writing() {
    let original = "before\nold value\nafter\n";
    let source = create_temp_source(original);
    let diff = create_temp_diff("@@ -1,3 +1,3 @@\n before\n-old value\n+new value\n after\n");

    let edit = run_from_diff("edit", &diff, Some(&source));
    let patch = run_from_diff("patch", &diff, Some(&source));
    let edit_response = response(&edit);
    let patch_response = response(&patch);

    assert_eq!(edit_response, patch_response);
    assert_eq!(edit_response["mode"], "failed_diff_handoff");
    assert_eq!(edit_response["preview_only"], true);
    assert_eq!(edit_response["summary"]["unique"], 1);
    assert_eq!(edit_response["summary"]["ambiguous"], 0);
    assert_eq!(edit_response["summary"]["missing"], 0);
    assert_eq!(edit_response["changes"][0]["status"], "unique");
    assert_eq!(
        edit_response["changes"][0]["candidates"][0]["op"],
        serde_json::json!({"type": "replace_lines", "new_text": "new value"})
    );
    assert_eq!(
        edit_response["changes"][0]["candidates"][0]["target"]["type"],
        "line"
    );
    assert_eq!(
        edit_response["changes"][0]["candidates"][0]["target"]["anchor"],
        format!("2:{}", common::compute_line_hash("old value"))
    );
    assert_source_unchanged(&source, original);
}

#[test]
fn stdin_diff_input_is_supported_and_remains_preview_only() {
    let original = "alpha\nold\nomega\n";
    let source = create_temp_source(original);
    let source_arg = source.to_string_lossy();
    let diff = "@@\n alpha\n-old\n+new\n omega\n";

    let output =
        common::run_identedit_with_stdin(&["patch", "--from-diff", "-", source_arg.as_ref()], diff);
    let value = response(&output);

    assert_eq!(value["summary"]["unique"], 1);
    assert_source_unchanged(&source, original);
}

#[test]
fn unified_diff_header_infers_the_source_path() {
    let original = "old\n";
    let source = create_temp_source(original);
    let source_label = source.to_string_lossy();
    let diff = create_temp_diff(&format!(
        "--- {source_label}\n+++ {source_label}\n@@ -1 +1 @@\n-old\n+new\n"
    ));

    let value = response(&run_from_diff("edit", &diff, None));

    assert_eq!(value["file"], source_label.as_ref());
    assert_eq!(value["summary"]["unique"], 1);
    assert_source_unchanged(&source, original);
}

#[test]
fn apply_patch_wrapper_infers_the_source_path() {
    let original = "old\n";
    let source = create_temp_source(original);
    let source_label = source.to_string_lossy();
    let diff = create_temp_diff(&format!(
        "apply_patch failed:\n*** Begin Patch\n*** Update File: {source_label}\n@@\n-old\n+new\n*** End Patch\n"
    ));

    let value = response(&run_from_diff("patch", &diff, None));

    assert_eq!(value["file"], source_label.as_ref());
    assert_eq!(value["summary"]["unique"], 1);
    assert_source_unchanged(&source, original);
}

#[test]
fn repeated_old_block_preserves_every_candidate_in_source_order() {
    let original = "old\nmiddle\nold\n";
    let source = create_temp_source(original);
    let diff = create_temp_diff("@@\n-old\n+new\n");

    let value = response(&run_from_diff("edit", &diff, Some(&source)));
    let candidates = value["changes"][0]["candidates"]
        .as_array()
        .expect("candidates should be an array");

    assert_eq!(value["changes"][0]["status"], "ambiguous");
    assert_eq!(value["summary"]["ambiguous"], 1);
    assert_eq!(candidates.len(), 2);
    assert_eq!(serialized_anchor_line(&candidates[0], "anchor"), 1);
    assert_eq!(serialized_anchor_line(&candidates[1], "anchor"), 3);
    assert_source_unchanged(&source, original);
}

#[test]
fn overlapping_old_blocks_are_all_reported() {
    let original = "same\nsame\nsame\n";
    let source = create_temp_source(original);
    let diff = create_temp_diff("@@\n-same\n-same\n+replacement\n");

    let value = response(&run_from_diff("patch", &diff, Some(&source)));
    let candidates = value["changes"][0]["candidates"]
        .as_array()
        .expect("candidates should be an array");

    assert_eq!(value["changes"][0]["status"], "ambiguous");
    assert_eq!(candidates.len(), 2);
    assert_eq!(serialized_anchor_line(&candidates[0], "anchor"), 1);
    assert_eq!(serialized_anchor_line(&candidates[0], "end_anchor"), 2);
    assert_eq!(serialized_anchor_line(&candidates[1], "anchor"), 2);
    assert_eq!(serialized_anchor_line(&candidates[1], "end_anchor"), 3);
}

#[test]
fn absent_old_block_is_a_successful_missing_preview() {
    let original = "current\n";
    let source = create_temp_source(original);
    let diff = create_temp_diff("@@\n-stale\n+new\n");

    let value = response(&run_from_diff("patch", &diff, Some(&source)));

    assert_eq!(value["changes"][0]["status"], "missing");
    assert_eq!(value["changes"][0]["candidates"], serde_json::json!([]));
    assert_eq!(value["summary"]["missing"], 1);
    assert_source_unchanged(&source, original);
}

#[test]
fn deletion_emits_an_empty_replacement() {
    let original = "before\ndrop me\nafter\n";
    let source = create_temp_source(original);
    let diff = create_temp_diff("@@\n before\n-drop me\n after\n");

    let value = response(&run_from_diff("edit", &diff, Some(&source)));

    assert_eq!(
        value["changes"][0]["candidates"][0]["op"],
        serde_json::json!({"type": "replace_lines", "new_text": ""})
    );
    assert_eq!(value["changes"][0]["status"], "unique");
}

#[test]
fn contextual_insertion_emits_a_reusable_insert_after_target() {
    let original = "before\nafter\n";
    let source = create_temp_source(original);
    let diff = create_temp_diff("@@\n before\n+inserted\n after\n");

    let value = response(&run_from_diff("patch", &diff, Some(&source)));

    assert_eq!(
        value["changes"][0]["candidates"][0]["op"],
        serde_json::json!({"type": "insert_after", "text": "inserted"})
    );
    assert_eq!(value["changes"][0]["status"], "unique");
    assert_eq!(
        value["changes"][0]["candidates"][0]["target"]["anchor"],
        format!("1:{}", common::compute_line_hash("before"))
    );
    assert_source_unchanged(&source, original);
}

#[test]
fn insertion_at_file_start_uses_a_file_start_target() {
    let original = "first\n";
    let source = create_temp_source(original);
    let diff = create_temp_diff("@@\n+inserted\n first\n");

    let value = response(&run_from_diff("edit", &diff, Some(&source)));

    assert_eq!(
        value["changes"][0]["candidates"][0]["op"],
        serde_json::json!({"type": "insert", "new_text": "inserted"})
    );
    assert_eq!(
        value["changes"][0]["candidates"][0]["target"]["type"],
        "file_start"
    );
}

#[test]
fn unicode_and_crlf_are_matched_by_logical_line_content() {
    let original = "앞\r\n오래된 값\r\n뒤\r\n";
    let source = create_temp_source(original);
    let diff = create_temp_diff("@@\r\n 앞\r\n-오래된 값\r\n+새 값\r\n 뒤\r\n");

    let value = response(&run_from_diff("patch", &diff, Some(&source)));

    assert_eq!(value["changes"][0]["status"], "unique");
    assert_eq!(
        value["changes"][0]["candidates"][0]["target"]["anchor"],
        format!("2:{}", common::compute_line_hash("오래된 값"))
    );
    assert_source_unchanged(&source, original);
}

#[test]
fn no_final_newline_markers_do_not_hide_a_valid_replacement() {
    let original = "old";
    let source = create_temp_source(original);
    let diff = create_temp_diff(
        "@@ -1 +1 @@\n-old\n\\ No newline at end of file\n+new\n\\ No newline at end of file\n",
    );

    let value = response(&run_from_diff("edit", &diff, Some(&source)));

    assert_eq!(value["changes"][0]["status"], "unique");
    assert_eq!(
        value["changes"][0]["candidates"][0]["op"]["new_text"],
        "new"
    );
    assert_source_unchanged(&source, original);
}

#[test]
fn malformed_and_multi_file_diffs_are_rejected() {
    let source = create_temp_source("old\n");
    let malformed = create_temp_diff("@@\nold\n");
    let malformed_error = error(&run_from_diff("edit", &malformed, Some(&source)));
    assert_eq!(malformed_error["error"]["type"], "invalid_request");

    let second = create_temp_source("other\n");
    let second_label = second.to_string_lossy();
    let source_label = source.to_string_lossy();
    let multi_file = create_temp_diff(&format!(
        "--- {source_label}\n+++ {source_label}\n@@\n-old\n+new\n--- {second_label}\n+++ {second_label}\n@@\n-other\n+changed\n"
    ));
    let multi_file_error = error(&run_from_diff("patch", &multi_file, None));
    assert_eq!(multi_file_error["error"]["type"], "invalid_request");
    assert!(
        multi_file_error["error"]["message"]
            .as_str()
            .expect("message should be a string")
            .contains("one file")
    );
}

#[test]
fn create_delete_and_conflicting_paths_are_rejected() {
    let source = create_temp_source("old\n");
    let source_label = source.to_string_lossy();
    let create = create_temp_diff(&format!(
        "--- /dev/null\n+++ {source_label}\n@@ -0,0 +1 @@\n+new\n"
    ));
    assert_eq!(
        error(&run_from_diff("edit", &create, None))["error"]["type"],
        "invalid_request"
    );

    let other = create_temp_source("old\n");
    let other_label = other.to_string_lossy();
    let conflict = create_temp_diff(&format!(
        "--- {other_label}\n+++ {other_label}\n@@\n-old\n+new\n"
    ));
    let conflict_error = error(&run_from_diff("patch", &conflict, Some(&source)));
    assert_eq!(conflict_error["error"]["type"], "invalid_request");
    assert!(
        conflict_error["error"]["message"]
            .as_str()
            .expect("message should be a string")
            .contains("conflicts")
    );
}

#[test]
fn from_diff_rejects_ordinary_edit_and_patch_flags_consistently() {
    let source = create_temp_source("old\n");
    let diff = create_temp_diff("@@\n-old\n+new\n");
    let source_arg = source.to_string_lossy();
    let diff_arg = diff.to_string_lossy();
    let edit = common::run_identedit(&[
        "edit",
        "--from-diff",
        diff_arg.as_ref(),
        "--at",
        "file-end",
        "--insert",
        "text",
        source_arg.as_ref(),
    ]);
    let patch = common::run_identedit(&[
        "patch",
        "--from-diff",
        diff_arg.as_ref(),
        "--at",
        "file-end",
        "--insert",
        "text",
        source_arg.as_ref(),
    ]);
    let edit_error = error(&edit);
    let patch_error = error(&patch);

    assert_eq!(edit_error, patch_error);
    assert!(
        edit_error["error"]["message"]
            .as_str()
            .expect("message should be a string")
            .contains("cannot be combined")
    );
}

#[test]
fn from_diff_rejects_patch_execution_options() {
    let source = create_temp_source("old\n");
    let diff = create_temp_diff("@@\n-old\n+new\n");
    let source_arg = source.to_string_lossy();
    let diff_arg = diff.to_string_lossy();

    for option in ["--dry-run", "--diff", "--auto-repair", "--verbose"] {
        let output = common::run_identedit(&[
            "patch",
            "--from-diff",
            diff_arg.as_ref(),
            option,
            source_arg.as_ref(),
        ]);
        assert_eq!(
            error(&output)["error"]["type"],
            "invalid_request",
            "option should be rejected: {option}"
        );
    }
}

#[test]
fn unique_candidate_target_and_op_feed_directly_into_patch_json() {
    let source = create_temp_source("before\nold\nafter\n");
    let diff = create_temp_diff("@@\n before\n-old\n+new\n after\n");
    let handoff = response(&run_from_diff("edit", &diff, Some(&source)));
    let applied = apply_handoff_candidate(&handoff, 0, 0);

    assert!(
        applied.status.success(),
        "candidate apply failed: {}",
        String::from_utf8_lossy(&applied.stdout)
    );
    assert_source_unchanged(&source, "before\nnew\nafter\n");
}

#[test]
fn failed_diff_candidate_preserves_unrelated_mixed_line_endings() {
    let source = create_temp_source("before\r\nold\nafter\r");
    let diff = create_temp_diff("@@\n-old\n+new\n");
    let handoff = response(&run_from_diff("edit", &diff, Some(&source)));
    let applied = apply_handoff_candidate(&handoff, 0, 0);

    assert!(
        applied.status.success(),
        "candidate apply failed: {}",
        String::from_utf8_lossy(&applied.stdout)
    );
    assert_source_unchanged(&source, "before\r\nnew\nafter\r");
}

#[test]
fn line_insertion_candidate_feeds_directly_into_patch_json() {
    let source = create_temp_source("before\nafter\n");
    let diff = create_temp_diff("@@\n before\n+inserted\n after\n");
    let handoff = response(&run_from_diff("patch", &diff, Some(&source)));

    let applied = apply_handoff_candidate(&handoff, 0, 0);

    assert!(
        applied.status.success(),
        "candidate apply failed: {}",
        String::from_utf8_lossy(&applied.stdout)
    );
    assert_source_unchanged(&source, "before\ninserted\nafter\n");
}

#[test]
fn empty_file_insertion_candidate_feeds_directly_into_patch_json() {
    let source = create_temp_source("");
    let diff = create_temp_diff("@@ -0,0 +1,2 @@\n+first\n+second\n");
    let handoff = response(&run_from_diff("edit", &diff, Some(&source)));

    let applied = apply_handoff_candidate(&handoff, 0, 0);

    assert!(
        applied.status.success(),
        "candidate apply failed: {}",
        String::from_utf8_lossy(&applied.stdout)
    );
    assert_source_unchanged(&source, "first\nsecond");
}
