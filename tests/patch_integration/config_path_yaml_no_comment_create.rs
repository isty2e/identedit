use super::*;

#[test]
fn patch_json_config_path_set_create_missing_yaml_no_comment_rejects_folded_indent_indicator() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"data:\n  existing: value\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "data.message"
        },
        "op": {
            "type": "set",
            "new_text": ">2\n  unsupported\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "no-comment YAML create-missing should reject explicit folded block indent indicators too"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "rejected operation must not mutate YAML");
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_no_comment_quotes_nan_like_key() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"data:\n  existing: value\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": r#"data[".nan"]"#
        },
        "op": {
            "type": "set",
            "new_text": "|\n  nan-like key\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "no-comment YAML create-missing should quote .nan-like keys: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(
        updated.contains("\".nan\": |"),
        ".nan-like key must be quoted: {updated}"
    );
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed["data"][".nan"].as_str(), Some("nan-like key\n"));
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_no_comment_quotes_question_key() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"data:\n  existing: value\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": r#"data["?query"]"#
        },
        "op": {
            "type": "set",
            "new_text": "|\n  question key\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML create-missing should quote question-mark-prefixed keys: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(
        updated.contains("\"?query\": |"),
        "question-mark-prefixed key must be quoted: {updated}"
    );
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed["data"]["?query"].as_str(), Some("question key\n"));
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_no_comment_quotes_colon_prefix_key() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"data:\n  existing: value\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": r#"data[":port"]"#
        },
        "op": {
            "type": "set",
            "new_text": "|\n  colon key\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML create-missing should quote colon-prefixed keys: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(
        updated.contains("\":port\": |"),
        "colon-prefixed key must be quoted: {updated}"
    );
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed["data"][":port"].as_str(), Some("colon key\n"));
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_no_comment_rejects_sequence_auto_create_midpath()
{
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"data:\n  existing: value\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "data.steps[0].run"
        },
        "op": {
            "type": "set",
            "new_text": "|\n  echo no auto sequence\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "YAML create-missing should reject auto-creating sequence indexes in the missing path"
    );
    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "rejected operation must not mutate YAML");
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_no_comment_quotes_at_prefixed_key() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"data:\n  existing: value\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": r#"data["@scope"]"#
        },
        "op": {
            "type": "set",
            "new_text": "|\n  scoped key\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML create-missing should quote @-prefixed keys: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(
        updated.contains("\"@scope\": |"),
        "@-prefixed key must be quoted: {updated}"
    );
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed["data"]["@scope"].as_str(), Some("scoped key\n"));
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_no_comment_accepts_header_trailing_spaces() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"data:\n  existing: value\n")
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
            "new_text": "|   \n  echo spaces\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML block scalar headers with trailing spaces should be accepted in no-comment create-missing too: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed["data"]["script"].as_str(), Some("echo spaces\n"));
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_no_comment_quotes_embedded_quote_key() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"data:\n  existing: value\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": r#"data["quote\"key"]"#
        },
        "op": {
            "type": "set",
            "new_text": "|\n  quoted key\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML create-missing should quote and escape embedded quote keys: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed["data"]["quote\"key"].as_str(), Some("quoted key\n"));
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_no_comment_quotes_backslash_key() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"data:\n  existing: value\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": r#"data["backslash\\key"]"#
        },
        "op": {
            "type": "set",
            "new_text": "|\n  backslash key\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML create-missing should quote and escape backslash keys: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(
        parsed["data"]["backslash\\key"].as_str(),
        Some("backslash key\n")
    );
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_no_comment_quotes_newline_key() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"data:\n  existing: value\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "data[\"line\\nkey\"]"
        },
        "op": {
            "type": "set",
            "new_text": "|\n  newline key\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML create-missing should quote newline-containing keys without splitting the mapping: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed["data"]["line\nkey"].as_str(), Some("newline key\n"));
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_no_comment_quotes_percent_and_backtick_keys() {
    for key in ["%percent", "`tick"] {
        let mut temp_file = Builder::new()
            .suffix(".yaml")
            .tempfile()
            .expect("temp yaml file should be created");
        temp_file
            .write_all(b"data:\n  existing: value\n")
            .expect("yaml fixture write should succeed");
        let file_path = temp_file.keep().expect("temp file should persist").1;

        let request = json!({
            "command": "patch",
            "file": file_path.to_string_lossy().to_string(),
            "target": {
                "type": "config_path",
                "path": format!(r#"data["{key}"]"#)
            },
            "op": {
                "type": "set",
                "new_text": "|\n  indicator key\n",
                "create_missing": true
            }
        });

        let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
        assert!(
            output.status.success(),
            "YAML create-missing should quote flow/reserved indicator key {key:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
        let parsed: serde_yaml::Value =
            serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
        assert_eq!(parsed["data"][key].as_str(), Some("indicator key\n"));
    }
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_no_comment_quotes_tab_key() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"data:\n  existing: value\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "data[\"tab\\tkey\"]"
        },
        "op": {
            "type": "set",
            "new_text": "|\n  tab key\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML create-missing should quote tab-containing keys: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed["data"]["tab\tkey"].as_str(), Some("tab key\n"));
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_no_comment_quotes_space_padded_key() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"data:\n  existing: value\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": r#"data[" padded "]"#
        },
        "op": {
            "type": "set",
            "new_text": "|\n  padded key\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML create-missing should quote space-padded keys: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed["data"][" padded "].as_str(), Some("padded key\n"));
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_no_comment_quotes_comment_like_key() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"data:\n  existing: value\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": r#"data["value # not comment"]"#
        },
        "op": {
            "type": "set",
            "new_text": "|\n  comment-like key\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML create-missing should quote keys containing comment delimiters: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(
        parsed["data"]["value # not comment"].as_str(),
        Some("comment-like key\n")
    );
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_no_comment_quotes_nested_colon_space_key() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"data:\n  existing: value\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": r#"data.parent["child: key"]"#
        },
        "op": {
            "type": "set",
            "new_text": "|\n  nested colon key\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML create-missing should quote nested colon-space keys: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(
        parsed["data"]["parent"]["child: key"].as_str(),
        Some("nested colon key\n")
    );
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_no_comment_quotes_anchor_like_key() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"data:\n  existing: value\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": r#"data["&anchor"]"#
        },
        "op": {
            "type": "set",
            "new_text": "|\n  anchor-looking key\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML create-missing should quote anchor-looking keys: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(
        parsed["data"]["&anchor"].as_str(),
        Some("anchor-looking key\n")
    );
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_no_comment_quotes_alias_like_key() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"data:\n  existing: value\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": r#"data["*alias"]"#
        },
        "op": {
            "type": "set",
            "new_text": "|\n  alias-looking key\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML create-missing should quote alias-looking keys: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(
        parsed["data"]["*alias"].as_str(),
        Some("alias-looking key\n")
    );
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_no_comment_quotes_tag_like_key() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"data:\n  existing: value\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": r#"data["!tag"]"#
        },
        "op": {
            "type": "set",
            "new_text": "|\n  tag-looking key\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML create-missing should quote tag-looking keys: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed["data"]["!tag"].as_str(), Some("tag-looking key\n"));
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_no_comment_quotes_brace_like_key() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"data:\n  existing: value\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": r#"data["{brace}"]"#
        },
        "op": {
            "type": "set",
            "new_text": "|\n  brace key\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML create-missing should quote brace-like keys: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed["data"]["{brace}"].as_str(), Some("brace key\n"));
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_no_comment_quotes_comma_key() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"data:\n  existing: value\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": r#"data[",comma"]"#
        },
        "op": {
            "type": "set",
            "new_text": "|\n  comma key\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML create-missing should quote comma-prefixed keys: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed["data"][",comma"].as_str(), Some("comma key\n"));
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_no_comment_quotes_bracket_key() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"data:\n  existing: value\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": r#"data["[bracket]"]"#
        },
        "op": {
            "type": "set",
            "new_text": "|\n  bracket key\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML create-missing should quote bracket-looking keys: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed["data"]["[bracket]"].as_str(), Some("bracket key\n"));
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_no_comment_quotes_pipe_key() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"data:\n  existing: value\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": r#"data["|pipe"]"#
        },
        "op": {
            "type": "set",
            "new_text": "|\n  pipe key\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML create-missing should quote pipe-prefixed keys: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed["data"]["|pipe"].as_str(), Some("pipe key\n"));
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_no_comment_quotes_greater_key() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"data:\n  existing: value\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": r#"data[">fold"]"#
        },
        "op": {
            "type": "set",
            "new_text": "|\n  greater key\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML create-missing should quote greater-than-prefixed keys: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed["data"][">fold"].as_str(), Some("greater key\n"));
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_no_comment_nested_scalar_is_surgical() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\n  name: app\nnext: keep\n")
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
        "YAML no-comment create-missing should use the surgical insertion path: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert_eq!(
        updated,
        "service:\n  name: app\n  enabled: true\nnext: keep\n"
    );
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed["service"]["enabled"].as_bool(), Some(true));
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_no_comment_preserves_cr_only_newlines() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service:\r  name: app\rnext: keep\r")
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
        "YAML create-missing should preserve CR-only source newline style: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    assert!(
        updated.contains("\r  enabled: true\r"),
        "inserted YAML line should use CR-only newlines in a CR-only file"
    );
    let normalized = updated.replace('\r', "\n");
    let parsed: serde_yaml::Value = serde_yaml::from_str(&normalized)
        .expect("updated YAML should stay valid after newline normalization");
    assert_eq!(parsed["service"]["enabled"].as_bool(), Some(true));
}
