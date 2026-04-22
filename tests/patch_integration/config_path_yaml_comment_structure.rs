use super::*;

#[test]
fn patch_json_config_path_set_create_missing_yaml_comment_inserts_before_next_root_key() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  # keep service config\n  name: identedit\nother: true\n")
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
        "nested insert before next root key should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(
        updated.contains(
            "service:\n  # keep service config\n  name: identedit\n  sidecar:\n    port: 9000\nother: true"
        ),
        "inserted mapping must stay under service, not after other: {updated}"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_comment_creates_nested_intermediate_mappings() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"# root comment\nservice:\n  # keep service config\n  name: identedit\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.sidecar.http.port"
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
        "nested YAML create-missing with comments should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(
        updated.contains(
            "# root comment\nservice:\n  # keep service config\n  name: identedit\n  sidecar:\n    http:\n      port: 9000\n"
        ),
        "inserted nested mapping should stay under service with stable indentation: {updated}"
    );
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(
        parsed["service"]["sidecar"]["http"]["port"].as_i64(),
        Some(9000)
    );
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_comment_creates_key_inside_sequence_mapping() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"# services\nservices:\n  - name: api\n    # keep item comment\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "services[0].port"
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
        "YAML create-missing should traverse existing sequence item mapping: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(
        updated.contains("services:\n  - name: api\n    # keep item comment\n    port: 9000\n"),
        "sequence item insertion should align with the compact mapping keys: {updated}"
    );
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed["services"][0]["port"].as_i64(), Some(9000));
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_comment_creates_nested_key_inside_sequence_mapping()
 {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"# services\nservices:\n  - name: api\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "services[0].sidecar.http.port"
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
        "YAML create-missing should render nested mappings under existing sequence item: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(
        updated
            .contains("services:\n  - name: api\n    sidecar:\n      http:\n        port: 9000\n"),
        "nested sequence item insertion should avoid '- sidecar' misindentation: {updated}"
    );
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(
        parsed["services"][0]["sidecar"]["http"]["port"].as_i64(),
        Some(9000)
    );
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_comment_handles_no_final_newline() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  # keep service config\n  name: identedit")
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
            "new_text": "true",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "no-final-newline YAML insert should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(updated.ends_with('\n'));
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed["service"]["enabled"].as_bool(), Some(true));
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_comment_preserves_crlf_newlines() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\r\n  # keep service config\r\n  name: identedit\r\n")
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
            "new_text": "true",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "commented CRLF YAML insert should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read(&file_path).expect("updated YAML should be readable");
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
fn patch_json_config_path_set_create_missing_yaml_comment_creates_github_actions_run_block() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(
            b"name: ci\njobs:\n  build:\n    steps:\n      - name: test\n        # keep step comment\n",
        )
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "jobs.build.steps[0].run"
        },
        "op": {
            "type": "set",
            "new_text": "|\n  cargo test\n  cargo clippy --all-targets -- -D warnings\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML block scalar create-missing should support GitHub Actions run blocks: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(
        updated.contains(
            "      - name: test\n        # keep step comment\n        run: |\n          cargo test\n          cargo clippy --all-targets -- -D warnings\n"
        ),
        "run block should be inserted under the existing sequence item mapping: {updated}"
    );
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(
        parsed["jobs"]["build"]["steps"][0]["run"].as_str(),
        Some("cargo test\ncargo clippy --all-targets -- -D warnings\n")
    );
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_comment_creates_kubernetes_configmap_block() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(
            b"apiVersion: v1\nkind: ConfigMap\ndata:\n  # keep data comment\n  existing: value\n",
        )
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": r#"data["app.conf"]"#
        },
        "op": {
            "type": "set",
            "new_text": "|-\n  port=8080\n  debug=false\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML block scalar create-missing should support ConfigMap data strings: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(
        updated.contains("  app.conf: |-\n    port=8080\n    debug=false\n"),
        "ConfigMap block scalar should be inserted as a literal key value: {updated}"
    );
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(
        parsed["data"]["app.conf"].as_str(),
        Some("port=8080\ndebug=false")
    );
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_comment_preserves_relative_block_indent() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"scripts:\n  # keep script config\n  existing: true\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "scripts.install"
        },
        "op": {
            "type": "set",
            "new_text": "|\n    if test -n \"$CI\"; then\n      echo ci\n    fi\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML block scalar create-missing should preserve relative body indentation: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(
        parsed["scripts"]["install"].as_str(),
        Some("if test -n \"$CI\"; then\n  echo ci\nfi\n")
    );
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_comment_ignores_space_only_blank_lines_for_indent()
 {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"scripts:\n  # keep script config\n  existing: true\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "scripts.install"
        },
        "op": {
            "type": "set",
            "new_text": "|\n    echo before\n  \n    echo after\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML block scalar create-missing should not let whitespace-only blank lines skew base indentation: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(
        parsed["scripts"]["install"].as_str(),
        Some("echo before\n\necho after\n")
    );
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_comment_preserves_leading_empty_scalar_line() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"scripts:\n  # keep script config\n  existing: true\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "scripts.install"
        },
        "op": {
            "type": "set",
            "new_text": "|\n\n  echo after blank\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML block scalar create-missing should preserve a leading empty scalar line: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(
        parsed["scripts"]["install"].as_str(),
        Some("\necho after blank\n")
    );
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_comment_accepts_header_trailing_spaces() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"scripts:\n  # keep script config\n  existing: true\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "scripts.install"
        },
        "op": {
            "type": "set",
            "new_text": ">-   \n  first line\n  second line\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML block scalar create-missing should accept harmless trailing spaces on the block header: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(
        parsed["scripts"]["install"].as_str(),
        Some("first line second line")
    );
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_comment_inserts_block_before_next_root_key() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"data:\n  # keep data config\n  existing: value\nmetadata:\n  name: sample\n")
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
            "new_text": "|\n  echo before metadata\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML block scalar create-missing should insert inside the target mapping before the next root key: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(
        updated.contains("  existing: value\n  script: |\n    echo before metadata\nmetadata:"),
        "block scalar should stay inside data mapping before metadata: {updated}"
    );
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(
        parsed["data"]["script"].as_str(),
        Some("echo before metadata\n")
    );
    assert_eq!(parsed["metadata"]["name"].as_str(), Some("sample"));
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_comment_only_root_adds_key_after_comment() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"# intentionally empty config\n")
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
        "comment-only YAML root create-missing should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert_eq!(updated, "# intentionally empty config\nenabled: true\n");
}
