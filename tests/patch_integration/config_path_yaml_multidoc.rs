use super::*;

#[test]
fn patch_json_config_path_set_create_missing_rejects_yaml_multi_document() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"---\nservice:\n  name: identedit\n---\nmetadata:\n  owner: team\n")
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
        "yaml multi-document create-missing should be rejected"
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("multiple YAML documents")),
        "error should explain multi-document create-missing limitation"
    );
}

#[test]
fn patch_json_config_path_yaml_multi_document_rejection_does_not_mutate_file() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"---\nservice:\n  name: identedit\n---\nmetadata:\n  owner: team\n")
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
    assert_eq!(
        before, after,
        "multi-document rejection must not mutate file"
    );
}

#[test]
fn patch_flag_config_path_create_missing_rejects_yaml_multi_document() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"---\nservice:\n  name: identedit\n---\nmetadata:\n  owner: team\n")
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
        "flag-mode YAML multi-document fallback should be rejected"
    );
}

#[test]
fn patch_json_config_path_create_missing_yaml_multi_document_selected_first_doc() {
    let file_path = create_temp_yaml_source(
        "---\nmetadata:\n  name: first\n---\nmetadata:\n  name: middle\n---\nmetadata:\n  name: last\n",
    );
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let output =
        patch_yaml_config_path_document(&file_path, "metadata.labels.owner", "\"platform\"", 0);
    assert!(
        output.status.success(),
        "create-missing in first YAML document should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(updated.contains(
        "---\nmetadata:\n  name: first\n  labels:\n    owner: \"platform\"\n---\nmetadata:\n  name: middle\n"
    ));
    assert!(
        updated.ends_with("---\nmetadata:\n  name: last\n"),
        "later documents should be preserved byte-for-byte aside from shifted offset"
    );
    assert_ne!(updated, before, "selected document should be modified");
}

#[test]
fn patch_json_config_path_create_missing_yaml_multi_document_selected_middle_doc() {
    let file_path = create_temp_yaml_source(
        "---\nmetadata:\n  name: first\n---\nmetadata:\n  name: middle\n---\nmetadata:\n  name: last\n",
    );

    let output =
        patch_yaml_config_path_document(&file_path, "metadata.labels.owner", "\"platform\"", 1);
    assert!(
        output.status.success(),
        "create-missing in middle YAML document should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(updated.starts_with("---\nmetadata:\n  name: first\n---\n"));
    assert!(updated.contains(
        "---\nmetadata:\n  name: middle\n  labels:\n    owner: \"platform\"\n---\nmetadata:\n  name: last\n"
    ));
}

#[test]
fn patch_json_config_path_create_missing_yaml_multi_document_selected_last_doc() {
    let file_path = create_temp_yaml_source(
        "---\nmetadata:\n  name: first\n---\nmetadata:\n  name: middle\n---\nmetadata:\n  name: last\n",
    );

    let output =
        patch_yaml_config_path_document(&file_path, "metadata.labels.owner", "\"platform\"", 2);
    assert!(
        output.status.success(),
        "create-missing in last YAML document should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(
        updated.starts_with("---\nmetadata:\n  name: first\n---\nmetadata:\n  name: middle\n---\n")
    );
    assert!(updated.ends_with("metadata:\n  name: last\n  labels:\n    owner: \"platform\"\n"));
}

#[test]
fn patch_flag_config_path_create_missing_yaml_multi_document_selected_middle_doc() {
    let file_path = create_temp_yaml_source(
        "---\nmetadata:\n  name: first\n---\nmetadata:\n  name: middle\n---\nmetadata:\n  name: last\n",
    );

    let output = run_identedit(&[
        "patch",
        "--config-path",
        "metadata.labels.owner",
        "--document-index",
        "1",
        "--set-value",
        "\"platform\"",
        "--create-missing",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "flag-mode create-missing in selected YAML document should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(updated.contains("name: middle\n  labels:\n    owner: \"platform\""));
    assert!(!updated.contains("name: first\n  labels:"));
    assert!(!updated.contains("name: last\n  labels:"));
}

#[test]
fn patch_json_config_path_create_missing_yaml_multi_document_out_of_range_document_rejects() {
    let file_path =
        create_temp_yaml_source("---\nmetadata:\n  name: first\n---\nmetadata:\n  name: last\n");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let output =
        patch_yaml_config_path_document(&file_path, "metadata.labels.owner", "\"platform\"", 2);
    assert!(
        !output.status.success(),
        "out-of-range YAML document_index should fail"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("document_index 2"))
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "rejected edit must not mutate YAML source");
}

#[test]
fn patch_json_config_path_create_missing_yaml_multi_document_stale_precondition_rejects() {
    let file_path =
        create_temp_yaml_source("---\nmetadata:\n  name: first\n---\nmetadata:\n  name: last\n");
    let stale_hash = identedit::hash::hash_bytes(
        fs::read(&file_path)
            .expect("fixture should read")
            .as_slice(),
    );
    fs::write(
        &file_path,
        "---\nmetadata:\n  name: first\n---\nmetadata:\n  name: changed\n",
    )
    .expect("fixture update should succeed");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "metadata.labels.owner",
            "document_index": 1,
            "expected_file_hash": stale_hash
        },
        "op": {
            "type": "set",
            "new_text": "\"platform\"",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "stale expected_file_hash should reject before mutation"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "precondition_failed");
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "stale precondition must not mutate file");
}

#[test]
fn patch_json_config_path_yaml_multi_document_index_disambiguates_existing_path() {
    let file_path =
        create_temp_yaml_source("---\nmetadata:\n  owner: app\n---\nmetadata:\n  owner: ops\n");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "metadata.owner",
            "document_index": 1
        },
        "op": {
            "type": "set",
            "new_text": "platform"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "document_index should disambiguate an existing duplicate YAML path: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(updated.contains("owner: app"));
    assert!(updated.contains("owner: platform"));
    assert!(!updated.contains("owner: ops"));
}

#[test]
fn patch_json_config_path_create_missing_yaml_multi_document_selected_doc_ignores_other_existing_path()
 {
    let file_path = create_temp_yaml_source(
        "---\nmetadata:\n  name: first\n---\nmetadata:\n  owner: existing\n",
    );

    let output = patch_yaml_config_path_document(&file_path, "metadata.owner", "\"created\"", 0);
    assert!(
        output.status.success(),
        "document_index should constrain create-missing strict probe to the selected YAML document: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(updated.contains("metadata:\n  name: first\n  owner: \"created\""));
    assert!(updated.contains("owner: existing"));
}

#[test]
fn patch_json_config_path_create_missing_yaml_multi_document_selected_doc_preserves_document_end_marker()
 {
    let file_path = create_temp_yaml_source(
        "---\nmetadata:\n  name: first\n...\n---\nmetadata:\n  name: second\n",
    );

    let output = patch_yaml_config_path_document(&file_path, "metadata.owner", "\"platform\"", 0);
    assert!(
        output.status.success(),
        "create-missing before document end marker should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(updated.contains(
        "---\nmetadata:\n  name: first\n  owner: \"platform\"\n...\n---\nmetadata:\n  name: second\n"
    ));
}

#[test]
fn patch_json_config_path_create_missing_yaml_multi_document_selected_empty_doc_rejects_without_mutation()
 {
    let file_path = create_temp_yaml_source(
        "---\nmetadata:\n  name: first\n---\n---\nmetadata:\n  name: last\n",
    );
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let output = patch_yaml_config_path_document(&file_path, "metadata.owner", "\"platform\"", 1);
    assert!(
        !output.status.success(),
        "selected empty YAML document should not silently fall through to another document"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("document_index 1 has no root value"))
    );
    let after = fs::read_to_string(&file_path).expect("fixture should remain readable");
    assert_eq!(before, after, "rejected edit must not mutate YAML stream");
}

#[test]
fn patch_json_config_path_create_missing_yaml_multi_document_selected_doc_allows_unrelated_anchor()
{
    let file_path = create_temp_yaml_source(
        "---\nmetadata:\n  name: first\n---\ndefaults: &defaults\n  owner: team\n",
    );

    let output = patch_yaml_config_path_document(&file_path, "metadata.owner", "\"platform\"", 0);
    assert!(
        output.status.success(),
        "YAML anchor in an unselected document should not block create-missing in the selected document: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(
        updated.contains(
            "---\nmetadata:\n  name: first\n  owner: \"platform\"\n---\ndefaults: &defaults\n  owner: team\n"
        ),
        "selected document should be updated while unrelated anchor document is preserved, got:\n{updated}"
    );
}

#[test]
fn patch_json_config_path_create_missing_yaml_multi_document_selected_middle_doc_preserves_crlf_stream()
 {
    let file_path = create_temp_yaml_source(
        "---\r\nmetadata:\r\n  name: first\r\n---\r\nmetadata:\r\n  name: middle\r\n---\r\nmetadata:\r\n  name: last\r\n",
    );

    let output =
        patch_yaml_config_path_document(&file_path, "metadata.labels.owner", "\"platform\"", 1);
    assert!(
        output.status.success(),
        "CRLF multi-document create-missing should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let updated = fs::read(&file_path).expect("updated YAML should be readable");
    assert!(
        updated.windows(2).any(|pair| pair == b"\r\n"),
        "updated stream should retain CRLF newlines"
    );
    for (index, byte) in updated.iter().enumerate() {
        if *byte == b'\n' {
            assert!(
                index > 0 && updated[index - 1] == b'\r',
                "every newline should remain CRLF, found lone LF at byte {index}"
            );
        }
    }
    let updated_text = String::from_utf8(updated).expect("updated YAML should be UTF-8");
    assert!(updated_text.contains("name: middle\r\n  labels:\r\n    owner: \"platform\"\r\n---"));
}

#[test]
fn patch_json_config_path_create_missing_yaml_multi_document_bom_prefixed_selected_second_doc_preserves_bom()
 {
    let file_path = create_temp_yaml_source(
        "\u{feff}---\nmetadata:\n  name: first\n---\nmetadata:\n  name: second\n",
    );

    let output = patch_yaml_config_path_document(&file_path, "metadata.owner", "\"platform\"", 1);
    assert!(
        output.status.success(),
        "BOM-prefixed multi-document create-missing should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let updated = fs::read(&file_path).expect("updated YAML should be readable");
    assert!(
        updated.starts_with(&[0xef, 0xbb, 0xbf]),
        "updated YAML should preserve UTF-8 BOM prefix"
    );
    let updated_text = String::from_utf8(updated).expect("updated YAML should be UTF-8");
    assert!(updated_text.contains("name: second\n  owner: \"platform\""));
}

#[test]
fn patch_json_config_path_create_missing_yaml_multi_document_selected_sequence_item_mapping() {
    let file_path = create_temp_yaml_source(
        "---\nmetadata:\n  name: first\n---\n- metadata:\n    name: one\n- metadata:\n    name: two\n",
    );

    let output =
        patch_yaml_config_path_document(&file_path, "[1].metadata.labels.owner", "\"platform\"", 1);
    assert!(
        output.status.success(),
        "create-missing inside selected YAML sequence item mapping should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(
        updated.contains("- metadata:\n    name: two\n    labels:\n      owner: \"platform\"\n")
    );
    assert!(updated.contains("metadata:\n  name: first"));
}

#[test]
fn patch_json_config_path_create_missing_yaml_multi_document_selected_doc_duplicate_parent_rejects()
{
    let file_path = create_temp_yaml_source(
        "---\nmetadata:\n  name: first\n---\nmetadata:\n  name: one\nmetadata:\n  name: two\n",
    );
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let output = patch_yaml_config_path_document(&file_path, "metadata.owner", "\"platform\"", 1);
    assert!(
        !output.status.success(),
        "duplicate parent in selected YAML document should reject instead of choosing one"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("ambiguous"))
    );
    let after = fs::read_to_string(&file_path).expect("fixture should remain readable");
    assert_eq!(
        before, after,
        "ambiguous parent must not mutate YAML stream"
    );
}

#[test]
fn patch_json_config_path_append_yaml_multi_document_index_disambiguates_duplicate_sequences() {
    let file_path = create_temp_yaml_source("---\nitems:\n  - app\n---\nitems:\n  - ops\n");
    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "items",
            "document_index": 0
        },
        "op": {
            "type": "append",
            "new_text": "platform"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "document_index should disambiguate append targets across YAML documents: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(updated.contains("items:\n  - app\n  - platform\n---"));
    assert!(updated.contains("items:\n  - ops\n"));
}

#[test]
fn patch_json_config_path_delete_yaml_multi_document_index_disambiguates_duplicate_keys() {
    let file_path = create_temp_yaml_source(
        "---\nmetadata:\n  owner: app\n  region: us\n---\nmetadata:\n  owner: ops\n  region: eu\n",
    );
    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "metadata.owner",
            "document_index": 1
        },
        "op": {
            "type": "delete"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "document_index should disambiguate delete targets across YAML documents: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(updated.contains("owner: app"));
    assert!(!updated.contains("owner: ops"));
    assert!(updated.contains("region: eu"));
}

#[test]
fn patch_json_config_path_create_missing_yaml_single_document_accepts_document_index_zero() {
    let file_path = create_temp_yaml_source("metadata:\n  name: single\n");

    let output =
        patch_yaml_config_path_document(&file_path, "metadata.labels.owner", "\"platform\"", 0);
    assert!(
        output.status.success(),
        "document_index 0 should be valid for a single YAML document: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(updated.contains("metadata:\n  name: single\n  labels:\n    owner: \"platform\"\n"));
}

#[test]
fn patch_json_config_path_create_missing_yaml_multi_document_implicit_first_document_selected_second()
 {
    let file_path =
        create_temp_yaml_source("metadata:\n  name: implicit\n---\nmetadata:\n  name: explicit\n");

    let output =
        patch_yaml_config_path_document(&file_path, "metadata.labels.owner", "\"platform\"", 1);
    assert!(
        output.status.success(),
        "document_index should count an implicit first YAML document: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(updated.starts_with("metadata:\n  name: implicit\n---\n"));
    assert!(updated.contains("metadata:\n  name: explicit\n  labels:\n    owner: \"platform\""));
}

#[test]
fn patch_json_config_path_create_missing_yaml_multi_document_selected_flow_mapping_rejects_without_mutation()
 {
    let file_path =
        create_temp_yaml_source("---\nmetadata:\n  name: first\n---\nmetadata: { name: flow }\n");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let output = patch_yaml_config_path_document(&file_path, "metadata.owner", "\"platform\"", 1);
    assert!(
        !output.status.success(),
        "selected flow mapping document should not be rewritten by block create-missing"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let after = fs::read_to_string(&file_path).expect("fixture should remain readable");
    assert_eq!(
        before, after,
        "rejected flow-root edit must not mutate YAML stream"
    );
}

#[test]
fn patch_json_config_path_create_missing_yaml_multi_document_selected_scalar_root_rejects_without_mutation()
 {
    let file_path =
        create_temp_yaml_source("---\nmetadata:\n  name: first\n---\nmetadata: disabled\n");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let output = patch_yaml_config_path_document(&file_path, "metadata.owner", "\"platform\"", 1);
    assert!(
        !output.status.success(),
        "selected scalar parent should not be auto-converted into a mapping"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let after = fs::read_to_string(&file_path).expect("fixture should remain readable");
    assert_eq!(
        before, after,
        "rejected scalar-root edit must not mutate YAML stream"
    );
}

#[test]
fn patch_json_config_path_document_index_rejects_non_integer_json_value_without_mutation() {
    let file_path =
        create_temp_yaml_source("---\nmetadata:\n  name: first\n---\nmetadata:\n  name: second\n");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "metadata.owner",
            "document_index": "1"
        },
        "op": {
            "type": "set",
            "new_text": "\"platform\"",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "non-integer document_index should be rejected by JSON schema"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let after = fs::read_to_string(&file_path).expect("fixture should remain readable");
    assert_eq!(
        before, after,
        "schema rejection must not mutate YAML stream"
    );
}

#[test]
fn patch_flag_document_index_rejects_non_config_target_mode_without_mutation() {
    let file_path = create_temp_yaml_source("metadata:\n  name: single\n");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let output = run_identedit(&[
        "patch",
        "--at",
        "file-end",
        "--document-index",
        "0",
        "--insert",
        "\nmetadata:\n  owner: platform\n",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "document_index should be rejected outside config path flag mode"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let after = fs::read_to_string(&file_path).expect("fixture should remain readable");
    assert_eq!(before, after, "mode rejection must not mutate YAML file");
}

#[test]
fn patch_json_config_path_create_missing_yaml_multi_document_selected_commented_doc_preserves_comments()
 {
    let file_path = create_temp_yaml_source(
        "---\nmetadata:\n  name: first\n---\n# middle doc\nmetadata:\n  # keep name\n  name: middle\n# keep tail\n---\nmetadata:\n  name: last\n",
    );

    let output =
        patch_yaml_config_path_document(&file_path, "metadata.labels.owner", "\"platform\"", 1);
    assert!(
        output.status.success(),
        "create-missing in selected commented YAML document should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(updated.contains("# middle doc\nmetadata:\n  # keep name\n  name: middle\n  labels:\n    owner: \"platform\"\n# keep tail\n---"));
    assert!(updated.contains("metadata:\n  name: first"));
    assert!(updated.contains("metadata:\n  name: last"));
}

#[test]
fn patch_json_config_path_create_missing_yaml_multi_document_with_yaml_directive_selected_second_doc()
 {
    let file_path = create_temp_yaml_source(
        "%YAML 1.2\n---\nmetadata:\n  name: first\n---\nmetadata:\n  name: second\n",
    );

    let output =
        patch_yaml_config_path_document(&file_path, "metadata.labels.owner", "\"platform\"", 1);
    assert!(
        output.status.success(),
        "document_index should count YAML documents even when the stream has a directive: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(updated.starts_with("%YAML 1.2\n---\nmetadata:\n  name: first\n---\n"));
    assert!(updated.contains("metadata:\n  name: second\n  labels:\n    owner: \"platform\""));
}

#[test]
fn patch_json_config_path_create_missing_yaml_multi_document_selected_doc_moves_before_blank_outdented_tail_comment()
 {
    let file_path = create_temp_yaml_source(
        "---\nmetadata:\n  name: first\n---\nmetadata:\n  name: middle\n\n# keep tail\n---\nmetadata:\n  name: last\n",
    );

    let output =
        patch_yaml_config_path_document(&file_path, "metadata.labels.owner", "\"platform\"", 1);
    assert!(
        output.status.success(),
        "create-missing should insert before blank/outdented tail comments in selected document: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(updated.contains(
        "metadata:\n  name: middle\n  labels:\n    owner: \"platform\"\n\n# keep tail\n---"
    ));
}

#[test]
fn patch_json_config_path_create_missing_yaml_multi_document_selected_doc_keeps_indented_parent_tail_comment()
 {
    let file_path = create_temp_yaml_source(
        "---\nmetadata:\n  name: first\n---\nmetadata:\n  name: middle\n  # keep inside metadata\n---\nmetadata:\n  name: last\n",
    );

    let output =
        patch_yaml_config_path_document(&file_path, "metadata.labels.owner", "\"platform\"", 1);
    assert!(
        output.status.success(),
        "create-missing should preserve indented parent comments inside selected mapping: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(updated.contains(
        "metadata:\n  name: middle\n  # keep inside metadata\n  labels:\n    owner: \"platform\"\n---"
    ));
}

#[test]
fn patch_json_config_path_create_missing_yaml_multi_document_exact_hash_precondition_succeeds() {
    let file_path =
        create_temp_yaml_source("---\nmetadata:\n  name: first\n---\nmetadata:\n  name: second\n");
    let expected_file_hash = identedit::hash::hash_bytes(
        fs::read(&file_path)
            .expect("fixture should read")
            .as_slice(),
    );
    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "metadata.owner",
            "document_index": 1,
            "expected_file_hash": expected_file_hash
        },
        "op": {
            "type": "set",
            "new_text": "\"platform\"",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "exact file hash precondition should allow selected-document create-missing: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(updated.contains("name: second\n  owner: \"platform\""));
}

#[test]
fn patch_flag_config_path_create_missing_yml_extension_selected_second_doc() {
    let mut temp_file = Builder::new()
        .suffix(".yml")
        .tempfile()
        .expect("temp yml file should be created");
    temp_file
        .write_all(b"---\nmetadata:\n  name: first\n---\nmetadata:\n  name: second\n")
        .expect("yml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let output = run_identedit(&[
        "patch",
        "--config-path",
        "metadata.owner",
        "--document-index",
        "1",
        "--set-value",
        "\"platform\"",
        "--create-missing",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        ".yml extension should support selected-document create-missing: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(updated.contains("name: second\n  owner: \"platform\""));
}

#[test]
fn patch_json_config_path_create_missing_yaml_multi_document_preserves_multiple_blank_lines_before_tail_comment()
 {
    let file_path = create_temp_yaml_source(
        "---\nmetadata:\n  name: first\n---\nmetadata:\n  name: middle\n\n\n# keep tail\n---\nmetadata:\n  name: last\n",
    );

    let output =
        patch_yaml_config_path_document(&file_path, "metadata.labels.owner", "\"platform\"", 1);
    assert!(
        output.status.success(),
        "create-missing should preserve multiple blank separators before a tail comment: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(updated.contains(
        "metadata:\n  name: middle\n  labels:\n    owner: \"platform\"\n\n\n# keep tail\n---"
    ));
}

#[test]
fn patch_json_config_path_create_missing_yaml_multi_document_nested_parent_before_sibling_key() {
    let file_path = create_temp_yaml_source(
        "---\nspec:\n  template:\n    name: first\n  replicas: 1\n---\nspec:\n  template:\n    name: second\n  replicas: 2\n",
    );

    let output = patch_yaml_config_path_document(
        &file_path,
        "spec.template.labels.owner",
        "\"platform\"",
        1,
    );
    assert!(
        output.status.success(),
        "create-missing should insert inside the selected nested parent before its sibling key: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(updated.contains(
        "spec:\n  template:\n    name: second\n    labels:\n      owner: \"platform\"\n  replicas: 2\n"
    ));
    assert!(updated.contains("spec:\n  template:\n    name: first\n  replicas: 1"));
}

#[test]
fn patch_json_config_path_create_missing_yaml_multi_document_sequence_item_before_sibling_item() {
    let file_path = create_temp_yaml_source(
        "---\nitems:\n  - metadata:\n      name: first\n    enabled: true\n  - metadata:\n      name: second\n    enabled: false\n---\nitems:\n  - metadata:\n      name: other\n",
    );

    let output = patch_yaml_config_path_document(
        &file_path,
        "items[0].metadata.labels.owner",
        "\"platform\"",
        0,
    );
    assert!(
        output.status.success(),
        "create-missing should modify only the selected sequence item before the sibling item: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(updated.contains(
        "items:\n  - metadata:\n      name: first\n      labels:\n        owner: \"platform\"\n    enabled: true\n  - metadata:\n      name: second\n"
    ));
}

#[test]
fn patch_json_config_path_create_missing_yaml_multi_document_nested_parent_keeps_indented_comment_before_sibling_key()
 {
    let file_path = create_temp_yaml_source(
        "---\nspec:\n  template:\n    name: first\n---\nspec:\n  template:\n    name: second\n    # keep template comment\n  replicas: 2\n",
    );

    let output = patch_yaml_config_path_document(
        &file_path,
        "spec.template.labels.owner",
        "\"platform\"",
        1,
    );
    assert!(
        output.status.success(),
        "create-missing should keep an indented parent comment before returning to a sibling key: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(updated.contains(
        "template:\n    name: second\n    # keep template comment\n    labels:\n      owner: \"platform\"\n  replicas: 2\n"
    ));
}

#[test]
fn patch_json_config_path_create_missing_yaml_multi_document_document_start_with_trailing_comment()
{
    let file_path = create_temp_yaml_source(
        "---\nmetadata:\n  name: first\n--- # selected doc\nmetadata:\n  name: second\n",
    );

    let output =
        patch_yaml_config_path_document(&file_path, "metadata.labels.owner", "\"platform\"", 1);
    assert!(
        output.status.success(),
        "document_index should handle a document start marker with a trailing comment: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(updated.contains(
        "--- # selected doc\nmetadata:\n  name: second\n  labels:\n    owner: \"platform\""
    ));
}

#[test]
fn patch_json_config_path_create_missing_yaml_multi_document_document_end_then_start_selected_second_doc()
 {
    let file_path = create_temp_yaml_source(
        "---\nmetadata:\n  name: first\n...\n---\nmetadata:\n  name: second\n",
    );

    let output =
        patch_yaml_config_path_document(&file_path, "metadata.labels.owner", "\"platform\"", 1);
    assert!(
        output.status.success(),
        "document_index should handle explicit document end before the selected second document: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(
        updated.contains("...\n---\nmetadata:\n  name: second\n  labels:\n    owner: \"platform\"")
    );
}

#[test]
fn patch_json_config_path_create_missing_yaml_multi_document_selected_last_doc_without_final_newline()
 {
    let file_path =
        create_temp_yaml_source("---\nmetadata:\n  name: first\n---\nmetadata:\n  name: second");

    let output =
        patch_yaml_config_path_document(&file_path, "metadata.labels.owner", "\"platform\"", 1);
    assert!(
        output.status.success(),
        "create-missing should handle a selected last document without final newline: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(updated.ends_with("metadata:\n  name: second\n  labels:\n    owner: \"platform\"\n"));
}

#[test]
fn patch_json_config_path_create_missing_yaml_multi_document_selected_doc_quotes_colon_space_key_segment()
 {
    let file_path =
        create_temp_yaml_source("---\nmetadata:\n  name: first\n---\nmetadata:\n  name: second\n");

    let output = patch_yaml_config_path_document(
        &file_path,
        r#"metadata["team: name"].owner"#,
        "\"platform\"",
        1,
    );
    assert!(
        output.status.success(),
        "create-missing should quote unsafe key segments in the selected YAML document: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(
        updated.contains("metadata:\n  name: second\n  \"team: name\":\n    owner: \"platform\"")
    );
}

#[test]
fn patch_json_config_path_create_missing_yaml_multi_document_selected_doc_block_scalar_reindents() {
    let file_path =
        create_temp_yaml_source("---\nmetadata:\n  name: first\n---\nmetadata:\n  name: second\n");

    let output = patch_yaml_config_path_document(
        &file_path,
        "metadata.script",
        "|\n  echo first\n  echo second\n",
        1,
    );
    assert!(
        output.status.success(),
        "create-missing should reindent block scalar payloads in the selected YAML document: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(
        updated
            .contains("metadata:\n  name: second\n  script: |\n    echo first\n    echo second\n")
    );
}

#[test]
fn patch_json_config_path_create_missing_yaml_multi_document_selected_doc_rejects_block_scalar_bad_indent_without_mutation()
 {
    let file_path =
        create_temp_yaml_source("---\nmetadata:\n  name: first\n---\nmetadata:\n  name: second\n");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let output = patch_yaml_config_path_document(&file_path, "metadata.script", "|\necho bad\n", 1);
    assert!(
        !output.status.success(),
        "badly indented block scalar payload should be rejected in selected YAML document"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let after = fs::read_to_string(&file_path).expect("fixture should remain readable");
    assert_eq!(
        before, after,
        "bad block scalar payload must not mutate YAML stream"
    );
}

#[test]
fn patch_json_config_path_document_index_rejects_negative_json_value_without_mutation() {
    let file_path =
        create_temp_yaml_source("---\nmetadata:\n  name: first\n---\nmetadata:\n  name: second\n");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "metadata.owner",
            "document_index": -1
        },
        "op": {
            "type": "set",
            "new_text": "\"platform\"",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "negative document_index should be rejected by JSON schema"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let after = fs::read_to_string(&file_path).expect("fixture should remain readable");
    assert_eq!(
        before, after,
        "negative document_index rejection must not mutate YAML stream"
    );
}

#[test]
fn patch_json_config_path_create_missing_yaml_single_document_rejects_document_index_one_without_mutation()
 {
    let file_path = create_temp_yaml_source("metadata:\n  name: single\n");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let output =
        patch_yaml_config_path_document(&file_path, "metadata.labels.owner", "\"platform\"", 1);
    assert!(
        !output.status.success(),
        "single-document YAML should reject document_index 1"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("out of range"))
    );
    let after = fs::read_to_string(&file_path).expect("fixture should remain readable");
    assert_eq!(
        before, after,
        "out-of-range single-document index must not mutate YAML stream"
    );
}

#[test]
fn patch_flag_config_path_create_missing_yaml_multi_document_document_index_accepts_text_file_payload()
 {
    let file_path =
        create_temp_yaml_source("---\nmetadata:\n  name: first\n---\nmetadata:\n  name: second\n");
    let payload_path = create_temp_text_file("\"platform\"\n");

    let output = run_identedit(&[
        "patch",
        "--config-path",
        "metadata.owner",
        "--document-index",
        "1",
        "--set-value",
        "--text-file",
        payload_path.to_str().expect("payload path should be utf-8"),
        "--create-missing",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "document_index should work with --text-file payloads: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(updated.contains("name: second\n  owner: \"platform\""));
}

#[test]
fn patch_flag_config_path_create_missing_yaml_multi_document_document_index_accepts_stdin_text_payload()
 {
    let file_path =
        create_temp_yaml_source("---\nmetadata:\n  name: first\n---\nmetadata:\n  name: second\n");

    let output = run_identedit_with_stdin(
        &[
            "patch",
            "--config-path",
            "metadata.owner",
            "--document-index",
            "1",
            "--set-value",
            "--stdin-text",
            "--create-missing",
            file_path.to_str().expect("path should be utf-8"),
        ],
        "\"platform\"\n",
    );
    assert!(
        output.status.success(),
        "document_index should work with --stdin-text payloads: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(updated.contains("name: second\n  owner: \"platform\""));
}

#[test]
fn patch_flag_config_path_document_index_rejects_json_file_without_mutation() {
    let file_path = copy_fixture_to_temp_json("example.json");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let output = run_identedit(&[
        "patch",
        "--config-path",
        "config.retries",
        "--document-index",
        "0",
        "--set-value",
        "7",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "document_index should be rejected for JSON config files"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("supported only for YAML"))
    );
    let after = fs::read_to_string(&file_path).expect("fixture should remain readable");
    assert_eq!(before, after, "JSON rejection must not mutate file");
}

#[test]
fn patch_flag_config_path_document_index_rejects_toml_file_without_mutation() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"[service]\nretries = 2\n")
        .expect("toml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let output = run_identedit(&[
        "patch",
        "--config-path",
        "service.retries",
        "--document-index",
        "0",
        "--set-value",
        "7",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "document_index should be rejected for TOML config files"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("supported only for YAML"))
    );
    let after = fs::read_to_string(&file_path).expect("fixture should remain readable");
    assert_eq!(before, after, "TOML rejection must not mutate file");
}

#[test]
fn patch_json_config_path_set_existing_yaml_multi_document_selected_doc_missing_path_does_not_fall_through()
 {
    let file_path =
        create_temp_yaml_source("---\nservice:\n  name: app\n---\nmetadata:\n  owner: ops\n");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "metadata.owner",
            "document_index": 0
        },
        "op": {
            "type": "set",
            "new_text": "platform"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "document_index should not fall through to another document when the selected document misses the path"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("metadata") && message.contains("not found"))
    );
    let after = fs::read_to_string(&file_path).expect("fixture should remain readable");
    assert_eq!(
        before, after,
        "selected-document target-missing must not mutate another document"
    );
}

#[test]
fn patch_flag_config_path_create_missing_yaml_multi_document_document_index_dry_run_does_not_mutate()
 {
    let file_path =
        create_temp_yaml_source("---\nmetadata:\n  name: first\n---\nmetadata:\n  name: second\n");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let output = run_identedit(&[
        "patch",
        "--config-path",
        "metadata.owner",
        "--document-index",
        "1",
        "--set-value",
        "\"platform\"",
        "--create-missing",
        "--dry-run",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "document_index dry-run should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let after = fs::read_to_string(&file_path).expect("fixture should remain readable");
    assert_eq!(
        before, after,
        "document_index dry-run must not mutate YAML stream"
    );
}

#[test]
fn patch_json_config_path_document_index_rejects_json_file_without_mutation() {
    let file_path = copy_fixture_to_temp_json("example.json");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "config.retries",
            "document_index": 0
        },
        "op": {
            "type": "set",
            "new_text": "7"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "JSON mode should reject document_index for JSON config files"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let after = fs::read_to_string(&file_path).expect("fixture should remain readable");
    assert_eq!(
        before, after,
        "JSON-mode document_index rejection must not mutate JSON file"
    );
}

#[test]
fn patch_json_config_path_document_index_rejects_toml_file_without_mutation() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"[service]\nretries = 2\n")
        .expect("toml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.retries",
            "document_index": 0
        },
        "op": {
            "type": "set",
            "new_text": "7"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "JSON mode should reject document_index for TOML config files"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let after = fs::read_to_string(&file_path).expect("fixture should remain readable");
    assert_eq!(
        before, after,
        "JSON-mode document_index rejection must not mutate TOML file"
    );
}

#[test]
fn patch_json_config_path_set_existing_yaml_multi_document_index_zero_updates_first_doc_only() {
    let file_path =
        create_temp_yaml_source("---\nmetadata:\n  owner: app\n---\nmetadata:\n  owner: ops\n");
    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "metadata.owner",
            "document_index": 0
        },
        "op": {
            "type": "set",
            "new_text": "platform"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "document_index 0 should update the first matching YAML document: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(updated.contains("owner: platform"));
    assert!(updated.contains("owner: ops"));
    assert!(!updated.contains("owner: app"));
}

#[test]
fn patch_json_config_path_append_yaml_multi_document_selected_doc_missing_sequence_does_not_fall_through()
 {
    let file_path = create_temp_yaml_source("---\nmetadata:\n  name: app\n---\nitems:\n  - one\n");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "items",
            "document_index": 0
        },
        "op": {
            "type": "append",
            "new_text": "two"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "document_index append should not fall through to another document"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should remain readable");
    assert_eq!(
        before, after,
        "selected-document append miss must not mutate another document"
    );
}

#[test]
fn patch_json_config_path_delete_yaml_multi_document_selected_doc_missing_key_does_not_fall_through()
 {
    let file_path =
        create_temp_yaml_source("---\nmetadata:\n  name: app\n---\nmetadata:\n  owner: ops\n");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "metadata.owner",
            "document_index": 0
        },
        "op": {
            "type": "delete"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "document_index delete should not fall through to another document"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should remain readable");
    assert_eq!(
        before, after,
        "selected-document delete miss must not mutate another document"
    );
}

#[test]
fn patch_json_config_path_create_missing_yaml_multi_document_document_index_dry_run_does_not_mutate()
 {
    let file_path =
        create_temp_yaml_source("---\nmetadata:\n  name: first\n---\nmetadata:\n  name: second\n");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "metadata.owner",
            "document_index": 1
        },
        "op": {
            "type": "set",
            "new_text": "\"platform\"",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json", "--dry-run"], &request.to_string());
    assert!(
        output.status.success(),
        "JSON-mode document_index dry-run should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let after = fs::read_to_string(&file_path).expect("fixture should remain readable");
    assert_eq!(
        before, after,
        "JSON-mode document_index dry-run must not mutate YAML stream"
    );
}

#[test]
fn patch_flag_config_path_delete_yaml_multi_document_index_disambiguates_duplicate_keys() {
    let file_path = create_temp_yaml_source(
        "---\nmetadata:\n  owner: app\n  region: us\n---\nmetadata:\n  owner: ops\n  region: eu\n",
    );

    let output = run_identedit(&[
        "patch",
        "--config-path",
        "metadata.owner",
        "--document-index",
        "0",
        "--delete",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "flag-mode document_index should disambiguate delete targets: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(!updated.contains("owner: app"));
    assert!(updated.contains("owner: ops"));
    assert!(updated.contains("region: us"));
}

#[test]
fn patch_json_config_path_create_missing_yaml_multi_document_selected_doc_allows_unrelated_alias() {
    let file_path = create_temp_yaml_source(
        "---\nmetadata:\n  name: first\n---\nmetadata:\n  name: second\n  owner: *defaults\n",
    );

    let output =
        patch_yaml_config_path_document(&file_path, "metadata.labels.owner", "\"platform\"", 0);
    assert!(
        output.status.success(),
        "YAML alias in another document should not block selected-document create-missing: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("fixture should remain readable");
    assert!(updated.contains("metadata:\n  name: first\n  labels:\n    owner: \"platform\"\n"));
    assert!(updated.contains("metadata:\n  name: second\n  owner: *defaults\n"));
}

#[test]
fn patch_json_config_path_set_existing_yaml_multi_document_second_doc_path() {
    let file_path =
        create_temp_yaml_source("---\nservice:\n  retries: 2\n---\nmetadata:\n  owner: team\n");

    let output = patch_yaml_config_path(&file_path, "metadata.owner", "platform", false);
    assert!(
        output.status.success(),
        "existing second-document path should be editable: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(updated.contains("service:\n  retries: 2"));
    assert!(updated.contains("owner: platform"));
}

#[test]
fn patch_json_config_path_set_existing_yaml_multi_document_rejects_extra_document_start() {
    let file_path =
        create_temp_yaml_source("---\nservice:\n  retries: 2\n---\nmetadata:\n  owner: team\n");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let output = patch_yaml_config_path(
        &file_path,
        "service.retries",
        "5\n---\ninjected: true",
        false,
    );
    assert!(
        !output.status.success(),
        "replacement must not introduce a new YAML document"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "rejected edit must not mutate YAML source");
}

#[test]
fn patch_json_config_path_set_existing_yaml_multi_document_rejects_document_end_start_pair() {
    let file_path =
        create_temp_yaml_source("---\nservice:\n  retries: 2\n---\nmetadata:\n  owner: team\n");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let output = patch_yaml_config_path(
        &file_path,
        "metadata.owner",
        "team\n...\n---\nrole: ops",
        false,
    );
    assert!(
        !output.status.success(),
        "replacement must not split the existing YAML document stream"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "rejected edit must not mutate YAML source");
}

#[test]
fn patch_json_config_path_set_existing_yaml_multi_document_allows_quoted_document_marker_string() {
    let file_path =
        create_temp_yaml_source("---\nservice:\n  retries: 2\n---\nmetadata:\n  owner: team\n");

    let output = patch_yaml_config_path(&file_path, "metadata.owner", "\"---\"", false);
    assert!(
        output.status.success(),
        "quoted document marker should remain a scalar string: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(updated.contains("owner: \"---\""));
    assert!(updated.contains("service:\n  retries: 2"));
}

#[test]
fn patch_json_config_path_set_existing_yaml_multi_document_preserves_crlf_stream() {
    let file_path = create_temp_yaml_source(
        "---\r\nservice:\r\n  retries: 2\r\n---\r\nmetadata:\r\n  owner: team\r\n",
    );

    let output = patch_yaml_config_path(&file_path, "metadata.owner", "platform", false);
    assert!(
        output.status.success(),
        "CRLF multi-document YAML edit should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read(&file_path).expect("updated YAML should be readable");
    assert!(
        updated.windows(2).any(|pair| pair == b"\r\n"),
        "updated multi-document YAML should retain CRLF newlines"
    );
    for (index, byte) in updated.iter().enumerate() {
        if *byte == b'\n' {
            assert!(
                index > 0 && updated[index - 1] == b'\r',
                "every newline should remain CRLF, found lone LF at byte {index}"
            );
        }
    }
}

#[test]
fn patch_json_config_path_set_existing_yaml_multi_document_preserves_utf8_bom_prefix() {
    let file_path = create_temp_yaml_source(
        "\u{feff}---\nservice:\n  retries: 2\n---\nmetadata:\n  owner: team\n",
    );

    let output = patch_yaml_config_path(&file_path, "service.retries", "5", false);
    assert!(
        output.status.success(),
        "BOM multi-document YAML edit should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read(&file_path).expect("updated YAML should be readable");
    assert!(
        updated.starts_with(&[0xef, 0xbb, 0xbf]),
        "updated YAML should preserve UTF-8 BOM prefix"
    );
    let updated_text = String::from_utf8(updated).expect("updated YAML should be UTF-8");
    assert!(updated_text.contains("retries: 5"));
}

#[test]
fn patch_json_config_path_set_existing_yaml_multi_document_duplicate_path_rejects_ambiguous() {
    let file_path =
        create_temp_yaml_source("---\nmetadata:\n  owner: app\n---\nmetadata:\n  owner: ops\n");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let output = patch_yaml_config_path(&file_path, "metadata.owner", "platform", false);
    assert!(
        !output.status.success(),
        "duplicate path across YAML documents should be ambiguous"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("ambiguous across YAML documents"))
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "ambiguous edit must not mutate YAML source");
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_multi_document_duplicate_path_rejects_ambiguous()
{
    let file_path =
        create_temp_yaml_source("---\nmetadata:\n  owner: app\n---\nmetadata:\n  owner: ops\n");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let output = patch_yaml_config_path(&file_path, "metadata.owner", "platform", true);
    assert!(
        !output.status.success(),
        "create-missing should not fall back when an existing YAML path is ambiguous"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("ambiguous across YAML documents"))
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "ambiguous edit must not mutate YAML source");
}

#[test]
fn patch_json_config_path_set_existing_yaml_multi_document_unique_path_after_scalar_prefix_succeeds()
 {
    let file_path =
        create_temp_yaml_source("---\nmetadata: disabled\n---\nmetadata:\n  owner: team\n");

    let output = patch_yaml_config_path(&file_path, "metadata.owner", "platform", false);
    assert!(
        output.status.success(),
        "unique path in one YAML document should win over unrelated scalar prefix: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(updated.contains("metadata: disabled"));
    assert!(updated.contains("owner: platform"));
}

#[test]
fn patch_json_config_path_set_existing_yaml_multi_document_empty_doc_before_target_succeeds() {
    let file_path = create_temp_yaml_source("---\n---\nmetadata:\n  owner: team\n");

    let output = patch_yaml_config_path(&file_path, "metadata.owner", "platform", false);
    assert!(
        output.status.success(),
        "empty YAML document before target document should be ignored: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(updated.contains("owner: platform"));
}

#[test]
fn patch_json_config_path_delete_yaml_multi_document_second_doc_key_preserves_first_doc() {
    let file_path = create_temp_yaml_source(
        "---\nservice:\n  retries: 2\n---\nmetadata:\n  owner: team\n  region: eu\n",
    );
    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "metadata.owner"
        },
        "op": {
            "type": "delete"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "delete in second YAML document should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(updated.contains("service:\n  retries: 2"));
    assert!(!updated.contains("owner: team"));
    assert!(updated.contains("region: eu"));
}

#[test]
fn patch_json_config_path_append_yaml_multi_document_second_doc_sequence() {
    let file_path = create_temp_yaml_source(
        "---\nservice:\n  name: api\n---\nmetadata:\n  owners:\n    - app\n",
    );
    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "metadata.owners"
        },
        "op": {
            "type": "append",
            "new_text": "ops"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "append in second YAML document should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(updated.contains("service:\n  name: api"));
    assert!(updated.contains("    - app"));
    assert!(updated.contains("    - ops"));
}

#[test]
fn patch_json_config_path_set_existing_yaml_multi_document_unrelated_anchor_succeeds() {
    let file_path = create_temp_yaml_source(
        "---\nservice:\n  retries: 2\n---\ndefaults: &defaults\n  owner: team\n",
    );

    let output = patch_yaml_config_path(&file_path, "service.retries", "5", false);
    assert!(
        output.status.success(),
        "YAML anchor in another document should not block existing-path edits: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert_eq!(
        updated,
        "---\nservice:\n  retries: 5\n---\ndefaults: &defaults\n  owner: team\n"
    );
}

#[test]
fn patch_json_config_path_set_existing_yaml_multi_document_second_doc_merge_rejects_without_index()
{
    let file_path = create_temp_yaml_source(
        "---\nmetadata:\n  owner: team\n---\ndefaults: &defaults\n  retries: 2\nservice:\n  <<: *defaults\n  retries: 2\n",
    );
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let output = patch_yaml_config_path(&file_path, "service.retries", "5", false);
    assert!(
        !output.status.success(),
        "unique second-document path under YAML merge semantics should still be rejected"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("anchor/alias/merge")),
        "second-document merge rejection should explain non-local semantics"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "rejected edit must not mutate YAML source");
}

#[test]
fn patch_json_config_path_append_yaml_multi_document_second_doc_anchor_rejects_without_index() {
    let file_path = create_temp_yaml_source(
        "---\nmetadata:\n  owner: team\n---\ndefaults: &defaults\n  tags:\n    - api\nservice: *defaults\n",
    );
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "defaults.tags"
        },
        "op": {
            "type": "append",
            "new_text": "worker"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "append inside a referenced anchor in a unique second-document path should be rejected"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("anchor/alias/merge")),
        "second-document anchor append rejection should explain non-local semantics"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "rejected append must not mutate YAML source");
}

#[test]
fn patch_json_config_path_delete_yaml_multi_document_second_doc_anchor_rejects_without_index() {
    let file_path = create_temp_yaml_source(
        "---\nmetadata:\n  owner: team\n---\ndefaults: &defaults\n  retries: 2\nservice: *defaults\n",
    );
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "defaults.retries"
        },
        "op": {
            "type": "delete"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "delete inside a referenced anchor in a unique second-document path should be rejected"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("anchor/alias/merge")),
        "second-document anchor delete rejection should explain non-local semantics"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "rejected delete must not mutate YAML source");
}

#[test]
fn patch_json_config_path_set_yaml_multi_document_second_doc_nested_anchor_rejects_without_index() {
    let file_path = create_temp_yaml_source(
        "---\nmetadata:\n  owner: team\n---\ndefaults: &defaults\n  sidecar:\n    retries: 2\nservice: *defaults\n",
    );
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let output = patch_yaml_config_path(&file_path, "defaults.sidecar.retries", "5", false);
    assert!(
        !output.status.success(),
        "existing edit inside a nested referenced anchor in the second document should be rejected"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("anchor/alias/merge")),
        "second-document nested anchor rejection should explain non-local semantics"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "rejected edit must not mutate YAML source");
}

#[test]
fn patch_json_config_path_set_yaml_multi_document_selected_second_doc_allows_unrelated_first_doc_alias()
 {
    let file_path = create_temp_yaml_source(
        "---\nmetadata:\n  owner: *defaults\n---\nservice:\n  retries: 2\n",
    );

    let output = patch_yaml_config_path_document(&file_path, "service.retries", "5", 1);
    assert!(
        output.status.success(),
        "alias in an unselected first document should not block selected second-document edit: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(updated.contains("owner: *defaults"));
    assert!(updated.contains("retries: 5"));
}

#[test]
fn patch_json_config_path_create_missing_yaml_multi_document_allows_same_name_alias_in_unselected_doc()
 {
    let file_path = create_temp_yaml_source(
        "---\ndefaults: &defaults\n  retries: 2\n---\nservice: *defaults\n",
    );

    let output = patch_yaml_config_path_document(&file_path, "defaults.timeout", "30", 0);
    assert!(
        output.status.success(),
        "alias with the same name in an unselected YAML document should not make the selected document non-local: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(updated.contains("defaults: &defaults\n  retries: 2\n  timeout: 30\n"));
    assert!(updated.contains("service: *defaults\n"));
}

#[test]
fn patch_json_config_path_set_yaml_multi_document_selected_second_doc_allows_same_name_first_doc_alias()
 {
    let file_path = create_temp_yaml_source(
        "---\ndefaults: &defaults\n  retries: 2\nservice: *defaults\n---\ndefaults: &defaults\n  retries: 2\n",
    );

    let output = patch_yaml_config_path_document(&file_path, "defaults.retries", "5", 1);
    assert!(
        output.status.success(),
        "same-name anchor referenced only in an unselected document should not block selected-document edit: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(updated.contains("service: *defaults\n"));
    assert!(updated.ends_with("---\ndefaults: &defaults\n  retries: 5\n"));
}

#[test]
fn patch_json_config_path_set_yaml_multi_document_selected_second_doc_rejects_own_merge() {
    let file_path = create_temp_yaml_source(
        "---\nmetadata:\n  owner: team\n---\ndefaults: &defaults\n  retries: 2\nservice:\n  <<: *defaults\n  retries: 2\n",
    );
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let output = patch_yaml_config_path_document(&file_path, "service.retries", "5", 1);
    assert!(
        !output.status.success(),
        "selected second-document path under YAML merge semantics should be rejected"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("anchor/alias/merge")),
        "selected second-document merge rejection should explain non-local semantics"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "rejected edit must not mutate YAML source");
}

#[test]
fn patch_json_config_path_set_existing_yaml_multi_document_allows_quoted_anchor_like_string() {
    let file_path =
        create_temp_yaml_source("---\nservice:\n  retries: 2\n---\nmetadata:\n  owner: team\n");

    let output = patch_yaml_config_path(&file_path, "metadata.owner", "\"&not_anchor\"", false);
    assert!(
        output.status.success(),
        "quoted anchor-looking text should remain a scalar in multi-document YAML: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(updated.contains("owner: \"&not_anchor\""));
}

#[test]
fn patch_json_config_path_set_existing_yaml_multi_document_flow_sequence_rejects_unquoted_comma() {
    let file_path = create_temp_yaml_source(
        "---\nservice:\n  name: api\n---\nmetadata: { owners: [app, ops] }\n",
    );
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let output = patch_yaml_config_path(&file_path, "metadata.owners[0]", "app,platform", false);
    assert!(
        !output.status.success(),
        "unquoted comma in second-document flow sequence should be rejected"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(
        before, after,
        "rejected flow edit must not mutate YAML source"
    );
}

#[test]
fn patch_json_config_path_set_existing_yaml_multi_document_flow_sequence_allows_quoted_comma() {
    let file_path = create_temp_yaml_source(
        "---\nservice:\n  name: api\n---\nmetadata: { owners: [app, ops] }\n",
    );

    let output =
        patch_yaml_config_path(&file_path, "metadata.owners[0]", "\"app,platform\"", false);
    assert!(
        output.status.success(),
        "quoted comma in second-document flow sequence should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(updated.contains("owners: [\"app,platform\", ops]"));
}

#[test]
fn patch_json_config_path_set_existing_yaml_multi_document_duplicate_key_in_second_doc_rejects() {
    let file_path = create_temp_yaml_source(
        "---\nservice:\n  name: api\n---\nmetadata:\n  owner: app\n  owner: ops\n",
    );
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let output = patch_yaml_config_path(&file_path, "metadata.owner", "platform", false);
    assert!(
        !output.status.success(),
        "duplicate key in a matching YAML document should remain ambiguous"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(
        before, after,
        "ambiguous duplicate-key edit must not mutate YAML source"
    );
}

#[test]
fn patch_json_config_path_set_existing_yaml_multi_document_rejects_raw_nul_in_second_doc_value() {
    let file_path =
        create_temp_yaml_source("---\nservice:\n  retries: 2\n---\nmetadata:\n  owner: team\n");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let output = patch_yaml_config_path(&file_path, "metadata.owner", "bad\0value", false);
    assert!(
        !output.status.success(),
        "raw NUL should be rejected when editing second YAML document"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(
        before, after,
        "raw-NUL rejection must not mutate YAML source"
    );
}

fn patch_yaml_config_path_document(
    file_path: &Path,
    raw_path: &str,
    new_text: &str,
    document_index: usize,
) -> Output {
    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": raw_path,
            "document_index": document_index
        },
        "op": {
            "type": "set",
            "new_text": new_text,
            "create_missing": true
        }
    });

    run_identedit_with_stdin(&["patch", "--json"], &request.to_string())
}
