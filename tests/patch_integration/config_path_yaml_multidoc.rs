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
fn patch_json_config_path_set_existing_yaml_multi_document_unrelated_anchor_rejects() {
    let file_path = create_temp_yaml_source(
        "---\nservice:\n  retries: 2\n---\ndefaults: &defaults\n  owner: team\n",
    );
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let output = patch_yaml_config_path(&file_path, "service.retries", "5", false);
    assert!(
        !output.status.success(),
        "YAML streams containing anchors should remain unsupported even when editing another document"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(
        before, after,
        "anchor rejection must not mutate YAML source"
    );
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
