use super::*;

#[test]
fn patch_json_config_path_set_create_missing_yaml_comment_accepts_multiline_block_scalar_value() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  # keep service config\n  name: identedit\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.description"
        },
        "op": {
            "type": "set",
            "new_text": "|\n  hello\n  world\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "comment-preserving YAML create-missing should accept explicit block scalar value text: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(
        parsed["service"]["description"].as_str(),
        Some("hello\nworld\n")
    );
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_comment_creates_folded_block_scalar() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(
            b"metadata:\n  annotations:\n    # keep annotation comment\n    existing: keep\n",
        )
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "metadata.annotations.summary"
        },
        "op": {
            "type": "set",
            "new_text": ">-\n  first line\n  second line\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML folded block scalar create-missing should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(
        updated.contains("    summary: >-\n      first line\n      second line\n"),
        "folded scalar should be rebased under the target mapping: {updated}"
    );
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(
        parsed["metadata"]["annotations"]["summary"].as_str(),
        Some("first line second line")
    );
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_comment_preserves_keep_chomp_blank_line() {
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
            "new_text": "|+\n  echo done\n\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML block scalar create-missing should preserve explicit keep-chomp trailing blank lines: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed["scripts"]["install"].as_str(), Some("echo done\n\n"));
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_comment_accepts_empty_block_scalar_body() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"data:\n  # keep data config\n  existing: value\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "data.empty"
        },
        "op": {
            "type": "set",
            "new_text": "|\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML block scalar create-missing should accept an empty scalar body: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed["data"]["empty"].as_str(), Some(""));
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_comment_keeps_document_marker_as_scalar_text() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"data:\n  # keep data config\n  existing: value\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "data.manifest"
        },
        "op": {
            "type": "set",
            "new_text": "|\n  ---\n  apiVersion: v1\n  kind: ConfigMap\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML block scalar create-missing should keep document marker-looking text inside the scalar: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(
        parsed["data"]["manifest"].as_str(),
        Some("---\napiVersion: v1\nkind: ConfigMap\n")
    );
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_comment_normalizes_crlf_block_fragment_to_source_lf()
 {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"data:\n  # keep data config\n  existing: value\n")
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
            "new_text": "|\r\n  echo start\r\n  echo done\r\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML block scalar create-missing should normalize CRLF value fragments to the source newline style: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read(&file_path).expect("updated YAML should be readable");
    assert!(
        !updated.windows(2).any(|window| window == b"\r\n"),
        "LF source should not gain CRLF from the value fragment"
    );
    let updated_text = String::from_utf8(updated).expect("updated YAML should stay UTF-8");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated_text).expect("updated YAML should stay valid");
    assert_eq!(
        parsed["data"]["script"].as_str(),
        Some("echo start\necho done\n")
    );
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_comment_block_scalar_quotes_colon_space_key() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"data:\n  # keep data config\n  existing: value\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": r#"data["app: conf"]"#
        },
        "op": {
            "type": "set",
            "new_text": "|\n  port=8080\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML block scalar create-missing should quote keys that cannot be safely rendered as plain scalars: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed["data"]["app: conf"].as_str(), Some("port=8080\n"));
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_comment_preserves_multiple_keep_chomp_blank_lines()
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
            "new_text": "|+\n  echo done\n\n\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML block scalar create-missing should preserve multiple keep-chomp blank lines: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(
        parsed["scripts"]["install"].as_str(),
        Some("echo done\n\n\n")
    );
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_comment_block_scalar_no_final_newline_source() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"data:\n  # keep data config\n  existing: value")
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
            "new_text": "|\n  echo done\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML block scalar create-missing should handle sources without a final newline: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed["data"]["script"].as_str(), Some("echo done\n"));
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_comment_preserves_strip_chomp_semantics() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"data:\n  # keep data config\n  existing: value\n")
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
            "new_text": "|-\n  echo done\n\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML block scalar create-missing should preserve strip chomp semantics: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed["data"]["script"].as_str(), Some("echo done"));
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_comment_keeps_tabs_as_scalar_content() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"data:\n  # keep data config\n  existing: value\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "data.makefile"
        },
        "op": {
            "type": "set",
            "new_text": "|\n  target:\n  \tmake build\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML block scalar create-missing should preserve tabs after the scalar indentation: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(
        parsed["data"]["makefile"].as_str(),
        Some("target:\n\tmake build\n")
    );
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_comment_block_scalar_under_created_parent() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  # keep service config\n  name: identedit\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service.generated.script"
        },
        "op": {
            "type": "set",
            "new_text": "|\n  echo generated\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML block scalar create-missing should work below newly created intermediate mappings: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(
        parsed["service"]["generated"]["script"].as_str(),
        Some("echo generated\n")
    );
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_comment_block_scalar_whitespace_only_root() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b" \n  \n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "script"
        },
        "op": {
            "type": "set",
            "new_text": "|\n  echo root\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML block scalar create-missing should bootstrap whitespace-only YAML documents: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed["script"].as_str(), Some("echo root\n"));
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_comment_block_scalar_preserves_crlf_newlines() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"data:\r\n  # keep data comment\r\n  existing: value\r\n")
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
            "new_text": "|\n  echo start\n  echo done\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML block scalar create-missing should preserve CRLF source style: {}",
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
    let updated_text = String::from_utf8(updated).expect("updated YAML should stay UTF-8");
    assert!(updated_text.contains("script: |\r\n    echo start\r\n    echo done\r\n"));
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_comment_only_root_accepts_block_scalar_leaf() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"# generated config\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "script"
        },
        "op": {
            "type": "set",
            "new_text": "|\n  echo start\n  echo done\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "comment-only YAML root should support block scalar leaf creation: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert_eq!(
        updated,
        "# generated config\nscript: |\n  echo start\n  echo done\n"
    );
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_comment_sequence_nested_parent_block_scalar() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"jobs:\n  build:\n    steps:\n      - name: setup\n        # keep step\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "jobs.build.steps[0].env.SCRIPT"
        },
        "op": {
            "type": "set",
            "new_text": "|\n  echo nested\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML block scalar create-missing should create nested mapping parents under sequence-item mappings: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(
        parsed["jobs"]["build"]["steps"][0]["env"]["SCRIPT"].as_str(),
        Some("echo nested\n")
    );
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_comment_keeps_mapping_like_lines_as_scalar_content()
 {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"data:\n  # keep data config\n  existing: value\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "data.fragment"
        },
        "op": {
            "type": "set",
            "new_text": "|\n  key: value\n  - list-looking item\n  # comment-looking text\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML block scalar create-missing should keep mapping/list/comment-looking lines as scalar content: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(
        parsed["data"]["fragment"].as_str(),
        Some("key: value\n- list-looking item\n# comment-looking text\n")
    );
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_comment_exact_hash_allows_block_scalar() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"data:\n  # keep data config\n  existing: value\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": r#"data["script: install"]"#,
            "expected_file_hash": identedit::hash::hash_text(&before)
        },
        "op": {
            "type": "set",
            "new_text": "|\n  echo exact\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML block scalar create-missing should honor exact file hash preconditions: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(
        parsed["data"]["script: install"].as_str(),
        Some("echo exact\n")
    );
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_comment_normalizes_mixed_newline_block_fragment()
{
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"data:\r\n  # keep data config\r\n  existing: value\r\n")
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
            "new_text": "|\r\n  echo crlf\n  echo lf\r  echo cr\r\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML block scalar create-missing should normalize mixed fragment newlines to the source newline style: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read(&file_path).expect("updated YAML should be readable");
    for (index, byte) in updated.iter().enumerate() {
        if *byte == b'\n' {
            assert!(
                index > 0 && updated[index - 1] == b'\r',
                "CRLF source should not gain bare LF at byte {index}"
            );
        }
    }
    let parsed_text = String::from_utf8(updated)
        .expect("updated YAML should stay UTF-8")
        .replace("\r\n", "\n");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&parsed_text).expect("updated YAML should stay valid");
    assert_eq!(
        parsed["data"]["script"].as_str(),
        Some("echo crlf\necho lf\necho cr\n")
    );
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_comment_preserves_cr_only_source_style_for_block_scalar()
 {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"data:\r  # keep data config\r  existing: value\r")
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
            "new_text": "|\r  echo start\r  echo done\r",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML block scalar create-missing should preserve CR-only source newline style: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read(&file_path).expect("updated YAML should be readable");
    assert!(
        !updated.contains(&b'\n'),
        "CR-only source should not gain LF bytes"
    );
    let normalized = String::from_utf8(updated)
        .expect("updated YAML should stay UTF-8")
        .replace('\r', "\n");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&normalized).expect("updated YAML should stay valid");
    assert_eq!(
        parsed["data"]["script"].as_str(),
        Some("echo start\necho done\n")
    );
}
