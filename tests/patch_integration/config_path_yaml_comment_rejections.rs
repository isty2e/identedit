use super::*;

#[test]
fn patch_json_config_path_set_create_missing_yaml_comment_rejects_missing_sequence_parent() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"# intentionally sparse config\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "services[0].name"
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
        "YAML create-missing must not auto-create sequences"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("append operation")),
        "missing sequence creation error should point to append operations"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(
        before, after,
        "rejected sequence auto-create must not mutate YAML"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_comment_rejects_sequence_index_out_of_bounds() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"# services\nservices:\n  - name: api\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "services[1].name"
        },
        "op": {
            "type": "set",
            "new_text": "\"worker\"",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "YAML create-missing must reject out-of-bounds sequence indexes"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("append operation")),
        "OOB sequence error should point to append operations"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "rejected sequence OOB must not mutate YAML");
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_comment_rejects_explicit_null_mapping_parent() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"# server config\nserver:\n")
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
            "new_text": "9000",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "explicit YAML null mapping value must not be promoted into a mapping"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "rejected null parent must not mutate YAML");
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_comment_rejects_multiline_mapping_fragment() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  # keep service config\n  name: identedit\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.extra"
        },
        "op": {
            "type": "set",
            "new_text": "nested:\n  value: 1\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "comment-preserving YAML create-missing should reject multiline non-block-scalar fragments"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("block scalar")),
        "error should explain the block-scalar-only multiline policy"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "rejected operation must not mutate YAML");
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_comment_rejects_explicit_indent_indicator() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  # keep service config\n  name: identedit\n")
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
            "new_text": "|2\n    echo start\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "comment-preserving YAML create-missing should reject explicit indentation indicators"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("indent indicator")),
        "error should mention the unsupported indentation indicator"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "rejected operation must not mutate YAML");
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_comment_rejects_header_comment() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  # keep service config\n  name: identedit\n")
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
            "new_text": "| # generated script\n  echo start\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "comment-preserving YAML create-missing should keep the explicit block-scalar header policy narrow"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "rejected operation must not mutate YAML");
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_comment_rejects_header_trailing_tab() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  # keep service config\n  name: identedit\n")
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
            "new_text": "|\t\n  echo start\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "comment-preserving YAML create-missing should reject tabbed block-scalar headers"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "rejected operation must not mutate YAML");
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_comment_rejects_tab_only_body_indent() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  # keep service config\n  name: identedit\n")
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
            "new_text": "|\n\techo start\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "comment-preserving YAML create-missing should reject body lines without space indentation"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "rejected operation must not mutate YAML");
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_comment_rejects_malformed_chomp_indicator() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  # keep service config\n  name: identedit\n")
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
            "new_text": "|-+\n  echo start\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "comment-preserving YAML create-missing should reject malformed chomp indicators"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "rejected operation must not mutate YAML");
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_comment_dry_run_block_scalar_does_not_mutate() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"data:\n  # keep data config\n  existing: value\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": r#"data["script: install"]"#
        },
        "op": {
            "type": "set",
            "new_text": "|\n  echo dry-run\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json", "--dry-run"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML block scalar create-missing dry-run should validate successfully: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "dry-run must not mutate YAML");
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["dry_run"]["status"], "ready");
    assert_eq!(response["dry_run"]["preconditions"], "passed");
    assert_eq!(
        response["dry_run"]["would_apply_operations"].as_u64(),
        Some(1)
    );
    assert_eq!(response["dry_run"]["would_modify_files"].as_u64(), Some(1));
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_comment_stale_hash_rejects_block_scalar() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"data:\n  # keep data config\n  existing: value\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": r#"data["script: install"]"#,
            "expected_file_hash": "deadbeefdeadbeef"
        },
        "op": {
            "type": "set",
            "new_text": "|\n  echo stale\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "YAML block scalar create-missing should reject stale expected_file_hash"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "precondition_failed");
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "stale-hash failure must not mutate YAML");
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_comment_rejects_non_ascii_header_whitespace() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  # keep service config\n  name: identedit\n")
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
            "new_text": "|\u{00a0}\n  echo start\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "comment-preserving YAML create-missing should reject non-ASCII block header whitespace"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "rejected operation must not mutate YAML");
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_comment_rejects_multidoc_like_non_block_fragment()
{
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  # keep service config\n  name: identedit\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.fragment"
        },
        "op": {
            "type": "set",
            "new_text": "---\nkind: ConfigMap\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "comment-preserving YAML create-missing should reject multi-document-looking non-block fragments"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "rejected operation must not mutate YAML");
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_comment_only_value_rejects_without_mutation() {
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
            "new_text": "# not a value",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "YAML create-missing should reject comment-only value text instead of creating null"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "rejected operation must not mutate YAML");
}
