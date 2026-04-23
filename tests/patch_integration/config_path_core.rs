use super::*;

#[test]
fn patch_flag_config_path_set_value_accepts_stdin_text_payload() {
    let file_path = copy_fixture_to_temp_json("example.json");

    let output = run_identedit_with_stdin(
        &[
            "patch",
            "--config-path",
            "config.retries",
            "--set-value",
            "--stdin-text",
            file_path.to_str().expect("path should be utf-8"),
        ],
        "7",
    );

    assert!(
        output.status.success(),
        "config path set-value with stdin text failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let modified: Value =
        serde_json::from_str(&fs::read_to_string(&file_path).expect("modified file should read"))
            .expect("modified JSON should parse");
    assert_eq!(modified["config"]["retries"], 7);
}

#[test]
fn patch_config_append_accepts_stdin_text_payload() {
    let file_path = copy_fixture_to_temp_json("example.json");

    let output = run_identedit_with_stdin(
        &[
            "patch",
            "--config-path",
            "items",
            "--append-value",
            "--stdin-text",
            file_path.to_str().expect("path should be utf-8"),
        ],
        "4",
    );

    assert!(
        output.status.success(),
        "config append with stdin text failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let modified: Value =
        serde_json::from_str(&fs::read_to_string(&file_path).expect("modified file should read"))
            .expect("modified JSON should parse");
    assert_eq!(modified["items"], json!([1, 2, 3, 4]));
}

#[test]
fn patch_config_append_stdin_dry_run_does_not_modify_file() {
    let file_path = copy_fixture_to_temp_json("example.json");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let output = run_identedit_with_stdin(
        &[
            "patch",
            "--config-path",
            "items",
            "--append-value",
            "--stdin-text",
            "--dry-run",
            file_path.to_str().expect("path should be utf-8"),
        ],
        "4",
    );

    assert!(
        output.status.success(),
        "append with stdin dry-run failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let after = fs::read_to_string(&file_path).expect("file should remain readable");
    assert_eq!(after, before);
}

#[test]
fn patch_config_append_text_file_directory_returns_io_error_without_mutation() {
    let file_path = copy_fixture_to_temp_json("example.json");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let payload_dir = Builder::new()
        .prefix("identedit-config-payload-dir")
        .tempdir()
        .expect("temp dir should be created");

    let output = run_identedit(&[
        "patch",
        "--config-path",
        "items",
        "--append-value",
        "--text-file",
        payload_dir
            .path()
            .to_str()
            .expect("payload path should be utf-8"),
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        !output.status.success(),
        "directory payload should fail for config append"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "io_error");
    let after = fs::read_to_string(&file_path).expect("file should remain readable");
    assert_eq!(after, before);
}

#[test]
fn patch_config_set_value_text_file_path_with_spaces_applies() {
    let file_path = copy_fixture_to_temp_json("example.json");
    let payload_dir = Builder::new()
        .prefix("identedit config payload ")
        .tempdir()
        .expect("temp dir should be created");
    let payload_path = payload_dir.path().join("value payload.txt");
    fs::write(&payload_path, "11").expect("payload file should be written");

    let output = run_identedit(&[
        "patch",
        "--config-path",
        "config.retries",
        "--set-value",
        "--text-file",
        payload_path.to_str().expect("payload path should be utf-8"),
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "config text-file path with spaces should work: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let modified: Value =
        serde_json::from_str(&fs::read_to_string(&file_path).expect("modified file should read"))
            .expect("modified JSON should parse");
    assert_eq!(modified["config"]["retries"], 11);
}

#[test]
fn patch_config_delete_stdin_text_rejects_unused_source() {
    let file_path = copy_fixture_to_temp_json("example.json");

    let output = run_identedit_with_stdin(
        &[
            "patch",
            "--config-path",
            "config.retries",
            "--delete",
            "--stdin-text",
            file_path.to_str().expect("path should be utf-8"),
        ],
        "unused",
    );

    assert!(
        !output.status.success(),
        "config delete should reject unused stdin source"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let message = response["error"]["message"]
        .as_str()
        .expect("error message should be present");
    assert!(
        message.contains("--text-file") && message.contains("--stdin-text"),
        "error should explain unused external source, got: {message}"
    );
}

#[test]
fn patch_flag_config_path_create_missing_rejects_unrelated_line_flags() {
    let file_path = copy_fixture_to_temp_with_suffix("example.json", ".json");
    let anchor = line_ref("a\nb\n", 1);

    let output = run_identedit(&[
        "patch",
        "--config-path",
        "config.sidecar.port",
        "--set-value",
        "9000",
        "--create-missing",
        "--anchor",
        &anchor,
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "config-path create-missing should reject unrelated line-target flags"
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let message = response["error"]["message"]
        .as_str()
        .expect("error message should be present");
    assert!(
        message.contains("--set-value")
            && message.contains("--append-value")
            && message.contains("--delete"),
        "config-mode error should list valid config path operations"
    );
}

#[test]
fn patch_flag_config_path_create_missing_array_oob_does_not_mutate_file() {
    let file_path = copy_fixture_to_temp_with_suffix("example.json", ".json");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let output = run_identedit(&[
        "patch",
        "--config-path",
        "items[99]",
        "--set-value",
        "10",
        "--create-missing",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "array OOB should fail in flag mode"
    );

    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "flag-mode failure must not mutate file");
}

#[test]
fn patch_flag_config_path_create_missing_array_oob_error_mentions_append() {
    let file_path = copy_fixture_to_temp_with_suffix("example.json", ".json");

    let output = run_identedit(&[
        "patch",
        "--config-path",
        "items[99]",
        "--set-value",
        "10",
        "--create-missing",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(!output.status.success(), "operation should fail");

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("append operation")),
        "array OOB error should include append-operation hint"
    );
}
