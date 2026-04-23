use super::*;

#[test]
fn patch_diff_without_dry_run_reports_invalid_request() {
    let file_path = copy_fixture_to_temp_python("example.py");

    let output = run_identedit(&[
        "patch",
        "--at",
        "file-end",
        "--insert",
        "\n# epilogue\n",
        "--diff",
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        !output.status.success(),
        "--diff without --dry-run should fail"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let message = response["error"]["message"]
        .as_str()
        .expect("error message should be present");
    assert!(
        message.contains("--diff") && message.contains("--dry-run"),
        "error should explain that diff output is dry-run only"
    );
}

#[test]
fn patch_color_without_diff_reports_invalid_request() {
    let file_path = copy_fixture_to_temp_python("example.py");

    let output = run_identedit(&[
        "patch",
        "--at",
        "file-end",
        "--insert",
        "\n# epilogue\n",
        "--dry-run",
        "--color",
        "never",
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        !output.status.success(),
        "--color without --diff should fail"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let message = response["error"]["message"]
        .as_str()
        .expect("error message should be present");
    assert!(
        message.contains("--color") && message.contains("--diff"),
        "error should explain that color only affects diff output"
    );
}

#[test]
fn patch_without_operation_flag_returns_invalid_request() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_function_handle(&file_path, "process_*");
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");

    let output = run_identedit(&[
        "patch",
        "--identity",
        identity,
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        !output.status.success(),
        "patch should reject requests with no operation selected"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}
