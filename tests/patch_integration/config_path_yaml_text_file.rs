use super::*;

#[test]
fn patch_config_set_value_text_file_invalid_yaml_does_not_mutate_file() {
    let file_path = copy_fixture_to_temp_with_suffix("example.yaml", ".yaml");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let payload_path = create_temp_text_file("name: [unterminated");

    let output = run_identedit(&[
        "patch",
        "--config-path",
        "service",
        "--set-value",
        "--text-file",
        payload_path.to_str().expect("payload path should be utf-8"),
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(!output.status.success(), "invalid YAML payload should fail");
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let after = fs::read_to_string(&file_path).expect("file should remain readable");
    assert_eq!(after, before);
}

#[test]
fn patch_config_set_value_text_file_valid_yaml_with_trailing_newline_applies() {
    let file_path = copy_fixture_to_temp_with_suffix("example.yaml", ".yaml");
    let payload_path = create_temp_text_file("5\n");

    let output = run_identedit(&[
        "patch",
        "--config-path",
        "service.retries",
        "--set-value",
        "--text-file",
        payload_path.to_str().expect("payload path should be utf-8"),
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "valid YAML payload with trailing newline should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    assert!(modified.contains("retries: 5"));
}

#[test]
fn patch_config_set_value_text_file_yaml_crlf_trailing_newline_stays_single_line() {
    let file_path = copy_fixture_to_temp_with_suffix("example.yaml", ".yaml");
    let payload_path = create_temp_text_file("7\r\n");

    let output = run_identedit(&[
        "patch",
        "--config-path",
        "service.retries",
        "--set-value",
        "--text-file",
        payload_path.to_str().expect("payload path should be utf-8"),
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "YAML single-line value from CRLF text-file should not be forced into block-scalar mode: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(updated.contains("retries: 7"));
    assert!(
        !updated.contains("retries: |"),
        "single-line text-file payload should not render as a block scalar: {updated}"
    );
}

#[test]
fn patch_config_set_value_text_file_yaml_cr_only_trailing_newline_stays_single_line() {
    let file_path = copy_fixture_to_temp_with_suffix("example.yaml", ".yaml");
    let payload_path = create_temp_text_file("8\r");

    let output = run_identedit(&[
        "patch",
        "--config-path",
        "service.retries",
        "--set-value",
        "--text-file",
        payload_path.to_str().expect("payload path should be utf-8"),
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "YAML single-line value from CR-only text-file should not become a block scalar: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(updated.contains("retries: 8"));
    assert!(
        !updated.contains("retries: |"),
        "single-line CR-only text-file payload should not render as a block scalar: {updated}"
    );
}

#[test]
fn patch_config_set_value_text_file_yaml_extra_blank_line_rejects_without_mutation() {
    let file_path = copy_fixture_to_temp_with_suffix("example.yaml", ".yaml");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let payload_path = create_temp_text_file("9\n\n");

    let output = run_identedit(&[
        "patch",
        "--config-path",
        "service.retries",
        "--set-value",
        "--text-file",
        payload_path.to_str().expect("payload path should be utf-8"),
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        !output.status.success(),
        "YAML text-file scalar with extra blank line should not be silently treated as a safe single-line scalar"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "rejected operation must not mutate YAML");
}

#[test]
fn patch_config_set_value_text_file_yaml_empty_payload_rejects_without_mutation() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  script: old\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let payload_path = create_temp_text_file("");

    let output = run_identedit(&[
        "patch",
        file_path.to_str().expect("path should be utf-8"),
        "--config-path",
        "service.script",
        "--set-value",
        "--text-file",
        payload_path.to_str().expect("payload path should be utf-8"),
    ]);

    assert!(
        !output.status.success(),
        "empty YAML set-value payload should not silently set null"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "rejected operation must not mutate YAML");
}

#[test]
fn patch_config_set_value_text_file_yaml_whitespace_payload_rejects_without_mutation() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  script: old\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let payload_path = create_temp_text_file("  \n");

    let output = run_identedit(&[
        "patch",
        file_path.to_str().expect("path should be utf-8"),
        "--config-path",
        "service.script",
        "--set-value",
        "--text-file",
        payload_path.to_str().expect("payload path should be utf-8"),
    ]);

    assert!(
        !output.status.success(),
        "whitespace-only YAML set-value payload should not silently set null"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "rejected operation must not mutate YAML");
}

#[test]
fn patch_config_create_missing_yaml_text_file_trailing_lf_stays_single_line() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  name: app\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let payload_path = create_temp_text_file("true\n");

    let output = run_identedit(&[
        "patch",
        file_path.to_str().expect("path should be utf-8"),
        "--config-path",
        "service.enabled",
        "--set-value",
        "--create-missing",
        "--text-file",
        payload_path.to_str().expect("payload path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "YAML create-missing text-file with one trailing LF should stay single-line: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(updated.contains("enabled: true\n"));
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed["service"]["enabled"].as_bool(), Some(true));
}

#[test]
fn patch_config_create_missing_yaml_text_file_trailing_crlf_stays_single_line() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\r\n  name: app\r\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let payload_path = create_temp_text_file("false\r\n");

    let output = run_identedit(&[
        "patch",
        file_path.to_str().expect("path should be utf-8"),
        "--config-path",
        "service.enabled",
        "--set-value",
        "--create-missing",
        "--text-file",
        payload_path.to_str().expect("payload path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "YAML create-missing text-file with one trailing CRLF should stay single-line: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(updated.contains("enabled: false\r\n"));
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed["service"]["enabled"].as_bool(), Some(false));
}

#[test]
fn patch_config_create_missing_yaml_text_file_comment_with_trailing_lf_rejects() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  name: app\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let payload_path = create_temp_text_file("# not a value\n");

    let output = run_identedit(&[
        "patch",
        file_path.to_str().expect("path should be utf-8"),
        "--config-path",
        "service.script",
        "--set-value",
        "--create-missing",
        "--text-file",
        payload_path.to_str().expect("payload path should be utf-8"),
    ]);

    assert!(
        !output.status.success(),
        "YAML create-missing should reject comment-only text-file payloads after trimming one line ending"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "rejected operation must not mutate YAML");
}

#[test]
fn patch_config_create_missing_yaml_text_file_empty_with_trailing_lf_rejects() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  name: app\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let payload_path = create_temp_text_file("\n");

    let output = run_identedit(&[
        "patch",
        file_path.to_str().expect("path should be utf-8"),
        "--config-path",
        "service.script",
        "--set-value",
        "--create-missing",
        "--text-file",
        payload_path.to_str().expect("payload path should be utf-8"),
    ]);

    assert!(
        !output.status.success(),
        "YAML create-missing should reject empty text-file payloads after trimming one line ending"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "rejected operation must not mutate YAML");
}

#[test]
fn patch_config_create_missing_yaml_text_file_quoted_empty_with_trailing_lf_succeeds() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  name: app\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let payload_path = create_temp_text_file("\"\"\n");

    let output = run_identedit(&[
        "patch",
        file_path.to_str().expect("path should be utf-8"),
        "--config-path",
        "service.script",
        "--set-value",
        "--create-missing",
        "--text-file",
        payload_path.to_str().expect("payload path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "YAML create-missing should accept quoted empty string from text-file payload: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed["service"]["script"].as_str(), Some(""));
}

#[test]
fn patch_config_create_missing_yaml_text_file_double_trailing_lf_rejects() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  name: app\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let payload_path = create_temp_text_file("true\n\n");

    let output = run_identedit(&[
        "patch",
        file_path.to_str().expect("path should be utf-8"),
        "--config-path",
        "service.enabled",
        "--set-value",
        "--create-missing",
        "--text-file",
        payload_path.to_str().expect("payload path should be utf-8"),
    ]);

    assert!(
        !output.status.success(),
        "YAML create-missing should reject ambiguous text-file payloads with more than one trailing line ending"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "rejected operation must not mutate YAML");
}

#[test]
fn patch_config_create_missing_empty_yaml_text_file_trailing_lf_stays_single_line() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let payload_path = create_temp_text_file("true\n");

    let output = run_identedit(&[
        "patch",
        file_path.to_str().expect("path should be utf-8"),
        "--config-path",
        "enabled",
        "--set-value",
        "--create-missing",
        "--text-file",
        payload_path.to_str().expect("payload path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "empty YAML create-missing should normalize one trailing LF from text-file payloads: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed["enabled"].as_bool(), Some(true));
}

#[test]
fn patch_config_create_missing_empty_yaml_text_file_comment_rejects_without_mutation() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let payload_path = create_temp_text_file("# not a value\n");

    let output = run_identedit(&[
        "patch",
        file_path.to_str().expect("path should be utf-8"),
        "--config-path",
        "script",
        "--set-value",
        "--create-missing",
        "--text-file",
        payload_path.to_str().expect("payload path should be utf-8"),
    ]);

    assert!(
        !output.status.success(),
        "empty YAML create-missing should reject comment-only text-file payloads"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(after, "", "rejected operation must not mutate YAML");
}

#[test]
fn patch_config_create_missing_whitespace_yaml_text_file_trailing_lf_preserves_prefix() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"\n\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let payload_path = create_temp_text_file("8080\n");

    let output = run_identedit(&[
        "patch",
        file_path.to_str().expect("path should be utf-8"),
        "--config-path",
        "service.port",
        "--set-value",
        "--create-missing",
        "--text-file",
        payload_path.to_str().expect("payload path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "whitespace-only YAML create-missing should normalize one trailing LF and preserve prefix: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(updated.starts_with("\n\n"));
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed["service"]["port"].as_i64(), Some(8080));
}

#[test]
fn patch_config_create_missing_comment_only_yaml_text_file_trailing_lf() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"# keep file header\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let payload_path = create_temp_text_file("\"# literal\"\n");

    let output = run_identedit(&[
        "patch",
        file_path.to_str().expect("path should be utf-8"),
        "--config-path",
        "script",
        "--set-value",
        "--create-missing",
        "--text-file",
        payload_path.to_str().expect("payload path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "comment-only YAML create-missing should normalize text-file payloads without dropping comments: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(updated.starts_with("# keep file header\n"));
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed["script"].as_str(), Some("# literal"));
}

#[test]
fn patch_config_create_missing_empty_yaml_text_file_block_scalar_succeeds() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let payload_path = create_temp_text_file("|\n  echo empty root\n");

    let output = run_identedit(&[
        "patch",
        file_path.to_str().expect("path should be utf-8"),
        "--config-path",
        "script",
        "--set-value",
        "--create-missing",
        "--text-file",
        payload_path.to_str().expect("payload path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "empty YAML create-missing should accept explicit block scalar text-file payloads: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed["script"].as_str(), Some("echo empty root\n"));
}

#[test]
fn patch_config_create_missing_whitespace_yaml_text_file_comment_crlf_rejects() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"  \r\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let payload_path = create_temp_text_file("# not a value\r\n");

    let output = run_identedit(&[
        "patch",
        file_path.to_str().expect("path should be utf-8"),
        "--config-path",
        "script",
        "--set-value",
        "--create-missing",
        "--text-file",
        payload_path.to_str().expect("payload path should be utf-8"),
    ]);

    assert!(
        !output.status.success(),
        "whitespace-only YAML create-missing should reject comment-only CRLF text-file payloads"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "rejected operation must not mutate YAML");
}

#[test]
fn patch_config_create_missing_comment_only_yaml_empty_text_file_rejects() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"# header only\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let payload_path = create_temp_text_file("");

    let output = run_identedit(&[
        "patch",
        file_path.to_str().expect("path should be utf-8"),
        "--config-path",
        "script",
        "--set-value",
        "--create-missing",
        "--text-file",
        payload_path.to_str().expect("payload path should be utf-8"),
    ]);

    assert!(
        !output.status.success(),
        "comment-only YAML create-missing should reject empty text-file payloads"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "rejected operation must not mutate YAML");
}

#[test]
fn patch_config_create_missing_bom_empty_yaml_text_file_trailing_lf_preserves_bom() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all("\u{feff}".as_bytes())
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let payload_path = create_temp_text_file("true\n");

    let output = run_identedit(&[
        "patch",
        file_path.to_str().expect("path should be utf-8"),
        "--config-path",
        "enabled",
        "--set-value",
        "--create-missing",
        "--text-file",
        payload_path.to_str().expect("payload path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "BOM-only YAML create-missing should normalize text-file payloads and preserve BOM: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(updated.starts_with('\u{feff}'));
    let parsed: serde_yaml::Value = serde_yaml::from_str(updated.trim_start_matches('\u{feff}'))
        .expect("updated YAML should stay valid after BOM removal for parser");
    assert_eq!(parsed["enabled"].as_bool(), Some(true));
}

#[test]
fn patch_config_create_missing_cr_only_whitespace_yaml_text_file_trailing_cr() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"\r\r")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let payload_path = create_temp_text_file("false\r");

    let output = run_identedit(&[
        "patch",
        file_path.to_str().expect("path should be utf-8"),
        "--config-path",
        "enabled",
        "--set-value",
        "--create-missing",
        "--text-file",
        payload_path.to_str().expect("payload path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "CR-only whitespace YAML create-missing should normalize one trailing CR from text-file payloads: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(updated.starts_with("\r\r"));
    assert!(updated.contains("enabled: false"));
}

#[test]
fn patch_config_create_missing_comment_only_yaml_text_file_keep_chomp_empty_body() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"# header only\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let payload_path = create_temp_text_file("|+\n");

    let output = run_identedit(&[
        "patch",
        file_path.to_str().expect("path should be utf-8"),
        "--config-path",
        "script",
        "--set-value",
        "--create-missing",
        "--text-file",
        payload_path.to_str().expect("payload path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "comment-only YAML create-missing should preserve keep-chomp empty block scalars: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed["script"].as_str(), Some("\n"));
}

#[test]
fn patch_config_set_existing_yaml_block_mapping_text_file_rejects_raw_nul() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  value: old\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let payload_path = create_temp_text_file("alpha\0beta");

    let output = run_identedit(&[
        "patch",
        file_path.to_str().expect("path should be utf-8"),
        "--config-path",
        "service.value",
        "--set-value",
        "--text-file",
        payload_path.to_str().expect("payload path should be utf-8"),
    ]);

    assert!(
        !output.status.success(),
        "YAML existing text-file set should reject raw NUL characters"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "rejected operation must not mutate YAML");
}

#[test]
fn patch_config_set_create_missing_yaml_text_file_rejects_raw_nul() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  name: app\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let payload_path = create_temp_text_file("alpha\0beta");

    let output = run_identedit(&[
        "patch",
        file_path.to_str().expect("path should be utf-8"),
        "--config-path",
        "service.value",
        "--set-value",
        "--create-missing",
        "--text-file",
        payload_path.to_str().expect("payload path should be utf-8"),
    ]);

    assert!(
        !output.status.success(),
        "YAML create-missing text-file set should reject raw NUL characters"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "rejected operation must not mutate YAML");
}

#[test]
fn patch_config_set_existing_yaml_block_mapping_text_file_escaped_nul_succeeds() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  value: old\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let payload_path = create_temp_text_file("\"alpha\\0beta\"\n");

    let output = run_identedit(&[
        "patch",
        file_path.to_str().expect("path should be utf-8"),
        "--config-path",
        "service.value",
        "--set-value",
        "--text-file",
        payload_path.to_str().expect("payload path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "YAML existing text-file set should allow escaped NUL in quoted scalar: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed["service"]["value"].as_str(), Some("alpha\0beta"));
}

#[test]
fn patch_config_set_create_missing_yaml_text_file_escaped_line_separator_succeeds() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  name: app\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let payload_path = create_temp_text_file("\"alpha\\u2028beta\"\n");

    let output = run_identedit(&[
        "patch",
        file_path.to_str().expect("path should be utf-8"),
        "--config-path",
        "service.value",
        "--set-value",
        "--create-missing",
        "--text-file",
        payload_path.to_str().expect("payload path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "YAML create-missing text-file set should allow escaped Unicode line separator in quoted scalar: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(
        parsed["service"]["value"].as_str(),
        Some("alpha\u{2028}beta")
    );
}

#[test]
fn patch_config_set_create_missing_yaml_text_file_quoted_colon_comma_succeeds() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  name: app\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let payload_path = create_temp_text_file("\"foo: bar, baz\"\n");

    let output = run_identedit(&[
        "patch",
        file_path.to_str().expect("path should be utf-8"),
        "--config-path",
        "service.value",
        "--set-value",
        "--create-missing",
        "--text-file",
        payload_path.to_str().expect("payload path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "YAML create-missing text-file set should allow quoted scalars containing colon-space and comma: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed["service"]["value"].as_str(), Some("foo: bar, baz"));
}
