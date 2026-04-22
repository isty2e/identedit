use super::*;

#[test]
fn patch_json_config_path_set_create_missing_creates_toml_quoted_intermediate_table_with_comments()
{
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\n[tool]\nname = \"identedit\"\n")
        .expect("toml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": r#"tool["weird.section"].port"#
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
        "quoted intermediate TOML table creation should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.contains("[tool.\"weird.section\"]\nport = 9090"),
        "new quoted intermediate table should use TOML quoted-key syntax, got:\n{updated}"
    );
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(
        parsed["tool"]["weird.section"]["port"].as_integer(),
        Some(9090)
    );
}

#[test]
fn patch_json_config_path_set_create_missing_keeps_toml_literal_dotted_table_distinct_from_dotted_path()
 {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\n[\"server.sidecar\"]\nhost = \"literal\"\n")
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
        "literal dotted table and dotted path should coexist: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(updated.contains("[\"server.sidecar\"]\nhost = \"literal\""));
    assert!(updated.contains("[server.sidecar]\nport = 9090"));
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["server.sidecar"]["host"].as_str(), Some("literal"));
    assert_eq!(parsed["server"]["sidecar"]["port"].as_integer(), Some(9090));
}

#[test]
fn patch_json_config_path_set_create_missing_creates_toml_quoted_parent_before_quoted_descendant_table_with_comments()
 {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\n[tool.\"a.b\".child]\nname = \"leaf\"\n")
        .expect("toml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": r#"tool["a.b"].port"#
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
        "quoted parent before quoted descendant should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    let parent = updated
        .find("[tool.\"a.b\"]\nport = 9090")
        .expect("created quoted parent table should exist");
    let child = updated
        .find("[tool.\"a.b\".child]\nname = \"leaf\"")
        .expect("existing quoted descendant table should remain");
    assert!(
        parent < child,
        "created quoted parent should precede descendant, got:\n{updated}"
    );
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["tool"]["a.b"]["port"].as_integer(), Some(9090));
    assert_eq!(
        parsed["tool"]["a.b"]["child"]["name"].as_str(),
        Some("leaf")
    );
}

#[test]
fn patch_json_config_path_set_create_missing_creates_toml_escaped_quoted_intermediate_table_with_comments()
 {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\n[tool]\nname = \"identedit\"\n")
        .expect("toml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": r#"tool["quote\"slash\\segment"].port"#
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
        "escaped quoted intermediate TOML table creation should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.contains(r#"[tool."quote\"slash\\segment"]"#),
        "new quoted intermediate table should escape TOML key segment, got:\n{updated}"
    );
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(
        parsed["tool"]["quote\"slash\\segment"]["port"].as_integer(),
        Some(9090)
    );
}

#[test]
fn patch_json_config_path_set_create_missing_creates_toml_unicode_quoted_intermediate_table_with_comments()
 {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\n[tool]\nname = \"identedit\"\n")
        .expect("toml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": r#"tool["한 점"].port"#
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
        "unicode quoted intermediate TOML table creation should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.contains("[tool.\"한 점\"]\nport = 9090"),
        "new intermediate table should preserve unicode quoted segment, got:\n{updated}"
    );
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["tool"]["한 점"]["port"].as_integer(), Some(9090));
}

#[test]
fn patch_json_config_path_set_create_missing_quotes_toml_escaped_leaf_key_with_comments() {
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
            "path": r#"server["quote\"leaf\\key"]"#
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
        "escaped quoted TOML leaf key create-missing should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.contains(r#""quote\"leaf\\key" = 9090"#),
        "new escaped leaf key should use TOML quoted-key syntax, got:\n{updated}"
    );
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(
        parsed["server"]["quote\"leaf\\key"].as_integer(),
        Some(9090)
    );
}

#[test]
fn patch_json_config_path_set_create_missing_uses_existing_toml_single_quoted_intermediate_table() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\n[tool.'a.b']\nhost = \"127.0.0.1\"\n")
        .expect("toml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": r#"tool["a.b"].port"#
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
        "single-quoted TOML table segment should resolve as an existing table: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.contains("[tool.'a.b']\nhost = \"127.0.0.1\"\nport = 9090\n"),
        "new key should be inserted into the existing single-quoted table, got:\n{updated}"
    );
    assert!(
        !updated.contains("[tool.\"a.b\"]"),
        "create-missing must not create a duplicate double-quoted equivalent table, got:\n{updated}"
    );
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["tool"]["a.b"]["port"].as_integer(), Some(9090));
}

#[test]
fn patch_json_config_path_set_create_missing_updates_existing_toml_single_quoted_leaf_key() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\n[server]\n'listen.port' = 8080\n")
        .expect("toml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": r#"server["listen.port"]"#
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
        "single-quoted TOML leaf key should resolve as an existing path: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.contains("'listen.port' = 9090"),
        "existing single-quoted key should be updated in place, got:\n{updated}"
    );
    assert!(
        !updated.contains("\"listen.port\" = 9090"),
        "create-missing must not create a duplicate double-quoted equivalent key, got:\n{updated}"
    );
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["server"]["listen.port"].as_integer(), Some(9090));
}

#[test]
fn patch_json_config_path_set_create_missing_uses_existing_toml_basic_unicode_escape_table() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(
            "# keep-this-comment\n[tool.\"emoji \\U0001F600\"]\nhost = \"127.0.0.1\"\n".as_bytes(),
        )
        .expect("toml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "tool[\"emoji 😀\"].port"
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
        "TOML basic string \\U escape in table key should resolve as an existing table: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.contains("[tool.\"emoji \\U0001F600\"]\nhost = \"127.0.0.1\"\nport = 9090\n"),
        "new key should be inserted into the existing escaped-key table, got:\n{updated}"
    );
    assert!(
        !updated.contains("[tool.\"emoji 😀\"]"),
        "create-missing must not create a duplicate literal-unicode equivalent table, got:\n{updated}"
    );
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["tool"]["emoji 😀"]["port"].as_integer(), Some(9090));
}

#[test]
fn patch_json_config_path_set_create_missing_updates_existing_toml_basic_unicode_escape_leaf_key() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all("# keep-this-comment\n[server]\n\"emoji \\U0001F600\" = 8080\n".as_bytes())
        .expect("toml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "server[\"emoji 😀\"]"
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
        "TOML basic string \\U escape in leaf key should resolve as an existing key: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.contains("\"emoji \\U0001F600\" = 9090"),
        "existing escaped key should be updated in place, got:\n{updated}"
    );
    assert!(
        !updated.contains("\"emoji 😀\" = 9090"),
        "create-missing must not create a duplicate literal-unicode equivalent key, got:\n{updated}"
    );
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["server"]["emoji 😀"].as_integer(), Some(9090));
}

#[test]
fn patch_json_config_path_set_create_missing_uses_existing_toml_single_quoted_unicode_table() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all("# keep-this-comment\n[tool.'한.점']\nhost = \"127.0.0.1\"\n".as_bytes())
        .expect("toml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": r#"tool["한.점"].port"#
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
        "single-quoted unicode TOML table segment should resolve as an existing table: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(updated.contains("[tool.'한.점']\nhost = \"127.0.0.1\"\nport = 9090\n"));
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["tool"]["한.점"]["port"].as_integer(), Some(9090));
}

#[test]
fn patch_json_config_path_set_create_missing_uses_existing_empty_single_quoted_toml_table() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\n[tool.'']\nhost = \"127.0.0.1\"\n")
        .expect("toml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": r#"tool[""].port"#
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
        "empty single-quoted TOML table segment should resolve as an existing table: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.contains("[tool.'']\nhost = \"127.0.0.1\"\nport = 9090\n"),
        "new key should be inserted into the existing empty single-quoted table, got:\n{updated}"
    );
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["tool"][""]["port"].as_integer(), Some(9090));
}

#[test]
fn patch_json_config_path_set_create_missing_uses_existing_empty_double_quoted_toml_leaf_key() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\n[server]\n\"\" = 8080\n")
        .expect("toml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": r#"server[""]"#
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
        "empty double-quoted TOML leaf key should resolve as an existing key: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.contains("\"\" = 9090"),
        "existing empty key should be updated in place, got:\n{updated}"
    );
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["server"][""].as_integer(), Some(9090));
}

#[test]
fn patch_json_config_path_set_create_missing_creates_toml_leaf_key_with_escaped_newline_segment() {
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
            "path": "server[\"line\\nkey\"]"
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
        "escaped newline TOML key segment should be creatable: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.contains("\"line\\nkey\" = true"),
        "escaped newline key should be rendered as a quoted TOML key, got:\n{updated}"
    );
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["server"]["line\nkey"].as_bool(), Some(true));
}

#[test]
fn patch_json_config_path_set_create_missing_uses_existing_toml_leaf_key_with_escaped_newline() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\n[server]\n\"line\\nkey\" = false\n")
        .expect("toml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "server[\"line\\nkey\"]"
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
        "existing escaped newline TOML key should be updated in place: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.contains("\"line\\nkey\" = true"),
        "existing escaped newline key should be updated in place, got:\n{updated}"
    );
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["server"]["line\nkey"].as_bool(), Some(true));
}

#[test]
fn patch_json_config_path_set_create_missing_creates_toml_leaf_key_with_escaped_tab_segment() {
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
            "path": "server[\"tab\\tkey\"]"
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
        "escaped tab TOML key segment should be creatable: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.contains("\"tab\\tkey\" = true"),
        "escaped tab key should be rendered as a quoted TOML key, got:\n{updated}"
    );
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["server"]["tab\tkey"].as_bool(), Some(true));
}

#[test]
fn patch_json_config_path_set_create_missing_uses_existing_toml_leaf_key_with_escaped_tab() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\n[server]\n\"tab\\tkey\" = false\n")
        .expect("toml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "server[\"tab\\tkey\"]"
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
        "existing escaped tab TOML key should be updated in place: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.contains("\"tab\\tkey\" = true"),
        "existing escaped tab key should be updated in place, got:\n{updated}"
    );
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["server"]["tab\tkey"].as_bool(), Some(true));
}

#[test]
fn patch_json_config_path_set_create_missing_uses_existing_toml_single_quoted_backslash_table() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\n[tool.'path\\literal']\nhost = \"127.0.0.1\"\n")
        .expect("toml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": r#"tool["path\\literal"].port"#
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
        "single-quoted TOML table with literal backslash should resolve as existing: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.contains("[tool.'path\\literal']\nhost = \"127.0.0.1\"\nport = 9090\n"),
        "new key should be inserted into existing literal-backslash table, got:\n{updated}"
    );
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(
        parsed["tool"]["path\\literal"]["port"].as_integer(),
        Some(9090)
    );
}

#[test]
fn patch_json_config_path_set_create_missing_creates_toml_leaf_key_with_escaped_backslash_segment()
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
            "path": r#"server["path\\literal"]"#
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
        "escaped backslash TOML key segment should be creatable: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.contains(r#""path\\literal" = true"#),
        "escaped backslash key should be rendered as a quoted TOML key, got:\n{updated}"
    );
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["server"]["path\\literal"].as_bool(), Some(true));
}

#[test]
fn patch_json_config_path_set_create_missing_uses_existing_toml_basic_backslash_leaf_key() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\n[server]\n\"path\\\\literal\" = false\n")
        .expect("toml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": r#"server["path\\literal"]"#
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
        "existing TOML basic-string key with escaped backslash should update in place: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.contains(r#""path\\literal" = true"#),
        "existing escaped backslash key should be updated in place, got:\n{updated}"
    );
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["server"]["path\\literal"].as_bool(), Some(true));
}

#[test]
fn patch_json_config_path_set_create_missing_creates_toml_leaf_key_with_combining_mark_segment() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\n[server]\nhost = \"127.0.0.1\"\n")
        .expect("toml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let decomposed = "e\u{301}";

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": format!("server[\"{decomposed}\"]")
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
        "combining-mark TOML key segment should be creatable without normalization: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.contains(&format!("\"{decomposed}\" = true")),
        "combining-mark key should be preserved byte-for-byte, got:\n{updated}"
    );
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["server"][decomposed].as_bool(), Some(true));
    assert!(parsed["server"].get("é").is_none());
}

#[test]
fn patch_json_config_path_set_create_missing_uses_existing_toml_single_quoted_backslash_leaf_key() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\n[server]\n'path\\literal' = false\n")
        .expect("toml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": r#"server["path\\literal"]"#
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
        "existing TOML single-quoted key with literal backslash should update in place: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.contains("'path\\literal' = true"),
        "existing literal-backslash key should be updated in place, got:\n{updated}"
    );
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["server"]["path\\literal"].as_bool(), Some(true));
}

#[test]
fn patch_json_config_path_set_create_missing_creates_toml_intermediate_table_with_escaped_backslash_segment()
 {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\n[tool]\nname = \"identedit\"\n")
        .expect("toml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": r#"tool["path\\literal"].port"#
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
        "escaped backslash TOML table segment should be creatable: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.contains(r#"[tool."path\\literal"]"#),
        "escaped backslash table segment should be rendered as quoted TOML, got:\n{updated}"
    );
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(
        parsed["tool"]["path\\literal"]["port"].as_integer(),
        Some(9090)
    );
}

#[test]
fn patch_json_config_path_set_create_missing_creates_toml_intermediate_table_with_escaped_tab_segment()
 {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\n[tool]\nname = \"identedit\"\n")
        .expect("toml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "tool[\"tab\\tkey\"].port"
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
        "escaped tab TOML table segment should be creatable: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.contains("[tool.\"tab\\tkey\"]\nport = 9090"),
        "escaped tab table segment should be rendered as quoted TOML, got:\n{updated}"
    );
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["tool"]["tab\tkey"]["port"].as_integer(), Some(9090));
}

#[test]
fn patch_json_config_path_set_create_missing_creates_empty_quoted_toml_intermediate_table() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\n[tool]\nname = \"identedit\"\n")
        .expect("toml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": r#"tool[""].port"#
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
        "empty quoted TOML table segment should be creatable: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.contains("[tool.\"\"]\nport = 9090"),
        "empty TOML key segment should be rendered as a quoted table segment, got:\n{updated}"
    );
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["tool"][""]["port"].as_integer(), Some(9090));
}

#[test]
fn patch_json_config_path_set_create_missing_toml_hyphenated_table_with_comments() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\n[server-sidecar]\nhost = \"127.0.0.1\"\n")
        .expect("toml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "server-sidecar.port"
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
        "hyphenated table TOML create-missing should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(updated.contains("[server-sidecar]\nhost = \"127.0.0.1\"\nport = 9090\n"));
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["server-sidecar"]["port"].as_integer(), Some(9090));
}

#[test]
fn patch_json_config_path_set_create_missing_toml_quoted_table_segment_with_comments() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\n[server.\"sidecar\"]\nhost = \"127.0.0.1\"\n")
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
        "quoted table segment TOML create-missing should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(updated.contains("[server.\"sidecar\"]\nhost = \"127.0.0.1\"\nport = 9090\n"));
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["server"]["sidecar"]["port"].as_integer(), Some(9090));
}

#[test]
fn patch_json_config_path_set_create_missing_supports_toml_quoted_table_segments() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"\"x.y\" = 1\n\n[tool.\"weird.section\"]\nname = \"identedit\"\n")
        .expect("toml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": r#"tool["weird.section"].port"#
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
        "quoted TOML table path create-missing should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(updated.contains("[tool.\"weird.section\"]\nname = \"identedit\"\nport = 9090"));
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["x.y"].as_integer(), Some(1));
    assert_eq!(
        parsed["tool"]["weird.section"]["port"].as_integer(),
        Some(9090)
    );
}

#[test]
fn patch_json_config_path_set_create_missing_toml_literal_dotted_root_table_path() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\n")
        .expect("toml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": r#"["server.config"].port"#
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
        "literal dotted root table path should create quoted TOML table: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.contains("[\"server.config\"]\nport = 9090"),
        "literal dotted root key should be quoted, got:\n{updated}"
    );
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["server.config"]["port"].as_integer(), Some(9090));
}

#[test]
fn patch_json_config_path_set_create_missing_creates_toml_leaf_key_with_space_padded_segment() {
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
            "path": r#"server["  padded key  "]"#
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
        "TOML quoted key segment with edge spaces should be creatable: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.contains("\"  padded key  \" = true"),
        "space-padded key should stay quoted, got:\n{updated}"
    );
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["server"]["  padded key  "].as_bool(), Some(true));
}

#[test]
fn patch_json_config_path_set_create_missing_creates_toml_leaf_key_with_slash_segment() {
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
            "path": r#"server["path/like/key"]"#
        },
        "op": {
            "type": "set",
            "new_text": r#""value""#,
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "TOML slash-containing quoted key should be creatable: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(updated.contains("\"path/like/key\" = \"value\""));
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["server"]["path/like/key"].as_str(), Some("value"));
}

#[test]
fn patch_json_config_path_set_create_missing_creates_toml_leaf_key_with_emoji_segment() {
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
            "path": "server[\"emoji😀key\"]"
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
        "TOML emoji-containing key should be creatable: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(updated.contains("\"emoji😀key\" = true"));
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["server"]["emoji😀key"].as_bool(), Some(true));
}

#[test]
fn patch_json_config_path_set_create_missing_creates_toml_leaf_key_with_colon_segment() {
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
            "path": r#"server["host:port"]"#
        },
        "op": {
            "type": "set",
            "new_text": r#""127.0.0.1:9090""#,
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "TOML colon-containing key should be creatable: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(updated.contains("\"host:port\" = \"127.0.0.1:9090\""));
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(
        parsed["server"]["host:port"].as_str(),
        Some("127.0.0.1:9090")
    );
}

#[test]
fn patch_json_config_path_set_create_missing_creates_toml_leaf_key_with_equals_segment() {
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
            "path": r#"server["env=prod"]"#
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
        "TOML equals-containing key should be creatable: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(updated.contains("\"env=prod\" = true"));
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["server"]["env=prod"].as_bool(), Some(true));
}
