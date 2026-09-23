use std::fs;

use serde_json::{Value, json};
use tempfile::tempdir;

mod common;

#[test]
fn apply_wrong_stage_guidance_covers_all_input_routes_without_writes() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("source.py");
    fs::write(&file, "x = 1\n").unwrap();
    for request in [
        json!({"command":"edit", "file":file, "operations":[]}),
        json!({"command":"edit", "files":[{"file":file,"operations":[]}]}),
    ] {
        let body = request.to_string();
        let path = dir.path().join("request.json");
        fs::write(&path, &body).unwrap();
        for output in [
            common::run_identedit_with_stdin(&["apply", "--json"], &body),
            common::run_identedit_with_stdin(&["apply"], &body),
            common::run_identedit(&["apply", path.to_str().unwrap()]),
        ] {
            assert!(!output.status.success());
            let response: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(response["error"]["type"], "invalid_request");
            assert!(
                response["error"]["suggestion"]
                    .as_str()
                    .unwrap()
                    .contains("identedit edit --json")
            );
            assert_eq!(fs::read_to_string(&file).unwrap(), "x = 1\n");
        }
    }
}

#[test]
fn malformed_apply_inputs_are_not_accepted_or_misclassified() {
    for body in [
        "{",
        r#"{"command":"apply","changeset":null}"#,
        r#"{"command":"other"}"#,
    ] {
        let output = common::run_identedit_with_stdin(&["apply", "--json"], body);
        assert!(!output.status.success());
        let response: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(response["error"]["type"], "invalid_request");
        assert!(response["error"].get("line_check").is_none());
        assert!(response["error"].get("suggestion").is_none());
    }
}

#[test]
fn stale_patch_exposes_structured_line_check_without_writing() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("source.py");
    fs::write(&file, "x = 1\n").unwrap();
    let read = common::run_identedit(&["read", "--mode", "line", file.to_str().unwrap()]);
    let text = String::from_utf8(read.stdout).unwrap();
    let anchor = text.split('|').next().unwrap();
    fs::write(&file, "x = 2\n").unwrap();
    for repair in [false, true] {
        let mut args = vec![
            "patch",
            file.to_str().unwrap(),
            "--at",
            anchor,
            "--replace",
            "x = 3",
        ];
        if repair {
            args.push("--auto-repair");
        }
        let output = common::run_identedit(&args);
        assert!(!output.status.success());
        let response: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(response["error"]["type"], "invalid_request");
        assert_eq!(response["error"]["line_check"]["ok"], false);
        assert_eq!(response["error"]["line_check"]["summary"]["mismatched"], 1);
        assert!(!response["error"]["message"].as_str().unwrap().contains('{'));
        assert!(
            response["error"]["suggestion"]
                .as_str()
                .unwrap()
                .contains("read --mode line")
        );
        assert_eq!(fs::read_to_string(&file).unwrap(), "x = 2\n");
    }
}

#[test]
fn repair_apply_and_json_patch_report_ambiguous_anchors_without_writes() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("source.py");
    fs::write(&file, "x = 1\n").unwrap();
    let anchor = format!("1:{}", common::compute_line_hash("x = 1"));
    let plan = common::run_identedit(&[
        "edit",
        file.to_str().unwrap(),
        "--at",
        &anchor,
        "--replace",
        "x = 3",
    ]);
    assert!(plan.status.success());
    let changed = "y = 0\nx = 1\nx = 1\n";
    fs::write(&file, changed).unwrap();
    let request = json!({"command":"patch", "file":file, "target":{"type":"line", "anchor":anchor}, "op":{"type":"replace", "new_text":"x = 3"}});
    for output in [
        common::run_identedit_with_stdin(
            &["apply", "--repair"],
            std::str::from_utf8(&plan.stdout).unwrap(),
        ),
        common::run_identedit_with_stdin(&["patch", "--json"], &request.to_string()),
    ] {
        assert!(!output.status.success());
        let response: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            response["error"]["line_check"]["summary"]["ambiguous"], 1,
            "{response}"
        );
        assert_eq!(fs::read_to_string(&file).unwrap(), changed);
    }
}

#[test]
fn wrong_stage_hint_does_not_normalize_duplicate_keys_into_valid_input() {
    for body in [
        r#"{"command":"apply","command":"edit","changeset":{"files":[]}}"#,
        r#"{"command":"edit","command":"apply","changeset":{"files":[]}}"#,
    ] {
        let output = common::run_identedit_with_stdin(&["apply", "--json"], body);
        assert!(!output.status.success());
        let response: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(response["error"]["type"], "invalid_request");
        assert!(
            response["error"]["message"]
                .as_str()
                .unwrap()
                .contains("duplicate field")
        );
    }
}
