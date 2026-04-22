use super::*;

#[test]
fn patch_json_config_path_set_create_missing_yaml_comment_quotes_boolean_like_key() {
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
            "path": r#"data["true"]"#
        },
        "op": {
            "type": "set",
            "new_text": "|\n  enabled flag text\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML block scalar create-missing should quote boolean-looking string keys: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed["data"]["true"].as_str(), Some("enabled flag text\n"));
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_comment_quotes_null_like_key() {
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
            "path": r#"data["null"]"#
        },
        "op": {
            "type": "set",
            "new_text": "|\n  null literal text\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML block scalar create-missing should quote null-looking string keys: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed["data"]["null"].as_str(), Some("null literal text\n"));
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_comment_quotes_other_implicit_scalar_keys() {
    for key in ["123", "~", "yes", "on", "off"] {
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
                "path": format!(r#"data["{key}"]"#)
            },
            "op": {
                "type": "set",
                "new_text": "|\n  implicit scalar key text\n",
                "create_missing": true
            }
        });

        let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
        assert!(
            output.status.success(),
            "YAML block scalar create-missing should quote implicit-scalar-like key {key:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
        let parsed: serde_yaml::Value =
            serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
        assert_eq!(
            parsed["data"][key].as_str(),
            Some("implicit scalar key text\n"),
            "key {key:?} should round-trip as a string mapping key"
        );
    }
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_comment_quotes_numeric_and_timestamp_like_keys() {
    for key in ["0", "1e3", "0x10", ".nan", "2026-04-22"] {
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
                "path": format!(r#"data["{key}"]"#)
            },
            "op": {
                "type": "set",
                "new_text": "|\n  scalar-looking key text\n",
                "create_missing": true
            }
        });

        let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
        assert!(
            output.status.success(),
            "YAML block scalar create-missing should quote numeric/date-like key {key:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
        let parsed: serde_yaml::Value =
            serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
        assert_eq!(
            parsed["data"][key].as_str(),
            Some("scalar-looking key text\n"),
            "key {key:?} should round-trip as a string mapping key"
        );
    }
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_comment_quotes_flow_indicator_keys() {
    for key in [
        "[bracket]",
        "{brace}",
        "?question",
        ":colon",
        "%percent",
        "@at",
        "`tick",
    ] {
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
                "path": format!(r#"data["{key}"]"#)
            },
            "op": {
                "type": "set",
                "new_text": "|\n  indicator key text\n",
                "create_missing": true
            }
        });

        let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
        assert!(
            output.status.success(),
            "YAML block scalar create-missing should quote indicator-like key {key:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
        let parsed: serde_yaml::Value =
            serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
        assert_eq!(
            parsed["data"][key].as_str(),
            Some("indicator key text\n"),
            "key {key:?} should round-trip as a string mapping key"
        );
    }
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_comment_round_trips_space_padded_key() {
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
            "path": "data[\" leading and trailing \"]"
        },
        "op": {
            "type": "set",
            "new_text": "|\n  padded key text\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML block scalar create-missing should preserve leading/trailing spaces in quoted keys: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(
        parsed["data"][" leading and trailing "].as_str(),
        Some("padded key text\n")
    );
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_comment_round_trips_newline_key() {
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
            "path": "data[\"line\\nbreak\"]"
        },
        "op": {
            "type": "set",
            "new_text": "|\n  newline key text\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML block scalar create-missing should safely quote key segments containing escaped newlines: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(
        parsed["data"]["line\nbreak"].as_str(),
        Some("newline key text\n")
    );
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_comment_quotes_comment_like_key() {
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
            "path": r#"data["app # conf"]"#
        },
        "op": {
            "type": "set",
            "new_text": "|\n  hash text\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML block scalar create-missing should quote keys that would otherwise start comments: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed["data"]["app # conf"].as_str(), Some("hash text\n"));
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_comment_quotes_dash_prefixed_key() {
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
            "path": r#"data["-dash"]"#
        },
        "op": {
            "type": "set",
            "new_text": "|\n  dash text\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML block scalar create-missing should quote dash-prefixed mapping keys: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed["data"]["-dash"].as_str(), Some("dash text\n"));
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_comment_quotes_escaped_key() {
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
            "path": r#"data["quote\"slash\\key"]"#
        },
        "op": {
            "type": "set",
            "new_text": "|\n  escaped key text\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML block scalar create-missing should escape quoted key segments safely: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(
        parsed["data"]["quote\"slash\\key"].as_str(),
        Some("escaped key text\n")
    );
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_comment_keeps_unicode_plain_key() {
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
            "path": r#"data["emoji-🦀"]"#
        },
        "op": {
            "type": "set",
            "new_text": "|\n  crab text\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML block scalar create-missing should handle unicode key text: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed["data"]["emoji-🦀"].as_str(), Some("crab text\n"));
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_comment_handles_double_quoted_existing_parent() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"\"data: bucket\":\n  # keep bucket config\n  existing: value\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": r#"["data: bucket"].script"#
        },
        "op": {
            "type": "set",
            "new_text": "|\n  echo quoted parent\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML block scalar create-missing should resolve double-quoted existing parent keys: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(
        parsed["data: bucket"]["script"].as_str(),
        Some("echo quoted parent\n")
    );
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_comment_quotes_unsafe_intermediate_key() {
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
            "path": r#"data["app: conf"].script"#
        },
        "op": {
            "type": "set",
            "new_text": "|\n  echo unsafe parent\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML block scalar create-missing should quote unsafe intermediate keys: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(
        parsed["data"]["app: conf"]["script"].as_str(),
        Some("echo unsafe parent\n")
    );
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_comment_handles_single_quoted_existing_parent() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"'data: bucket':\n  # keep bucket config\n  existing: value\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": r#"["data: bucket"].script"#
        },
        "op": {
            "type": "set",
            "new_text": "|\n  echo quoted parent\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML block scalar create-missing should resolve single-quoted existing parent keys: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(
        parsed["data: bucket"]["script"].as_str(),
        Some("echo quoted parent\n")
    );
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_comment_updates_existing_quoted_implicit_key() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"data:\n  # keep data config\n  \"true\": old\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": r#"data["true"]"#
        },
        "op": {
            "type": "set",
            "new_text": "|\n  new text\n",
            "create_missing": true
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "YAML config path set should update an existing quoted implicit-looking key: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(parsed["data"]["true"].as_str(), Some("new text\n"));
}

#[test]
fn patch_json_config_path_set_create_missing_yaml_comment_hyphenated_keys() {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(b"service-config:\n  # keep service config\n  name: identedit\n")
        .expect("yaml fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": "service-config.sidecar-port"
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
        "hyphenated YAML keys should be supported: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = fs::read_to_string(&file_path).expect("updated YAML should be readable");
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&updated).expect("updated YAML should stay valid");
    assert_eq!(
        parsed["service-config"]["sidecar-port"].as_i64(),
        Some(9000)
    );
}
