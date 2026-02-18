use super::*;

#[test]
fn transform_json_mode_rejects_invalid_json_payload() {
    let output = run_identedit_with_stdin(&["transform", "--json"], "{");
    assert!(
        !output.status.success(),
        "transform should fail for malformed JSON payload"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}

#[test]
fn transform_json_mode_rejects_missing_operations_field() {
    let request = json!({
        "command": "transform",
        "file": fixture_path("example.py").to_string_lossy().to_string()
    });

    let output = run_identedit_with_stdin(&["transform", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "transform should reject missing operations field"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}

#[test]
fn transform_json_mode_rejects_missing_file_field() {
    let request = json!({
        "command": "transform",
        "operations": []
    });

    let output = run_identedit_with_stdin(&["transform", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "transform should reject missing file field"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}

#[test]
fn transform_json_mode_accepts_batch_files_shape() {
    let file_a = copy_fixture_to_temp_python("example.py");
    let file_b = copy_fixture_to_temp_python("example.py");
    let handle_a = select_first_handle(&file_a, "function_definition", Some("process_*"));
    let handle_b = select_first_handle(&file_b, "function_definition", Some("process_*"));

    let request = json!({
        "command": "transform",
        "files": [
            {
                "file": file_a.to_string_lossy().to_string(),
                "operations": [
                    {
                        "identity": handle_a["identity"],
                        "kind": handle_a["kind"],
                        "span_hint": handle_a["span"],
                        "expected_old_hash": identedit::changeset::hash_text(
                            handle_a["text"].as_str().expect("text should be string")
                        ),
                        "op": {
                            "type": "replace",
                            "new_text": "def process_data(value):\n    return value * 2"
                        }
                    }
                ]
            },
            {
                "file": file_b.to_string_lossy().to_string(),
                "operations": [
                    {
                        "identity": handle_b["identity"],
                        "kind": handle_b["kind"],
                        "span_hint": handle_b["span"],
                        "expected_old_hash": identedit::changeset::hash_text(
                            handle_b["text"].as_str().expect("text should be string")
                        ),
                        "op": {
                            "type": "replace",
                            "new_text": "def process_data(value):\n    return value * 3"
                        }
                    }
                ]
            }
        ]
    });

    let output = run_identedit_with_stdin(&["transform", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "batch files transform should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(
        response["files"].as_array().map(std::vec::Vec::len),
        Some(2)
    );
    assert_eq!(
        response["files"][0]["file"],
        file_a.to_string_lossy().to_string()
    );
    assert_eq!(
        response["files"][1]["file"],
        file_b.to_string_lossy().to_string()
    );
}

#[test]
fn transform_json_mode_rejects_ambiguous_single_and_batch_shapes() {
    let request = json!({
        "command": "transform",
        "file": fixture_path("example.py").to_string_lossy().to_string(),
        "operations": [],
        "files": []
    });

    let output = run_identedit_with_stdin(&["transform", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "transform should reject ambiguous payload containing both 'file' and 'files'"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("both 'file' and 'files'")),
        "expected explicit ambiguous-shape message"
    );
}

#[test]
fn transform_json_mode_rejects_batch_request_with_top_level_handle_table() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_first_handle(&file_path, "function_definition", Some("process_*"));
    let old_text = handle["text"].as_str().expect("text should be string");
    let request = json!({
        "command": "transform",
        "handle_table": {
            "h1": {
                "identity": handle["identity"],
                "kind": handle["kind"],
                "span_hint": handle["span"],
                "expected_old_hash": identedit::changeset::hash_text(old_text)
            }
        },
        "files": [
            {
                "file": file_path.to_string_lossy().to_string(),
                "operations": []
            }
        ]
    });

    let output = run_identedit_with_stdin(&["transform", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "batch transform should reject top-level handle_table namespace"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(
                |message| message.contains("single-file fields") && message.contains("'files'")
            ),
        "expected explicit shape conflict diagnostic for top-level handle_table with files"
    );
}

#[test]
fn transform_json_mode_rejects_handle_ref_without_handle_table() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let request = json!({
        "command": "transform",
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": { "type": "handle_ref", "ref": "h1" },
                "op": {
                    "type": "replace",
                    "new_text": "def process_data(value):\n    return value + 2"
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["transform", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "transform should reject handle_ref targets without handle_table"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("handle_table")),
        "expected missing handle_table diagnostic for handle_ref"
    );
}

#[test]
fn transform_json_mode_rejects_unknown_handle_ref() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_first_handle(&file_path, "function_definition", Some("process_*"));
    let old_text = handle["text"].as_str().expect("text should be string");
    let request = json!({
        "command": "transform",
        "file": file_path.to_string_lossy().to_string(),
        "handle_table": {
            "known": {
                "identity": handle["identity"],
                "kind": handle["kind"],
                "span_hint": handle["span"],
                "expected_old_hash": identedit::changeset::hash_text(old_text)
            }
        },
        "operations": [
            {
                "target": { "type": "handle_ref", "ref": "missing" },
                "op": {
                    "type": "replace",
                    "new_text": "def process_data(value):\n    return value + 3"
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["transform", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "transform should reject unknown handle_ref keys"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("unknown handle_ref")),
        "expected unknown handle_ref diagnostic"
    );
}

#[test]
fn transform_json_mode_rejects_empty_batch_files_array() {
    let request = json!({
        "command": "transform",
        "files": []
    });

    let output = run_identedit_with_stdin(&["transform", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "transform should reject empty batch files array"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("at least one file entry")),
        "expected explicit empty-batch diagnostic"
    );
}

#[test]
fn transform_json_mode_rejects_unknown_top_level_field() {
    let request = json!({
        "command": "transform",
        "file": fixture_path("example.py").to_string_lossy().to_string(),
        "operations": [],
        "unexpected": true
    });

    let output = run_identedit_with_stdin(&["transform", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "transform should reject unknown top-level fields"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("unknown field `unexpected`")),
        "expected unknown top-level field message"
    );
}

#[test]
fn transform_json_mode_rejects_operations_object_type() {
    let request = json!({
        "command": "transform",
        "file": fixture_path("example.py").to_string_lossy().to_string(),
        "operations": {
            "identity": "nope"
        }
    });

    let output = run_identedit_with_stdin(&["transform", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "transform should reject non-array operations payload"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}

#[test]
fn transform_json_mode_rejects_unknown_operation_field() {
    let handle = select_first_handle(
        &fixture_path("example.py"),
        "function_definition",
        Some("process_*"),
    );
    let request = json!({
        "command": "transform",
        "file": fixture_path("example.py").to_string_lossy().to_string(),
        "operations": [
            {
                "identity": handle["identity"],
                "identiy": handle["identity"],
                "kind": handle["kind"],
                "expected_old_hash": identedit::changeset::hash_text(
                    handle["text"].as_str().expect("text should be string")
                ),
                "op": {
                    "type": "replace",
                    "new_text": "def process_data(value):\n    return value + 2"
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["transform", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "transform should reject unknown operation fields"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("unknown field `identiy`")),
        "expected unknown operation field message"
    );
}

#[test]
fn transform_json_mode_rejects_missing_operation_identity_field() {
    let request = json!({
        "command": "transform",
        "file": fixture_path("example.py").to_string_lossy().to_string(),
        "operations": [
            {
                "kind": "function_definition",
                "expected_old_hash": "00",
                "op": {
                    "type": "replace",
                    "new_text": "def process_data(value):\n    return value + 2"
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["transform", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "transform should reject operations missing identity fields"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("missing field `identity`")),
        "expected explicit missing identity field message"
    );
}

#[test]
fn transform_json_mode_rejects_unsupported_operation_type() {
    let request = json!({
        "command": "transform",
        "file": fixture_path("example.py").to_string_lossy().to_string(),
        "operations": [
            {
                "identity": "id-1",
                "kind": "function_definition",
                "expected_old_hash": "00",
                "op": {
                    "type": "rename"
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["transform", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "transform should reject unsupported op.type"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}

#[test]
fn transform_json_mode_rejects_unknown_replace_payload_field() {
    let handle = select_first_handle(
        &fixture_path("example.py"),
        "function_definition",
        Some("process_*"),
    );
    let request = json!({
        "command": "transform",
        "file": fixture_path("example.py").to_string_lossy().to_string(),
        "operations": [
            {
                "identity": handle["identity"],
                "kind": handle["kind"],
                "expected_old_hash": identedit::changeset::hash_text(
                    handle["text"].as_str().expect("text should be string")
                ),
                "op": {
                    "type": "replace",
                    "new_text": "def process_data(value):\n    return value + 2",
                    "unexpected": "extra"
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["transform", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "transform should reject unknown replace payload fields"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("unknown field `unexpected`")),
        "expected unknown op payload field message"
    );
}

#[test]
fn transform_json_mode_rejects_operation_expected_old_hash_number_type() {
    let handle = select_first_handle(
        &fixture_path("example.py"),
        "function_definition",
        Some("process_*"),
    );
    let request = json!({
        "command": "transform",
        "file": fixture_path("example.py").to_string_lossy().to_string(),
        "operations": [
            {
                "identity": handle["identity"],
                "kind": handle["kind"],
                "span_hint": {
                    "start": handle["span"]["start"],
                    "end": handle["span"]["end"]
                },
                "expected_old_hash": 12345,
                "op": {
                    "type": "replace",
                    "new_text": "def process_data(value):\n    return value + 2"
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["transform", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "transform should reject numeric expected_old_hash"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}

#[test]
fn transform_json_mode_rejects_operations_null_type() {
    let request = json!({
        "command": "transform",
        "file": fixture_path("example.py").to_string_lossy().to_string(),
        "operations": Value::Null
    });

    let output = run_identedit_with_stdin(&["transform", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "transform should reject null operations payload"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}

#[test]
fn transform_json_mode_rejects_non_object_operation_entries() {
    let file = fixture_path("example.py").to_string_lossy().to_string();
    let payloads = [
        json!({
            "command": "transform",
            "file": file,
            "operations": [Value::Null]
        }),
        json!({
            "command": "transform",
            "file": file,
            "operations": [123]
        }),
        json!({
            "command": "transform",
            "file": file,
            "operations": ["not-an-object"]
        }),
        json!({
            "command": "transform",
            "file": file,
            "operations": [true]
        }),
    ];

    for payload in payloads {
        let output = run_identedit_with_stdin(&["transform", "--json"], &payload.to_string());
        assert!(
            !output.status.success(),
            "transform should reject non-object entries inside operations array: {payload}"
        );

        let response: Value =
            serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
        assert_eq!(response["error"]["type"], "invalid_request");
        assert!(
            response["error"]["message"]
                .as_str()
                .is_some_and(|message| message.contains("invalid type")),
            "expected deterministic invalid-type diagnostic for non-object operation entries"
        );
    }
}

#[test]
fn transform_json_mode_rejects_non_transform_command() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let request = json!({
        "command": "apply",
        "file": file_path.to_string_lossy().to_string(),
        "operations": []
    });

    let output = run_identedit_with_stdin(&["transform", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "transform should reject command mismatch in JSON mode"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("expected 'transform'")),
        "expected command mismatch message"
    );
}

#[test]
fn transform_json_mode_rejects_missing_command_field() {
    let request = json!({
        "file": fixture_path("example.py").to_string_lossy().to_string(),
        "operations": []
    });

    let output = run_identedit_with_stdin(&["transform", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "transform should reject missing command field in JSON mode"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("missing field `command`")),
        "expected missing command field message"
    );
}

#[test]
fn transform_json_mode_rejects_command_with_trailing_whitespace() {
    let request = json!({
        "command": "transform ",
        "file": fixture_path("example.py").to_string_lossy().to_string(),
        "operations": []
    });

    let output = run_identedit_with_stdin(&["transform", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "transform should reject trailing-whitespace command token"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}

#[test]
fn transform_json_mode_rejects_uppercase_command_token() {
    let request = json!({
        "command": "TRANSFORM",
        "file": fixture_path("example.py").to_string_lossy().to_string(),
        "operations": []
    });

    let output = run_identedit_with_stdin(&["transform", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "transform should reject uppercase command token"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}

#[test]
fn transform_json_mode_rejects_missing_operation_kind_field() {
    let request = json!({
        "command": "transform",
        "file": fixture_path("example.py").to_string_lossy().to_string(),
        "operations": [
            {
                "identity": "id-1",
                "expected_old_hash": "00",
                "op": {
                    "type": "delete"
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["transform", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "transform should reject operation missing kind field"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("missing field `kind`")),
        "expected missing kind field message"
    );
}

#[test]
fn transform_json_mode_rejects_operation_op_missing_type_tag() {
    let request = json!({
        "command": "transform",
        "file": fixture_path("example.py").to_string_lossy().to_string(),
        "operations": [
            {
                "identity": "id-1",
                "kind": "function_definition",
                "expected_old_hash": "00",
                "op": {}
            }
        ]
    });

    let output = run_identedit_with_stdin(&["transform", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "transform should reject operation payload missing tagged op.type"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("missing field `type`")),
        "expected missing op.type diagnostic"
    );
}

#[test]
fn transform_json_mode_rejects_replace_op_missing_new_text() {
    let request = json!({
        "command": "transform",
        "file": fixture_path("example.py").to_string_lossy().to_string(),
        "operations": [
            {
                "identity": "id-1",
                "kind": "function_definition",
                "expected_old_hash": "00",
                "op": {
                    "type": "replace"
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["transform", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "transform should reject replace payload missing new_text"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("missing field `new_text`")),
        "expected missing new_text diagnostic"
    );
}

#[test]
fn transform_json_mode_returns_precondition_failed_when_hash_mismatches() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_first_handle(&file_path, "function_definition", Some("process_*"));
    let request = json!({
        "command": "transform",
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "identity": handle["identity"].as_str().expect("identity should be string"),
                "kind": handle["kind"].as_str().expect("kind should be string"),
                "span_hint": {
                    "start": handle["span"]["start"].as_u64().expect("span start"),
                    "end": handle["span"]["end"].as_u64().expect("span end")
                },
                "expected_old_hash": "0000000000000000000000000000000000000000000000000000000000000000",
                "op": {
                    "type": "replace",
                    "new_text": "def process_data(value):\n    return value"
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["transform", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "transform should fail for hash mismatch"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "precondition_failed");
}

#[test]
fn transform_json_mode_treats_canonically_equivalent_unicode_reorder_as_stale() {
    let mut temp_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp python file should be created");
    let original_source = "def process_data(value):\n    return \"a\u{0301}\u{0323}\"\n";
    temp_file
        .write_all(original_source.as_bytes())
        .expect("unicode fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let handle = select_first_handle(&file_path, "function_definition", Some("process_*"));
    let span = &handle["span"];
    let expected_old_hash =
        identedit::changeset::hash_text(handle["text"].as_str().expect("text should be string"));

    let mutated_source = "def process_data(value):\n    return \"a\u{0323}\u{0301}\"\n";
    fs::write(&file_path, mutated_source).expect("fixture mutation should succeed");

    let request = json!({
        "command": "transform",
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "identity": handle["identity"],
                "kind": handle["kind"],
                "span_hint": {
                    "start": span["start"],
                    "end": span["end"]
                },
                "expected_old_hash": expected_old_hash,
                "op": {
                    "type": "replace",
                    "new_text": "def process_data(value):\n    return \"patched\""
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["transform", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "transform should fail for canonically equivalent but byte-different source"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "precondition_failed");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("Expected hash")),
        "expected hash mismatch detail in precondition failure"
    );
}

#[test]
fn transform_json_mode_returns_target_missing_when_kind_mismatches() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_first_handle(&file_path, "function_definition", Some("process_*"));
    let request = json!({
        "command": "transform",
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "identity": handle["identity"],
                "kind": "class_definition",
                "span_hint": {
                    "start": handle["span"]["start"],
                    "end": handle["span"]["end"]
                },
                "expected_old_hash": identedit::changeset::hash_text(
                    handle["text"].as_str().expect("text should be string")
                ),
                "op": {
                    "type": "replace",
                    "new_text": "class ProcessData:\n    pass"
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["transform", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "transform should fail when kind guard does not match"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "target_missing");
}

#[test]
fn transform_json_mode_returns_target_missing_for_json_kind_mismatch_with_stale_span_hint() {
    let file_path = copy_fixture_to_temp_json("example.json");
    let handle = select_first_handle(&file_path, "object", Some("config"));
    let span_start = handle["span"]["start"].as_u64().expect("span start") as usize;
    let span_end = handle["span"]["end"].as_u64().expect("span end") as usize;
    let stale_identity = handle["identity"]
        .as_str()
        .expect("identity should be present")
        .to_string();

    let original = fs::read_to_string(&file_path).expect("fixture should be readable");
    let mutated = original.replacen("\"retries\": 3", "\"retries\": 4", 1);
    fs::write(&file_path, mutated).expect("fixture mutation should succeed");

    let request = json!({
        "command": "transform",
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "identity": stale_identity,
                "kind": "array",
                "span_hint": {
                    "start": span_start,
                    "end": span_end
                },
                "expected_old_hash": identedit::changeset::hash_text(
                    handle["text"].as_str().expect("text should be string")
                ),
                "op": {
                    "type": "replace",
                    "new_text": "[]"
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["transform", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "transform should fail when stale span_hint resolves only wrong kinds"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "target_missing");
}

#[test]
fn transform_json_mode_rejects_span_hint_with_start_greater_than_end() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_first_handle(&file_path, "function_definition", Some("process_*"));
    let request = json!({
        "command": "transform",
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "identity": handle["identity"],
                "kind": handle["kind"],
                "span_hint": {
                    "start": 100,
                    "end": 10
                },
                "expected_old_hash": identedit::changeset::hash_text(
                    handle["text"].as_str().expect("text should be string")
                ),
                "op": {
                    "type": "replace",
                    "new_text": "def process_data(value):\n    return value + 2"
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["transform", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "transform should reject invalid span_hint boundaries"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("span_hint")),
        "expected span_hint validation message"
    );
}

#[test]
fn transform_json_mode_rejects_zero_length_span_hint() {
    let fixture = fixture_path("ambiguous.py");
    let handle = select_first_handle(&fixture, "function_definition", Some("duplicate"));
    let start = handle["span"]["start"].as_u64().expect("span start");
    let request = json!({
        "command": "transform",
        "file": fixture.to_string_lossy().to_string(),
        "operations": [
            {
                "identity": handle["identity"],
                "kind": handle["kind"],
                "span_hint": {
                    "start": start,
                    "end": start
                },
                "expected_old_hash": identedit::changeset::hash_text(
                    handle["text"].as_str().expect("text should be string")
                ),
                "op": {
                    "type": "replace",
                    "new_text": "def duplicate():\n    return 123"
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["transform", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "transform should reject zero-length span_hint"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("zero-length")),
        "expected zero-length span_hint validation message"
    );
}

#[test]
fn transform_json_mode_accepts_extreme_span_values_when_target_is_unique() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_first_handle(&file_path, "function_definition", Some("process_*"));
    let request = json!({
        "command": "transform",
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "identity": handle["identity"],
                "kind": handle["kind"],
                "span_hint": {
                    "start": usize::MAX - 1,
                    "end": usize::MAX
                },
                "expected_old_hash": identedit::changeset::hash_text(
                    handle["text"].as_str().expect("text should be string")
                ),
                "op": {
                    "type": "replace",
                    "new_text": "def process_data(value):\n    return value + 2"
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["transform", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "transform should accept extreme span_hint values when kind/hash resolves uniquely: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    let operation = &response["files"][0]["operations"][0];
    assert_eq!(
        operation["target"]["span_hint"], handle["span"],
        "transform output should canonicalize target span_hint to resolved span",
    );
    assert_eq!(
        operation["preview"]["matched_span"], handle["span"],
        "transform output should use resolved span in preview",
    );
}

#[test]
fn transform_json_mode_accepts_non_matching_span_hint_for_unique_target() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_first_handle(&file_path, "function_definition", Some("process_*"));
    let request = json!({
        "command": "transform",
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "identity": handle["identity"],
                "kind": handle["kind"],
                "span_hint": {
                    "start": 9999,
                    "end": 10000
                },
                "expected_old_hash": identedit::changeset::hash_text(
                    handle["text"].as_str().expect("text should be string")
                ),
                "op": {
                    "type": "replace",
                    "new_text": "def process_data(value):\n    return value + 2"
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["transform", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "transform should accept non-matching span_hint when target remains uniquely resolvable: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    let operation = &response["files"][0]["operations"][0];
    assert_eq!(
        operation["target"]["span_hint"], handle["span"],
        "transform output should canonicalize target span_hint to resolved span",
    );
    assert_eq!(
        operation["preview"]["matched_span"], handle["span"],
        "transform output should use resolved span in preview",
    );
}
