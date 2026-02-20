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

#[test]
fn transform_json_mode_insert_preview_order_stability_for_json_anchor() {
    let file_path = copy_fixture_to_temp_json("example.json");
    let handle = select_first_handle(&file_path, "object", Some("config"));
    let span_start = handle["span"]["start"].as_u64().expect("span start");
    let span_end = handle["span"]["end"].as_u64().expect("span end");
    let before_insert = "__before__";
    let after_insert = "__after__";

    let request_a = json!({
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
                    "new_text": before_insert
                }
            },
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
                    "new_text": after_insert
                }
            }
        ]
    });
    let output_a = run_identedit_with_stdin(&["edit", "--json"], &request_a.to_string());
    assert!(
        output_a.status.success(),
        "first permutation should succeed: {}",
        String::from_utf8_lossy(&output_a.stderr)
    );
    let response_a: Value =
        serde_json::from_slice(&output_a.stdout).expect("stdout should be valid JSON");
    let operations_a = response_a["files"][0]["operations"]
        .as_array()
        .expect("operations should be an array");

    let request_b = json!({
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
                    "new_text": after_insert
                }
            },
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
                    "new_text": before_insert
                }
            }
        ]
    });
    let output_b = run_identedit_with_stdin(&["edit", "--json"], &request_b.to_string());
    assert!(
        output_b.status.success(),
        "second permutation should succeed: {}",
        String::from_utf8_lossy(&output_b.stderr)
    );
    let response_b: Value =
        serde_json::from_slice(&output_b.stdout).expect("stdout should be valid JSON");
    let operations_b = response_b["files"][0]["operations"]
        .as_array()
        .expect("operations should be an array");

    let before_a = operations_a
        .iter()
        .find(|operation| operation["op"]["type"] == "insert_before")
        .expect("first response should include insert_before");
    let after_a = operations_a
        .iter()
        .find(|operation| operation["op"]["type"] == "insert_after")
        .expect("first response should include insert_after");
    let before_b = operations_b
        .iter()
        .find(|operation| operation["op"]["type"] == "insert_before")
        .expect("second response should include insert_before");
    let after_b = operations_b
        .iter()
        .find(|operation| operation["op"]["type"] == "insert_after")
        .expect("second response should include insert_after");

    assert_eq!(before_a["preview"]["new_text"], before_insert);
    assert_eq!(before_b["preview"]["new_text"], before_insert);
    assert_eq!(before_a["preview"]["matched_span"]["start"], span_start);
    assert_eq!(before_a["preview"]["matched_span"]["end"], span_start);
    assert_eq!(before_b["preview"]["matched_span"]["start"], span_start);
    assert_eq!(before_b["preview"]["matched_span"]["end"], span_start);

    assert_eq!(after_a["preview"]["new_text"], after_insert);
    assert_eq!(after_b["preview"]["new_text"], after_insert);
    assert_eq!(after_a["preview"]["matched_span"]["start"], span_end);
    assert_eq!(after_a["preview"]["matched_span"]["end"], span_end);
    assert_eq!(after_b["preview"]["matched_span"]["start"], span_end);
    assert_eq!(after_b["preview"]["matched_span"]["end"], span_end);
}

#[test]
fn transform_json_mode_rejects_replace_and_insert_on_same_anchor() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_first_handle(&file_path, "function_definition", Some("process_*"));

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
                    "type": "replace",
                    "new_text": "def process_data(value):\n    return value * 2"
                }
            },
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
                    "new_text": "# conflicting insert\n"
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["edit", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "transform should reject replace+insert conflict on the same anchor"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}

#[test]
fn transform_json_mode_reports_deterministic_error_for_three_operation_conflicts() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let process_handle = select_first_handle(&file_path, "function_definition", Some("process_*"));
    let helper_handle = select_first_handle(&file_path, "function_definition", Some("helper"));
    let process_hash = identedit::changeset::hash_text(
        process_handle["text"]
            .as_str()
            .expect("process handle text should be string"),
    );
    let helper_hash = identedit::changeset::hash_text(
        helper_handle["text"]
            .as_str()
            .expect("helper handle text should be string"),
    );

    let process_replace_a = json!({
        "identity": process_handle["identity"],
        "kind": process_handle["kind"],
        "span_hint": {
            "start": process_handle["span"]["start"],
            "end": process_handle["span"]["end"]
        },
        "expected_old_hash": process_hash,
        "op": {
            "type": "replace",
            "new_text": "def process_data(value):\n    return value + 10"
        }
    });
    let process_replace_b = json!({
        "identity": process_handle["identity"],
        "kind": process_handle["kind"],
        "span_hint": {
            "start": process_handle["span"]["start"],
            "end": process_handle["span"]["end"]
        },
        "expected_old_hash": process_hash,
        "op": {
            "type": "replace",
            "new_text": "def process_data(value):\n    return value + 20"
        }
    });
    let helper_replace = json!({
        "identity": helper_handle["identity"],
        "kind": helper_handle["kind"],
        "span_hint": {
            "start": helper_handle["span"]["start"],
            "end": helper_handle["span"]["end"]
        },
        "expected_old_hash": helper_hash,
        "op": {
            "type": "replace",
            "new_text": "def helper():\n    return \"changed\""
        }
    });

    let operations = [process_replace_a, process_replace_b, helper_replace];
    let permutations = [
        [0_usize, 1_usize, 2_usize],
        [0_usize, 2_usize, 1_usize],
        [1_usize, 0_usize, 2_usize],
        [1_usize, 2_usize, 0_usize],
        [2_usize, 0_usize, 1_usize],
        [2_usize, 1_usize, 0_usize],
    ];

    let mut first_error_message: Option<String> = None;
    for permutation in permutations {
        let ordered_operations: Vec<Value> = permutation
            .iter()
            .map(|index| operations[*index].clone())
            .collect();
        let request = json!({
            "command": "edit",
            "file": file_path.to_string_lossy().to_string(),
            "operations": ordered_operations
        });

        let output = run_identedit_with_stdin(&["edit", "--json"], &request.to_string());
        assert!(
            !output.status.success(),
            "transform should fail for conflicting three-operation permutation {permutation:?}"
        );

        let response: Value =
            serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
        assert_eq!(response["error"]["type"], "invalid_request");
        let message = response["error"]["message"]
            .as_str()
            .expect("error message should be a string")
            .to_string();

        if let Some(expected_message) = &first_error_message {
            assert_eq!(
                message, *expected_message,
                "transform conflict message should remain deterministic across permutations"
            );
        } else {
            assert!(!message.is_empty(), "error message should not be empty");
            first_error_message = Some(message);
        }
    }
}

#[test]
fn transform_json_mode_insert_after_supports_crlf_source_files() {
    let mut temp_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp python file should be created");
    let source = "def process_data(value):\r\n    result = value + 1\r\n    return result\r\n\r\n\r\ndef helper():\r\n    return \"helper\"\r\n";
    temp_file
        .write_all(source.as_bytes())
        .expect("crlf fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
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
                    "new_text": "\r\n# inserted-crlf"
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["edit", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "transform should support insert_after on CRLF source: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    let preview = &response["files"][0]["operations"][0]["preview"];
    assert_compact_preview_old_state(preview, "");
    assert_eq!(preview["matched_span"]["start"], span_end);
    assert_eq!(preview["matched_span"]["end"], span_end);
}

#[test]
fn transform_json_mode_insert_before_supports_utf8_bom_prefixed_source() {
    let mut temp_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp python file should be created");
    temp_file
        .write_all(b"\xEF\xBB\xBFdef process_data(value):\n    return value + 1\n")
        .expect("bom python fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let handle = select_first_handle(&file_path, "function_definition", Some("process_*"));

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
                    "new_text": "# bom-header\n"
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["edit", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "transform should support insert_before on BOM source: {}",
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
}

#[test]
fn transform_json_mode_insert_returns_parse_failure_for_nul_python_source() {
    let mut temp_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp python file should be created");
    temp_file
        .write_all(b"def process_data(value):\n    return value + 1\x00\n")
        .expect("nul python fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let request = json!({
        "command": "edit",
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "identity": "placeholder",
                "kind": "function_definition",
                "span_hint": {
                    "start": 0,
                    "end": 1
                },
                "expected_old_hash": "placeholder-hash",
                "op": {
                    "type": "insert_before",
                    "new_text": "# should not apply\n"
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["edit", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "transform should fail for NUL python source even for insert operations"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "parse_failure");
}

#[test]
fn transform_json_mode_returns_ambiguous_target_when_span_hint_misses_candidates() {
    let fixture = fixture_path("ambiguous.py");
    let handle = select_first_handle(&fixture, "function_definition", Some("duplicate"));
    let request = json!({
        "command": "edit",
        "file": fixture.to_string_lossy().to_string(),
        "operations": [
            {
                "identity": handle["identity"],
                "kind": handle["kind"],
                "span_hint": {
                    "start": 9999,
                    "end": 10001
                },
                "expected_old_hash": identedit::changeset::hash_text(
                    handle["text"].as_str().expect("text should be string")
                ),
                "op": {
                    "type": "replace",
                    "new_text": "def duplicate():\n    return 99"
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["edit", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "transform should fail when span_hint cannot resolve duplicates"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "ambiguous_target");
}

#[test]
fn transform_json_mode_uses_span_hint_to_disambiguate_targets() {
    let fixture = fixture_path("ambiguous.py");
    let select_output = run_identedit(&[
        "read",
        "--json",
        "--verbose",
        "--kind",
        "function_definition",
        fixture.to_str().expect("path"),
    ]);
    assert!(
        select_output.status.success(),
        "select failed: {}",
        String::from_utf8_lossy(&select_output.stderr)
    );

    let select_response: Value =
        serde_json::from_slice(&select_output.stdout).expect("stdout should be valid JSON");
    let handles = select_response["handles"]
        .as_array()
        .expect("handles should be array");
    assert_eq!(
        handles.len(),
        2,
        "fixture should have two duplicate functions"
    );

    let first = &handles[0];
    let request = json!({
        "command": "edit",
        "file": fixture.to_string_lossy().to_string(),
        "operations": [
            {
                "identity": first["identity"],
                "kind": first["kind"],
                "span_hint": {
                    "start": first["span"]["start"],
                    "end": first["span"]["end"]
                },
                "expected_old_hash": identedit::changeset::hash_text(
                    first["text"].as_str().expect("text should be string")
                ),
                "op": {
                    "type": "replace",
                    "new_text": "def duplicate():\n    return 42"
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["edit", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "transform should succeed when span_hint resolves ambiguity: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(
        response["files"][0]["operations"].as_array().map(Vec::len),
        Some(1)
    );
    assert_eq!(
        response["files"][0]["operations"][0]["preview"]["matched_span"]["start"],
        first["span"]["start"]
    );
}

#[test]
fn transform_json_mode_detects_stale_file_after_selection() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_first_handle(&file_path, "function_definition", Some("process_*"));
    let mutated_source = "def process_data(value):\n    result = value + 2\n    return result\n\n\ndef helper():\n    return \"helper\"";
    fs::write(&file_path, mutated_source).expect("fixture mutation should succeed");

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
                    "type": "replace",
                    "new_text": "def process_data(value):\n    return value + 5"
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["edit", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "transform should fail for stale file content"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "precondition_failed");

    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(
        after, mutated_source,
        "stale-detection failure must not mutate file contents"
    );
}

#[test]
fn transform_json_mode_accepts_empty_operation_list_as_noop_changeset() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let request = json!({
        "command": "edit",
        "file": file_path.to_string_lossy().to_string(),
        "operations": []
    });

    let output = run_identedit_with_stdin(&["edit", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "transform should allow empty operation list: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(
        response["files"][0]["operations"].as_array().map(Vec::len),
        Some(0)
    );
}

#[test]
fn transform_json_mode_empty_operations_unsupported_extension_is_noop_success() {
    let mut temporary_file = Builder::new()
        .suffix(".txt")
        .tempfile()
        .expect("temp txt file should be created");
    temporary_file
        .write_all(b"plain text")
        .expect("fixture write should succeed");
    let file_path = temporary_file.path().to_path_buf();

    let request = json!({
        "command": "edit",
        "file": file_path.to_string_lossy().to_string(),
        "operations": []
    });
    let output = run_identedit_with_stdin(&["edit", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "transform should allow empty operation list through fallback: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(
        response["files"][0]["operations"]
            .as_array()
            .expect("operations should be an array")
            .len(),
        0
    );
}

#[test]
fn transform_json_mode_empty_operations_missing_file_returns_io_error() {
    let missing_path = std::env::temp_dir().join(format!(
        "identedit-transform-missing-noop-{}.py",
        std::process::id()
    ));
    if missing_path.exists() {
        fs::remove_file(&missing_path).expect("stale temp file should be removable");
    }

    let request = json!({
        "command": "edit",
        "file": missing_path.to_string_lossy().to_string(),
        "operations": []
    });
    let output = run_identedit_with_stdin(&["edit", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "transform should fail for missing files even with empty operations"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "io_error");
}

#[test]
fn transform_json_mode_operation_against_empty_json_returns_target_missing() {
    let mut temporary_file = Builder::new()
        .suffix(".json")
        .tempfile()
        .expect("temp json file should be created");
    temporary_file
        .write_all(b"")
        .expect("empty fixture write should succeed");
    let file_path = temporary_file.path().to_path_buf();

    let request = json!({
        "command": "edit",
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "identity": "non-existent-identity",
                "kind": "object",
                "expected_old_hash": "deadbeef",
                "op": {
                    "type": "replace",
                    "new_text": "{}"
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["edit", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "transform should fail against empty JSON when target does not exist"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "target_missing");
}

#[test]
fn transform_flags_mode_extensionless_file_uses_fallback_and_reports_target_missing() {
    let mut temporary_file = Builder::new()
        .tempfile()
        .expect("temp file should be created");
    temporary_file
        .write_all(b"def process_data(value):\n    return value + 1\n")
        .expect("fixture write should succeed");
    let file_path = temporary_file.path().to_path_buf();

    let output = run_identedit(&[
        "edit",
        "--identity",
        "irrelevant",
        "--replace",
        "def process_data(value):\n    return value + 2",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "transform with stale identity should fail after fallback resolution"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "target_missing");
}

#[test]
fn transform_fallback_duplicate_identity_reports_ambiguous_target() {
    let mut temporary_file = Builder::new()
        .suffix(".txt")
        .tempfile()
        .expect("temp file should be created");
    temporary_file
        .write_all(
            b"const repeated = (value) => value + 1;\nconst repeated = (value) => value + 1;\n",
        )
        .expect("fixture write should succeed");
    let file_path = temporary_file.path().to_path_buf();

    let select_output = run_identedit(&[
        "read",
        "--json",
        "--verbose",
        "--kind",
        "function_definition",
        "--name",
        "repeated",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        select_output.status.success(),
        "select should succeed via fallback: {}",
        String::from_utf8_lossy(&select_output.stderr)
    );

    let select_response: Value =
        serde_json::from_slice(&select_output.stdout).expect("stdout should be valid JSON");
    assert_eq!(select_response["summary"]["matches"], 2);

    let handles = select_response["handles"]
        .as_array()
        .expect("handles should be an array");
    let identity = handles[0]["identity"]
        .as_str()
        .expect("identity should be present");
    assert_eq!(
        handles[1]["identity"].as_str(),
        Some(identity),
        "duplicate fallback handles should share identity when text/kind/name are identical"
    );

    let transform_output = run_identedit(&[
        "edit",
        "--identity",
        identity,
        "--replace",
        "const repeated = (value) => value + 2;",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !transform_output.status.success(),
        "transform should fail with ambiguous target for duplicate identity"
    );

    let transform_response: Value =
        serde_json::from_slice(&transform_output.stdout).expect("stdout should be valid JSON");
    assert_eq!(transform_response["error"]["type"], "ambiguous_target");
}

#[test]
fn transform_flags_mode_hidden_dotfile_without_basename_uses_fallback_and_reports_target_missing() {
    let directory = tempdir().expect("tempdir should be created");
    let dotfile_path = directory.path().join(".json");
    fs::write(
        &dotfile_path,
        "def process_data(value):\n    return value + 1\n",
    )
    .expect("dotfile write should succeed");

    let output = run_identedit(&[
        "edit",
        "--identity",
        "irrelevant",
        "--replace",
        "def process_data(value):\n    return value + 2",
        dotfile_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "transform with stale identity should fail after fallback resolution"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "target_missing");
}

#[test]
fn transform_flags_mode_returns_io_error_for_directory_input() {
    let directory = tempdir().expect("tempdir should be created");
    let directory_path = directory.path().to_path_buf();

    let output = run_identedit(&[
        "edit",
        "--identity",
        "irrelevant",
        "--replace",
        "def process_data(value):\n    return value + 2",
        directory_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "transform should fail when FILE points to a directory"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "io_error");
}

#[test]
fn transform_json_mode_treats_env_token_file_path_as_literal() {
    let request = json!({
        "command": "edit",
        "file": format!("${{IDENTEDIT_TRANSFORM_JSON_PATH_{}}}/example.py", std::process::id()),
        "operations": []
    });

    let output = run_identedit_with_stdin(&["edit", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "json-mode transform path should not expand env tokens"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "io_error");
}

#[test]
fn transform_json_mode_empty_operations_hidden_dotfile_without_basename_is_noop_success() {
    let directory = tempdir().expect("tempdir should be created");
    let dotfile_path = directory.path().join(".json");
    fs::write(
        &dotfile_path,
        "def process_data(value):\n    return value + 1\n",
    )
    .expect("dotfile write should succeed");
    let request = json!({
        "command": "edit",
        "file": dotfile_path.to_string_lossy().to_string(),
        "operations": []
    });

    let output = run_identedit_with_stdin(&["edit", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "transform JSON mode should allow no-op changeset via fallback: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(
        response["files"][0]["operations"]
            .as_array()
            .expect("operations should be an array")
            .len(),
        0
    );
}

#[test]
fn transform_json_mode_returns_io_error_for_directory_input() {
    let directory = tempdir().expect("tempdir should be created");
    let request = json!({
        "command": "edit",
        "file": directory.path().to_string_lossy().to_string(),
        "operations": []
    });

    let output = run_identedit_with_stdin(&["edit", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "transform JSON mode should fail when file path is a directory"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "io_error");
}

#[cfg(unix)]
#[test]
fn transform_non_utf8_path_argument_returns_io_error_without_panicking() {
    let mut command = Command::new(env!("CARGO_BIN_EXE_identedit"));
    command.arg("edit");
    command.arg("--identity");
    command.arg("irrelevant");
    command.arg("--replace");
    command.arg("def x():\n    return 1");
    command.arg(OsString::from_vec(vec![0xFF, 0x2E, 0x70, 0x79]));

    let output = command.output().expect("failed to run identedit binary");
    assert!(
        !output.status.success(),
        "transform should fail for non-UTF8 path arguments"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "io_error");
}
