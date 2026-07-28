use super::*;

#[test]
fn patch_json_config_path_set_create_missing_creates_toml_intermediate_table_preserving_mixed_newlines()
 {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\r\n[server]\nhost = \"127.0.0.1\"\n\n[database]\nurl = \"sqlite://db\"\n")
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
        "mixed-newline intermediate TOML table creation should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.contains("[server.sidecar]\r\nport = 9090\r\n"),
        "new intermediate table should use detected CRLF line endings, got:\n{updated:?}"
    );
    assert!(
        updated.contains("host = \"127.0.0.1\"\n"),
        "existing LF line should remain unchanged, got:\n{updated:?}"
    );
    let normalized = updated.replace("\r\n", "\n");
    let parsed: toml::Value =
        toml::from_str(&normalized).expect("normalized TOML should stay valid");
    assert_eq!(parsed["server"]["sidecar"]["port"].as_integer(), Some(9090));
}

#[test]
fn patch_json_config_path_set_create_missing_creates_toml_intermediate_table_after_bom_comment_only_file()
 {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all("\u{feff}# keep-root-comment\n".as_bytes())
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
        "BOM comment-only intermediate TOML table creation should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.starts_with("\u{feff}# keep-root-comment\n"),
        "BOM and root comment should be preserved, got:\n{updated:?}"
    );
    assert!(
        updated.contains("[server.sidecar]\nport = 9090\n"),
        "new intermediate table should be inserted after comment-only root, got:\n{updated}"
    );
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["server"]["sidecar"]["port"].as_integer(), Some(9090));
}

#[test]
fn patch_json_config_path_set_create_missing_creates_toml_intermediate_table_after_bom_parent_table()
 {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all("\u{feff}# keep-this-comment\n[server]\nhost = \"127.0.0.1\"\n".as_bytes())
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
        "BOM parent-table intermediate TOML creation should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert_eq!(
        updated
            .chars()
            .filter(|character| *character == '\u{feff}')
            .count(),
        1,
        "intermediate table insertion must not duplicate the BOM, got:\n{updated:?}"
    );
    assert!(updated.starts_with("\u{feff}# keep-this-comment\n"));
    assert!(updated.contains("[server.sidecar]\nport = 9090\n"));
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["server"]["sidecar"]["port"].as_integer(), Some(9090));
}

#[test]
fn patch_json_config_path_set_create_missing_creates_toml_intermediate_table_with_cr_only_comments()
{
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\r[server]\rhost = \"127.0.0.1\"\r")
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
        "CR-only intermediate TOML table creation should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.contains("[server.sidecar]\rport = 9090\r"),
        "new intermediate table should use CR-only line endings, got:\n{updated:?}"
    );
    assert!(
        !updated.contains('\n'),
        "CR-only TOML update must not introduce LF separators, got:\n{updated:?}"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_creates_toml_intermediate_table_without_final_newline()
{
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\n[server]\nhost = \"127.0.0.1\"")
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
        "EOF intermediate TOML table creation should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.ends_with("host = \"127.0.0.1\"\n\n[server.sidecar]\nport = 9090\n"),
        "new intermediate table should be separated from unterminated parent table, got:\n{updated}"
    );
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["server"]["sidecar"]["port"].as_integer(), Some(9090));
}

#[test]
fn patch_json_config_path_set_create_missing_creates_toml_root_key_after_bom_whitespace_comment() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all("\u{feff}  # root-comment-with-leading-space\n".as_bytes())
        .expect("toml fixture write should succeed");
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
        "BOM+leading-space comment-only TOML root key creation should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.starts_with("\u{feff}  # root-comment-with-leading-space\n"),
        "BOM and leading-space comment should be preserved, got:\n{updated:?}"
    );
    assert_eq!(
        updated
            .chars()
            .filter(|character| *character == '\u{feff}')
            .count(),
        1,
        "root key create-missing must not duplicate the BOM, got:\n{updated:?}"
    );
    assert!(updated.contains("enabled = true\n"));
}

#[test]
fn patch_json_config_path_set_create_missing_toml_bom_without_comments_updates_existing_table() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all("\u{feff}[server]\nhost = \"127.0.0.1\"\n".as_bytes())
        .expect("toml fixture write should succeed");
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
        "BOM-prefixed TOML without comments should support create-missing: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert_eq!(
        updated
            .chars()
            .filter(|character| *character == '\u{feff}')
            .count(),
        1,
        "BOM-prefixed TOML without comments must not duplicate the BOM, got:\n{updated:?}"
    );
    assert!(updated.starts_with('\u{feff}'));
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["server"]["port"].as_integer(), Some(9090));
}

#[test]
fn patch_json_config_path_set_create_missing_toml_bom_without_comments_creates_intermediate_table()
{
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all("\u{feff}[server]\nhost = \"127.0.0.1\"\n".as_bytes())
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
        "BOM-prefixed TOML without comments should create intermediate table: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert_eq!(
        updated
            .chars()
            .filter(|character| *character == '\u{feff}')
            .count(),
        1,
        "BOM-prefixed TOML intermediate create must not duplicate the BOM, got:\n{updated:?}"
    );
    assert!(updated.starts_with('\u{feff}'));
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["server"]["sidecar"]["port"].as_integer(), Some(9090));
}

#[test]
fn patch_json_config_path_set_create_missing_toml_bom_only_file_preserves_hash_precondition() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all("\u{feff}".as_bytes())
        .expect("toml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "enabled",
            "expected_file_hash": crate::common::hash_text(&before)
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
        "BOM-only TOML create-missing should honor exact file hash precondition: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(updated.starts_with('\u{feff}'));
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["enabled"].as_bool(), Some(true));
}

#[test]
fn patch_json_config_path_set_create_missing_whitespace_only_toml_file_preserves_prefix() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b" \t\n \n")
        .expect("toml fixture write should succeed");
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
        "whitespace-only TOML create-missing should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.starts_with(" \t\n \n"),
        "whitespace prefix should be preserved, got:\n{updated:?}"
    );
    assert!(updated.contains("enabled = true"));
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["enabled"].as_bool(), Some(true));
}

#[test]
fn patch_json_config_path_set_create_missing_bom_whitespace_only_toml_file_preserves_prefix() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all("\u{feff} \t\n \n".as_bytes())
        .expect("toml fixture write should succeed");
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
        "BOM+whitespace-only TOML create-missing should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.starts_with("\u{feff} \t\n \n"),
        "BOM and whitespace prefix should be preserved, got:\n{updated:?}"
    );
    assert_eq!(
        updated
            .chars()
            .filter(|character| *character == '\u{feff}')
            .count(),
        1,
        "BOM+whitespace-only TOML create-missing must preserve exactly one BOM, got:\n{updated:?}"
    );
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["enabled"].as_bool(), Some(true));
}

#[test]
fn patch_json_config_path_set_create_missing_whitespace_only_toml_with_stale_hash_fails_without_mutation()
 {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b" \t\n \n")
        .expect("toml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

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
        "stale hash must reject whitespace-only TOML create-missing"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "precondition_failed");
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(
        before, after,
        "stale precondition failure must not mutate whitespace-only TOML"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_crlf_whitespace_only_toml_file_preserves_prefix() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b" \t\r\n \r\n")
        .expect("toml fixture write should succeed");
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
        "CRLF whitespace-only TOML create-missing should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.starts_with(" \t\r\n \r\n"),
        "CRLF whitespace prefix should be preserved, got:\n{updated:?}"
    );
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["enabled"].as_bool(), Some(true));
}

#[test]
fn patch_json_config_path_set_create_missing_cr_only_whitespace_only_toml_file_preserves_prefix() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b" \t\r \r")
        .expect("toml fixture write should succeed");
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
        "CR-only whitespace-only TOML create-missing should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.starts_with(" \t\r \r"),
        "CR-only whitespace prefix should be preserved, got:\n{updated:?}"
    );
    let normalized = updated.replace('\r', "\n");
    let parsed: toml::Value =
        toml::from_str(&normalized).expect("normalized TOML should stay valid");
    assert_eq!(parsed["enabled"].as_bool(), Some(true));
}

#[test]
fn patch_json_config_path_set_create_missing_bom_whitespace_only_toml_creates_intermediate_table() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all("\u{feff} \n".as_bytes())
        .expect("toml fixture write should succeed");
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
        "BOM+whitespace-only TOML intermediate create should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(updated.starts_with("\u{feff} \n"));
    assert_eq!(
        updated
            .chars()
            .filter(|character| *character == '\u{feff}')
            .count(),
        1,
        "BOM+whitespace-only TOML intermediate create must preserve one BOM"
    );
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["server"]["port"].as_integer(), Some(9090));
}

#[test]
fn patch_json_config_path_set_create_missing_toml_comment_only_without_final_newline_creates_intermediate()
 {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-root-comment")
        .expect("toml fixture write should succeed");
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
        "unterminated comment-only TOML should create intermediate table: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.starts_with("# keep-root-comment\n"),
        "unterminated root comment should get a separator before new table, got:\n{updated:?}"
    );
    assert!(updated.contains("[server]\nport = 9090"));
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["server"]["port"].as_integer(), Some(9090));
}

#[test]
fn patch_json_config_path_set_create_missing_toml_bom_only_file_creates_intermediate_table_once() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all("\u{feff}".as_bytes())
        .expect("toml fixture write should succeed");
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
        "BOM-only TOML intermediate create-missing should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert_eq!(
        updated
            .chars()
            .filter(|character| *character == '\u{feff}')
            .count(),
        1,
        "BOM-only TOML intermediate create-missing must preserve exactly one BOM, got:\n{updated:?}"
    );
    assert!(updated.contains("[server]\nport = 9090"));
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["server"]["port"].as_integer(), Some(9090));
}

#[test]
fn patch_json_config_path_set_create_missing_toml_bom_only_file_creates_root_key_once() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all("\u{feff}".as_bytes())
        .expect("toml fixture write should succeed");
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
        "BOM-only TOML create-missing should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert_eq!(
        updated
            .chars()
            .filter(|character| *character == '\u{feff}')
            .count(),
        1,
        "BOM-only TOML create-missing must preserve exactly one BOM, got:\n{updated:?}"
    );
    assert!(updated.contains("enabled = true"));
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["enabled"].as_bool(), Some(true));
}

#[test]
fn patch_json_config_path_set_create_missing_empty_toml_file_creates_root_key() {
    let temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
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
        "empty TOML create-missing should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert_eq!(updated, "enabled = true");
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["enabled"].as_bool(), Some(true));
}

#[test]
fn patch_json_config_path_set_create_missing_creates_toml_intermediate_table_after_bom_crlf_comment_only_file()
 {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all("\u{feff}# keep-root-comment\r\n".as_bytes())
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
        "BOM+CRLF comment-only TOML table creation should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert_eq!(
        updated
            .chars()
            .filter(|character| *character == '\u{feff}')
            .count(),
        1,
        "BOM+CRLF create-missing must not duplicate the BOM, got:\n{updated:?}"
    );
    assert!(
        updated.starts_with("\u{feff}# keep-root-comment\r\n"),
        "BOM+CRLF root comment should be preserved, got:\n{updated:?}"
    );
    assert!(
        updated.contains("[server.sidecar]\r\nport = 9090\r\n"),
        "new block should use CRLF separators, got:\n{updated:?}"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_creates_toml_intermediate_table_with_crlf_comments() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\r\n[server]\r\nhost = \"127.0.0.1\"\r\n")
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
        "CRLF intermediate TOML table creation should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.contains("[server.sidecar]\r\nport = 9090\r\n"),
        "new intermediate table should use CRLF line endings, got:\n{updated:?}"
    );
    assert!(
        !updated.contains("[server.sidecar]\nport"),
        "new intermediate table must not introduce LF-only separators, got:\n{updated:?}"
    );
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["server"]["sidecar"]["port"].as_integer(), Some(9090));
}

#[test]
fn patch_json_config_path_set_create_missing_preserves_toml_crlf_comments() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\r\n[server]\r\nhost = \"127.0.0.1\"\r\n")
        .expect("toml fixture write should succeed");
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
        "CRLF TOML create-missing should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(updated.contains("# keep-this-comment\r\n"));
    assert!(updated.contains("host = \"127.0.0.1\"\r\nport = 9090\r\n"));
    assert!(
        !updated.contains('\n') || updated.contains("\r\n"),
        "line endings should remain CRLF in updated TOML"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_toml_table_without_final_newline() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\n[server]\nhost = \"127.0.0.1\"")
        .expect("toml fixture write should succeed");
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
        "EOF TOML create-missing should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.ends_with("host = \"127.0.0.1\"\nport = 9090\n"),
        "new key should be appended on its own line, got:\n{updated}"
    );
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["server"]["port"].as_integer(), Some(9090));
}

#[test]
fn patch_json_config_path_set_create_missing_preserves_toml_cr_only_comments() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\r[server]\rhost = \"127.0.0.1\"\r")
        .expect("toml fixture write should succeed");
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
        "CR-only TOML create-missing should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(updated.contains("# keep-this-comment\r"));
    assert!(updated.contains("host = \"127.0.0.1\"\rport = 9090\r"));
    assert!(
        !updated.contains('\n'),
        "line endings should remain CR-only in updated TOML"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_toml_bom_prefixed_comments() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all("\u{feff}# keep-this-comment\n[server]\nhost = \"127.0.0.1\"\n".as_bytes())
        .expect("toml fixture write should succeed");
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
        "BOM-prefixed TOML create-missing should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(updated.starts_with('\u{feff}'));
    assert_eq!(
        updated
            .chars()
            .filter(|character| *character == '\u{feff}')
            .count(),
        1,
        "BOM-prefixed TOML create-missing must not duplicate the BOM, got:\n{updated:?}"
    );
    assert!(updated.contains("# keep-this-comment"));
    assert!(updated.contains("port = 9090"));
}

#[test]
fn patch_json_config_path_set_create_missing_preserves_mixed_toml_line_endings() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\r\n[server]\nhost = \"127.0.0.1\"\n")
        .expect("toml fixture write should succeed");
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
        "mixed-line-ending TOML create-missing should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.contains("# keep-this-comment\r\n"),
        "existing CRLF comment line should remain unchanged"
    );
    assert!(
        updated.contains("host = \"127.0.0.1\"\nport = 9090\r\n"),
        "new line should use detected first line ending without rewriting existing LF line, got:\n{updated:?}"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_preserves_cr_only_toml_newlines() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"[server]\rhost = \"127.0.0.1\"\r")
        .expect("toml fixture write should succeed");
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
        "CR-only TOML create-missing should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let updated = fs::read(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.contains(&b'\r'),
        "updated TOML should retain CR-only line endings"
    );
    for (index, byte) in updated.iter().enumerate() {
        if *byte == b'\n' {
            assert!(
                index > 0 && updated[index - 1] == b'\r',
                "every newline should be CRLF or CR-only compatible; found lone LF at byte {index}"
            );
        }
    }
}

#[test]
fn patch_json_config_path_set_create_missing_bom_whitespace_toml_with_exact_hash_succeeds() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    let source = "\u{feff}\r\n \t\r\n";
    temp_file
        .write_all(source.as_bytes())
        .expect("toml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "server.port",
            "expected_file_hash": crate::common::hash_text(source)
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
        "BOM+whitespace TOML with exact hash should create missing path: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.starts_with(source),
        "BOM+whitespace prefix should remain hash-guarded, got:\n{updated:?}"
    );
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["server"]["port"].as_integer(), Some(9090));
}

#[test]
fn patch_json_config_path_set_create_missing_bom_whitespace_toml_with_stale_hash_fails() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    let source = "\u{feff}\r\n \t\r\n";
    temp_file
        .write_all(source.as_bytes())
        .expect("toml fixture write should succeed");
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
        "stale hash should reject BOM+whitespace TOML create-missing"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "precondition_failed");
    let updated = fs::read_to_string(&file_path).expect("TOML fixture should be readable");
    assert_eq!(
        updated, source,
        "stale hash failure must not mutate BOM+whitespace TOML"
    );
}
