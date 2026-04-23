use super::*;

#[test]
fn patch_json_config_path_set_create_missing_rejects_toml_deep_array_value_parent_conflict() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\nserver.sidecar = [{ host = \"127.0.0.1\" }]\n")
        .expect("toml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

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
        !output.status.success(),
        "array-valued deep TOML parent must not be promoted into a standard table"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(
        before, after,
        "rejected operation must not mutate TOML source"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_rejects_toml_trailing_dotted_key_in_value_fragment() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\n[server]\nhost = \"127.0.0.1\"\n")
        .expect("toml fixture write should succeed");
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
            "new_text": "9090\nevil.path = 1",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "TOML value fragments must reject trailing dotted-key injection"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(
        before, after,
        "rejected operation must not mutate TOML source"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_accepts_toml_value_fragment_with_inline_comment() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\n[server]\nhost = \"127.0.0.1\"\n")
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
            "new_text": "9090 # keep-port-comment",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "TOML value fragment with inline comment should remain a single value: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.contains("port = 9090 # keep-port-comment"),
        "inline value comment should be preserved, got:\n{updated}"
    );
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["server"]["port"].as_integer(), Some(9090));
}

#[test]
fn patch_json_config_path_set_create_missing_accepts_toml_inline_table_value_fragment() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\n[server]\nhost = \"127.0.0.1\"\n")
        .expect("toml fixture write should succeed");
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
            "new_text": "{ port = 9090, enabled = true }",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "TOML inline table value fragment should be accepted as a single value: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(updated.contains("sidecar = { port = 9090, enabled = true }"));
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["server"]["sidecar"]["port"].as_integer(), Some(9090));
    assert_eq!(parsed["server"]["sidecar"]["enabled"].as_bool(), Some(true));
}

#[test]
fn patch_json_config_path_set_create_missing_rejects_toml_trailing_quoted_key_in_value_fragment() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\n[server]\nhost = \"127.0.0.1\"\n")
        .expect("toml fixture write should succeed");
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
            "new_text": "9090\n\"evil.key\" = 1",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "TOML value fragments must reject trailing quoted-key injection"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(
        before, after,
        "rejected operation must not mutate TOML source"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_rejects_toml_trailing_array_table_in_value_fragment() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\n[server]\nhost = \"127.0.0.1\"\n")
        .expect("toml fixture write should succeed");
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
            "new_text": "9090\n[[evil]]\nx = 1",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "TOML value fragments must reject trailing array-table injection"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(
        before, after,
        "rejected operation must not mutate TOML source"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_rejects_toml_value_fragment_with_duplicate_temp_key() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\n[server]\nhost = \"127.0.0.1\"\n")
        .expect("toml fixture write should succeed");
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
            "new_text": "9090\n__identedit_tmp__ = 1",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "TOML value fragments must reject duplicate wrapped temp key injection"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(
        before, after,
        "rejected operation must not mutate TOML source"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_rejects_toml_array_value_parent_conflict() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\nserver = [{ host = \"127.0.0.1\" }]\n")
        .expect("toml fixture write should succeed");
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
        "array-valued TOML parent must not be promoted into a standard table"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(
        before, after,
        "rejected operation must not mutate TOML source"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_rejects_toml_trailing_table_in_value_fragment() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\n[server]\nhost = \"127.0.0.1\"\n")
        .expect("toml fixture write should succeed");
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
            "new_text": "9090\n[evil]\nx = 1",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "TOML value fragments must reject trailing table injection"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("single TOML value")),
        "error should explain that only a single TOML value is accepted"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(
        before, after,
        "rejected operation must not mutate TOML source"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_rejects_toml_trailing_key_in_value_fragment() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\n[server]\nhost = \"127.0.0.1\"\n")
        .expect("toml fixture write should succeed");
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
            "new_text": "9090\nevil = 1",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "TOML value fragments must reject trailing key injection"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("single TOML value")),
        "error should explain that only a single TOML value is accepted"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(
        before, after,
        "rejected operation must not mutate TOML source"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_accepts_toml_value_fragment_with_standalone_comment() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\n[server]\nhost = \"127.0.0.1\"\n")
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
            "new_text": "9090\n# generated port",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "TOML value fragment plus standalone comment should stay a single value fragment: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.contains("port = 9090\n# generated port"),
        "standalone comment following value should be preserved, got:\n{updated}"
    );
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["server"]["port"].as_integer(), Some(9090));
}

#[test]
fn patch_json_config_path_set_create_missing_rejects_toml_leading_table_in_value_fragment() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\n[server]\nhost = \"127.0.0.1\"\n")
        .expect("toml fixture write should succeed");
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
            "new_text": "[evil]\nx = 1\n9090",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "TOML value fragment must reject leading table injection"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(
        before, after,
        "rejected leading-table value fragment must not mutate TOML source"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_accepts_toml_array_of_inline_tables_value_fragment() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\n[server]\nhost = \"127.0.0.1\"\n")
        .expect("toml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "server.backends"
        },
        "op": {
            "type": "set",
            "new_text": r#"[{ name = "primary" }, { name = "replica", enabled = true }]"#,
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "TOML array-of-inline-tables value fragment should be accepted as one value: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.contains(
            "backends = [{ name = \"primary\" }, { name = \"replica\", enabled = true }]"
        )
    );
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    let backends = parsed["server"]["backends"]
        .as_array()
        .expect("backends should be an array");
    assert_eq!(backends.len(), 2);
    assert_eq!(backends[1]["enabled"].as_bool(), Some(true));
}

#[test]
fn patch_json_config_path_set_create_missing_accepts_toml_literal_string_value_with_comment_chars()
{
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\n[server]\nhost = \"127.0.0.1\"\n")
        .expect("toml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "server.note"
        },
        "op": {
            "type": "set",
            "new_text": "'# not a comment = value'",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "TOML literal string containing comment-like chars should be a single value: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(updated.contains("note = '# not a comment = value'"));
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(
        parsed["server"]["note"].as_str(),
        Some("# not a comment = value")
    );
}

#[test]
fn patch_json_config_path_set_create_missing_rejects_toml_leading_dotted_key_in_value_fragment() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\n[server]\nhost = \"127.0.0.1\"\n")
        .expect("toml fixture write should succeed");
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
            "new_text": "evil.key = 1\n9090",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "TOML value fragment must reject leading dotted-key injection"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(
        before, after,
        "rejected leading dotted-key fragment must not mutate TOML source"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_accepts_toml_date_value_fragment() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\n[release]\nname = \"v1\"\n")
        .expect("toml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "release.date"
        },
        "op": {
            "type": "set",
            "new_text": "1979-05-27",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "TOML date value fragment should be accepted as one value: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(updated.contains("date = 1979-05-27"));
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert!(parsed["release"]["date"].is_datetime());
}

#[test]
fn patch_json_config_path_set_create_missing_accepts_toml_value_fragment_with_crlf_comment() {
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
            "new_text": "9090\r\n# generated port",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "CRLF TOML value fragment comment should be accepted: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(updated.contains("port = 9090\r\n# generated port"));
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["server"]["port"].as_integer(), Some(9090));
}

#[test]
fn patch_json_config_path_set_create_missing_accepts_toml_array_value_with_inline_comment() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\n[server]\nhost = \"127.0.0.1\"\n")
        .expect("toml fixture write should succeed");
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
            "new_text": "[8000, 9000] # generated ports",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "TOML array value with inline comment should be accepted: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(updated.contains("ports = [8000, 9000] # generated ports"));
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["server"]["ports"].as_array().map(Vec::len), Some(2));
}

#[test]
fn patch_json_config_path_set_create_missing_accepts_toml_special_float_value_fragment() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\n[metrics]\nname = \"latency\"\n")
        .expect("toml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "metrics.baseline"
        },
        "op": {
            "type": "set",
            "new_text": "+inf",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "TOML special float value fragment should be accepted: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(updated.contains("baseline = +inf"));
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert!(
        parsed["metrics"]["baseline"]
            .as_float()
            .is_some_and(f64::is_infinite)
    );
}

#[test]
fn patch_json_config_path_set_create_missing_accepts_toml_basic_string_escape_value_fragment() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\n[server]\nhost = \"127.0.0.1\"\n")
        .expect("toml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "server.note"
        },
        "op": {
            "type": "set",
            "new_text": r#""line\nbreak""#,
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "TOML basic string escape value fragment should be accepted: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(updated.contains(r#"note = "line\nbreak""#));
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["server"]["note"].as_str(), Some("line\nbreak"));
}

#[test]
fn patch_json_config_path_set_create_missing_accepts_toml_nested_array_value_fragment() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\n[server]\nhost = \"127.0.0.1\"\n")
        .expect("toml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "server.matrix"
        },
        "op": {
            "type": "set",
            "new_text": "[[1, 2], [3, 4]]",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "TOML nested array value fragment should be accepted: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(updated.contains("matrix = [[1, 2], [3, 4]]"));
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["server"]["matrix"][1][1].as_integer(), Some(4));
}
