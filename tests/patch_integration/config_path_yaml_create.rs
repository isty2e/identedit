use super::*;

#[test]
fn patch_json_config_path_set_create_missing_preserves_crlf_yaml_newlines() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\r\n  name: identedit\r\n")
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
        output.status.success(),
        "yaml create-missing should succeed on CRLF source: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let updated = fs::read(&file_path).expect("updated YAML should be readable");
    assert!(
        updated.windows(2).any(|pair| pair == b"\r\n"),
        "updated YAML should retain CRLF line endings"
    );
    for (index, byte) in updated.iter().enumerate() {
        if *byte == b'\n' {
            assert!(
                index > 0 && updated[index - 1] == b'\r',
                "every newline should be CRLF, found lone LF at byte {index}"
            );
        }
    }
}

#[test]
fn patch_json_config_path_set_create_missing_bom_yaml_comment_only_root_adds_key_after_comment() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all("\u{feff}# keep-root-comment\n".as_bytes())
        .expect("yaml fixture write should succeed");
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
        "BOM-prefixed YAML comment-only create-missing should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(
        updated.starts_with("\u{feff}# keep-root-comment\n"),
        "BOM and root comment should stay at top, got:\n{updated:?}"
    );
    assert!(updated.contains("server:\n  port: 9090"));
}

#[test]
fn patch_json_config_path_set_create_missing_empty_yaml_with_exact_hash_succeeds() {
    let temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "server.enabled",
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
        "empty YAML exact-hash create-missing should bootstrap mapping: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(updated.contains("server:\n  enabled: true"));
}

#[test]
fn patch_json_config_path_set_create_missing_empty_yaml_rejects_array_auto_create() {
    let temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
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
            "new_text": "primary",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "create-missing must not auto-create YAML array indexes"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("append operation")),
        "YAML array auto-create rejection should point to append operation"
    );
    let updated = fs::read_to_string(&file_path).expect("YAML fixture should be readable");
    assert_eq!(
        updated, "",
        "rejected YAML array auto-create must not mutate file"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_bom_whitespace_yaml_with_stale_hash_fails() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    let source = "\u{feff} \n";
    temp_file
        .write_all(source.as_bytes())
        .expect("yaml fixture write should succeed");
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
        "stale hash must reject BOM+whitespace-only YAML create-missing"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "precondition_failed");
    let updated = fs::read_to_string(&file_path).expect("YAML fixture should be readable");
    assert_eq!(updated, source, "stale hash failure must not mutate YAML");
}

#[test]
fn patch_json_config_path_set_create_missing_cr_only_whitespace_yaml_preserves_prefix() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    let source = " \r\r";
    temp_file
        .write_all(source.as_bytes())
        .expect("yaml fixture write should succeed");
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
        "CR-only whitespace YAML should bootstrap mapping: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(
        updated.starts_with(source),
        "CR-only YAML whitespace prefix should be preserved, got:\n{updated:?}"
    );
    assert!(updated.contains("server:\r  port: 9090"));
}

#[test]
fn patch_json_config_path_set_create_missing_empty_yaml_root_leaf_with_stale_hash_fails() {
    let temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "enabled",
            "expected_file_hash": "0000000000000000"
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
        "stale hash must reject empty YAML root-leaf create"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "precondition_failed");
    let updated = fs::read_to_string(&file_path).expect("YAML fixture should be readable");
    assert_eq!(updated, "", "stale hash failure must not mutate empty YAML");
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_sequence_item_block_preserves_next_item() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(
            b"steps:\n  - name: setup\n    env:\n      CARGO_TERM_COLOR: always\n  - name: test\n",
        )
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "steps[0].run"
        },
        "op": {
            "type": "set",
            "new_text": "|\n  cargo check\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML block scalar create-missing under one sequence item must not attach to the next item: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(
        updated.contains("    run: |\n      cargo check\n  - name: test"),
        "inserted run block should land before the next sequence item at the item boundary: {updated}"
    );
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed["steps"][0]["run"].as_str(), Some("cargo check\n"));
    assert_eq!(parsed["steps"][1]["name"].as_str(), Some("test"));
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_rejects_directive_like_multiline_value() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"data:\n  existing: value\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "data.directive"
        },
        "op": {
            "type": "set",
            "new_text": "%YAML 1.2\n---\nvalue\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "directive-looking multiline YAML fragments should be rejected unless wrapped as block scalars"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "rejected operation must not mutate YAML");
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_document_marker_preserves_marker() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"---\ndata:\n  existing: value\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "data.script"
        },
        "op": {
            "type": "set",
            "new_text": "|\n  echo marker\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML create-missing should preserve an explicit document marker: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(
        updated.starts_with("---\n"),
        "document marker should remain outside the root replacement: {updated}"
    );
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed["data"]["script"].as_str(), Some("echo marker\n"));
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_bom_document_marker_preserves_prefix() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all("\u{feff}---\ndata:\n  existing: value\n".as_bytes())
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "data.script"
        },
        "op": {
            "type": "set",
            "new_text": "|\n  echo bom marker\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML block scalar create-missing should preserve BOM plus explicit document marker: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(
        updated.starts_with("\u{feff}---\n"),
        "BOM and document marker should remain outside the root replacement: {updated:?}"
    );
    let parsed_text = updated.trim_start_matches('\u{feff}');
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(parsed_text).expect("updated YAML should stay valid");
    assert_eq!(parsed["data"]["script"].as_str(), Some("echo bom marker\n"));
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_crlf_document_marker_preserves_crlf() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"---\r\ndata:\r\n  existing: value\r\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "data.script"
        },
        "op": {
            "type": "set",
            "new_text": "|\n  echo crlf marker\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML CRLF document-marker create-missing should preserve CRLF line endings: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read(&file_path).expect("updated YAML should be readable");
    for (index, byte) in updated.iter().enumerate() {
        if *byte == b'\n' {
            assert!(
                index > 0 && updated[index - 1] == b'\r',
                "CRLF source should not gain bare LF at byte {index}"
            );
        }
    }
    let parsed_text = String::from_utf8(updated)
        .expect("updated YAML should stay UTF-8")
        .replace("\r\n", "\n");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&parsed_text).expect("updated YAML should stay valid");
    assert_eq!(
        parsed["data"]["script"].as_str(),
        Some("echo crlf marker\n")
    );
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_rejects_form_feed_block_header() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"data:\n  existing: value\n")
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
            "new_text": "|\u{000c}\n  body\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "YAML block scalar headers with form-feed suffix should be rejected"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "rejected operation must not mutate YAML");
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_root_sequence_item_mapping() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"- name: setup\n- name: test\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "[0].run"
        },
        "op": {
            "type": "set",
            "new_text": "|\n  echo root sequence\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML create-missing should support root sequence item mappings without touching sibling items: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed[0]["run"].as_str(), Some("echo root sequence\n"));
    assert_eq!(parsed[1]["name"].as_str(), Some("test"));
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_root_sequence_index_out_of_bounds_rejects() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"- name: setup\n- name: test\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "[2].run"
        },
        "op": {
            "type": "set",
            "new_text": "|\n  echo missing\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "YAML create-missing should reject out-of-bounds root sequence indexes"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "rejected operation must not mutate YAML");
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_root_sequence_scalar_parent_rejects() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"- scalar\n- name: test\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "[0].run"
        },
        "op": {
            "type": "set",
            "new_text": "|\n  echo invalid\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "YAML create-missing should reject creating mapping keys under scalar root sequence items"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "rejected operation must not mutate YAML");
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_second_root_sequence_item_mapping() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"- name: setup\n- name: test\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "[1].run"
        },
        "op": {
            "type": "set",
            "new_text": "|\n  cargo test\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML root sequence create-missing should handle nonzero indexes: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed[0]["name"].as_str(), Some("setup"));
    assert_eq!(parsed[1]["run"].as_str(), Some("cargo test\n"));
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_root_sequence_no_final_newline() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"- name: setup")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "[0].run"
        },
        "op": {
            "type": "set",
            "new_text": "|\n  echo no final newline\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML root sequence create-missing should handle no-final-newline sources: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed[0]["run"].as_str(), Some("echo no final newline\n"));
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_duplicate_parent_rejects_without_mutation() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"data:\n  existing: one\ndata:\n  existing: two\n")
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
            "new_text": "|\n  echo ambiguous parent\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "YAML create-missing should reject duplicate matching parent keys as ambiguous"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "rejected operation must not mutate YAML");
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_inserts_before_document_end_marker() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"---\nservice:\n  name: app\n...\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.script"
        },
        "op": {
            "type": "set",
            "new_text": "|\n  echo before end marker\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML create-missing should insert inside the document before an explicit end marker: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let script_offset = updated
        .find("script:")
        .expect("script key should be inserted");
    let end_marker_offset = updated
        .find("...")
        .expect("document end marker should remain");
    assert!(
        script_offset < end_marker_offset,
        "inserted key must be inside the document, not after the end marker"
    );
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(
        parsed["service"]["script"].as_str(),
        Some("echo before end marker\n")
    );
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_parent_trailing_comment_stays_comment() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  name: app\n  # managed by humans\nnext: keep\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.script"
        },
        "op": {
            "type": "set",
            "new_text": "|\n  echo after comment\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML create-missing should not turn a parent trailing comment into scalar content: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(
        updated.contains("  # managed by humans"),
        "existing parent comment must remain as a YAML comment"
    );
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(
        parsed["service"]["script"].as_str(),
        Some("echo after comment\n")
    );
    assert_eq!(parsed["next"].as_str(), Some("keep"));
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_quotes_non_breaking_space_key() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"data:\n  existing: value\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "data[\"non\u{00a0}breaking\"]"
        },
        "op": {
            "type": "set",
            "new_text": "|\n  nbsp key\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML create-missing should handle non-breaking-space keys without changing key identity: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(
        parsed["data"]["non\u{00a0}breaking"].as_str(),
        Some("nbsp key\n")
    );
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_allows_indexed_sequence_parent() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"items:\n  - name: dup\n  - name: dup\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "items[0].script"
        },
        "op": {
            "type": "set",
            "new_text": "|\n  echo item\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "indexed YAML sequence parent should allow create-missing inside the selected item: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed["items"][0]["script"].as_str(), Some("echo item\n"));
    assert!(
        parsed["items"][1].get("script").is_none(),
        "indexed create-missing must not affect the sibling item with the same content"
    );
    assert_ne!(before, updated);
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_rejects_flow_root_mapping_without_mutation() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"{name: app}\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "enabled"
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
        "YAML create-missing should not reserialize a flow root mapping into block style"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "rejected operation must not mutate YAML");
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_preserves_blank_line_before_next_root() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  name: app\n\nnext: keep\n")
        .expect("yaml fixture write should succeed");
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
        "YAML create-missing should not collapse blank-line separation before the next root key: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(
        updated.contains("  enabled: true\n\nnext: keep"),
        "insertion should preserve the blank-line separator before the next root key"
    );
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed["service"]["enabled"].as_bool(), Some(true));
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_single_line_after_parent_comment() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  name: app\n  # keep service note\nnext: keep\n")
        .expect("yaml fixture write should succeed");
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
        "YAML single-line create-missing should preserve trailing parent comments: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(updated.contains("  # keep service note\n  enabled: true\nnext: keep"));
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed["service"]["enabled"].as_bool(), Some(true));
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_quoted_backslash_key_surgical_path() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"data:\n  existing: value\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": r#"data["C:\\temp\\file"]"#
        },
        "op": {
            "type": "set",
            "new_text": "path value",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML create-missing should quote backslash-heavy keys on the surgical path: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(
        parsed["data"]["C:\\temp\\file"].as_str(),
        Some("path value")
    );
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_exact_hash_succeeds_on_surgical_path() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  name: app\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.port",
            "expected_file_hash": identedit::hash::hash_text(&before)
        },
        "op": {
            "type": "set",
            "new_text": "8080",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML create-missing should honor exact file hash on the surgical path: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed["service"]["port"].as_i64(), Some(8080));
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_single_line_url_value_stays_scalar() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  name: app\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.endpoint"
        },
        "op": {
            "type": "set",
            "new_text": "https://example.com/api?x=1#frag",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML create-missing should accept URL-like plain scalars that parse as strings: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(
        parsed["service"]["endpoint"].as_str(),
        Some("https://example.com/api?x=1#frag")
    );
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_empty_value_rejects_without_mutation() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  name: app\n")
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
            "new_text": "",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "YAML create-missing should reject empty value text instead of creating null"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "rejected operation must not mutate YAML");
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_quoted_empty_string_succeeds() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  name: app\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.script"
        },
        "op": {
            "type": "set",
            "new_text": "\"\"",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "quoted empty YAML string should be accepted as an explicit value: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed["service"]["script"].as_str(), Some(""));
}

#[test]
fn patch_json_config_path_set_create_missing_empty_yaml_comment_only_value_rejects() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "script"
        },
        "op": {
            "type": "set",
            "new_text": "# not a value",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "empty YAML create-missing should reject comment-only JSON payloads"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(after, "", "rejected operation must not mutate YAML");
}

#[test]
fn patch_json_config_path_set_create_missing_whitespace_yaml_quoted_empty_string() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"  \n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "script"
        },
        "op": {
            "type": "set",
            "new_text": "\"\"",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "whitespace-only YAML create-missing should accept explicit quoted empty strings: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed["script"].as_str(), Some(""));
}

#[test]
fn patch_json_config_path_set_create_missing_empty_yaml_quoted_comment_string_succeeds() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "script"
        },
        "op": {
            "type": "set",
            "new_text": "\"# literal\"",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "empty YAML create-missing should accept explicitly quoted comment-like strings: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed["script"].as_str(), Some("# literal"));
}

#[test]
fn patch_json_config_path_set_create_missing_empty_yaml_quotes_date_like_root_key() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": r#"["2026-04-22"].enabled"#
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
        "empty YAML create-missing should quote timestamp-like root keys: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed["2026-04-22"]["enabled"].as_bool(), Some(true));
}

#[test]
fn patch_json_config_path_set_create_missing_whitespace_yaml_quotes_boolean_like_root_key() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": r#"["on"].enabled"#
        },
        "op": {
            "type": "set",
            "new_text": "false",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "whitespace YAML create-missing should quote boolean-like root keys: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed["on"]["enabled"].as_bool(), Some(false));
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_allows_quoted_anchor_like_string() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  name: app\n")
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
            "new_text": "\"&anchor value\"",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML create-missing should allow quoted anchor-like strings: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed["service"]["value"].as_str(), Some("&anchor value"));
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_rejects_tag_value() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  name: app\n")
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
            "new_text": "!secret value",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "YAML create-missing should reject tag syntax instead of introducing YAML tag semantics"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "rejected operation must not mutate YAML");
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_rejects_nul_scalar() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  name: app\n")
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
            "new_text": "alpha\0beta",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "YAML create-missing should reject raw NUL bytes"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "rejected operation must not mutate YAML");
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_rejects_unicode_paragraph_separator_scalar() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  name: app\n")
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
            "new_text": "alpha\u{2029}beta",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "YAML create-missing should reject raw Unicode paragraph separator"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "rejected operation must not mutate YAML");
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_allows_quoted_tag_like_string() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  name: app\n")
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
            "new_text": "\"!secret value\"",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML create-missing should allow quoted tag-like strings: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed["service"]["value"].as_str(), Some("!secret value"));
}
