use super::*;

#[test]
fn patch_json_config_path_set_create_missing_rejects_yaml_anchor_alias() {
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
            "path": "service.sidecar.port"
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
        "yaml anchor/alias create-missing should be rejected"
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("anchor/alias")),
        "error should explain anchor/alias create-missing limitation"
    );
}

#[test]
fn patch_json_config_path_set_existing_yaml_comment_value_rejects_tabbed_block_header() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"data:\n  # keep data config\n  script: old\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "data.script"
        },
        "op": {
            "type": "set",
            "new_text": "|\t\n  echo existing\n"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "existing YAML value set should reject tabbed block scalar headers"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "rejected operation must not mutate YAML");
}

#[test]
fn patch_json_config_path_yaml_anchor_alias_rejection_does_not_mutate_file() {
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
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.sidecar.port"
        },
        "op": {
            "type": "set",
            "new_text": "9000",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(!output.status.success(), "operation should fail");
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "anchor/alias rejection must not mutate file");
}

#[test]
fn patch_flag_config_path_create_missing_rejects_yaml_anchor_alias() {
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

    let output = run_identedit(&[
        "patch",
        "--config-path",
        "service.sidecar.port",
        "--set-value",
        "9000",
        "--create-missing",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "flag-mode YAML anchor/alias fallback should be rejected"
    );
}

#[test]
fn patch_flag_yaml_multidoc_rejection_keeps_file_unchanged() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"---\nservice:\n  name: identedit\n---\nmetadata:\n  owner: team\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let output = run_identedit(&[
        "patch",
        "--config-path",
        "service.sidecar.port",
        "--set-value",
        "9000",
        "--create-missing",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(!output.status.success(), "operation should fail");
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(
        before, after,
        "rejected flag-mode operation must not mutate file"
    );
}

#[test]
fn patch_flag_yaml_anchor_alias_rejection_keeps_file_unchanged() {
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
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let output = run_identedit(&[
        "patch",
        "--config-path",
        "service.sidecar.port",
        "--set-value",
        "9000",
        "--create-missing",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(!output.status.success(), "operation should fail");
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(
        before, after,
        "rejected flag-mode operation must not mutate file"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_rejects_explicit_yaml_null_parent() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"null\n")
        .expect("yaml fixture write should succeed");
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
        "explicit YAML null must not be silently promoted into a mapping"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "rejected explicit null must not mutate YAML");
}

#[test]
fn patch_json_config_path_set_create_missing_rejects_explicit_yaml_bool_parent() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"false\n")
        .expect("yaml fixture write should succeed");
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
        "explicit YAML bool must not be silently promoted into a mapping"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "rejected explicit bool must not mutate YAML");
}

#[test]
fn patch_json_config_path_set_create_missing_rejects_explicit_yaml_sequence_parent() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"[]\n")
        .expect("yaml fixture write should succeed");
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
        "explicit YAML sequence root must not be silently promoted into a mapping"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(
        before, after,
        "rejected YAML sequence parent must not mutate file"
    );
}

#[test]
fn patch_json_config_path_set_existing_yaml_duplicate_key_rejects_without_mutation() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"data:\n  script: old\n  script: duplicate\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "data.script"
        },
        "op": {
            "type": "set",
            "new_text": "|\n  echo ambiguous\n"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "YAML config path set should reject duplicate matching keys as ambiguous"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "rejected operation must not mutate YAML");
}

#[test]
fn patch_json_config_path_set_existing_yaml_duplicate_root_key_rejects_without_mutation() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"script: one\nscript: two\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "script"
        },
        "op": {
            "type": "set",
            "new_text": "|\n  echo ambiguous root\n"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "YAML config path set should reject duplicate root keys as ambiguous"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "rejected operation must not mutate YAML");
}

#[test]
fn patch_json_config_path_set_existing_yaml_missing_value_rejects_without_mutation() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  script: # intentionally empty\n  next: keep\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.script"
        },
        "op": {
            "type": "set",
            "new_text": "|\n  echo missing value\n"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "YAML existing key with no value node should fail explicitly rather than guessing a span"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "rejected operation must not mutate YAML");
}

#[test]
fn patch_json_config_path_set_existing_yaml_rejects_mapping_like_plain_scalar_without_mutation() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  script: old\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.script"
        },
        "op": {
            "type": "set",
            "new_text": "foo: bar"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "YAML scalar set should reject mapping-like plain text that would make the document invalid"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "rejected operation must not mutate YAML");
}

#[test]
fn patch_json_config_path_set_existing_yaml_rejects_comment_only_plain_scalar_without_mutation() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  script: old\n  next: keep\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.script"
        },
        "op": {
            "type": "set",
            "new_text": "# not a scalar value"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "YAML scalar set should reject comment-only replacement text"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "rejected operation must not mutate YAML");
}

#[test]
fn patch_json_config_path_set_existing_yaml_block_mapping_allows_plain_comma_scalar() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  value: old\n  keep: true\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.value"
        },
        "op": {
            "type": "set",
            "new_text": "a,b"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML block mapping scalar set should allow plain comma-containing scalars: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed["service"]["value"].as_str(), Some("a,b"));
    assert_eq!(parsed["service"]["keep"].as_bool(), Some(true));
}

#[test]
fn patch_json_config_path_set_existing_yaml_block_mapping_rejects_anchor_value() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  value: old\n  keep: true\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.value"
        },
        "op": {
            "type": "set",
            "new_text": "&anchor value"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "YAML block mapping value set should reject anchor syntax instead of introducing YAML anchor semantics"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "rejected operation must not mutate YAML");
}

#[test]
fn patch_json_config_path_set_existing_yaml_block_mapping_allows_quoted_hash_string() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  value: old\n  keep: true\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.value"
        },
        "op": {
            "type": "set",
            "new_text": "\"# not a comment\""
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML block mapping value set should allow explicitly quoted hash-prefixed strings: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed["service"]["value"].as_str(), Some("# not a comment"));
}

#[test]
fn patch_json_config_path_set_existing_yaml_block_mapping_rejects_tag_value() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  value: old\n  keep: true\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.value"
        },
        "op": {
            "type": "set",
            "new_text": "!secret value"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "YAML block mapping value set should reject tag syntax instead of introducing YAML tag semantics"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "rejected operation must not mutate YAML");
}

#[test]
fn patch_json_config_path_set_existing_yaml_block_mapping_allows_quoted_tag_like_string() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  value: old\n  keep: true\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.value"
        },
        "op": {
            "type": "set",
            "new_text": "\"!secret value\""
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML block mapping value set should allow quoted tag-like strings: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed["service"]["value"].as_str(), Some("!secret value"));
}

#[test]
fn patch_json_config_path_set_existing_yaml_block_mapping_rejects_multidoc_value() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  value: old\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.value"
        },
        "op": {
            "type": "set",
            "new_text": "---\nother: value\n"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "YAML existing value set should reject replacement text that would introduce another document"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "rejected operation must not mutate YAML");
}

#[test]
fn patch_json_config_path_set_existing_yaml_block_mapping_rejects_unicode_line_separator_scalar() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  value: old\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.value"
        },
        "op": {
            "type": "set",
            "new_text": "alpha\u{2028}beta"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "YAML existing scalar set should reject Unicode line separator because YAML treats it as a line break"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "rejected operation must not mutate YAML");
}

#[test]
fn patch_json_config_path_set_existing_yaml_block_mapping_rejects_document_end_then_document_start()
{
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  value: old\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.value"
        },
        "op": {
            "type": "set",
            "new_text": "...\n---\nother: value\n"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "YAML existing value set should reject replacement text that introduces document end/start markers"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "rejected operation must not mutate YAML");
}

#[test]
fn patch_json_config_path_set_existing_yaml_block_mapping_allows_quoted_anchor_like_string() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  value: old\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.value"
        },
        "op": {
            "type": "set",
            "new_text": "\"&anchor value\""
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML existing scalar set should allow quoted anchor-like strings: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed["service"]["value"].as_str(), Some("&anchor value"));
}

#[test]
fn patch_json_config_path_set_existing_yaml_block_mapping_rejects_unicode_paragraph_separator_scalar()
 {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  value: old\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.value"
        },
        "op": {
            "type": "set",
            "new_text": "alpha\u{2029}beta"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "YAML existing scalar set should reject Unicode paragraph separator because YAML treats it as a line break"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "rejected operation must not mutate YAML");
}

#[test]
fn patch_json_config_path_set_existing_yaml_block_mapping_rejects_nul_scalar() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  value: old\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.value"
        },
        "op": {
            "type": "set",
            "new_text": "alpha\0beta"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "YAML existing scalar set should reject raw NUL bytes"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "rejected operation must not mutate YAML");
}

#[test]
fn patch_json_config_path_set_existing_yaml_block_mapping_allows_escaped_nul_string() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  value: old\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.value"
        },
        "op": {
            "type": "set",
            "new_text": "\"alpha\\0beta\""
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML block mapping set should allow escaped NUL in a quoted YAML string: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed["service"]["value"].as_str(), Some("alpha\0beta"));
}

#[test]
fn patch_json_config_path_set_existing_yaml_block_mapping_allows_emoji_scalar() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  value: old\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.value"
        },
        "op": {
            "type": "set",
            "new_text": "\"status ✅\""
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML block mapping set should allow ordinary quoted Unicode scalars: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed["service"]["value"].as_str(), Some("status ✅"));
}

#[test]
fn patch_json_config_path_set_existing_yaml_block_mapping_rejects_multiline_nul_value() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  value: old\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.value"
        },
        "op": {
            "type": "set",
            "new_text": "|\n  alpha\0beta\n"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "YAML existing multiline set should reject raw NUL characters before rendering block scalar text"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "rejected operation must not mutate YAML");
}

#[test]
fn patch_json_config_path_set_existing_yaml_block_mapping_rejects_multiline_paragraph_separator() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  value: old\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.value"
        },
        "op": {
            "type": "set",
            "new_text": "|\n  alpha\u{2029}beta\n"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "YAML existing multiline set should reject raw Unicode paragraph separator before rendering block scalar text"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "rejected operation must not mutate YAML");
}
