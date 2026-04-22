use super::*;

#[test]
fn patch_file_start_dry_run_does_not_modify_file() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let before = fs::read_to_string(&file_path).expect("file should be readable");

    let output = run_identedit(&[
        "patch",
        "--at",
        "file-start",
        "--insert",
        "# preamble\n",
        "--dry-run",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "file-start dry-run should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["transaction"]["status"], "dry_run");
    assert_eq!(
        fs::read_to_string(&file_path).expect("file should be readable"),
        before,
        "file-start dry-run must not modify the file"
    );
}

#[test]
fn patch_file_end_dry_run_does_not_modify_file() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let before = fs::read_to_string(&file_path).expect("file should be readable");

    let output = run_identedit(&[
        "patch",
        "--at",
        "file-end",
        "--insert",
        "\n# epilogue\n",
        "--dry-run",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "file-end dry-run should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["transaction"]["status"], "dry_run");
    assert_eq!(
        fs::read_to_string(&file_path).expect("file should be readable"),
        before,
        "file-end dry-run must not modify the file"
    );
}

#[test]
fn patch_file_end_dry_run_diff_outputs_insert_preview_without_writing() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let before = fs::read_to_string(&file_path).expect("file should be readable");

    let output = run_identedit(&[
        "patch",
        "--at",
        "file-end",
        "--insert",
        "\n# epilogue\n",
        "--dry-run",
        "--diff",
        "--color",
        "never",
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "file-end dry-run diff should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let diff = String::from_utf8(output.stdout).expect("stdout should be utf-8");
    assert!(diff.contains("@@ -8,0 +8,2 @@"));
    assert!(diff.contains("+# epilogue"));
    assert_eq!(
        fs::read_to_string(&file_path).expect("file should be readable"),
        before,
        "file-end dry-run diff must not modify the file"
    );
}
