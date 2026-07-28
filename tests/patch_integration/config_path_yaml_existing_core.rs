use super::*;

#[test]
fn patch_json_config_path_set_updates_yaml_value() {
    let file_path = copy_fixture_to_temp_with_suffix("example.yaml", ".yaml");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.retries"
        },
        "op": {
            "type": "set",
            "new_text": "5"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "yaml config path set should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(
        updated.contains("retries: 5"),
        "yaml value should be updated in-place"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_updates_yaml_value() {
    let file_path = copy_fixture_to_temp_with_suffix("example.yaml", ".yaml");

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
        "yaml config path create-missing should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed["service"]["sidecar"]["port"].as_i64(), Some(9000));
}

#[test]
fn patch_json_config_path_set_create_missing_existing_path_preserves_yaml_comments() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  # keep-this-comment\n  retries: 2\n  name: identedit\n")
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
        "yaml existing-path create-missing should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(
        updated.contains("# keep-this-comment"),
        "existing-path create-missing should not drop nearby comments"
    );
    assert!(
        updated.contains("retries: 5"),
        "targeted value should be updated"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_preserves_yaml_comments() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  # keep-this-comment\n  name: identedit\n")
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
        "missing-path YAML create-missing with comments should preserve comments: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(updated.contains("# keep-this-comment"));
    assert!(updated.contains("name: identedit"));
    assert!(updated.contains("port: 9000"));
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed["service"]["sidecar"]["port"].as_i64(), Some(9000));
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_sorted_group_inserts_in_order() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"metadata:\n  alpha: 1\n  beta: 2\n  delta: 4\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "metadata.charlie"
        },
        "op": {
            "type": "set",
            "new_text": "3",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "sorted YAML group insertion should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(
        updated.contains("metadata:\n  alpha: 1\n  beta: 2\n  charlie: 3\n  delta: 4\n"),
        "new key should preserve sorted YAML group order, got:\n{updated}"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_unsorted_group_appends() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"metadata:\n  build: fast\n  test: strict\n  deploy: manual\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "metadata.cache"
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
        "unsorted YAML group insertion should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(
        updated.contains(
            "metadata:\n  build: fast\n  test: strict\n  deploy: manual\n  cache: true\n"
        ),
        "new key should append to unsorted YAML group, got:\n{updated}"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_sorted_insert_preserves_following_key_comment() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"metadata:\n  alpha: 1\n  beta: 2\n  # delta setting\n  delta: 4\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "metadata.charlie"
        },
        "op": {
            "type": "set",
            "new_text": "3",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "comment-owned sorted YAML insertion should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(
        updated.contains(
            "metadata:\n  alpha: 1\n  beta: 2\n  charlie: 3\n  # delta setting\n  delta: 4\n"
        ),
        "new key should be inserted before the following key's leading comment, got:\n{updated}"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_blank_line_group_boundary_is_preserved() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"metadata:\n  alpha: 1\n  beta: 2\n\n  # delta setting\n  delta: 4\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "metadata.charlie"
        },
        "op": {
            "type": "set",
            "new_text": "3",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML insertion should preserve blank-line group boundaries: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(
        updated.contains(
            "metadata:\n  alpha: 1\n  beta: 2\n  charlie: 3\n\n  # delta setting\n  delta: 4\n"
        ),
        "new key should stay in the first group and preserve the following group comment, got:\n{updated}"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_prefix_family_inserts_near_run() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(
            b"service:\n  sidecar_api_host: localhost\n  sidecar_api_tls: false\n  retries: 3\n",
        )
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.sidecar_api_port"
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
        "YAML prefix family insertion should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(
        updated.contains(
            "service:\n  sidecar_api_host: localhost\n  sidecar_api_port: 9000\n  sidecar_api_tls: false\n  retries: 3\n"
        ),
        "new key should be inserted within the sidecar_api prefix family, got:\n{updated}"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_unsorted_prefix_family_appends_to_run_end() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(
            b"service:\n  sidecar_api_tls: false\n  sidecar_api_host: localhost\n  retries: 3\n",
        )
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.sidecar_api_port"
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
        "YAML unsorted prefix-family insertion should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(
        updated.contains(
            "service:\n  sidecar_api_tls: false\n  sidecar_api_host: localhost\n  sidecar_api_port: 9000\n  retries: 3\n"
        ),
        "new key should append to an unsorted prefix run instead of sorting it, got:\n{updated}"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_sequence_item_sorted_group_inserts_in_order() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"items:\n  - alpha: 1\n    beta: 2\n    delta: 4\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "items[0].charlie"
        },
        "op": {
            "type": "set",
            "new_text": "3",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML sequence-item sorted insertion should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(
        updated.contains("items:\n  - alpha: 1\n    beta: 2\n    charlie: 3\n    delta: 4\n"),
        "new key should preserve sorted order inside sequence item mapping, got:\n{updated}"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_blank_line_separated_prefixes_do_not_merge() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(
            b"service:\n  sidecar_api_host: localhost\n\n  sidecar_api_tls: false\n  retries: 3\n",
        )
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.sidecar_api_port"
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
        "YAML separated prefix-family insertion should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(
        updated.contains(
            "service:\n  sidecar_api_host: localhost\n\n  sidecar_api_tls: false\n  retries: 3\n  sidecar_api_port: 9000\n"
        ),
        "blank-line separated prefix keys should not be merged into one inferred run, got:\n{updated}"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_cr_only_blank_line_group_boundary() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"metadata:\r  alpha: 1\r  beta: 2\r\r  # delta setting\r  delta: 4\r")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "metadata.charlie"
        },
        "op": {
            "type": "set",
            "new_text": "3",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML CR-only blank-line group insertion should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(
        updated.contains(
            "metadata:\r  alpha: 1\r  beta: 2\r  charlie: 3\r\r  # delta setting\r  delta: 4\r"
        ),
        "new key should preserve CR-only blank-line group boundary, got:\n{updated:?}"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_quoted_keys_participate_in_sorted_group() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"metadata:\n  alpha: 1\n  \"beta key\": 2\n  delta: 4\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "metadata[\"charlie key\"]"
        },
        "op": {
            "type": "set",
            "new_text": "3",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML quoted-key sorted insertion should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(
        updated
            .contains("metadata:\n  alpha: 1\n  \"beta key\": 2\n  charlie key: 3\n  delta: 4\n"),
        "decoded quoted keys should participate in sorted placement without forcing unnecessary YAML quotes, got:\n{updated}"
    );
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should parse");
    assert_eq!(parsed["metadata"]["charlie key"].as_i64(), Some(3));
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_nested_comment_owned_sorted_insert() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"outer:\n  metadata:\n    alpha: 1\n    beta: 2\n    # delta setting\n    delta: 4\n  tail: true\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "outer.metadata.charlie"
        },
        "op": {
            "type": "set",
            "new_text": "3",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML nested comment-owned sorted insertion should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(
        updated.contains(
            "outer:\n  metadata:\n    alpha: 1\n    beta: 2\n    charlie: 3\n    # delta setting\n    delta: 4\n  tail: true\n"
        ),
        "new key should stay inside nested mapping and before the owned comment block, got:\n{updated}"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_adds_root_yaml_key_with_leading_comment() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"# top-level comment\nservice:\n  name: identedit\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "sidecar_enabled"
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
        "root YAML create-missing with leading comments should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(updated.starts_with("# top-level comment\n"));
    assert!(updated.contains("service:\n  name: identedit\n"));
    assert!(updated.contains("sidecar_enabled: true"));
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed["sidecar_enabled"].as_bool(), Some(true));
}

#[test]
fn patch_json_config_path_set_existing_yaml_comment_value_keeps_yaml_like_scalar_lines() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"data:\n  # keep data config\n  fragment: old\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "data.fragment"
        },
        "op": {
            "type": "set",
            "new_text": "|\n  key: value\n  - item\n"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "existing YAML value set should preserve YAML-looking lines as scalar content: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(
        parsed["data"]["fragment"].as_str(),
        Some("key: value\n- item\n")
    );
}

#[test]
fn patch_json_config_path_set_create_missing_existing_yaml_path_preserves_comment() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  # keep-this-comment\n  retries: 2\n")
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
        "existing YAML path create-missing should still succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(updated.contains("# keep-this-comment"));
    assert!(updated.contains("retries: 5"));
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_hyphen_prefix_family_preserves_comment_owner() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(
            b"service:\n  sidecar-api-host: localhost\n  # TLS setting\n  sidecar-api-tls: false\n  retries: 3\n",
        )
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service[\"sidecar-api-port\"]"
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
        "YAML hyphen-prefix family insertion should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(
        updated.contains(
            "service:\n  sidecar-api-host: localhost\n  sidecar-api-port: 9000\n  # TLS setting\n  sidecar-api-tls: false\n  retries: 3\n"
        ),
        "new hyphen-family key should insert before the following key's owned comment, got:\n{updated}"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_sequence_item_prefix_family_preserves_comment_owner()
 {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(
            b"items:\n  - sidecar_api_host: localhost\n    # TLS setting\n    sidecar_api_tls: false\n    retries: 3\n",
        )
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "items[0].sidecar_api_port"
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
        "YAML sequence item prefix-family insertion should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(
        updated.contains(
            "items:\n  - sidecar_api_host: localhost\n    sidecar_api_port: 9000\n    # TLS setting\n    sidecar_api_tls: false\n    retries: 3\n"
        ),
        "sequence-item prefix family should preserve the following key's owned comment, got:\n{updated}"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_root_sorted_group_before_document_end_marker() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"---\nalpha: 1\nbeta: 2\ndelta: 4\n...\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "charlie"
        },
        "op": {
            "type": "set",
            "new_text": "3",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML root sorted group insertion before document end should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(
        updated.contains("---\nalpha: 1\nbeta: 2\ncharlie: 3\ndelta: 4\n...\n"),
        "root sorted insertion should stay before delta and document end marker, got:\n{updated}"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_prefix_family_before_first_owned_comment() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(
            b"service:\n  # host setting\n  sidecar_api_host: localhost\n  sidecar_api_tls: false\n  retries: 3\n",
        )
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.sidecar_api_enabled"
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
        "YAML prefix-family insertion before first entry should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(
        updated.contains(
            "service:\n  sidecar_api_enabled: true\n  # host setting\n  sidecar_api_host: localhost\n  sidecar_api_tls: false\n  retries: 3\n"
        ),
        "insertion before the first prefix-family entry should preserve that entry's owned comment, got:\n{updated}"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_prefix_family_after_run_before_unrelated_key() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(
            b"service:\n  sidecar_api_host: localhost\n  sidecar_api_tls: false\n  sidecar_cache_host: localhost\n",
        )
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.sidecar_api_url"
        },
        "op": {
            "type": "set",
            "new_text": "http://localhost",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML prefix-family insertion after run should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(
        updated.contains(
            "service:\n  sidecar_api_host: localhost\n  sidecar_api_tls: false\n  sidecar_api_url: http://localhost\n  sidecar_cache_host: localhost\n"
        ),
        "prefix-family insertion after the run should not drift past the next family, got:\n{updated}"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_selects_later_sorted_group() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"metadata:\n  alpha: 1\n  beta: 2\n\n  omega: 24\n  zeta: 26\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "metadata.sigma"
        },
        "op": {
            "type": "set",
            "new_text": "25",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML insertion into later sorted group should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(
        updated
            .contains("metadata:\n  alpha: 1\n  beta: 2\n\n  omega: 24\n  sigma: 25\n  zeta: 26\n"),
        "new key should choose the sorted group whose bounds contain it, got:\n{updated}"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_supports_yaml_quoted_key_segments_with_comments() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"\"x.y\":\n  # keep-this-comment\n  existing: 1\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": r#"["x.y"]["build/test"]"#
        },
        "op": {
            "type": "set",
            "new_text": "\"ok\"",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "quoted YAML path create-missing should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(updated.contains("# keep-this-comment"));
    assert!(updated.contains("build/test: \"ok\""));
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(
        parsed["x.y"]["build/test"].as_str(),
        Some("ok"),
        "quoted path segment should address the literal YAML key"
    );
}

#[test]
fn patch_json_config_path_yaml_comment_create_missing_preserves_file_context() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  # keep-this-comment\n  name: identedit\n")
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
        "operation should preserve commented YAML: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert!(after.contains("# keep-this-comment"));
    assert!(after.contains("name: identedit"));
    assert!(after.contains("port: 9000"));
}

#[test]
fn patch_json_config_path_missing_path_without_create_missing_bypasses_yaml_comment_guard() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  # keep-this-comment\n  name: identedit\n")
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
            "new_text": "9000"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(!output.status.success(), "operation should fail");
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("was not found")),
        "missing path without create-missing should keep strict missing-path diagnostic"
    );
}

#[test]
fn patch_flag_config_path_create_missing_preserves_yaml_comment_fallback() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  # keep-this-comment\n  name: identedit\n")
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
        output.status.success(),
        "flag-mode YAML fallback with comments should preserve comments: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(updated.contains("# keep-this-comment"));
    assert!(updated.contains("name: identedit"));
    assert!(updated.contains("port: 9000"));
}

#[test]
fn patch_flag_config_path_create_missing_existing_yaml_path_preserves_comment() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  # keep-this-comment\n  retries: 2\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let output = run_identedit(&[
        "patch",
        "--config-path",
        "service.retries",
        "--set-value",
        "5",
        "--create-missing",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "existing YAML path should still use strict rewrite path: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(updated.contains("# keep-this-comment"));
    assert!(updated.contains("retries: 5"));
}

#[test]
fn patch_flag_config_path_create_missing_yaml_comment_keeps_existing_lines() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  # keep-this-comment\n  name: identedit\n")
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
        output.status.success(),
        "operation should preserve commented YAML: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(updated.contains("  # keep-this-comment\n  name: identedit\n"));
    assert!(updated.contains("  port: 9000"));
}

#[test]
fn patch_flag_yaml_comment_create_missing_preserves_file_context() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  # keep-this-comment\n  name: identedit\n")
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
        output.status.success(),
        "operation should preserve commented YAML: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert!(after.contains("# keep-this-comment"));
    assert!(after.contains("name: identedit"));
    assert!(after.contains("port: 9000"));
}

#[test]
fn patch_json_create_missing_existing_yaml_path_with_hash_precondition_preserves_comment() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  # keep-this-comment\n  retries: 2\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.retries",
            "expected_file_hash": crate::common::hash_text(&before)
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
    assert!(updated.contains("# keep-this-comment"));
    assert!(updated.contains("retries: 5"));
}

#[test]
fn patch_json_create_missing_existing_yaml_path_stale_hash_fails_precondition_no_mutation() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  # keep-this-comment\n  retries: 2\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.retries",
            "expected_file_hash": "deadbeefdeadbeef"
        },
        "op": {
            "type": "set",
            "new_text": "5",
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
fn patch_json_create_missing_yaml_comment_fallback_with_stale_hash_fails_precondition_first() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  # keep-this-comment\n  name: identedit\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.sidecar.port",
            "expected_file_hash": "deadbeefdeadbeef"
        },
        "op": {
            "type": "set",
            "new_text": "9000",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(!output.status.success(), "operation should fail");
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "precondition_failed");
}

#[test]
fn patch_json_config_path_append_appends_yaml_block_sequence() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  tags:\n    - api\n    - worker\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.tags"
        },
        "op": {
            "type": "append",
            "new_text": "batch"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "yaml block-sequence append should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(
        parsed["service"]["tags"],
        serde_yaml::to_value(vec!["api", "worker", "batch"]).expect("yaml list should serialize")
    );
}

#[test]
fn patch_json_config_path_set_create_missing_whitespace_only_yaml_preserves_prefix() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    let source = " \n\n";
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
        "whitespace-only YAML create-missing should bootstrap a mapping: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(
        updated.starts_with(source),
        "YAML whitespace prefix should be preserved, got:\n{updated:?}"
    );
    assert!(updated.contains("server:\n  port: 9090"));
}

#[test]
fn patch_json_config_path_set_create_missing_whitespace_only_yaml_with_stale_hash_fails() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    let source = " \n\n";
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
        "stale hash must reject whitespace-only YAML create-missing"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "precondition_failed");
    let updated = fs::read_to_string(&file_path).expect("YAML fixture should be readable");
    assert_eq!(
        updated, source,
        "stale hash failure must not mutate whitespace-only YAML"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_bom_whitespace_only_yaml_preserves_prefix() {
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
        "BOM+whitespace-only YAML create-missing should bootstrap mapping: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(
        updated.starts_with(source),
        "BOM+whitespace YAML prefix should be preserved, got:\n{updated:?}"
    );
    assert!(updated.contains("server:\n  port: 9090"));
}

#[test]
fn patch_json_config_path_set_create_missing_crlf_whitespace_only_yaml_preserves_prefix() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    let source = " \r\n\r\n";
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
        "CRLF whitespace-only YAML create-missing should bootstrap mapping: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(
        updated.starts_with(source),
        "CRLF YAML whitespace prefix should be preserved, got:\n{updated:?}"
    );
    assert!(updated.contains("server:\r\n  port: 9090"));
}

#[test]
fn patch_json_config_path_set_create_missing_whitespace_only_yaml_root_leaf() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    let source = "\n";
    temp_file
        .write_all(source.as_bytes())
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

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
        output.status.success(),
        "whitespace-only YAML root leaf create should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(updated.starts_with(source));
    assert!(updated.contains("enabled: true"));
}

#[test]
fn patch_json_config_path_set_create_missing_whitespace_only_yaml_with_exact_hash_succeeds() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    let source = " \n\n";
    temp_file
        .write_all(source.as_bytes())
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "server.enabled",
            "expected_file_hash": crate::common::hash_text(source)
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
        "whitespace-only YAML exact-hash create should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(updated.starts_with(source));
    assert!(updated.contains("server:\n  enabled: true"));
}

#[test]
fn patch_json_config_path_set_create_missing_whitespace_only_yaml_sets_sequence_value() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    let source = "\n";
    temp_file
        .write_all(source.as_bytes())
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "server.ports"
        },
        "op": {
            "type": "set",
            "new_text": "[8000, 9000]",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "whitespace-only YAML should accept sequence-valued create-missing leaf: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(updated.starts_with(source));
    assert!(updated.contains("ports: [8000, 9000]"));
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed["server"]["ports"][0].as_i64(), Some(8000));
    assert_eq!(parsed["server"]["ports"][1].as_i64(), Some(9000));
}

#[test]
fn patch_json_config_path_set_existing_yaml_block_sequence_scalar_trailing_spaces() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"items:\n  - old   \n  - keep\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "items[0]"
        },
        "op": {
            "type": "set",
            "new_text": "|\n  first line\n  second line\n"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML sequence scalar replacement should consume trailing spaces without affecting the next item: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(
        parsed["items"][0].as_str(),
        Some("first line\nsecond line\n")
    );
    assert_eq!(parsed["items"][1].as_str(), Some("keep"));
}

#[test]
fn patch_json_config_path_set_existing_yaml_literal_block_replaced_by_literal_block() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"data:\n  script: |\n    old line\n")
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
            "new_text": "|\n  new line\n  second line\n"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "existing YAML literal block value should be replaceable with another literal block: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(
        parsed["data"]["script"].as_str(),
        Some("new line\nsecond line\n")
    );
}

#[test]
fn patch_json_config_path_set_existing_yaml_scalar_preserves_inline_comment() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  enabled: true # keep operator note\n  retries: 3\n")
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
            "new_text": "false"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML scalar replacement should preserve same-line comments: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(
        updated.contains("enabled: false # keep operator note"),
        "inline comment must stay outside the replaced scalar"
    );
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed["service"]["enabled"].as_bool(), Some(false));
    assert_eq!(parsed["service"]["retries"].as_i64(), Some(3));
}

#[test]
fn patch_json_config_path_set_existing_yaml_scalar_text_file_crlf_preserves_inline_comment() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  enabled: true # keep comment\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let payload_path = create_temp_text_file("false\r\n");

    let output = run_identedit(&[
        "patch",
        file_path.to_str().expect("path should be utf-8"),
        "--config-path",
        "service.enabled",
        "--set-value",
        "--text-file",
        payload_path.to_str().expect("payload path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "YAML single-line text-file replacement should trim one CRLF and preserve inline comments: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(updated.contains("enabled: false # keep comment"));
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed["service"]["enabled"].as_bool(), Some(false));
}

#[test]
fn patch_json_config_path_append_yaml_block_sequence_no_final_newline() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"items:\n  - old")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "items"
        },
        "op": {
            "type": "append",
            "new_text": "new"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML block sequence append should work when the source has no final newline: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert_eq!(updated, "items:\n  - old\n  - new");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed["items"][1].as_str(), Some("new"));
}

#[test]
fn patch_json_config_path_set_existing_yaml_quoted_comment_like_string_succeeds() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  script: old\n")
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
            "new_text": "\"# literal string\""
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "quoted YAML comment-like strings should remain valid scalar values: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(
        parsed["service"]["script"].as_str(),
        Some("# literal string")
    );
}
