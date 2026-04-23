use super::*;

#[test]
fn patch_json_config_path_set_create_missing_rejects_toml_inline_table_intermediate_parent_with_comments()
 {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\nserver = { host = \"127.0.0.1\" }\n")
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
        "inline-table intermediate parent should stay rejected"
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
fn patch_json_config_path_set_create_missing_rejects_toml_dotted_key_root_table_redeclaration_with_comments()
 {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\nserver.host = \"127.0.0.1\"\n")
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
        "dotted-key root table redeclaration must be rejected"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("already resolves to a TOML value")),
        "error should explain existing value conflict"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(
        before, after,
        "rejected operation must not mutate TOML source"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_rejects_toml_deep_dotted_key_parent_redeclaration_with_comments()
 {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\nserver.sidecar.db.host = \"127.0.0.1\"\n")
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
        "deep dotted-key parent table redeclaration must be rejected"
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
fn patch_json_config_path_set_create_missing_rejects_toml_quoted_table_array_parent_conflict() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\n[[server.\"side.car\"]]\nhost = \"127.0.0.1\"\n")
        .expect("toml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": r#"server["side.car"].port"#
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
        "quoted table-array parent must not be promoted into a standard table"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("array of tables")),
        "error should explain table-array conflict"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(
        before, after,
        "rejected operation must not mutate TOML source"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_rejects_toml_ancestor_table_array_conflict() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\n[[server]]\nhost = \"127.0.0.1\"\n")
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
        "ancestor table-array must block descendant standard table creation"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("array of tables")),
        "error should explain ancestor table-array conflict"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(
        before, after,
        "rejected operation must not mutate TOML source"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_rejects_toml_datetime_parent_conflict() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\nserver = 1979-05-27T07:32:00Z\n")
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
        "datetime-valued TOML parent must not be promoted into a standard table"
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
fn patch_json_config_path_set_create_missing_rejects_toml_table_array_parent_conflict_with_comments()
 {
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
            "path": "servers.port"
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
        "standard table creation must reject existing table-array parents"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("array of tables")),
        "error should explain the table-array conflict"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(
        before, after,
        "rejected operation must not mutate TOML source"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_rejects_toml_dotted_key_table_redeclaration_with_comments()
 {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\nserver.sidecar.host = \"127.0.0.1\"\n")
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
        "dotted-key implicit table redeclaration must be rejected"
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
fn patch_json_config_path_set_create_missing_rejects_toml_inline_table_with_comments() {
    let mut temp_file = Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temp toml file should be created");
    temp_file
        .write_all(b"# keep-this-comment\nserver = { host = \"127.0.0.1\" }\n")
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
        "inline-table TOML create-missing should stay rejected"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("already resolves to a TOML value")),
        "error should explain existing value conflict"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(
        before, after,
        "rejected operation must not mutate TOML source"
    );
}
