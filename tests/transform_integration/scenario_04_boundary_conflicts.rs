use super::*;

#[test]
fn transform_json_mode_rejects_file_start_insert_and_insert_before_same_boundary() {
    let mut temp_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp python file should be created");
    temp_file
        .write_all(b"def only(value):\n    return value + 1\n")
        .expect("python fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let source = fs::read_to_string(&file_path).expect("fixture should be readable");
    let expected_file_hash = identedit::changeset::hash_text(&source);
    let handle = select_first_handle(&file_path, "function_definition", Some("only"));
    let span_start = handle["span"]["start"].as_u64().expect("span start");
    assert_eq!(
        span_start, 0,
        "fixture precondition: first function should start at byte 0"
    );

    let request = json!({
        "command": "transform",
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "type": "file_start",
                    "expected_file_hash": expected_file_hash
                },
                "op": {
                    "type": "insert",
                    "new_text": "# file-header\n"
                }
            },
            {
                "target": {
                    "identity": handle["identity"],
                    "kind": handle["kind"],
                    "span_hint": {
                        "start": handle["span"]["start"],
                        "end": handle["span"]["end"]
                    },
                    "expected_old_hash": identedit::changeset::hash_text(
                        handle["text"].as_str().expect("text should be string")
                    )
                },
                "op": {
                    "type": "insert_before",
                    "new_text": "# node-header\n"
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["transform", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "transform should reject file_start insert colliding with insert_before"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("Overlapping operations")),
        "expected overlap conflict message"
    );
}

#[test]
fn transform_json_mode_rejects_file_end_insert_and_insert_after_same_boundary() {
    let mut temp_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp python file should be created");
    temp_file
        .write_all(b"def tail(value):\n    return value + 1")
        .expect("python fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let source = fs::read_to_string(&file_path).expect("fixture should be readable");
    let expected_file_hash = identedit::changeset::hash_text(&source);
    let file_end = source.len() as u64;
    let handle = select_first_handle(&file_path, "function_definition", Some("tail"));
    let span_end = handle["span"]["end"].as_u64().expect("span end");
    assert_eq!(
        span_end, file_end,
        "fixture precondition: tail function should end at file boundary"
    );

    let request = json!({
        "command": "transform",
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "type": "file_end",
                    "expected_file_hash": expected_file_hash
                },
                "op": {
                    "type": "insert",
                    "new_text": "\n# file-tail\n"
                }
            },
            {
                "target": {
                    "identity": handle["identity"],
                    "kind": handle["kind"],
                    "span_hint": {
                        "start": handle["span"]["start"],
                        "end": handle["span"]["end"]
                    },
                    "expected_old_hash": identedit::changeset::hash_text(
                        handle["text"].as_str().expect("text should be string")
                    )
                },
                "op": {
                    "type": "insert_after",
                    "new_text": "\n# node-tail\n"
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["transform", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "transform should reject file_end insert colliding with insert_after"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("Overlapping operations")),
        "expected overlap conflict message"
    );
}

#[test]
fn transform_json_mode_rejects_file_start_insert_and_replace_same_boundary() {
    let mut temp_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp python file should be created");
    temp_file
        .write_all(b"def rewrite_me(value):\n    return value + 1\n")
        .expect("python fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let source = fs::read_to_string(&file_path).expect("fixture should be readable");
    let expected_file_hash = identedit::changeset::hash_text(&source);
    let handle = select_first_handle(&file_path, "function_definition", Some("rewrite_me"));
    let span_start = handle["span"]["start"].as_u64().expect("span start");
    assert_eq!(
        span_start, 0,
        "fixture precondition: rewrite target should start at byte 0"
    );

    let request = json!({
        "command": "transform",
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "type": "file_start",
                    "expected_file_hash": expected_file_hash
                },
                "op": {
                    "type": "insert",
                    "new_text": "# file-header\n"
                }
            },
            {
                "target": {
                    "identity": handle["identity"],
                    "kind": handle["kind"],
                    "span_hint": {
                        "start": handle["span"]["start"],
                        "end": handle["span"]["end"]
                    },
                    "expected_old_hash": identedit::changeset::hash_text(
                        handle["text"].as_str().expect("text should be string")
                    )
                },
                "op": {
                    "type": "replace",
                    "new_text": "def rewrite_me(value):\n    return value + 2\n"
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["transform", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "transform should reject file_start insert colliding with boundary replace"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("Overlapping operations")),
        "expected overlap conflict message"
    );
}

#[test]
fn transform_json_mode_rejects_file_start_and_file_end_inserts_on_bom_only_file() {
    let mut temp_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp python file should be created");
    temp_file
        .write_all(b"\xEF\xBB\xBF")
        .expect("bom-only fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let source = fs::read_to_string(&file_path).expect("fixture should be readable");
    let expected_file_hash = identedit::changeset::hash_text(&source);

    let request = json!({
        "command": "transform",
        "file": file_path.to_string_lossy().to_string(),
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

    let output = run_identedit_with_stdin(&["transform", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "transform should reject file_start/file_end insert overlap on BOM-only file"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("Overlapping operations")),
        "expected overlap conflict message"
    );
}

#[test]
fn transform_json_mode_rejects_file_start_insert_and_insert_before_same_boundary_on_bom_file() {
    let mut temp_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp python file should be created");
    temp_file
        .write_all(b"\xEF\xBB\xBFdef bom_start(value):\n    return value + 1\n")
        .expect("bom fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let source = fs::read_to_string(&file_path).expect("fixture should be readable");
    let expected_file_hash = identedit::changeset::hash_text(&source);
    let handle = select_first_handle(&file_path, "function_definition", Some("bom_start"));
    let span_start = handle["span"]["start"].as_u64().expect("span start");
    assert_eq!(
        span_start, 3,
        "fixture precondition: BOM file first function should start at byte 3"
    );

    let request = json!({
        "command": "transform",
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "type": "file_start",
                    "expected_file_hash": expected_file_hash
                },
                "op": {
                    "type": "insert",
                    "new_text": "# file-header\n"
                }
            },
            {
                "target": {
                    "identity": handle["identity"],
                    "kind": handle["kind"],
                    "span_hint": {
                        "start": handle["span"]["start"],
                        "end": handle["span"]["end"]
                    },
                    "expected_old_hash": identedit::changeset::hash_text(
                        handle["text"].as_str().expect("text should be string")
                    )
                },
                "op": {
                    "type": "insert_before",
                    "new_text": "# node-header\n"
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["transform", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "transform should reject BOM file_start insert colliding with insert_before"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("Overlapping operations")),
        "expected overlap conflict message"
    );
}

#[test]
fn transform_json_mode_rejects_file_end_insert_and_insert_after_same_boundary_on_bom_file() {
    let mut temp_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp python file should be created");
    temp_file
        .write_all(b"\xEF\xBB\xBFdef bom_end(value):\n    return value + 1")
        .expect("bom fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let source = fs::read_to_string(&file_path).expect("fixture should be readable");
    let expected_file_hash = identedit::changeset::hash_text(&source);
    let file_end = source.len() as u64;
    let handle = select_first_handle(&file_path, "function_definition", Some("bom_end"));
    let span_end = handle["span"]["end"].as_u64().expect("span end");
    assert_eq!(
        span_end, file_end,
        "fixture precondition: BOM file function should end at file boundary"
    );

    let request = json!({
        "command": "transform",
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "type": "file_end",
                    "expected_file_hash": expected_file_hash
                },
                "op": {
                    "type": "insert",
                    "new_text": "\n# file-tail\n"
                }
            },
            {
                "target": {
                    "identity": handle["identity"],
                    "kind": handle["kind"],
                    "span_hint": {
                        "start": handle["span"]["start"],
                        "end": handle["span"]["end"]
                    },
                    "expected_old_hash": identedit::changeset::hash_text(
                        handle["text"].as_str().expect("text should be string")
                    )
                },
                "op": {
                    "type": "insert_after",
                    "new_text": "\n# node-tail\n"
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["transform", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "transform should reject BOM file_end insert colliding with insert_after"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("Overlapping operations")),
        "expected overlap conflict message"
    );
}

#[test]
fn transform_json_mode_boundary_conflict_message_is_order_independent() {
    let mut temp_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp python file should be created");
    temp_file
        .write_all(b"def stable(value):\n    return value + 1\n")
        .expect("python fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let source = fs::read_to_string(&file_path).expect("fixture should be readable");
    let expected_file_hash = identedit::changeset::hash_text(&source);
    let handle = select_first_handle(&file_path, "function_definition", Some("stable"));
    let span_start = handle["span"]["start"].as_u64().expect("span start");
    let span_end = handle["span"]["end"].as_u64().expect("span end");
    let expected_old_hash =
        identedit::changeset::hash_text(handle["text"].as_str().expect("text should be string"));

    let file_start_insert = json!({
        "target": {
            "type": "file_start",
            "expected_file_hash": expected_file_hash
        },
        "op": {
            "type": "insert",
            "new_text": "# file-header\n"
        }
    });
    let node_insert_before = json!({
        "target": {
            "identity": handle["identity"],
            "kind": handle["kind"],
            "span_hint": {
                "start": span_start,
                "end": span_end
            },
            "expected_old_hash": expected_old_hash
        },
        "op": {
            "type": "insert_before",
            "new_text": "# node-header\n"
        }
    });

    let permutations = [
        [file_start_insert.clone(), node_insert_before.clone()],
        [node_insert_before, file_start_insert],
    ];
    let mut first_error_message: Option<String> = None;

    for operations in permutations {
        let request = json!({
            "command": "transform",
            "file": file_path.to_string_lossy().to_string(),
            "operations": operations
        });

        let output = run_identedit_with_stdin(&["transform", "--json"], &request.to_string());
        assert!(
            !output.status.success(),
            "transform should fail for same-boundary file/node insert conflicts"
        );

        let response: Value =
            serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
        assert_eq!(response["error"]["type"], "invalid_request");
        let message = response["error"]["message"]
            .as_str()
            .expect("error message should be string")
            .to_string();

        if let Some(expected_message) = &first_error_message {
            assert_eq!(
                message, *expected_message,
                "conflict error message should be order-independent"
            );
        } else {
            first_error_message = Some(message);
        }
    }
}

#[test]
fn transform_json_mode_rejects_unknown_file_level_target_type() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let request = json!({
        "command": "transform",
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "type": "file_middle",
                    "expected_file_hash": "irrelevant"
                },
                "op": {
                    "type": "insert",
                    "new_text": "# invalid target type\n"
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["transform", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "transform should reject unknown file-level target variant"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("unknown variant")),
        "expected unknown variant parse error"
    );
}

#[test]
fn transform_json_mode_builds_file_start_insert_preview_on_bom_only_file() {
    let mut temp_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp python file should be created");
    temp_file
        .write_all(b"\xEF\xBB\xBF")
        .expect("bom-only fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let expected_file_hash = identedit::changeset::hash_text(&before);

    let request = json!({
        "command": "transform",
        "file": file_path.to_string_lossy().to_string(),
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
            }
        ]
    });

    let output = run_identedit_with_stdin(&["transform", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "transform should build file_start preview on BOM-only file: {}",
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

    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "transform must remain dry-run");
}

#[test]
fn transform_json_mode_builds_file_end_insert_preview_on_bom_only_file() {
    let mut temp_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp python file should be created");
    temp_file
        .write_all(b"\xEF\xBB\xBF")
        .expect("bom-only fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let expected_file_hash = identedit::changeset::hash_text(&before);

    let request = json!({
        "command": "transform",
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
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

    let output = run_identedit_with_stdin(&["transform", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "transform should build file_end preview on BOM-only file: {}",
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

    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "transform must remain dry-run");
}

#[test]
fn transform_json_mode_allows_insert_before_and_after_on_same_anchor() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_first_handle(&file_path, "function_definition", Some("process_*"));
    let span_start = handle["span"]["start"].as_u64().expect("span start");
    let span_end = handle["span"]["end"].as_u64().expect("span end");

    let request = json!({
        "command": "transform",
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
                    "new_text": "# before\n"
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
                    "new_text": "\n# after"
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["transform", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "transform should allow insert_before + insert_after on same anchor: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    let operations = response["files"][0]["operations"]
        .as_array()
        .expect("operations should be an array");
    assert_eq!(operations.len(), 2);
    assert_eq!(
        operations[0]["preview"]["matched_span"]["start"],
        span_start
    );
    assert_eq!(operations[0]["preview"]["matched_span"]["end"], span_start);
    assert_eq!(operations[1]["preview"]["matched_span"]["start"], span_end);
    assert_eq!(operations[1]["preview"]["matched_span"]["end"], span_end);
}
