use std::fs;

use serde_json::{Value, json};
use tempfile::tempdir;

mod common;

const POSITIVE_EXPONENT_NUMBERS: [&str; 6] =
    ["1e+09", "1E+09", "-1.5e+16", "1e+21", "0E+00", "9e+9999"];

#[test]
fn reads_positive_exponents_without_changing_spans_text_or_guards() {
    let root = tempdir().unwrap();
    let file = root.path().join("numbers.json");
    let prefix = "{\"label\":\"λ\",\"value\":";
    let suffix = ",\"escaped\":\"\\u0041\",\"lookalike\":\"1e+09\"}\r\n";

    for number in POSITIVE_EXPONENT_NUMBERS {
        let before = format!("{prefix}{number}{suffix}");
        fs::write(&file, &before).unwrap();

        let handle = common::select_first_handle(&file, "number", Some("value"));
        assert_eq!(handle["text"], number);
        assert_eq!(handle["span"]["start"], prefix.len() as u64);
        assert_eq!(handle["span"]["end"], (prefix.len() + number.len()) as u64);
        assert_eq!(handle["expected_old_hash"], common::hash_text(number));
        assert_eq!(fs::read(&file).unwrap(), before.as_bytes());
    }
}

#[test]
fn node_edits_accept_positive_exponents_in_previews_and_results() {
    let root = tempdir().unwrap();
    let file = root.path().join("numbers.json");
    let before = "{\"value\":0,\"escaped\":\"\\u0041\"}\r\n";

    for number in POSITIVE_EXPONENT_NUMBERS {
        fs::write(&file, before).unwrap();
        let handle = common::select_first_handle(&file, "number", Some("value"));
        let expected = before.replacen("\"value\":0", &format!("\"value\":{number}"), 1);

        for dry_run in [true, false] {
            let request = json!({
                "command": "patch",
                "file": file,
                "target": {
                    "type": "node",
                    "identity": handle["identity"],
                    "kind": handle["kind"],
                    "expected_old_hash": handle["expected_old_hash"],
                    "span_hint": handle["span"]
                },
                "op": {"type": "replace", "new_text": number},
                "options": {"dry_run": dry_run}
            });
            let output =
                common::run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stdout)
            );
            let response: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(
                response["transaction"]["status"],
                if dry_run { "dry_run" } else { "committed" }
            );
            assert_eq!(
                fs::read_to_string(&file).unwrap(),
                if dry_run { before } else { &expected }
            );
        }
    }
}

#[test]
fn config_set_preserves_positive_exponents_in_source_and_payload() {
    let root = tempdir().unwrap();
    let file = root.path().join("numbers.json");

    for (keep, payload) in [
        ("1e+09", "7"),
        ("1", "1e+09"),
        ("1E+21", " \t-1.5e+16 \r\n "),
        ("1", "9E+9999"),
    ] {
        let before = format!(
            "{{\r\n  \"label\":\"λ\",\r\n  \"keep\":{keep},\r\n  \"value\":0,\r\n  \"escaped\":\"\\u0041\"\r\n}}\r\n"
        );
        fs::write(&file, &before).unwrap();
        let expected = before.replacen("\"value\":0", &format!("\"value\":{payload}"), 1);

        for dry_run in [true, false] {
            let request = json!({
                "command": "patch",
                "file": file,
                "target": {
                    "type": "config_path",
                    "path": "value",
                    "expected_file_hash": common::hash_text(&before)
                },
                "op": {"type": "set", "new_text": payload},
                "options": {"dry_run": dry_run}
            });
            let output =
                common::run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stdout)
            );
            assert_eq!(
                fs::read_to_string(&file).unwrap(),
                if dry_run {
                    before.as_str()
                } else {
                    expected.as_str()
                }
            );
        }
    }
}

#[test]
fn config_append_and_create_preserve_exponent_text() {
    let root = tempdir().unwrap();
    let file = root.path().join("numbers.json");
    let before = r#"{"keep":1e+09,"items":[2E+03]}"#;

    for create in [false, true] {
        fs::write(&file, before).unwrap();
        let expected = if create {
            r#"{"keep":1e+09,"items":[2E+03],"created":3e+08}"#
        } else {
            r#"{"keep":1e+09,"items":[2E+03, 3e+08]}"#
        };

        for dry_run in [true, false] {
            let mut arguments = vec![
                "patch",
                file.to_str().unwrap(),
                "--config-path",
                if create { "created" } else { "items" },
            ];
            if create {
                arguments.extend(["--set-value", "3e+08", "--create-missing"]);
            } else {
                arguments.extend(["--append-value", "3e+08"]);
            }
            if dry_run {
                arguments.push("--dry-run");
            }

            let output = common::run_identedit(&arguments);
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stdout)
            );
            assert_eq!(
                fs::read_to_string(&file).unwrap(),
                if dry_run { before } else { expected }
            );
        }
    }
}

#[test]
fn malformed_exponents_remain_source_errors_without_writes() {
    let root = tempdir().unwrap();
    let file = root.path().join("numbers.json");

    for number in ["1e+", "1E+", "1e++2", "1e+-2", "+1"] {
        let before = format!("{{\"value\":{number}}}");
        fs::write(&file, &before).unwrap();
        let output = common::run_identedit(&["read", file.to_str().unwrap(), "--json"]);
        assert!(!output.status.success());
        let response: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(response["error"]["type"], "parse_failure");
        assert_eq!(response["error"]["stage"], "source");
        assert_eq!(fs::read_to_string(&file).unwrap(), before);
    }
}

#[test]
fn malformed_config_exponents_are_rejected_without_writes() {
    let root = tempdir().unwrap();
    let file = root.path().join("numbers.json");
    let before = r#"{"keep":1e+09,"value":0}"#;
    fs::write(&file, before).unwrap();

    for number in ["1e+", "1E+", "1e++2", "1e+-2", "+1"] {
        let output = common::run_identedit(&[
            "patch",
            file.to_str().unwrap(),
            "--config-path",
            "value",
            "--set-value",
            number,
        ]);
        assert!(!output.status.success());
        let response: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(response["error"]["type"], "invalid_request");
        assert_eq!(fs::read_to_string(&file).unwrap(), before);
    }
}

#[test]
fn config_guards_stay_stale_after_only_exponent_spelling_changes() {
    let root = tempdir().unwrap();
    let file = root.path().join("numbers.json");
    let before = r#"{"keep":1e+09,"value":0}"#;
    let current = before.replace("1e+09", "1E+09");
    fs::write(&file, &current).unwrap();
    let request = json!({
        "command": "patch",
        "file": file,
        "target": {
            "type": "config_path",
            "path": "value",
            "expected_file_hash": common::hash_text(before)
        },
        "op": {"type": "set", "new_text": "2e+08"}
    });

    let output = common::run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(!output.status.success());
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["error"]["type"], "precondition_failed");
    assert_eq!(fs::read_to_string(&file).unwrap(), current);
}
