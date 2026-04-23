use super::*;

#[test]
fn patch_json_config_path_delete_removes_json_key_and_keeps_valid_document() {
    let file_path = copy_fixture_to_temp_with_suffix("example.json", ".json");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "config.enabled"
        },
        "op": {
            "type": "delete"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "config path delete should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let updated = fs::read_to_string(&file_path).expect("updated file should be readable");
    let parsed: Value = serde_json::from_str(&updated).expect("updated JSON should stay valid");
    assert!(
        parsed["config"].get("enabled").is_none(),
        "deleted key should not exist in config object"
    );
}

#[test]
fn patch_flag_config_path_delete_rejects_create_missing() {
    let file_path = copy_fixture_to_temp_with_suffix("example.json", ".json");

    let output = run_identedit(&[
        "patch",
        "--config-path",
        "config.enabled",
        "--delete",
        "--create-missing",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "config path delete should reject --create-missing"
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("--create-missing")),
        "error should mention create-missing restriction"
    );
}

#[test]
fn patch_json_config_path_delete_rejects_create_missing_payload_field() {
    let file_path = copy_fixture_to_temp_with_suffix("example.json", ".json");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "config.enabled"
        },
        "op": {
            "type": "delete",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "delete payload should reject create_missing field"
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}

#[test]
fn patch_json_config_path_append_appends_json_array() {
    let file_path = copy_fixture_to_temp_with_suffix("example.json", ".json");
    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "items"
        },
        "op": {
            "type": "append",
            "new_text": "4"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "json config append should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let updated = fs::read_to_string(&file_path).expect("updated JSON should be readable");
    let parsed: Value = serde_json::from_str(&updated).expect("updated JSON should stay valid");
    assert_eq!(parsed["items"], json!([1, 2, 3, 4]));
}

#[test]
fn patch_flag_config_path_append_value_appends_json_array() {
    let file_path = copy_fixture_to_temp_with_suffix("example.json", ".json");
    let output = run_identedit(&[
        "patch",
        "--config-path",
        "items",
        "--append-value",
        "4",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "flag config append should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let updated = fs::read_to_string(&file_path).expect("updated JSON should be readable");
    let parsed: Value = serde_json::from_str(&updated).expect("updated JSON should stay valid");
    assert_eq!(parsed["items"], json!([1, 2, 3, 4]));
}

#[test]
fn patch_json_config_path_append_rejects_non_array_target() {
    let file_path = copy_fixture_to_temp_with_suffix("example.json", ".json");
    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "config.retries"
        },
        "op": {
            "type": "append",
            "new_text": "4"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "append on non-array path must fail"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("append") && message.contains("array")),
        "non-array append should return a clear array-target diagnostic"
    );
}

#[test]
fn patch_json_config_path_append_rejects_create_missing_payload_field() {
    let file_path = copy_fixture_to_temp_with_suffix("example.json", ".json");
    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "items"
        },
        "op": {
            "type": "append",
            "new_text": "4",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "append payload should reject create_missing field"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}

#[test]
fn patch_flag_config_path_append_rejects_create_missing() {
    let file_path = copy_fixture_to_temp_with_suffix("example.json", ".json");
    let output = run_identedit(&[
        "patch",
        "--config-path",
        "items",
        "--append-value",
        "4",
        "--create-missing",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "append should reject create-missing"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("--create-missing")),
        "error should mention create-missing restriction"
    );
}

#[test]
fn patch_json_config_path_append_with_stale_hash_fails_without_mutation() {
    let file_path = copy_fixture_to_temp_with_suffix("example.json", ".json");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "items",
            "expected_file_hash": "deadbeefdeadbeef"
        },
        "op": {
            "type": "append",
            "new_text": "4"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "stale hash must fail for append operation"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "precondition_failed");
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(
        before, after,
        "stale-hash append failure must not mutate file"
    );
}

#[test]
fn patch_json_config_path_append_rejects_missing_path() {
    let file_path = copy_fixture_to_temp_with_suffix("example.json", ".json");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "config.not_found"
        },
        "op": {
            "type": "append",
            "new_text": "4"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "append should fail for missing path"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("was not found")),
        "missing path should report deterministic diagnostic"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(
        before, after,
        "missing-path append failure must not mutate file"
    );
}

#[test]
fn patch_json_config_path_append_supports_index_targeting_nested_array() {
    let mut temp_file = Builder::new()
        .suffix(".json")
        .tempfile()
        .expect("temp json file should be created");
    temp_file
        .write_all(br#"{"matrix":[[1,2],[3]]}"#)
        .expect("json fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "matrix[1]"
        },
        "op": {
            "type": "append",
            "new_text": "4"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "append should allow index path that resolves to nested array: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let updated = fs::read_to_string(&file_path).expect("updated JSON should be readable");
    let parsed: Value = serde_json::from_str(&updated).expect("updated JSON should stay valid");
    assert_eq!(parsed["matrix"], json!([[1, 2], [3, 4]]));
}

#[test]
fn patch_json_config_path_append_rejects_index_targeting_scalar() {
    let file_path = copy_fixture_to_temp_with_suffix("example.json", ".json");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "items[0]"
        },
        "op": {
            "type": "append",
            "new_text": "4"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "append should fail when index resolves to scalar value"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("array/sequence")),
        "scalar append should report array-target diagnostic"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "failed scalar append must not mutate file");
}
