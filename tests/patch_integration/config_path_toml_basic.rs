use super::*;

#[test]
fn patch_config_set_value_text_file_invalid_toml_does_not_mutate_file() {
    let file_path = copy_fixture_to_temp_with_suffix("example.toml", ".toml");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let payload_path = create_temp_text_file("{ invalid = }");

    let output = run_identedit(&[
        "patch",
        "--config-path",
        "server",
        "--set-value",
        "--text-file",
        payload_path.to_str().expect("payload path should be utf-8"),
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(!output.status.success(), "invalid TOML payload should fail");
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let after = fs::read_to_string(&file_path).expect("file should remain readable");
    assert_eq!(after, before);
}

#[test]
fn patch_config_set_value_text_file_valid_toml_with_trailing_newline_applies() {
    let file_path = copy_fixture_to_temp_with_suffix("example.toml", ".toml");
    let payload_path = create_temp_text_file("9090\n");

    let output = run_identedit(&[
        "patch",
        "--config-path",
        "server.port",
        "--set-value",
        "--text-file",
        payload_path.to_str().expect("payload path should be utf-8"),
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "valid TOML payload with trailing newline should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    assert!(modified.contains("port = 9090"));
}

#[test]
fn patch_json_config_path_delete_removes_toml_key() {
    let file_path = copy_fixture_to_temp_with_suffix("example.toml", ".toml");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "database.settings.enabled"
        },
        "op": {
            "type": "delete"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "toml config path delete should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(!updated.contains("enabled = true"));
    assert!(updated.contains("max_connections = 32"));
}

#[test]
fn patch_json_config_path_set_create_missing_updates_toml_value() {
    let file_path = copy_fixture_to_temp_with_suffix("example.toml", ".toml");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "database.settings.sidecar.port"
        },
        "op": {
            "type": "set",
            "new_text": "9100",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "toml config path create-missing should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(
        parsed["database"]["settings"]["sidecar"]["port"].as_integer(),
        Some(9100)
    );
}

#[test]
fn patch_json_config_path_set_create_missing_toml_with_exact_file_hash_succeeds() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\n[server]\nhost = \"127.0.0.1\"\n")
        .expect("toml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "server.port",
            "expected_file_hash": crate::common::hash_text(&before)
        },
        "op": {
            "type": "set",
            "new_text": "9090",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "exact-hash TOML create-missing should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(updated.contains("# keep-this-comment"));
    assert!(updated.contains("port = 9090"));
}

#[test]
fn patch_json_config_path_create_missing_invalid_toml_value_does_not_mutate_file() {
    let file_path = copy_fixture_to_temp_with_suffix("example.toml", ".toml");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "database.settings.sidecar.port"
        },
        "op": {
            "type": "set",
            "new_text": "{invalid-toml",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(!output.status.success(), "invalid value should fail");
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "failure must not mutate TOML source");
}

#[test]
fn patch_json_create_missing_existing_toml_path_stale_hash_fails_precondition_no_mutation() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\n[server]\nport = 8080\nhost = \"127.0.0.1\"\n")
        .expect("toml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "server.port",
            "expected_file_hash": "deadbeefdeadbeef"
        },
        "op": {
            "type": "set",
            "new_text": "9090",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(!output.status.success(), "operation should fail");
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "precondition_failed");
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "stale-hash failure must not mutate file");
}

#[test]
fn patch_json_config_path_append_appends_toml_array() {
    let file_path = copy_fixture_to_temp_with_suffix("example.toml", ".toml");
    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "pipelines[0].steps"
        },
        "op": {
            "type": "append",
            "new_text": "\"qa\""
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "toml config append should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(
        parsed["pipelines"][0]["steps"],
        toml::Value::Array(vec![
            toml::Value::String("fmt".to_string()),
            toml::Value::String("clippy".to_string()),
            toml::Value::String("qa".to_string())
        ])
    );
}
