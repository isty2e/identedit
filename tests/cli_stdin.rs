mod common;

use serde_json::{Value, json};

#[test]
fn early_json_flag_rejection_preserves_the_child_error_response() {
    let input = " ".repeat(2 * 1024 * 1024);
    let output = common::run_identedit_with_stdin(
        &["read", "--mode", "ast", "--json", "--name", "target"],
        &input,
    );
    assert!(!output.status.success());
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .unwrap()
            .contains("--name")
    );
}

#[test]
fn early_help_exit_preserves_success_without_consuming_stdin() {
    let input = " ".repeat(2 * 1024 * 1024);
    let output = common::run_identedit_with_stdin(&["--help"], &input);
    assert!(output.status.success());
    assert!(!output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn normal_json_request_still_consumes_large_input() {
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("sample.py");
    std::fs::write(&file, "def target():\n    return 1\n").unwrap();
    let mut input =
        json!({"command":"read", "file":file, "selector":{"kind":"function_definition"}})
            .to_string();
    input.push_str(&" ".repeat(2 * 1024 * 1024));
    let output = common::run_identedit_with_stdin(&["read", "--mode", "ast", "--json"], &input);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["handles"].as_array().unwrap().len(), 1);
    assert_eq!(response["handles"][0]["name"], "target");
}
