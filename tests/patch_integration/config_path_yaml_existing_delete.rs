use super::*;

#[test]
fn patch_json_config_path_set_existing_yaml_sequence_item_value_keeps_following_item() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"steps:\n  - name: setup\n    run: old\n  - name: test\n    run: cargo test\n")
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
            "new_text": "|\n  cargo fmt --check\n  cargo test\n"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "existing YAML sequence item value should accept a block scalar without consuming the following item: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(
        updated.contains("  - name: test\n    run: cargo test"),
        "following sequence item must remain outside the replacement: {updated}"
    );
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(
        parsed["steps"][0]["run"].as_str(),
        Some("cargo fmt --check\ncargo test\n")
    );
    assert_eq!(parsed["steps"][1]["name"].as_str(), Some("test"));
}

#[test]
fn patch_json_config_path_set_existing_yaml_document_marker_preserves_marker() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"---\ndata:\n  script: old\n")
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
            "new_text": "|\n  echo existing marker\n"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML existing value replacement should preserve an explicit document marker: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(
        updated.starts_with("---\n"),
        "document marker should remain outside the root replacement: {updated}"
    );
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(
        parsed["data"]["script"].as_str(),
        Some("echo existing marker\n")
    );
}

#[test]
fn patch_json_config_path_set_existing_yaml_bom_document_marker_preserves_prefix() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all("\u{feff}---\ndata:\n  script: old\n".as_bytes())
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
            "new_text": "|\n  echo existing bom marker\n"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML existing value replacement should preserve BOM plus explicit document marker: {}",
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
    assert_eq!(
        parsed["data"]["script"].as_str(),
        Some("echo existing bom marker\n")
    );
}

#[test]
fn patch_json_config_path_delete_yaml_block_mapping_first_pair_preserves_following_comment() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  remove: yes\n  # keep with remaining key\n  keep: value\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.remove"
        },
        "op": {
            "type": "delete"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML block mapping delete should remove only the selected pair: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(!updated.contains("remove:"));
    assert!(updated.contains("  # keep with remaining key"));
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed["service"]["keep"].as_str(), Some("value"));
}

#[test]
fn patch_json_config_path_delete_yaml_block_sequence_middle_item_preserves_crlf_style() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"items:\r\n  - first\r\n  - remove\r\n  - third\r\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "items[1]"
        },
        "op": {
            "type": "delete"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML block sequence delete should remove only the middle item: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(
        updated.contains("\r\n"),
        "CRLF source style should survive sequence deletion"
    );
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed["items"][0].as_str(), Some("first"));
    assert_eq!(parsed["items"][1].as_str(), Some("third"));
    assert!(parsed["items"].get(2).is_none());
}

#[test]
fn patch_json_config_path_delete_yaml_root_key_preserves_document_end_marker() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"---\nremove: yes\nkeep: value\n...\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "remove"
        },
        "op": {
            "type": "delete"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML root key delete should preserve the explicit document end marker: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(updated.contains("...\n"));
    assert!(!updated.contains("remove:"));
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed["keep"].as_str(), Some("value"));
}

#[test]
fn patch_json_config_path_delete_yaml_only_nested_key_leaves_valid_parent() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  only: value\nnext: keep\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.only"
        },
        "op": {
            "type": "delete"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML delete of the only nested key should leave parseable YAML: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(!updated.contains("only:"));
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert!(parsed["service"].is_null());
    assert_eq!(parsed["next"].as_str(), Some("keep"));
}

#[test]
fn patch_json_config_path_delete_yaml_only_root_key_leaves_empty_file() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"only: value\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "only"
        },
        "op": {
            "type": "delete"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML delete of the only root key should be an empty valid document: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert_eq!(updated, "");
}
