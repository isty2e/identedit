use super::*;

#[test]
fn patch_config_set_value_text_file_invalid_json_does_not_mutate_file() {
    let file_path = copy_fixture_to_temp_json("example.json");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let payload_path = create_temp_text_file("{invalid-json");

    let output = run_identedit(&[
        "patch",
        "--config-path",
        "config",
        "--set-value",
        "--text-file",
        payload_path.to_str().expect("payload path should be utf-8"),
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(!output.status.success(), "invalid JSON payload should fail");
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let after = fs::read_to_string(&file_path).expect("file should remain readable");
    assert_eq!(after, before);
}

#[test]
fn patch_config_set_value_stdin_invalid_json_does_not_mutate_file() {
    let file_path = copy_fixture_to_temp_json("example.json");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let output = run_identedit_with_stdin(
        &[
            "patch",
            "--config-path",
            "config",
            "--set-value",
            "--stdin-text",
            file_path.to_str().expect("path should be utf-8"),
        ],
        "{invalid-json",
    );

    assert!(
        !output.status.success(),
        "invalid JSON stdin payload should fail"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let after = fs::read_to_string(&file_path).expect("file should remain readable");
    assert_eq!(after, before);
}

#[test]
fn patch_config_append_text_file_invalid_json_does_not_mutate_file() {
    let file_path = copy_fixture_to_temp_json("example.json");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let payload_path = create_temp_text_file("[invalid");

    let output = run_identedit(&[
        "patch",
        "--config-path",
        "items",
        "--append-value",
        "--text-file",
        payload_path.to_str().expect("payload path should be utf-8"),
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        !output.status.success(),
        "invalid append payload should fail"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let after = fs::read_to_string(&file_path).expect("file should remain readable");
    assert_eq!(after, before);
}

#[test]
fn patch_config_set_value_stdin_json_string_with_leading_dash_applies() {
    let file_path = copy_fixture_to_temp_json("example.json");

    let output = run_identedit_with_stdin(
        &[
            "patch",
            "--config-path",
            "name",
            "--set-value",
            "--stdin-text",
            file_path.to_str().expect("path should be utf-8"),
        ],
        "\"--help\"",
    );

    assert!(
        output.status.success(),
        "JSON string payload with leading dash should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let modified: Value =
        serde_json::from_str(&fs::read_to_string(&file_path).expect("modified file should read"))
            .expect("modified JSON should parse");
    assert_eq!(modified["name"], "--help");
}

#[test]
fn patch_json_config_path_set_updates_json_value() {
    let file_path = copy_fixture_to_temp_with_suffix("example.json", ".json");
    let original = fs::read_to_string(&file_path).expect("fixture should be readable");
    let expected_file_hash = identedit::hash::hash_text(&original);

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "config.retries",
            "expected_file_hash": expected_file_hash
        },
        "op": {
            "type": "set",
            "new_text": "10"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "config path set should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["summary"]["operations_applied"], 1);

    let updated = fs::read_to_string(&file_path).expect("updated file should be readable");
    assert!(updated.contains("\"retries\": 10"));
}

#[test]
fn patch_json_config_path_set_options_dry_run_does_not_modify_json() {
    let file_path = copy_fixture_to_temp_with_suffix("example.json", ".json");
    let before = fs::read_to_string(&file_path).expect("json should be readable");
    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "config.enabled"
        },
        "op": {
            "type": "set",
            "new_text": "false"
        },
        "options": {
            "dry_run": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "json config path dry-run should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["transaction"]["status"], "dry_run");
    assert_eq!(
        fs::read_to_string(&file_path).expect("json should be readable"),
        before,
        "json config-path dry-run must not mutate the file"
    );
}

#[test]
fn patch_json_config_path_reports_missing_path() {
    let file_path = copy_fixture_to_temp_with_suffix("example.yaml", ".yaml");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.not_found"
        },
        "op": {
            "type": "set",
            "new_text": "9"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(!output.status.success(), "missing config path should fail");

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("was not found")),
        "expected missing-path diagnostic"
    );
}

#[test]
fn patch_flag_config_path_set_value_updates_json() {
    let file_path = copy_fixture_to_temp_with_suffix("example.json", ".json");

    let output = run_identedit(&[
        "patch",
        "--config-path",
        "config.enabled",
        "--set-value",
        "false",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "flag config path set should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let updated = fs::read_to_string(&file_path).expect("updated JSON should be readable");
    let parsed: Value = serde_json::from_str(&updated).expect("updated JSON should stay valid");
    assert_eq!(parsed["config"]["enabled"], Value::Bool(false));
}

#[test]
fn patch_flag_config_path_set_value_dry_run_does_not_modify_json() {
    let file_path = copy_fixture_to_temp_with_suffix("example.json", ".json");
    let before = fs::read_to_string(&file_path).expect("json should be readable");

    let output = run_identedit(&[
        "patch",
        "--config-path",
        "config.enabled",
        "--set-value",
        "false",
        "--dry-run",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "flag config path dry-run should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["transaction"]["status"], "dry_run");
    assert_eq!(
        fs::read_to_string(&file_path).expect("json should be readable"),
        before,
        "config dry-run must not mutate the source file"
    );
}

#[test]
fn patch_flag_config_path_create_missing_dry_run_does_not_modify_json() {
    let file_path = copy_fixture_to_temp_with_suffix("example.json", ".json");
    let before = fs::read_to_string(&file_path).expect("json should be readable");

    let output = run_identedit(&[
        "patch",
        "--config-path",
        "config.sidecar.host",
        "--set-value",
        "\"127.0.0.1\"",
        "--create-missing",
        "--dry-run",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "flag config path create-missing dry-run should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["transaction"]["status"], "dry_run");
    assert_eq!(
        fs::read_to_string(&file_path).expect("json should be readable"),
        before,
        "config create-missing dry-run must not mutate the source file"
    );
}

#[test]
fn patch_flag_config_path_set_value_create_missing_sets_nested_json_keys() {
    let file_path = copy_fixture_to_temp_with_suffix("example.json", ".json");

    let output = run_identedit(&[
        "patch",
        "--config-path",
        "config.sidecar.host",
        "--set-value",
        "\"127.0.0.1\"",
        "--create-missing",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "flag config path create-missing should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let updated = fs::read_to_string(&file_path).expect("updated JSON should be readable");
    let parsed: Value = serde_json::from_str(&updated).expect("updated JSON should stay valid");
    assert_eq!(
        parsed["config"]["sidecar"]["host"],
        Value::String("127.0.0.1".to_string())
    );
}

#[test]
fn patch_json_config_path_set_create_missing_rejects_array_oob_with_append_hint() {
    let file_path = copy_fixture_to_temp_with_suffix("example.json", ".json");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "items[9]"
        },
        "op": {
            "type": "set",
            "new_text": "10",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "out-of-bounds array index should remain an error"
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("append operation")),
        "array OOB error should include append-operation hint"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_bootstraps_empty_json_root() {
    let mut temp_file = Builder::new()
        .suffix(".json")
        .tempfile()
        .expect("temp json file should be created");
    temp_file
        .write_all(b"")
        .expect("json fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.enabled"
        },
        "op": {
            "type": "set",
            "new_text": "true",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "empty-json create-missing should bootstrap root object: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let updated = fs::read_to_string(&file_path).expect("updated JSON should be readable");
    let parsed: Value = serde_json::from_str(&updated).expect("updated JSON should stay valid");
    assert_eq!(parsed["service"]["enabled"], Value::Bool(true));
}

#[test]
fn patch_json_config_path_set_create_missing_nested_array_oob_has_append_hint() {
    let file_path = copy_fixture_to_temp_with_suffix("example.json", ".json");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "config.targets[0].name"
        },
        "op": {
            "type": "set",
            "new_text": "\"api\"",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "nested array slot creation should remain out-of-bounds error"
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("append operation")),
        "nested array OOB error should include append-operation hint"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_keeps_file_header_comment_at_top_before_descendant() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# file header\n[server.sidecar.db]\nhost = \"db\"\n")
        .expect("toml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "server.sidecar.port"
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
        "parent creation after file header comment should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.starts_with("# file header\n\n[server.sidecar]\nport = 9090\n"),
        "file header comment should remain at the top, separated from the new parent table, got:\n{updated}"
    );
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["server"]["sidecar"]["port"].as_integer(), Some(9090));
}

#[test]
fn patch_json_config_path_set_create_missing_moves_multiple_descendant_comment_lines_together() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(
            b"# root-comment\n\n# first descendant comment\n# second descendant comment\n[server.sidecar.db]\nhost = \"db\"\n",
        )
        .expect("toml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "server.sidecar.port"
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
        "parent TOML table creation before multi-line descendant comment should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.contains(
            "[server.sidecar]\nport = 9090\n\n# first descendant comment\n# second descendant comment\n[server.sidecar.db]"
        ),
        "multi-line descendant comment block should stay attached to descendant table, got:\n{updated}"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_keeps_indented_descendant_comment_with_table() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(
            b"# root-comment\n\n  # indented descendant comment\n[server.sidecar.db]\nhost = \"db\"\n",
        )
        .expect("toml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "server.sidecar.port"
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
        "parent creation before indented descendant comment should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.contains(
            "[server.sidecar]\nport = 9090\n\n  # indented descendant comment\n[server.sidecar.db]"
        ),
        "indented descendant comment block should stay attached to descendant table, got:\n{updated}"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_with_stale_expected_file_hash_fails_precondition() {
    let file_path = copy_fixture_to_temp_with_suffix("example.json", ".json");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "config.sidecar.port",
            "expected_file_hash": "deadbeefdeadbeef"
        },
        "op": {
            "type": "set",
            "new_text": "9000",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "stale expected_file_hash should fail before create-missing"
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "precondition_failed");
}

#[test]
fn patch_json_config_path_set_create_missing_root_array_oob_has_append_hint() {
    let mut temp_file = Builder::new()
        .suffix(".json")
        .tempfile()
        .expect("temp json file should be created");
    temp_file
        .write_all(b"[]")
        .expect("json fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "[0].name"
        },
        "op": {
            "type": "set",
            "new_text": "\"api\"",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "root array out-of-bounds should remain an error"
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("append operation")),
        "root array OOB should include append-operation hint"
    );
}

#[test]
fn patch_json_config_path_set_supports_json_quoted_key_segments() {
    let mut temp_file = Builder::new()
        .suffix(".json")
        .tempfile()
        .expect("temp json file should be created");
    temp_file
        .write_all(br#"{"x.y":{"a:b":1},"regular":true}"#)
        .expect("json fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": r#"["x.y"]["a:b"]"#
        },
        "op": {
            "type": "set",
            "new_text": "2"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "quoted JSON path should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated JSON should be readable");
    let parsed: Value = serde_json::from_str(&updated).expect("updated JSON should stay valid");
    assert_eq!(parsed["x.y"]["a:b"].as_i64(), Some(2));
    assert_eq!(parsed["regular"].as_bool(), Some(true));
}

#[test]
fn patch_json_config_path_set_create_missing_still_rejects_invalid_path_characters() {
    let file_path = copy_fixture_to_temp_with_suffix("example.json", ".json");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service bad.path"
        },
        "op": {
            "type": "set",
            "new_text": "1",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "invalid path characters should still be rejected"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}

#[test]
fn patch_json_config_path_set_create_missing_empty_json_with_stale_hash_fails_precondition() {
    let mut temp_file = Builder::new()
        .suffix(".json")
        .tempfile()
        .expect("temp json file should be created");
    temp_file
        .write_all(b"")
        .expect("json fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.enabled",
            "expected_file_hash": "deadbeefdeadbeef"
        },
        "op": {
            "type": "set",
            "new_text": "true",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "stale hash should fail even when bootstrapping empty json"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "precondition_failed");
}

#[test]
fn patch_json_config_path_set_create_missing_array_oob_does_not_mutate_file() {
    let file_path = copy_fixture_to_temp_with_suffix("example.json", ".json");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "items[99]"
        },
        "op": {
            "type": "set",
            "new_text": "10",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(!output.status.success(), "array OOB should fail");

    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(
        before, after,
        "failed create-missing operation must not mutate file"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_empty_json_with_exact_hash_succeeds() {
    let mut temp_file = Builder::new()
        .suffix(".json")
        .tempfile()
        .expect("temp json file should be created");
    temp_file
        .write_all(b"")
        .expect("json fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.enabled",
            "expected_file_hash": identedit::hash::hash_text("")
        },
        "op": {
            "type": "set",
            "new_text": "true",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "exact hash should allow empty-json bootstrap: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn patch_json_config_path_create_missing_existing_anchor_path_rejects_with_no_mutation() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(
            b"defaults: &defaults\n  retries: 2\nservice:\n  <<: *defaults\n  name: identedit\n",
        )
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "defaults.retries"
        },
        "op": {
            "type": "set",
            "new_text": "5",
            "create_missing": true
        }
    });

    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "anchor/alias yaml should be rejected in create-missing mode"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "rejected operation must not mutate file");
}

#[test]
fn patch_json_config_path_create_missing_existing_multi_document_path_preserves_both_docs() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"---\nservice:\n  retries: 2\n---\nmetadata:\n  owner: team\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.retries"
        },
        "op": {
            "type": "set",
            "new_text": "5",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "existing multi-doc path should still use strict edit path: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(updated.contains("---\nservice:"));
    assert!(updated.contains("---\nmetadata:"));
    assert!(updated.contains("retries: 5"));
}

#[test]
fn patch_json_config_path_create_missing_invalid_json_value_does_not_mutate_file() {
    let file_path = copy_fixture_to_temp_with_suffix("example.json", ".json");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "config.sidecar.port"
        },
        "op": {
            "type": "set",
            "new_text": "{invalid-json",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(!output.status.success(), "invalid value should fail");
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "failure must not mutate JSON source");
}

#[test]
fn patch_json_config_path_create_missing_stale_hash_reports_empty_actual_hash() {
    let mut temp_file = Builder::new()
        .suffix(".json")
        .tempfile()
        .expect("temp json file should be created");
    temp_file
        .write_all(b"")
        .expect("json fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.enabled",
            "expected_file_hash": "deadbeefdeadbeef"
        },
        "op": {
            "type": "set",
            "new_text": "true",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(!output.status.success(), "operation should fail");
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "precondition_failed");
}

#[test]
fn patch_json_config_path_bootstrap_empty_json_writes_valid_object_document() {
    let mut temp_file = Builder::new()
        .suffix(".json")
        .tempfile()
        .expect("temp json file should be created");
    temp_file
        .write_all(b"")
        .expect("json fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.enabled"
        },
        "op": {
            "type": "set",
            "new_text": "true",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(output.status.success(), "operation should succeed");
    let updated = fs::read_to_string(&file_path).expect("updated file should be readable");
    let parsed: Value = serde_json::from_str(&updated).expect("updated file should be valid JSON");
    assert_eq!(parsed["service"]["enabled"], Value::Bool(true));
    assert!(
        updated.trim_start().starts_with('{'),
        "bootstrapped document should be object-shaped JSON text"
    );
}

#[test]
fn patch_json_config_path_create_missing_existing_multi_document_path_with_hash_precondition() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"---\nservice:\n  retries: 2\n---\nmetadata:\n  owner: team\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.retries",
            "expected_file_hash": identedit::hash::hash_text(&before)
        },
        "op": {
            "type": "set",
            "new_text": "5",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(output.status.success(), "operation should succeed");
    let updated = fs::read_to_string(&file_path).expect("updated file should be readable");
    assert!(updated.contains("retries: 5"));
    assert!(updated.contains("---\nmetadata:"));
}

#[test]
fn patch_json_config_delete_rejects_create_missing_false_field() {
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
            "create_missing": false
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "delete should reject create_missing field"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}

#[test]
fn patch_json_config_delete_rejects_create_missing_null_field() {
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
            "create_missing": null
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "delete should reject create_missing field"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}

#[test]
fn patch_json_config_set_rejects_non_boolean_create_missing_type() {
    let file_path = copy_fixture_to_temp_with_suffix("example.json", ".json");
    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "config.sidecar.port"
        },
        "op": {
            "type": "set",
            "new_text": "9000",
            "create_missing": "yes"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "non-boolean create_missing should be rejected"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}

#[test]
fn patch_json_config_set_omitted_create_missing_keeps_strict_missing_path_behavior() {
    let file_path = copy_fixture_to_temp_with_suffix("example.json", ".json");
    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "config.sidecar.port"
        },
        "op": {
            "type": "set",
            "new_text": "9000"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "omitted create_missing should keep strict missing-path behavior"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("was not found")),
        "strict mode should report missing path"
    );
}

#[test]
fn patch_json_config_set_explicit_false_create_missing_keeps_strict_missing_path_behavior() {
    let file_path = copy_fixture_to_temp_with_suffix("example.json", ".json");
    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "config.sidecar.port"
        },
        "op": {
            "type": "set",
            "new_text": "9000",
            "create_missing": false
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "create_missing=false should keep strict missing-path behavior"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("was not found")),
        "strict mode should report missing path"
    );
}

#[test]
fn patch_json_config_set_with_create_missing_rejects_unknown_payload_field() {
    let file_path = copy_fixture_to_temp_with_suffix("example.json", ".json");
    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "config.sidecar.port"
        },
        "op": {
            "type": "set",
            "new_text": "9000",
            "create_missing": true,
            "unexpected": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "unknown payload field should be rejected"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}

#[test]
fn patch_json_config_path_set_create_missing_keeps_crlf_descendant_comment_with_table() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(
            b"# root-comment\r\n\r\n# sidecar db table comment\r\n[server.sidecar.db]\r\nhost = \"db\"\r\n",
        )
        .expect("toml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "server.sidecar.port"
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
        "CRLF descendant table comment should stay attached when parent is created: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.contains(
            "[server.sidecar]\r\nport = 9090\r\n\r\n# sidecar db table comment\r\n[server.sidecar.db]"
        ),
        "CRLF descendant comment block should remain with descendant table, got:\n{updated:?}"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_whitespace_only_json_bootstraps_object() {
    let mut temp_file = Builder::new()
        .suffix(".json")
        .tempfile()
        .expect("temp json file should be created");
    let source = " \n\t";
    temp_file
        .write_all(source.as_bytes())
        .expect("json fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "server.port"
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
        "whitespace-only JSON create-missing should bootstrap an object: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated JSON should be readable");
    assert!(
        updated.starts_with(source),
        "whitespace prefix should be preserved before bootstrapped JSON object, got:\n{updated:?}"
    );
    let parsed: Value = serde_json::from_str(&updated).expect("updated JSON should stay valid");
    assert_eq!(parsed["server"]["port"], json!(9090));
}

#[test]
fn patch_json_config_path_set_create_missing_whitespace_only_json_with_stale_hash_fails() {
    let mut temp_file = Builder::new()
        .suffix(".json")
        .tempfile()
        .expect("temp json file should be created");
    let source = " \n\t";
    temp_file
        .write_all(source.as_bytes())
        .expect("json fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "server.port",
            "expected_file_hash": "0000000000000000"
        },
        "op": {
            "type": "set",
            "new_text": "9090",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "stale hash must reject whitespace-only JSON bootstrap"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "precondition_failed");
    let updated = fs::read_to_string(&file_path).expect("JSON fixture should be readable");
    assert_eq!(
        updated, source,
        "stale hash failure must not mutate whitespace-only JSON"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_whitespace_only_json_with_exact_hash_succeeds() {
    let mut temp_file = Builder::new()
        .suffix(".json")
        .tempfile()
        .expect("temp json file should be created");
    let source = "\n  \n";
    temp_file
        .write_all(source.as_bytes())
        .expect("json fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "server.enabled",
            "expected_file_hash": identedit::hash::hash_text(source)
        },
        "op": {
            "type": "set",
            "new_text": "true",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "whitespace-only JSON exact-hash bootstrap should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated JSON should be readable");
    assert!(updated.starts_with(source));
    let parsed: Value = serde_json::from_str(&updated).expect("updated JSON should stay valid");
    assert_eq!(parsed["server"]["enabled"], json!(true));
}

#[test]
fn patch_json_config_path_set_create_missing_whitespace_only_json_rejects_array_auto_create() {
    let mut temp_file = Builder::new()
        .suffix(".json")
        .tempfile()
        .expect("temp json file should be created");
    let source = " \n";
    temp_file
        .write_all(source.as_bytes())
        .expect("json fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "items[0].name"
        },
        "op": {
            "type": "set",
            "new_text": r#""primary""#,
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "create-missing must not auto-create JSON array indexes"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("append operation")),
        "array auto-create rejection should point to append operation"
    );
    let updated = fs::read_to_string(&file_path).expect("JSON fixture should be readable");
    assert_eq!(
        updated, source,
        "rejected array auto-create must not mutate whitespace-only JSON"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_crlf_whitespace_json_preserves_prefix() {
    let mut temp_file = Builder::new()
        .suffix(".json")
        .tempfile()
        .expect("temp json file should be created");
    let source = " \r\n\t\r\n";
    temp_file
        .write_all(source.as_bytes())
        .expect("json fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "server.port"
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
        "CRLF whitespace-only JSON should bootstrap object: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated JSON should be readable");
    assert!(updated.starts_with(source));
    let parsed: Value = serde_json::from_str(&updated).expect("updated JSON should stay valid");
    assert_eq!(parsed["server"]["port"], json!(9090));
}

#[test]
fn patch_json_config_path_set_create_missing_rejects_explicit_json_null_parent() {
    let mut temp_file = Builder::new()
        .suffix(".json")
        .tempfile()
        .expect("temp json file should be created");
    temp_file
        .write_all(b"null")
        .expect("json fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "server.port"
        },
        "op": {
            "type": "set",
            "new_text": "9090",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "explicit JSON null must not be silently promoted into an object"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "rejected explicit null must not mutate JSON");
}

#[test]
fn patch_json_config_path_set_create_missing_whitespace_only_json_root_leaf_with_exact_hash() {
    let mut temp_file = Builder::new()
        .suffix(".json")
        .tempfile()
        .expect("temp json file should be created");
    let source = "\t\n";
    temp_file
        .write_all(source.as_bytes())
        .expect("json fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "enabled",
            "expected_file_hash": identedit::hash::hash_text(source)
        },
        "op": {
            "type": "set",
            "new_text": "true",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "whitespace-only JSON root leaf exact-hash create should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated JSON should be readable");
    assert!(updated.starts_with(source));
    let parsed: Value = serde_json::from_str(&updated).expect("updated JSON should stay valid");
    assert_eq!(parsed["enabled"], json!(true));
}

#[test]
fn patch_json_config_path_set_create_missing_cr_only_whitespace_json_preserves_prefix() {
    let mut temp_file = Builder::new()
        .suffix(".json")
        .tempfile()
        .expect("temp json file should be created");
    let source = " \r\r";
    temp_file
        .write_all(source.as_bytes())
        .expect("json fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "server.port"
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
        "CR-only whitespace JSON should bootstrap object: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated JSON should be readable");
    assert!(updated.starts_with(source));
    let parsed: Value = serde_json::from_str(&updated).expect("updated JSON should stay valid");
    assert_eq!(parsed["server"]["port"], json!(9090));
}

#[test]
fn patch_json_config_path_set_create_missing_rejects_explicit_json_bool_parent() {
    let mut temp_file = Builder::new()
        .suffix(".json")
        .tempfile()
        .expect("temp json file should be created");
    temp_file
        .write_all(b"false")
        .expect("json fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "server.port"
        },
        "op": {
            "type": "set",
            "new_text": "9090",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "explicit JSON bool must not be silently promoted into an object"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "rejected explicit bool must not mutate JSON");
}

#[test]
fn patch_json_config_path_set_create_missing_whitespace_only_json_sets_object_value() {
    let mut temp_file = Builder::new()
        .suffix(".json")
        .tempfile()
        .expect("temp json file should be created");
    let source = "\n";
    temp_file
        .write_all(source.as_bytes())
        .expect("json fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "server.sidecar"
        },
        "op": {
            "type": "set",
            "new_text": r#"{"port": 9090, "enabled": true}"#,
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "whitespace-only JSON should accept object-valued create-missing leaf: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated JSON should be readable");
    assert!(updated.starts_with(source));
    let parsed: Value = serde_json::from_str(&updated).expect("updated JSON should stay valid");
    assert_eq!(parsed["server"]["sidecar"]["enabled"], json!(true));
}

#[test]
fn patch_json_config_path_set_create_missing_rejects_explicit_json_array_parent() {
    let mut temp_file = Builder::new()
        .suffix(".json")
        .tempfile()
        .expect("temp json file should be created");
    temp_file
        .write_all(b"[]")
        .expect("json fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "server.port"
        },
        "op": {
            "type": "set",
            "new_text": "9090",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "explicit JSON array root must not be silently promoted into an object"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(
        before, after,
        "rejected JSON array parent must not mutate file"
    );
}
