use super::*;

#[test]
fn patch_json_config_path_set_create_missing_existing_toml_path_preserves_comments() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(
            b"# keep-this-comment\n[server]\nport = 8080 # trailing-comment\nhost = \"127.0.0.1\"\n",
        )
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
        "existing TOML path create-missing should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.contains("# keep-this-comment"),
        "existing-path create-missing should keep TOML comments"
    );
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(
        parsed["server"]["port"].as_integer(),
        Some(9090),
        "targeted TOML value should be updated"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_inserts_toml_leaf_with_comments() {
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
            "new_text": "9090",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "missing-path TOML create-missing with comments should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(updated.contains("# keep-this-comment"));
    assert!(updated.contains("host = \"127.0.0.1\""));
    assert!(updated.contains("port = 9090"));
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(
        parsed["server"]["port"].as_integer(),
        Some(9090),
        "inserted TOML value should parse"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_inserts_toml_leaf_before_next_table() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\n[server]\nhost = \"127.0.0.1\"\n\n[database]\nurl = \"sqlite://db\"\n")
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
        "TOML create-missing should insert into the matched table: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.contains("[server]\nhost = \"127.0.0.1\"\nport = 9090\n\n[database]"),
        "new key should be inserted before the next table, got:\n{updated}"
    );
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["server"]["port"].as_integer(), Some(9090));
    assert_eq!(parsed["database"]["url"].as_str(), Some("sqlite://db"));
}

#[test]
fn patch_json_config_path_set_create_missing_toml_sorted_group_inserts_in_order() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"[metadata]\nalpha = 1\nbeta = 2\ndelta = 4\n")
        .expect("toml fixture write should succeed");
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
        "sorted TOML group insertion should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.contains("[metadata]\nalpha = 1\nbeta = 2\ncharlie = 3\ndelta = 4\n"),
        "new key should preserve sorted TOML group order, got:\n{updated}"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_toml_unsorted_group_appends() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"[metadata]\nbuild = \"fast\"\ntest = \"strict\"\ndeploy = \"manual\"\n")
        .expect("toml fixture write should succeed");
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
        "unsorted TOML group insertion should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.contains(
            "[metadata]\nbuild = \"fast\"\ntest = \"strict\"\ndeploy = \"manual\"\ncache = true\n"
        ),
        "new key should append to unsorted TOML group, got:\n{updated}"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_toml_sorted_insert_preserves_following_key_comment() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"[metadata]\nalpha = 1\nbeta = 2\n# delta setting\ndelta = 4\n")
        .expect("toml fixture write should succeed");
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
        "comment-owned sorted TOML insertion should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated
            .contains("[metadata]\nalpha = 1\nbeta = 2\ncharlie = 3\n# delta setting\ndelta = 4\n"),
        "new key should be inserted before the following key's leading comment, got:\n{updated}"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_toml_blank_line_group_boundary_is_preserved() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"[metadata]\nalpha = 1\nbeta = 2\n\n# delta setting\ndelta = 4\n")
        .expect("toml fixture write should succeed");
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
        "TOML insertion should preserve blank-line group boundaries: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.contains(
            "[metadata]\nalpha = 1\nbeta = 2\ncharlie = 3\n\n# delta setting\ndelta = 4\n"
        ),
        "new key should stay in the first group and preserve the following group comment, got:\n{updated}"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_toml_prefix_family_inserts_near_run() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(
            b"[service]\nsidecar_api_host = \"localhost\"\nsidecar_api_tls = false\nretries = 3\n",
        )
        .expect("toml fixture write should succeed");
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
        "TOML prefix family insertion should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.contains(
            "[service]\nsidecar_api_host = \"localhost\"\nsidecar_api_port = 9000\nsidecar_api_tls = false\nretries = 3\n"
        ),
        "new key should be inserted within the sidecar_api prefix family, got:\n{updated}"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_toml_unsorted_prefix_family_appends_to_run_end() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(
            b"[service]\nsidecar_api_tls = false\nsidecar_api_host = \"localhost\"\nretries = 3\n",
        )
        .expect("toml fixture write should succeed");
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
        "TOML unsorted prefix-family insertion should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.contains(
            "[service]\nsidecar_api_tls = false\nsidecar_api_host = \"localhost\"\nsidecar_api_port = 9000\nretries = 3\n"
        ),
        "new key should append to an unsorted prefix run instead of sorting it, got:\n{updated}"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_toml_root_sorted_group_before_table() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"alpha = 1\nbeta = 2\ndelta = 4\n\n[server]\nhost = \"localhost\"\n")
        .expect("toml fixture write should succeed");
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
        "TOML root sorted insertion should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.contains("alpha = 1\nbeta = 2\ncharlie = 3\ndelta = 4\n\n[server]"),
        "root key should stay in the root sorted group before the first table, got:\n{updated}"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_toml_crlf_comment_owned_sorted_insert() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"[metadata]\r\nalpha = 1\r\nbeta = 2\r\n# delta setting\r\ndelta = 4\r\n")
        .expect("toml fixture write should succeed");
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
        "TOML CRLF sorted insertion should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.contains(
            "[metadata]\r\nalpha = 1\r\nbeta = 2\r\ncharlie = 3\r\n# delta setting\r\ndelta = 4\r\n"
        ),
        "new key should preserve CRLF and stay before the owned comment block, got:\n{updated:?}"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_toml_blank_line_separated_prefixes_do_not_merge() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(
            b"[service]\nsidecar_api_host = \"localhost\"\n\nsidecar_api_tls = false\nretries = 3\n",
        )
        .expect("toml fixture write should succeed");
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
        "TOML separated prefix-family insertion should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.contains(
            "[service]\nsidecar_api_host = \"localhost\"\n\nsidecar_api_tls = false\nretries = 3\nsidecar_api_port = 9000\n"
        ),
        "blank-line separated prefix keys should not be merged into one inferred run, got:\n{updated}"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_toml_cr_only_blank_line_group_boundary() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"[metadata]\ralpha = 1\rbeta = 2\r\r# delta setting\rdelta = 4\r")
        .expect("toml fixture write should succeed");
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
        "TOML CR-only blank-line group insertion should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.contains(
            "[metadata]\ralpha = 1\rbeta = 2\rcharlie = 3\r\r# delta setting\rdelta = 4\r"
        ),
        "new key should preserve CR-only blank-line group boundary, got:\n{updated:?}"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_toml_dotted_sibling_keeps_conservative_append() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"[metadata]\nalpha = 1\nbeta.inner = 2\ndelta = 4\n")
        .expect("toml fixture write should succeed");
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
        "TOML dotted sibling conservative insertion should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.contains("[metadata]\nalpha = 1\nbeta.inner = 2\ndelta = 4\ncharlie = 3\n"),
        "dotted sibling should disable sorted inference and append conservatively, got:\n{updated}"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_toml_quoted_keys_participate_in_sorted_group() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"[metadata]\nalpha = 1\n\"beta key\" = 2\ndelta = 4\n")
        .expect("toml fixture write should succeed");
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
        "TOML quoted-key sorted insertion should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated
            .contains("[metadata]\nalpha = 1\n\"beta key\" = 2\n\"charlie key\" = 3\ndelta = 4\n"),
        "decoded quoted keys should participate in sorted placement, got:\n{updated}"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_inserts_toml_root_leaf_with_comments() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-root-comment\nname = \"identedit\"\n\n[server]\nhost = \"127.0.0.1\"\n")
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
        "root TOML create-missing should preserve comments: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.contains("# keep-root-comment\nname = \"identedit\"\nenabled = true\n\n[server]"),
        "new root key should be inserted before the first table, got:\n{updated}"
    );
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["enabled"].as_bool(), Some(true));
}

#[test]
fn patch_json_config_path_set_create_missing_creates_toml_intermediate_table_with_comments() {
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
        "intermediate TOML table creation should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.contains(
            "# keep-this-comment\n[server]\nhost = \"127.0.0.1\"\n\n[server.sidecar]\nport = 9090\n"
        ),
        "new intermediate table should be inserted after the existing parent table, got:\n{updated}"
    );
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["server"]["host"].as_str(), Some("127.0.0.1"));
    assert_eq!(parsed["server"]["sidecar"]["port"].as_integer(), Some(9090));
}

#[test]
fn patch_json_config_path_set_create_missing_places_toml_intermediate_table_after_nearest_prefix_before_siblings()
 {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(
            b"# keep-this-comment\n[server]\nhost = \"127.0.0.1\"\n\n[server.logging]\nlevel = \"info\"\n\n[server.sidecar.db]\nhost = \"db\"\n",
        )
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
        "intermediate table placement among siblings should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    let parent = updated
        .find("[server]\nhost = \"127.0.0.1\"")
        .expect("parent table should remain");
    let new_table = updated
        .find("[server.sidecar]\nport = 9090")
        .expect("new intermediate table should be inserted");
    let sibling = updated
        .find("[server.logging]\nlevel = \"info\"")
        .expect("sibling table should remain");
    assert!(
        parent < new_table && new_table < sibling,
        "new table should be inserted right after nearest prefix table, got:\n{updated}"
    );
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["server"]["sidecar"]["port"].as_integer(), Some(9090));
    assert_eq!(parsed["server"]["logging"]["level"].as_str(), Some("info"));
    assert_eq!(
        parsed["server"]["sidecar"]["db"]["host"].as_str(),
        Some("db")
    );
}

#[test]
fn patch_json_config_path_set_create_missing_creates_toml_intermediate_table_after_tail_comment_before_next_table()
 {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(
            b"# keep-this-comment\n[server]\nhost = \"127.0.0.1\"\n# keep-server-tail\n\n[database]\nurl = \"sqlite://db\"\n",
        )
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
        "intermediate table after tail comment should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.contains("# keep-server-tail\n\n[server.sidecar]\nport = 9090\n\n[database]"),
        "new intermediate table should follow tail comment and precede next table, got:\n{updated}"
    );
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["server"]["sidecar"]["port"].as_integer(), Some(9090));
    assert_eq!(parsed["database"]["url"].as_str(), Some("sqlite://db"));
}

#[test]
fn patch_json_config_path_set_create_missing_creates_toml_parent_before_deep_descendant_table_with_comments()
 {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\n[server.sidecar.db]\nhost = \"127.0.0.1\"\n")
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
        "parent TOML table creation before descendant should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    let parent_position = updated
        .find("[server.sidecar]\nport = 9090")
        .expect("updated TOML should contain the created parent table");
    let descendant_position = updated
        .find("[server.sidecar.db]\nhost = \"127.0.0.1\"")
        .expect("updated TOML should keep the existing descendant table");
    assert!(
        parent_position < descendant_position,
        "created parent table should precede its existing descendant, got:\n{updated}"
    );
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["server"]["sidecar"]["port"].as_integer(), Some(9090));
    assert_eq!(
        parsed["server"]["sidecar"]["db"]["host"].as_str(),
        Some("127.0.0.1")
    );
}

#[test]
fn patch_json_config_path_set_create_missing_allows_toml_dotted_sibling_then_child_table_with_comments()
 {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\nserver.host = \"127.0.0.1\"\n")
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
        "dotted sibling plus child table creation should be valid TOML: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.contains("server.host = \"127.0.0.1\""),
        "existing dotted sibling key should be preserved, got:\n{updated}"
    );
    assert!(
        updated.contains("[server.sidecar]\nport = 9090"),
        "new child table should be appended without rewriting dotted sibling, got:\n{updated}"
    );
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["server"]["host"].as_str(), Some("127.0.0.1"));
    assert_eq!(parsed["server"]["sidecar"]["port"].as_integer(), Some(9090));
}

#[test]
fn patch_json_config_path_set_create_missing_toml_hyphen_prefix_family_preserves_comment_owner() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(
            b"[service]\nsidecar-api-host = \"localhost\"\n# TLS setting\nsidecar-api-tls = false\nretries = 3\n",
        )
        .expect("toml fixture write should succeed");
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
        "TOML hyphen-prefix family insertion should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.contains(
            "[service]\nsidecar-api-host = \"localhost\"\nsidecar-api-port = 9000\n# TLS setting\nsidecar-api-tls = false\nretries = 3\n"
        ),
        "new hyphen-family key should insert before the following key's owned comment, got:\n{updated}"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_toml_root_prefix_family_before_table() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(
            b"sidecar_api_host = \"localhost\"\nsidecar_api_tls = false\n\n[service]\nretries = 3\n",
        )
        .expect("toml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "sidecar_api_port"
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
        "TOML root prefix-family insertion should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.contains(
            "sidecar_api_host = \"localhost\"\nsidecar_api_port = 9000\nsidecar_api_tls = false\n\n[service]\n"
        ),
        "root prefix-family insertion should stay before the table boundary, got:\n{updated}"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_toml_prefix_family_before_first_owned_comment() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(
            b"[service]\n# host setting\nsidecar_api_host = \"localhost\"\nsidecar_api_tls = false\nretries = 3\n",
        )
        .expect("toml fixture write should succeed");
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
        "TOML prefix-family insertion before first entry should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.contains(
            "[service]\nsidecar_api_enabled = true\n# host setting\nsidecar_api_host = \"localhost\"\nsidecar_api_tls = false\nretries = 3\n"
        ),
        "insertion before the first prefix-family entry should preserve that entry's owned comment, got:\n{updated}"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_toml_prefix_family_after_run_before_unrelated_key() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(
            b"[service]\nsidecar_api_host = \"localhost\"\nsidecar_api_tls = false\nsidecar_cache_host = \"localhost\"\n",
        )
        .expect("toml fixture write should succeed");
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
            "new_text": "\"http://localhost\"",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "TOML prefix-family insertion after run should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.contains(
            "[service]\nsidecar_api_host = \"localhost\"\nsidecar_api_tls = false\nsidecar_api_url = \"http://localhost\"\nsidecar_cache_host = \"localhost\"\n"
        ),
        "prefix-family insertion after the run should not drift past the next family, got:\n{updated}"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_toml_selects_later_sorted_group() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"[metadata]\nalpha = 1\nbeta = 2\n\nomega = 24\nzeta = 26\n")
        .expect("toml fixture write should succeed");
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
        "TOML insertion into later sorted group should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.contains("[metadata]\nalpha = 1\nbeta = 2\n\nomega = 24\nsigma = 25\nzeta = 26\n"),
        "new key should choose the sorted group whose bounds contain it, got:\n{updated}"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_quotes_toml_leaf_key_with_comments() {
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
        "quoted TOML leaf key create-missing should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.contains("\"listen.port\" = 9090"),
        "new leaf key should preserve literal key segment with TOML quotes, got:\n{updated}"
    );
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["server"]["listen.port"].as_integer(), Some(9090));
}

#[test]
fn patch_json_config_path_set_create_missing_creates_parent_before_toml_descendant_table_array() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# root-comment\n\n# descendant table-array comment\n[[server.sidecar.db]]\nhost = \"db\"\n")
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
        "parent TOML table before descendant table-array should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.contains(
            "# root-comment\n\n[server.sidecar]\nport = 9090\n\n# descendant table-array comment\n[[server.sidecar.db]]"
        ),
        "created parent table should precede the descendant table-array comment block, got:\n{updated}"
    );
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["server"]["sidecar"]["port"].as_integer(), Some(9090));
    assert_eq!(
        parsed["server"]["sidecar"]["db"][0]["host"].as_str(),
        Some("db")
    );
}

#[test]
fn patch_json_config_path_set_create_missing_keeps_descendant_toml_comment_with_descendant_table() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(
            b"# root-comment\n\n# sidecar db table comment\n[server.sidecar.db]\nhost = \"db\"\n",
        )
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
        "parent TOML table creation before descendant comment should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.contains(
            "# root-comment\n\n[server.sidecar]\nport = 9090\n\n# sidecar db table comment\n[server.sidecar.db]"
        ),
        "created parent table should not steal the descendant table comment, got:\n{updated}"
    );
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["server"]["sidecar"]["port"].as_integer(), Some(9090));
    assert_eq!(
        parsed["server"]["sidecar"]["db"]["host"].as_str(),
        Some("db")
    );
}

#[test]
fn patch_json_config_path_set_create_missing_preserves_toml_trailing_inline_comment() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\n[server]\nhost = \"127.0.0.1\" # keep-host-comment\n")
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
        "inline-comment TOML create-missing should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.contains("host = \"127.0.0.1\" # keep-host-comment\nport = 9090"),
        "new key should be inserted after the full inline-comment line, got:\n{updated}"
    );
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["server"]["port"].as_integer(), Some(9090));
}

#[test]
fn patch_json_config_path_set_create_missing_toml_dotted_table_with_comments() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\n[server.sidecar]\nhost = \"127.0.0.1\"\n")
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
        "dotted-table TOML create-missing should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(updated.contains("# keep-this-comment"));
    assert!(updated.contains("[server.sidecar]\nhost = \"127.0.0.1\"\nport = 9090\n"));
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["server"]["sidecar"]["port"].as_integer(), Some(9090));
}

#[test]
fn patch_json_config_path_set_create_missing_toml_rejects_array_index_path_with_comments() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\n[[servers]]\nhost = \"127.0.0.1\"\n")
        .expect("toml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "servers[0].port"
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
        "array-index TOML create-missing should stay rejected"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("array indexes are not auto-created")),
        "error should explain array index limitation"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(
        before, after,
        "rejected operation must not mutate TOML source"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_toml_preserves_table_tail_comment() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(
            b"# keep-this-comment\n[server]\nhost = \"127.0.0.1\"\n# keep-server-tail\n\n[database]\nurl = \"sqlite://db\"\n",
        )
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
        "tail-comment TOML create-missing should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.contains("# keep-server-tail\nport = 9090\n\n[database]"),
        "new key should preserve and follow the table tail comment, got:\n{updated}"
    );
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["server"]["port"].as_integer(), Some(9090));
}

#[test]
fn patch_json_config_path_set_create_missing_toml_comment_only_root_file() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-root-comment\n")
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
        "comment-only TOML create-missing should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert_eq!(updated, "# keep-root-comment\nenabled = true\n");
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["enabled"].as_bool(), Some(true));
}

#[test]
fn patch_json_config_path_set_create_missing_toml_root_leaf_before_table_array() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-root-comment\n[[servers]]\nhost = \"127.0.0.1\"\n")
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
        "root key before TOML table array should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.contains("# keep-root-comment\nenabled = true\n[[servers]]"),
        "root key should be inserted before the first table array, got:\n{updated}"
    );
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["enabled"].as_bool(), Some(true));
}

#[test]
fn patch_json_config_path_set_create_missing_toml_comment_invalid_value_does_not_mutate_file() {
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
            "new_text": "{invalid-toml",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(!output.status.success(), "invalid TOML value should fail");
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "invalid value failure must not mutate file");
}

#[test]
fn patch_json_config_path_set_create_missing_creates_toml_implicit_parent_table_with_comments() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\n[server.sidecar]\nhost = \"127.0.0.1\"\n")
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
        "implicit parent TOML table insertion should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    let parent_position = updated
        .find("[server]\nport = 9090")
        .expect("updated TOML should contain the created parent table");
    let child_position = updated
        .find("[server.sidecar]\nhost = \"127.0.0.1\"")
        .expect("updated TOML should keep the existing child table");
    assert!(
        parent_position < child_position,
        "new explicit parent table should be inserted before the existing child table, got:\n{updated}"
    );
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["server"]["port"].as_integer(), Some(9090));
    assert_eq!(
        parsed["server"]["sidecar"]["host"].as_str(),
        Some("127.0.0.1")
    );
}

#[test]
fn patch_json_config_path_set_create_missing_toml_parent_table_before_subtable() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\n[server]\nhost = \"127.0.0.1\"\n\n[server.sidecar]\nenabled = true\n")
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
        "parent-table TOML create-missing should insert before subtable: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.contains("[server]\nhost = \"127.0.0.1\"\nport = 9090\n\n[server.sidecar]"),
        "new key should stay in parent table, got:\n{updated}"
    );
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["server"]["port"].as_integer(), Some(9090));
    assert_eq!(parsed["server"]["sidecar"]["enabled"].as_bool(), Some(true));
}

#[test]
fn patch_json_config_path_set_create_missing_toml_multiline_string_value_with_comments() {
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
            "path": "server.description"
        },
        "op": {
            "type": "set",
            "new_text": "\"\"\"line one\nline two\"\"\"",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "multiline TOML value create-missing should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(updated.contains("description = \"\"\"line one\nline two\"\"\""));
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(
        parsed["server"]["description"].as_str(),
        Some("line one\nline two")
    );
}

#[test]
fn patch_json_config_path_set_create_missing_toml_root_leaf_keeps_comment_separator() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# root comment\n# second root comment\n\n[server]\nhost = \"127.0.0.1\"\n")
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
        "root key with separator should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.contains("# second root comment\nenabled = true\n\n[server]"),
        "root key should preserve blank separator before first table, got:\n{updated}"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_toml_table_before_table_array() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\n[server]\nhost = \"127.0.0.1\"\n\n[[servers]]\nhost = \"127.0.0.2\"\n")
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
        "TOML create-missing before table array should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(
        updated.contains("[server]\nhost = \"127.0.0.1\"\nport = 9090\n\n[[servers]]"),
        "new key should be inserted before the table array, got:\n{updated}"
    );
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["server"]["port"].as_integer(), Some(9090));
}

#[test]
fn patch_json_config_path_toml_comment_create_missing_preserves_file_context() {
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
            "new_text": "9090",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "TOML create-missing should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert!(after.contains("# keep-this-comment"));
    assert!(after.contains("host = \"127.0.0.1\""));
    assert!(after.contains("port = 9090"));
}

#[test]
fn patch_json_config_path_missing_path_without_create_missing_bypasses_toml_comment_guard() {
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
            "new_text": "9090"
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
fn patch_flag_config_path_create_missing_inserts_toml_leaf_with_comments() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\n[server]\nhost = \"127.0.0.1\"\n")
        .expect("toml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let output = run_identedit(&[
        "patch",
        "--config-path",
        "server.port",
        "--set-value",
        "9090",
        "--create-missing",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "flag-mode TOML create-missing should preserve comments: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(updated.contains("# keep-this-comment"));
    assert!(updated.contains("host = \"127.0.0.1\""));
    assert!(updated.contains("port = 9090"));
}

#[test]
fn patch_flag_config_path_create_missing_toml_comment_preserves_comment() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\n[server]\nhost = \"127.0.0.1\"\n")
        .expect("toml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let output = run_identedit(&[
        "patch",
        "--config-path",
        "server.port",
        "--set-value",
        "9090",
        "--create-missing",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "operation should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(updated.contains("# keep-this-comment"));
    assert!(updated.contains("port = 9090"));
}

#[test]
fn patch_flag_toml_comment_create_missing_mutates_only_target_table() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\n[server]\nhost = \"127.0.0.1\"\n")
        .expect("toml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let output = run_identedit(&[
        "patch",
        "--config-path",
        "server.port",
        "--set-value",
        "9090",
        "--create-missing",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "operation should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_ne!(before, after, "successful operation should mutate file");
    assert!(after.contains("# keep-this-comment"));
    assert!(after.contains("host = \"127.0.0.1\""));
    assert!(after.contains("port = 9090"));
}

#[test]
fn patch_json_create_missing_existing_toml_path_with_hash_precondition_preserves_comment() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\n[server]\nport = 8080\nhost = \"127.0.0.1\"\n")
        .expect("toml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "server.port",
            "expected_file_hash": identedit::hash::hash_text(&before)
        },
        "op": {
            "type": "set",
            "new_text": "9090",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(output.status.success(), "operation should succeed");
    let updated = fs::read_to_string(&file_path).expect("updated file should be readable");
    assert!(updated.contains("# keep-this-comment"));
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["server"]["port"].as_integer(), Some(9090));
}

#[test]
fn patch_json_create_missing_toml_comment_fallback_with_stale_hash_fails_precondition_first() {
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
            "path": "server.port",
            "expected_file_hash": "deadbeefdeadbeef"
        },
        "op": {
            "type": "set",
            "new_text": "9090",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(!output.status.success(), "operation should fail");
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "precondition_failed");
}

#[test]
fn patch_json_config_path_set_create_missing_accepts_toml_string_value_with_hash_and_comment() {
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
            "new_text": r#""literal # inside" # trailing comment"#,
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "TOML string value with hash and trailing comment should be accepted: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated TOML should be readable");
    assert!(updated.contains("note = \"literal # inside\" # trailing comment"));
    let parsed: toml::Value = toml::from_str(&updated).expect("updated TOML should stay valid");
    assert_eq!(parsed["server"]["note"].as_str(), Some("literal # inside"));
}
