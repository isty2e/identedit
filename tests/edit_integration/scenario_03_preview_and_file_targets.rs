use super::*;

fn assert_compact_preview_old_state(preview: &Value, expected_old_text: &str) {
    assert!(
        preview.get("old_text").is_none(),
        "compact preview should omit old_text by default"
    );
    assert_eq!(
        preview["old_hash"],
        identedit::changeset::hash_text(expected_old_text),
        "compact preview should include old_hash"
    );
    assert_eq!(
        preview["old_len"],
        expected_old_text.len(),
        "compact preview should include old_len"
    );
}

fn line_ref(source: &str, line: usize) -> String {
    let line_text = source
        .lines()
        .nth(line - 1)
        .expect("line should exist for anchor");
    format!(
        "{line}:{}",
        identedit::hashline::compute_line_hash(line_text)
    )
}

fn select_handle_by_name(file_path: &Path, name: &str) -> Value {
    let output = run_identedit(&[
        "read",
        "--json",
        "--verbose",
        "--kind",
        "function_definition",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "select failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    response["handles"]
        .as_array()
        .expect("handles should be array")
        .iter()
        .find(|handle| handle["name"].as_str() == Some(name))
        .cloned()
        .expect("named handle should exist")
}

#[test]
fn transform_json_mode_supports_multiple_operations() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let process_handle = select_first_handle(&file_path, "function_definition", Some("process_*"));
    let helper_handle = select_first_handle(&file_path, "function_definition", Some("helper"));

    let request = json!({
        "command": "edit",
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "identity": process_handle["identity"].as_str().expect("identity should be string"),
                "kind": process_handle["kind"].as_str().expect("kind should be string"),
                "span_hint": {
                    "start": process_handle["span"]["start"].as_u64().expect("span start"),
                    "end": process_handle["span"]["end"].as_u64().expect("span end")
                },
                "expected_old_hash": identedit::changeset::hash_text(
                    process_handle["text"].as_str().expect("text should be string")
                ),
                "op": {
                    "type": "replace",
                    "new_text": "def process_data(value):\n    return value + 10"
                }
            },
            {
                "identity": helper_handle["identity"].as_str().expect("identity should be string"),
                "kind": helper_handle["kind"].as_str().expect("kind should be string"),
                "span_hint": {
                    "start": helper_handle["span"]["start"].as_u64().expect("span start"),
                    "end": helper_handle["span"]["end"].as_u64().expect("span end")
                },
                "expected_old_hash": identedit::changeset::hash_text(
                    helper_handle["text"].as_str().expect("text should be string")
                ),
                "op": {
                    "type": "replace",
                    "new_text": "def helper():\n    return \"patched\""
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["edit", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "transform should succeed for multiple operations: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    let operations = response["files"][0]["operations"]
        .as_array()
        .expect("operations should be an array");
    assert_eq!(operations.len(), 2);

    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(
        before, after,
        "transform JSON mode must stay dry-run even for multiple operations"
    );
}

#[test]
fn transform_json_mode_supports_handle_ref_target_with_file_level_handle_table() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_first_handle(&file_path, "function_definition", Some("process_*"));
    let old_text = handle["text"].as_str().expect("text should be string");

    let request = json!({
        "command": "edit",
        "file": file_path.to_string_lossy().to_string(),
        "handle_table": {
            "proc": {
                "identity": handle["identity"],
                "kind": handle["kind"],
                "span_hint": handle["span"],
                "expected_old_hash": identedit::changeset::hash_text(old_text)
            }
        },
        "operations": [
            {
                "target": {
                    "type": "handle_ref",
                    "ref": "proc"
                },
                "op": {
                    "type": "replace",
                    "new_text": "def process_data(value):\n    return value * 10"
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["edit", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "transform should support handle_ref targets: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    let operation = &response["files"][0]["operations"][0];
    assert_eq!(operation["target"]["type"], "node");
    assert_eq!(operation["target"]["identity"], handle["identity"]);
    assert_eq!(operation["target"]["kind"], handle["kind"]);
    assert_eq!(
        operation["target"]["expected_old_hash"],
        identedit::changeset::hash_text(old_text)
    );
    let preview = &operation["preview"];
    assert_compact_preview_old_state(preview, old_text);
}

#[test]
fn transform_json_mode_supports_batch_handle_ref_with_file_scoped_namespaces() {
    let file_a = copy_fixture_to_temp_python("example.py");
    let file_b = copy_fixture_to_temp_python("example.py");
    let handle_a = select_first_handle(&file_a, "function_definition", Some("process_*"));
    let handle_b = select_first_handle(&file_b, "function_definition", Some("helper"));
    let old_text_a = handle_a["text"].as_str().expect("text should be string");
    let old_text_b = handle_b["text"].as_str().expect("text should be string");

    let request = json!({
        "command": "edit",
        "files": [
            {
                "file": file_a.to_string_lossy().to_string(),
                "handle_table": {
                    "h1": {
                        "identity": handle_a["identity"],
                        "kind": handle_a["kind"],
                        "span_hint": handle_a["span"],
                        "expected_old_hash": identedit::changeset::hash_text(old_text_a)
                    }
                },
                "operations": [
                    {
                        "target": { "type": "handle_ref", "ref": "h1" },
                        "op": {
                            "type": "replace",
                            "new_text": "def process_data(value):\n    return value - 1"
                        }
                    }
                ]
            },
            {
                "file": file_b.to_string_lossy().to_string(),
                "handle_table": {
                    "h1": {
                        "identity": handle_b["identity"],
                        "kind": handle_b["kind"],
                        "span_hint": handle_b["span"],
                        "expected_old_hash": identedit::changeset::hash_text(old_text_b)
                    }
                },
                "operations": [
                    {
                        "target": { "type": "handle_ref", "ref": "h1" },
                        "op": {
                            "type": "replace",
                            "new_text": "def helper():\n    return \"patched-from-ref\""
                        }
                    }
                ]
            }
        ]
    });

    let output = run_identedit_with_stdin(&["edit", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "batch transform should support file-scoped handle_ref namespaces: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["files"].as_array().map(Vec::len), Some(2));
    assert_eq!(
        response["files"][0]["operations"][0]["target"]["identity"],
        handle_a["identity"]
    );
    assert_eq!(
        response["files"][1]["operations"][0]["target"]["identity"],
        handle_b["identity"]
    );
}

#[test]
fn transform_json_mode_defaults_to_compact_preview_for_replace() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_first_handle(&file_path, "function_definition", Some("process_*"));
    let old_text = handle["text"].as_str().expect("text should be string");

    let request = json!({
        "command": "edit",
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "identity": handle["identity"],
                "kind": handle["kind"],
                "span_hint": {
                    "start": handle["span"]["start"],
                    "end": handle["span"]["end"]
                },
                "expected_old_hash": identedit::changeset::hash_text(old_text),
                "op": {
                    "type": "replace",
                    "new_text": "def process_data(value):\n    return value + 10"
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["edit", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "transform should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    let preview = &response["files"][0]["operations"][0]["preview"];
    assert_compact_preview_old_state(preview, old_text);
}

#[test]
fn transform_json_mode_verbose_includes_full_preview_old_text() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_first_handle(&file_path, "function_definition", Some("process_*"));
    let old_text = handle["text"].as_str().expect("text should be string");

    let request = json!({
        "command": "edit",
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "identity": handle["identity"],
                "kind": handle["kind"],
                "span_hint": {
                    "start": handle["span"]["start"],
                    "end": handle["span"]["end"]
                },
                "expected_old_hash": identedit::changeset::hash_text(old_text),
                "op": {
                    "type": "replace",
                    "new_text": "def process_data(value):\n    return value + 10"
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["edit", "--json", "--verbose"], &request.to_string());
    assert!(
        output.status.success(),
        "transform --verbose should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    let preview = &response["files"][0]["operations"][0]["preview"];
    assert_eq!(
        preview["old_text"], old_text,
        "verbose preview should include old_text"
    );
    assert!(
        preview.get("old_hash").is_none(),
        "verbose preview should omit compact old_hash field"
    );
    assert!(
        preview.get("old_len").is_none(),
        "verbose preview should omit compact old_len field"
    );
}

#[test]
fn transform_json_mode_builds_insert_before_preview_with_zero_width_span() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_first_handle(&file_path, "function_definition", Some("process_*"));
    let span_start = handle["span"]["start"].as_u64().expect("span start");

    let request = json!({
        "command": "edit",
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "identity": handle["identity"],
                "kind": handle["kind"],
                "span_hint": {
                    "start": handle["span"]["start"],
                    "end": handle["span"]["end"]
                },
                "expected_old_hash": identedit::changeset::hash_text(
                    handle["text"].as_str().expect("text should be string")
                ),
                "op": {
                    "type": "insert_before",
                    "new_text": "# inserted-before\n"
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["edit", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "transform should support insert_before: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(
        response["files"][0]["operations"][0]["op"]["type"],
        "insert_before"
    );
    let preview = &response["files"][0]["operations"][0]["preview"];
    assert_compact_preview_old_state(preview, "");
    assert_eq!(preview["new_text"], "# inserted-before\n");
    assert_eq!(preview["matched_span"]["start"], span_start);
    assert_eq!(preview["matched_span"]["end"], span_start);
}

#[test]
fn transform_json_mode_builds_delete_preview_with_anchor_span() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_first_handle(&file_path, "function_definition", Some("process_*"));
    let old_text = handle["text"].as_str().expect("text should be string");
    let span_start = handle["span"]["start"].as_u64().expect("span start");
    let span_end = handle["span"]["end"].as_u64().expect("span end");

    let request = json!({
        "command": "edit",
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "identity": handle["identity"],
                "kind": handle["kind"],
                "span_hint": {
                    "start": handle["span"]["start"],
                    "end": handle["span"]["end"]
                },
                "expected_old_hash": identedit::changeset::hash_text(old_text),
                "op": {
                    "type": "delete"
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["edit", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "transform should support delete: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(
        response["files"][0]["operations"][0]["op"]["type"],
        "delete"
    );
    let preview = &response["files"][0]["operations"][0]["preview"];
    assert_compact_preview_old_state(preview, old_text);
    assert_eq!(preview["new_text"], "");
    assert_eq!(preview["matched_span"]["start"], span_start);
    assert_eq!(preview["matched_span"]["end"], span_end);
}

#[test]
fn transform_json_mode_delete_and_empty_replace_have_equivalent_previews() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_first_handle(&file_path, "function_definition", Some("process_*"));
    let old_text = handle["text"].as_str().expect("text should be string");
    let expected_old_hash = identedit::changeset::hash_text(old_text);

    let delete_request = json!({
        "command": "edit",
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "identity": handle["identity"],
                "kind": handle["kind"],
                "span_hint": {
                    "start": handle["span"]["start"],
                    "end": handle["span"]["end"]
                },
                "expected_old_hash": expected_old_hash,
                "op": {
                    "type": "delete"
                }
            }
        ]
    });

    let replace_request = json!({
        "command": "edit",
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "identity": handle["identity"],
                "kind": handle["kind"],
                "span_hint": {
                    "start": handle["span"]["start"],
                    "end": handle["span"]["end"]
                },
                "expected_old_hash": expected_old_hash,
                "op": {
                    "type": "replace",
                    "new_text": ""
                }
            }
        ]
    });

    let delete_output = run_identedit_with_stdin(&["edit", "--json"], &delete_request.to_string());
    let replace_output =
        run_identedit_with_stdin(&["edit", "--json"], &replace_request.to_string());

    assert!(
        delete_output.status.success(),
        "delete transform should succeed: {}",
        String::from_utf8_lossy(&delete_output.stderr)
    );
    assert!(
        replace_output.status.success(),
        "empty replace transform should succeed: {}",
        String::from_utf8_lossy(&replace_output.stderr)
    );

    let delete_response: Value =
        serde_json::from_slice(&delete_output.stdout).expect("stdout should be valid JSON");
    let replace_response: Value =
        serde_json::from_slice(&replace_output.stdout).expect("stdout should be valid JSON");

    assert_eq!(
        delete_response["files"][0]["operations"][0]["preview"],
        replace_response["files"][0]["operations"][0]["preview"],
        "delete and empty replace should produce equivalent preview payload"
    );
    assert_eq!(
        delete_response["files"][0]["operations"][0]["op"]["type"],
        "delete"
    );
    assert_eq!(
        replace_response["files"][0]["operations"][0]["op"]["type"],
        "replace"
    );
}

#[test]
fn transform_json_mode_builds_insert_after_preview_with_zero_width_span() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_first_handle(&file_path, "function_definition", Some("process_*"));
    let span_end = handle["span"]["end"].as_u64().expect("span end");

    let request = json!({
        "command": "edit",
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "identity": handle["identity"],
                "kind": handle["kind"],
                "span_hint": {
                    "start": handle["span"]["start"],
                    "end": handle["span"]["end"]
                },
                "expected_old_hash": identedit::changeset::hash_text(
                    handle["text"].as_str().expect("text should be string")
                ),
                "op": {
                    "type": "insert_after",
                    "new_text": "\n# inserted-after"
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["edit", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "transform should support insert_after: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(
        response["files"][0]["operations"][0]["op"]["type"],
        "insert_after"
    );
    let preview = &response["files"][0]["operations"][0]["preview"];
    assert_compact_preview_old_state(preview, "");
    assert_eq!(preview["new_text"], "\n# inserted-after");
    assert_eq!(preview["matched_span"]["start"], span_end);
    assert_eq!(preview["matched_span"]["end"], span_end);
}

#[test]
fn transform_json_mode_builds_file_end_insert_preview_with_zero_width_span() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let expected_file_hash = identedit::changeset::hash_text(&before);
    let insert_text = "\n# appended-at-file-end\n";

    let request = json!({
        "command": "edit",
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "type": "file_end",
                    "expected_file_hash": expected_file_hash
                },
                "op": {
                    "type": "insert",
                    "new_text": insert_text
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["edit", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "transform should support file_end insert: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(
        response["files"][0]["operations"][0]["target"]["type"],
        "file_end"
    );
    assert_eq!(
        response["files"][0]["operations"][0]["op"]["type"],
        "insert"
    );
    let preview = &response["files"][0]["operations"][0]["preview"];
    assert_compact_preview_old_state(preview, "");
    assert_eq!(preview["new_text"], insert_text);
    assert_eq!(preview["matched_span"]["start"], before.len());
    assert_eq!(preview["matched_span"]["end"], before.len());

    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(
        before, after,
        "transform must remain dry-run for file_end insert"
    );
}

#[test]
fn transform_json_mode_builds_file_start_insert_preview_with_zero_width_span() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let expected_file_hash = identedit::changeset::hash_text(&before);
    let insert_text = "# prepended-at-file-start\n";

    let request = json!({
        "command": "edit",
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "type": "file_start",
                    "expected_file_hash": expected_file_hash
                },
                "op": {
                    "type": "insert",
                    "new_text": insert_text
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["edit", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "transform should support file_start insert: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(
        response["files"][0]["operations"][0]["target"]["type"],
        "file_start"
    );
    assert_eq!(
        response["files"][0]["operations"][0]["op"]["type"],
        "insert"
    );
    let preview = &response["files"][0]["operations"][0]["preview"];
    assert_compact_preview_old_state(preview, "");
    assert_eq!(preview["new_text"], insert_text);
    assert_eq!(preview["matched_span"]["start"], 0);
    assert_eq!(preview["matched_span"]["end"], 0);

    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(
        before, after,
        "transform must remain dry-run for file_start insert"
    );
}

#[test]
fn transform_json_mode_rejects_file_start_insert_with_stale_file_hash() {
    let file_path = copy_fixture_to_temp_python("example.py");

    let request = json!({
        "command": "edit",
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "type": "file_start",
                    "expected_file_hash": "stale-file-hash"
                },
                "op": {
                    "type": "insert",
                    "new_text": "# stale\n"
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["edit", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "transform should reject stale file hash for file_start insert"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "precondition_failed");
}

#[test]
fn transform_json_mode_rejects_file_start_target_missing_expected_file_hash() {
    let file_path = copy_fixture_to_temp_python("example.py");

    let request = json!({
        "command": "edit",
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "type": "file_start"
                },
                "op": {
                    "type": "insert",
                    "new_text": "# header\n"
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["edit", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "transform should reject file_start target missing expected_file_hash"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("missing field `expected_file_hash`")),
        "expected explicit missing expected_file_hash message"
    );
}

#[test]
fn transform_json_mode_rejects_node_target_with_expected_file_hash_field() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_first_handle(&file_path, "function_definition", Some("process_*"));
    let old_text = handle["text"].as_str().expect("text should be string");

    let request = json!({
        "command": "edit",
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "type": "node",
                    "identity": handle["identity"],
                    "kind": handle["kind"],
                    "span_hint": {
                        "start": handle["span"]["start"],
                        "end": handle["span"]["end"]
                    },
                    "expected_old_hash": identedit::changeset::hash_text(old_text),
                    "expected_file_hash": "not-allowed"
                },
                "op": {
                    "type": "replace",
                    "new_text": old_text
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["edit", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "transform should reject expected_file_hash on node target"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"].as_str().is_some_and(
            |message| message.contains("node target does not accept expected_file_hash")
        ),
        "expected node/file hash schema rejection message"
    );
}

#[test]
fn transform_json_mode_rejects_mixed_target_and_legacy_fields_in_operation() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let expected_file_hash = identedit::changeset::hash_text(&before);

    let request = json!({
        "command": "edit",
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "type": "file_start",
                    "expected_file_hash": expected_file_hash
                },
                "identity": "legacy-identity",
                "kind": "function_definition",
                "expected_old_hash": "legacy-hash",
                "op": {
                    "type": "insert",
                    "new_text": "# invalid-mixed-input\n"
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["edit", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "transform should reject mixed target/legacy operation fields"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("cannot be combined")),
        "expected explicit target/legacy mixing rejection message"
    );
}

#[test]
fn transform_json_mode_file_start_insert_preview_starts_after_utf8_bom() {
    let mut temp_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp python file should be created");
    temp_file
        .write_all(b"\xEF\xBB\xBFdef process_data(value):\n    return value + 1\n")
        .expect("bom python fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let expected_file_hash = identedit::changeset::hash_text(&before);
    let insert_text = "# file-start-bom\n";

    let request = json!({
        "command": "edit",
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "type": "file_start",
                    "expected_file_hash": expected_file_hash
                },
                "op": {
                    "type": "insert",
                    "new_text": insert_text
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["edit", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "transform should support file_start insert on BOM source: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    let preview = &response["files"][0]["operations"][0]["preview"];
    assert_eq!(preview["matched_span"]["start"], 3);
    assert_eq!(preview["matched_span"]["end"], 3);
    assert_compact_preview_old_state(preview, "");
    assert_eq!(preview["new_text"], insert_text);

    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(
        before, after,
        "transform must remain dry-run for file_start insert on BOM source"
    );
}

#[test]
fn transform_json_mode_rejects_file_end_replace_combo() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let expected_file_hash = identedit::changeset::hash_text(&before);

    let request = json!({
        "command": "edit",
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "type": "file_end",
                    "expected_file_hash": expected_file_hash
                },
                "op": {
                    "type": "replace",
                    "new_text": "invalid"
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["edit", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "transform should reject file target + replace combo"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("unsupported target/op combination")),
        "expected target/op compatibility message"
    );
}

#[test]
fn transform_json_mode_rejects_file_end_insert_with_stale_file_hash() {
    let file_path = copy_fixture_to_temp_python("example.py");

    let request = json!({
        "command": "edit",
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "type": "file_end",
                    "expected_file_hash": "stale-file-hash"
                },
                "op": {
                    "type": "insert",
                    "new_text": "\n# stale\n"
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["edit", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "transform should reject stale file hash for file_end insert"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "precondition_failed");
}

#[cfg(unix)]
#[test]
fn transform_json_mode_rejects_file_start_insert_with_hardlink_alias_stale_hash_after_canonical_mutation()
 {
    let workspace = tempdir().expect("tempdir should be created");
    let canonical_path = workspace.path().join("target.py");
    let alias_path = workspace.path().join("alias.py");
    let source =
        fs::read_to_string(fixture_path("example.py")).expect("fixture should be readable");
    fs::write(&canonical_path, &source).expect("canonical fixture write should succeed");
    fs::hard_link(&canonical_path, &alias_path).expect("hardlink alias should be created");

    let expected_file_hash = identedit::changeset::hash_text(
        &fs::read_to_string(&alias_path).expect("alias should be readable"),
    );
    fs::write(
        &canonical_path,
        format!("{source}\n# stale-hash-mutation\n"),
    )
    .expect("canonical mutation should succeed");

    let request = json!({
        "command": "edit",
        "file": alias_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "type": "file_start",
                    "expected_file_hash": expected_file_hash
                },
                "op": {
                    "type": "insert",
                    "new_text": "# header\n"
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["edit", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "transform should reject stale file_start hash through hardlink alias"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "precondition_failed");
}

#[cfg(unix)]
#[test]
fn transform_json_mode_rejects_file_end_insert_with_hardlink_alias_stale_hash_after_alias_mutation()
{
    let workspace = tempdir().expect("tempdir should be created");
    let canonical_path = workspace.path().join("target.py");
    let alias_path = workspace.path().join("alias.py");
    let source =
        fs::read_to_string(fixture_path("example.py")).expect("fixture should be readable");
    fs::write(&canonical_path, &source).expect("canonical fixture write should succeed");
    fs::hard_link(&canonical_path, &alias_path).expect("hardlink alias should be created");

    let expected_file_hash = identedit::changeset::hash_text(
        &fs::read_to_string(&canonical_path).expect("canonical should be readable"),
    );
    fs::write(&alias_path, format!("{source}\n# alias-mutation\n"))
        .expect("alias mutation should succeed");

    let request = json!({
        "command": "edit",
        "file": canonical_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "type": "file_end",
                    "expected_file_hash": expected_file_hash
                },
                "op": {
                    "type": "insert",
                    "new_text": "\n# trailer\n"
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["edit", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "transform should reject stale file_end hash after alias mutation"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "precondition_failed");
}

#[cfg(unix)]
#[test]
fn transform_json_mode_rejects_file_start_and_file_end_inserts_on_empty_hardlink_alias() {
    let workspace = tempdir().expect("tempdir should be created");
    let canonical_path = workspace.path().join("target.py");
    let alias_path = workspace.path().join("alias.py");
    fs::write(&canonical_path, "").expect("empty canonical fixture write should succeed");
    fs::hard_link(&canonical_path, &alias_path).expect("hardlink alias should be created");
    let expected_file_hash = identedit::changeset::hash_text("");

    let request = json!({
        "command": "edit",
        "file": alias_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "type": "file_start",
                    "expected_file_hash": expected_file_hash
                },
                "op": {
                    "type": "insert",
                    "new_text": "# start\n"
                }
            },
            {
                "target": {
                    "type": "file_end",
                    "expected_file_hash": expected_file_hash
                },
                "op": {
                    "type": "insert",
                    "new_text": "# end\n"
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["edit", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "transform should reject file_start/file_end overlap on empty hardlink alias"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("Overlapping operations")),
        "expected overlap conflict for empty hardlink alias file_start/file_end inserts"
    );
}

#[test]
fn transform_json_mode_rejects_file_start_insert_with_crlf_normalized_hash_mismatch() {
    let mut temp_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp python file should be created");
    let source = "def process_data(value):\r\n    return value + 1\r\n";
    temp_file
        .write_all(source.as_bytes())
        .expect("crlf fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let normalized_hash = identedit::changeset::hash_text(&source.replace("\r\n", "\n"));

    let request = json!({
        "command": "edit",
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "type": "file_start",
                    "expected_file_hash": normalized_hash
                },
                "op": {
                    "type": "insert",
                    "new_text": "# header\n"
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["edit", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "transform should reject normalized-hash mismatch for CRLF source"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "precondition_failed");
}

#[test]
fn transform_json_mode_builds_file_end_preview_with_multibyte_byte_length() {
    let mut temp_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp python file should be created");
    let source = "def process_data(value):\n    return \"한글🙂\"\n";
    temp_file
        .write_all(source.as_bytes())
        .expect("multibyte fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let expected_file_hash = identedit::changeset::hash_text(&before);

    let request = json!({
        "command": "edit",
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "type": "file_end",
                    "expected_file_hash": expected_file_hash
                },
                "op": {
                    "type": "insert",
                    "new_text": "\n# trailer\n"
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["edit", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "transform should support file_end preview on multibyte source: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    let matched_start = response["files"][0]["operations"][0]["preview"]["matched_span"]["start"]
        .as_u64()
        .expect("matched start should be number") as usize;
    let matched_end = response["files"][0]["operations"][0]["preview"]["matched_span"]["end"]
        .as_u64()
        .expect("matched end should be number") as usize;
    assert_eq!(matched_start, before.len());
    assert_eq!(matched_end, before.len());

    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "transform must remain dry-run");
}

#[cfg(unix)]
#[test]
fn transform_json_mode_file_start_preview_starts_after_bom_for_hardlink_alias_path() {
    let workspace = tempdir().expect("tempdir should be created");
    let canonical_path = workspace.path().join("target.py");
    let alias_path = workspace.path().join("alias.py");
    fs::write(
        &canonical_path,
        "\u{FEFF}def process_data(value):\n    return value + 1\n",
    )
    .expect("bom fixture write should succeed");
    fs::hard_link(&canonical_path, &alias_path).expect("hardlink alias should be created");
    let before = fs::read_to_string(&alias_path).expect("alias should be readable");
    let expected_file_hash = identedit::changeset::hash_text(&before);

    let request = json!({
        "command": "edit",
        "file": alias_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "type": "file_start",
                    "expected_file_hash": expected_file_hash
                },
                "op": {
                    "type": "insert",
                    "new_text": "# alias-header\n"
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["edit", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "transform should support BOM file_start via hardlink alias: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(
        response["files"][0]["operations"][0]["preview"]["matched_span"]["start"],
        3
    );
    assert_eq!(
        response["files"][0]["operations"][0]["preview"]["matched_span"]["end"],
        3
    );

    let canonical_after =
        fs::read_to_string(&canonical_path).expect("canonical should be readable");
    let alias_after = fs::read_to_string(&alias_path).expect("alias should be readable");
    assert_eq!(
        canonical_after, before,
        "transform must not mutate canonical source"
    );
    assert_eq!(
        alias_after, before,
        "transform must not mutate alias source"
    );
}

#[test]
fn transform_json_mode_supports_line_target_replace_lines_operation() {
    let source = "a\nb\nc\n";
    let mut temp_file = Builder::new()
        .suffix(".txt")
        .tempfile()
        .expect("temp text file should be created");
    temp_file
        .write_all(source.as_bytes())
        .expect("fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "edit",
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "type": "line",
                    "anchor": line_ref(source, 2),
                    "end_anchor": line_ref(source, 3)
                },
                "op": {
                    "type": "replace_lines",
                    "new_text": "x\ny"
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["edit", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "line-target transform should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    let operation = &response["files"][0]["operations"][0];
    assert_eq!(operation["target"]["type"], "line");
    assert_eq!(operation["op"]["type"], "replace");
    assert_eq!(operation["preview"]["matched_span"]["start"], 2);
    assert_eq!(operation["preview"]["matched_span"]["end"], 6);
    assert_compact_preview_old_state(&operation["preview"], "b\nc\n");

    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(after, source, "transform must remain dry-run");
}

#[test]
fn transform_json_mode_supports_line_target_insert_after_line_operation() {
    let source = "a\nb\n";
    let mut temp_file = Builder::new()
        .suffix(".txt")
        .tempfile()
        .expect("temp text file should be created");
    temp_file
        .write_all(source.as_bytes())
        .expect("fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let request = json!({
        "command": "edit",
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "type": "line",
                    "anchor": line_ref(source, 1)
                },
                "op": {
                    "type": "insert_after_line",
                    "text": "x"
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["edit", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "line-target insert_after_line transform should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    let operation = &response["files"][0]["operations"][0];
    assert_eq!(operation["target"]["type"], "line");
    assert_eq!(operation["op"]["type"], "insert_after");
    assert_eq!(operation["preview"]["matched_span"]["start"], 2);
    assert_eq!(operation["preview"]["matched_span"]["end"], 2);
    assert_compact_preview_old_state(&operation["preview"], "");
}

#[test]
fn transform_json_mode_mixed_node_line_overlap_error_is_order_independent() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let source = fs::read_to_string(&file_path).expect("fixture should be readable");
    let process_handle = select_first_handle(&file_path, "function_definition", Some("process_*"));
    let process_text = process_handle["text"]
        .as_str()
        .expect("process handle text should be present");
    let process_return_line = process_text
        .lines()
        .enumerate()
        .find_map(|(index, line)| line.contains("return result").then_some(index + 1))
        .expect("process return line should exist");
    let prefix_lines = source
        .lines()
        .take_while(|line| !line.contains("def process_data("))
        .count();
    let absolute_line = prefix_lines + process_return_line;
    let line_anchor = line_ref(&source, absolute_line);
    let node_target = json!({
        "identity": process_handle["identity"],
        "kind": process_handle["kind"],
        "span_hint": process_handle["span"],
        "expected_old_hash": identedit::changeset::hash_text(
            process_handle["text"].as_str().expect("text should be string")
        ),
    });
    let line_target = json!({
        "type": "line",
        "anchor": line_anchor
    });
    let node_operation = json!({
        "target": node_target,
        "op": {
            "type": "replace",
            "new_text": "def process_data(value):\n    return value * 10"
        }
    });
    let line_operation = json!({
        "target": line_target,
        "op": {
            "type": "set_line",
            "new_text": "    return value - 5"
        }
    });

    let request_a = json!({
        "command": "edit",
        "file": file_path.to_string_lossy().to_string(),
        "operations": [node_operation, line_operation]
    });
    let request_b = json!({
        "command": "edit",
        "file": file_path.to_string_lossy().to_string(),
        "operations": [line_operation, node_operation]
    });

    let output_a = run_identedit_with_stdin(&["edit", "--json"], &request_a.to_string());
    let output_b = run_identedit_with_stdin(&["edit", "--json"], &request_b.to_string());
    assert!(
        !output_a.status.success() && !output_b.status.success(),
        "both operation orderings should be rejected"
    );

    let error_a: Value = serde_json::from_slice(&output_a.stdout).expect("stdout should be JSON");
    let error_b: Value = serde_json::from_slice(&output_b.stdout).expect("stdout should be JSON");
    assert_eq!(error_a["error"]["type"], "invalid_request");
    assert_eq!(error_a["error"]["message"], error_b["error"]["message"]);
    let message = error_a["error"]["message"]
        .as_str()
        .expect("error message should be string");
    assert!(
        message.contains("Overlapping operations are not supported"),
        "expected overlap diagnostic, got: {message}"
    );
}

#[test]
fn transform_json_mode_supports_same_file_move_before_preview() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let source_handle = select_handle_by_name(&file_path, "helper");
    let destination_handle = select_handle_by_name(&file_path, "process_data");

    let request = json!({
        "command": "edit",
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "identity": source_handle["identity"],
                    "kind": source_handle["kind"],
                    "span_hint": source_handle["span"],
                    "expected_old_hash": identedit::changeset::hash_text(
                        source_handle["text"].as_str().expect("source text should be present")
                    )
                },
                "op": {
                    "type": "move_before",
                    "destination": {
                        "identity": destination_handle["identity"],
                        "kind": destination_handle["kind"],
                        "span_hint": destination_handle["span"],
                        "expected_old_hash": identedit::changeset::hash_text(
                            destination_handle["text"]
                                .as_str()
                                .expect("destination text should be present")
                        )
                    }
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["edit", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "transform should support same-file move_before: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    let operation = &response["files"][0]["operations"][0];
    assert_eq!(operation["op"]["type"], "move_before");
    assert_eq!(operation["op"]["destination"]["type"], "node");
    assert_eq!(
        operation["op"]["destination"]["identity"],
        destination_handle["identity"]
    );
    assert_eq!(operation["preview"]["new_text"], "");
    assert_eq!(
        operation["preview"]["matched_span"]["start"],
        source_handle["span"]["start"]
    );
    assert_eq!(
        operation["preview"]["matched_span"]["end"],
        source_handle["span"]["end"]
    );
    assert_compact_preview_old_state(
        &operation["preview"],
        source_handle["text"]
            .as_str()
            .expect("source text should be present"),
    );

    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(after, before, "transform must remain dry-run");
}

#[test]
fn transform_json_mode_rejects_same_file_move_with_overlapping_destination() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let source_handle = select_handle_by_name(&file_path, "helper");

    let request = json!({
        "command": "edit",
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "identity": source_handle["identity"],
                    "kind": source_handle["kind"],
                    "span_hint": source_handle["span"],
                    "expected_old_hash": identedit::changeset::hash_text(
                        source_handle["text"].as_str().expect("source text should be present")
                    )
                },
                "op": {
                    "type": "move_before",
                    "destination": {
                        "identity": source_handle["identity"],
                        "kind": source_handle["kind"],
                        "span_hint": source_handle["span"],
                        "expected_old_hash": identedit::changeset::hash_text(
                            source_handle["text"].as_str().expect("source text should be present")
                        )
                    }
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["edit", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "transform should reject overlapping same-file move destination"
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("destination overlaps source span")),
        "expected overlap destination diagnostic"
    );
}

#[test]
fn transform_json_mode_same_file_move_reports_missing_source_target() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let source_text = fs::read_to_string(&file_path).expect("fixture should be readable");
    let destination_handle = select_handle_by_name(&file_path, "process_data");

    let request = json!({
        "command": "edit",
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "identity": "missing-source",
                    "kind": "function_definition",
                    "expected_old_hash": identedit::changeset::hash_text("def missing():\n    pass")
                },
                "op": {
                    "type": "move_before",
                    "destination": {
                        "identity": destination_handle["identity"],
                        "kind": destination_handle["kind"],
                        "span_hint": destination_handle["span"],
                        "expected_old_hash": identedit::changeset::hash_text(
                            destination_handle["text"]
                                .as_str()
                                .expect("destination text should be present")
                        )
                    }
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["edit", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "transform should report missing source target for same-file move"
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "target_missing");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("missing-source")),
        "target_missing response should mention source identity"
    );

    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(
        after, source_text,
        "transform must remain dry-run on missing source"
    );
}

#[test]
fn transform_json_mode_same_file_move_reports_missing_destination_target() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let source_handle = select_handle_by_name(&file_path, "helper");

    let request = json!({
        "command": "edit",
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "identity": source_handle["identity"],
                    "kind": source_handle["kind"],
                    "span_hint": source_handle["span"],
                    "expected_old_hash": identedit::changeset::hash_text(
                        source_handle["text"].as_str().expect("source text should be present")
                    )
                },
                "op": {
                    "type": "move_before",
                    "destination": {
                        "identity": "missing-destination",
                        "kind": "function_definition",
                        "expected_old_hash": identedit::changeset::hash_text(
                            "def missing_destination():\n    pass"
                        )
                    }
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["edit", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "transform should report missing destination target for same-file move"
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "target_missing");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("missing-destination")),
        "target_missing response should mention destination identity"
    );
}

#[test]
fn transform_json_mode_same_file_move_reports_ambiguous_destination_target() {
    let fixture = fixture_path("ambiguous.py");
    let source_handle = select_first_handle(&fixture, "function_definition", Some("duplicate"));
    let source_text = source_handle["text"]
        .as_str()
        .expect("source text should be present");

    let request = json!({
        "command": "edit",
        "file": fixture.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "identity": source_handle["identity"],
                    "kind": source_handle["kind"],
                    "span_hint": source_handle["span"],
                    "expected_old_hash": identedit::changeset::hash_text(source_text)
                },
                "op": {
                    "type": "move_before",
                    "destination": {
                        "identity": source_handle["identity"],
                        "kind": source_handle["kind"],
                        "expected_old_hash": identedit::changeset::hash_text(source_text)
                    }
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["edit", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "transform should report ambiguous destination target for same-file move"
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "ambiguous_target");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| {
                message.contains(
                    source_handle["identity"]
                        .as_str()
                        .expect("identity should be string"),
                )
            }),
        "ambiguous_target response should mention destination identity"
    );
}

#[test]
fn transform_json_mode_supports_cross_file_move_to_before_preview() {
    let workspace = tempdir().expect("tempdir should be created");
    let source_file = workspace.path().join("source.py");
    let destination_file = workspace.path().join("destination.py");
    fs::write(
        &source_file,
        "def source_fn(value):\n    return value + 1\n\n\ndef keep_source():\n    return value + 2\n",
    )
    .expect("source fixture write should succeed");
    fs::write(
        &destination_file,
        "def destination_anchor(value):\n    return value * 2\n",
    )
    .expect("destination fixture write should succeed");

    let source_before = fs::read_to_string(&source_file).expect("source should be readable");
    let destination_before =
        fs::read_to_string(&destination_file).expect("destination should be readable");
    let source_handle = select_handle_by_name(&source_file, "source_fn");
    let destination_handle = select_handle_by_name(&destination_file, "destination_anchor");
    let source_text = source_handle["text"]
        .as_str()
        .expect("source text should be present");

    let request = json!({
        "command": "edit",
        "file": source_file.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "identity": source_handle["identity"],
                    "kind": source_handle["kind"],
                    "span_hint": source_handle["span"],
                    "expected_old_hash": identedit::changeset::hash_text(source_text)
                },
                "op": {
                    "type": "move_to_before",
                    "destination_file": destination_file.to_string_lossy().to_string(),
                    "destination": {
                        "identity": destination_handle["identity"],
                        "kind": destination_handle["kind"],
                        "span_hint": destination_handle["span"],
                        "expected_old_hash": identedit::changeset::hash_text(
                            destination_handle["text"].as_str().expect("destination text should be present")
                        )
                    }
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["edit", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "cross-file move_to_before transform should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    let files = response["files"]
        .as_array()
        .expect("response files should be array");
    assert_eq!(
        files.len(),
        2,
        "cross-file move should normalize to two file changes"
    );

    let source_entry = files
        .iter()
        .find(|entry| entry["file"] == source_file.to_string_lossy().to_string())
        .expect("source file entry should exist");
    let destination_entry = files
        .iter()
        .find(|entry| entry["file"] == destination_file.to_string_lossy().to_string())
        .expect("destination file entry should exist");

    assert_eq!(source_entry["operations"][0]["op"]["type"], "delete");
    assert_eq!(source_entry["operations"][0]["preview"]["new_text"], "");
    assert_eq!(
        destination_entry["operations"][0]["op"]["type"],
        "insert_before"
    );
    assert_eq!(
        destination_entry["operations"][0]["preview"]["new_text"],
        source_text
    );
    assert_eq!(
        destination_entry["operations"][0]["target"]["identity"],
        destination_handle["identity"]
    );

    let source_after = fs::read_to_string(&source_file).expect("source should be readable");
    let destination_after =
        fs::read_to_string(&destination_file).expect("destination should be readable");
    assert_eq!(source_after, source_before, "transform must stay dry-run");
    assert_eq!(
        destination_after, destination_before,
        "transform must stay dry-run"
    );
}

#[test]
fn transform_json_mode_cross_file_move_reports_missing_destination_target() {
    let workspace = tempdir().expect("tempdir should be created");
    let source_file = workspace.path().join("source.py");
    let destination_file = workspace.path().join("destination.py");
    fs::write(
        &source_file,
        "def source_fn(value):\n    return value + 1\n",
    )
    .expect("source fixture write should succeed");
    fs::write(
        &destination_file,
        "def destination_anchor(value):\n    return value * 2\n",
    )
    .expect("destination fixture write should succeed");

    let source_handle = select_handle_by_name(&source_file, "source_fn");
    let source_text = source_handle["text"]
        .as_str()
        .expect("source text should be present");

    let request = json!({
        "command": "edit",
        "file": source_file.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "identity": source_handle["identity"],
                    "kind": source_handle["kind"],
                    "span_hint": source_handle["span"],
                    "expected_old_hash": identedit::changeset::hash_text(source_text)
                },
                "op": {
                    "type": "move_to_before",
                    "destination_file": destination_file.to_string_lossy().to_string(),
                    "destination": {
                        "identity": "missing-destination-target",
                        "kind": "function_definition",
                        "expected_old_hash": identedit::changeset::hash_text("def missing():\n    return 0\n")
                    }
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["edit", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "cross-file move should fail when destination target is missing"
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "target_missing");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("missing-destination-target")),
        "expected missing destination identity in diagnostic"
    );
}

#[test]
fn transform_json_mode_cross_file_move_reports_ambiguous_source_target() {
    let workspace = tempdir().expect("tempdir should be created");
    let source_file = fixture_path("ambiguous.py");
    let destination_file = workspace.path().join("destination.py");
    fs::write(
        &destination_file,
        "def destination_anchor(value):\n    return value * 2\n",
    )
    .expect("destination fixture write should succeed");

    let source_handle = select_first_handle(&source_file, "function_definition", Some("duplicate"));
    let source_text = source_handle["text"]
        .as_str()
        .expect("source text should be present");
    let destination_handle = select_handle_by_name(&destination_file, "destination_anchor");

    let request = json!({
        "command": "edit",
        "file": source_file.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "identity": source_handle["identity"],
                    "kind": source_handle["kind"],
                    "expected_old_hash": identedit::changeset::hash_text(source_text)
                },
                "op": {
                    "type": "move_to_before",
                    "destination_file": destination_file.to_string_lossy().to_string(),
                    "destination": {
                        "identity": destination_handle["identity"],
                        "kind": destination_handle["kind"],
                        "span_hint": destination_handle["span"],
                        "expected_old_hash": identedit::changeset::hash_text(
                            destination_handle["text"].as_str().expect("destination text should be present")
                        )
                    }
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["edit", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "cross-file move should fail when source target is ambiguous"
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "ambiguous_target");
}

#[test]
fn transform_json_mode_cross_file_move_rejects_same_file_destination() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let source_handle = select_handle_by_name(&file_path, "helper");
    let source_text = source_handle["text"]
        .as_str()
        .expect("source text should be present");
    let destination_handle = select_handle_by_name(&file_path, "process_data");

    let request = json!({
        "command": "edit",
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "identity": source_handle["identity"],
                    "kind": source_handle["kind"],
                    "span_hint": source_handle["span"],
                    "expected_old_hash": identedit::changeset::hash_text(source_text)
                },
                "op": {
                    "type": "move_to_before",
                    "destination_file": file_path.to_string_lossy().to_string(),
                    "destination": {
                        "identity": destination_handle["identity"],
                        "kind": destination_handle["kind"],
                        "span_hint": destination_handle["span"],
                        "expected_old_hash": identedit::changeset::hash_text(
                            destination_handle["text"].as_str().expect("destination text should be present")
                        )
                    }
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["edit", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "cross-file move should reject same-file destination"
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| {
                message.contains("same file") && message.contains("move_before")
            }),
        "expected same-file destination diagnostic with guidance"
    );
}
