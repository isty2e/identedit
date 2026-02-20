use super::*;

#[test]
fn apply_accepts_compact_preview_for_replace_operation() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_handle(&file_path, "process_*");
    let span = &handle["span"];
    let start = span["start"].as_u64().expect("span start") as usize;
    let end = span["end"].as_u64().expect("span end") as usize;
    let old_text = handle["text"].as_str().expect("text should be string");
    let expected_hash = identedit::changeset::hash_text(old_text);
    let replacement = "def process_data(value):\n    return value + 42";

    let changeset = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "identity": handle["identity"],
                    "kind": handle["kind"],
                    "span_hint": {"start": start, "end": end},
                    "expected_old_hash": expected_hash
                },
                "op": {"type": "replace", "new_text": replacement},
                "preview": {
                    "old_hash": expected_hash,
                    "old_len": old_text.len(),
                    "new_text": replacement,
                    "matched_span": {"start": start, "end": end}
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["apply"], &changeset.to_string());
    assert!(
        output.status.success(),
        "apply should accept compact preview: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["summary"]["operations_applied"], 1);
    let after = fs::read_to_string(&file_path).expect("file should be readable");
    assert!(
        after.contains("return value + 42"),
        "apply should write compact preview changes"
    );
}

#[test]
fn apply_rejects_compact_preview_with_tampered_old_hash() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_handle(&file_path, "process_*");
    let span = &handle["span"];
    let start = span["start"].as_u64().expect("span start") as usize;
    let end = span["end"].as_u64().expect("span end") as usize;
    let old_text = handle["text"].as_str().expect("text should be string");
    let expected_hash = identedit::changeset::hash_text(old_text);
    let replacement = "def process_data(value):\n    return value + 42";

    let changeset = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "identity": handle["identity"],
                    "kind": handle["kind"],
                    "span_hint": {"start": start, "end": end},
                    "expected_old_hash": expected_hash
                },
                "op": {"type": "replace", "new_text": replacement},
                "preview": {
                    "old_hash": "aaaaaaaaaaaaaaaa",
                    "old_len": old_text.len(),
                    "new_text": replacement,
                    "matched_span": {"start": start, "end": end}
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["apply"], &changeset.to_string());
    assert!(
        !output.status.success(),
        "apply should reject tampered compact preview hash"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("preview.old_hash")),
        "expected compact preview hash mismatch message"
    );
}

#[test]
fn apply_rejects_multiple_inserts_at_same_byte_position() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_handle(&file_path, "process_*");
    let span = &handle["span"];
    let span_start = span["start"].as_u64().expect("span start") as usize;
    let span_end = span["end"].as_u64().expect("span end") as usize;
    let expected_hash =
        identedit::changeset::hash_text(handle["text"].as_str().expect("text should be string"));

    let changeset = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "identity": handle["identity"],
                    "kind": handle["kind"],
                    "span_hint": {"start": span_start, "end": span_end},
                    "expected_old_hash": expected_hash
                },
                "op": {"type": "insert_before", "new_text": "# first\n"},
                "preview": {
                    "old_text": "",
                    "new_text": "# first\n",
                    "matched_span": {"start": span_start, "end": span_start}
                }
            },
            {
                "target": {
                    "identity": handle["identity"],
                    "kind": handle["kind"],
                    "span_hint": {"start": span_start, "end": span_end},
                    "expected_old_hash": expected_hash
                },
                "op": {"type": "insert_before", "new_text": "# second\n"},
                "preview": {
                    "old_text": "",
                    "new_text": "# second\n",
                    "matched_span": {"start": span_start, "end": span_start}
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["apply"], &changeset.to_string());
    assert!(
        !output.status.success(),
        "apply should reject multiple inserts at the same byte position"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}

#[test]
fn apply_rejects_insert_touching_replace_boundary() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_handle(&file_path, "process_*");
    let span = &handle["span"];
    let span_start = span["start"].as_u64().expect("span start") as usize;
    let span_end = span["end"].as_u64().expect("span end") as usize;
    let old_text = handle["text"].as_str().expect("text should be string");
    let expected_hash = identedit::changeset::hash_text(old_text);
    let replacement = "def process_data(value):\n    return value + 99";

    let changeset = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "identity": handle["identity"],
                    "kind": handle["kind"],
                    "span_hint": {"start": span_start, "end": span_end},
                    "expected_old_hash": expected_hash
                },
                "op": {"type": "replace", "new_text": replacement},
                "preview": {
                    "old_text": old_text,
                    "new_text": replacement,
                    "matched_span": {"start": span_start, "end": span_end}
                }
            },
            {
                "target": {
                    "identity": handle["identity"],
                    "kind": handle["kind"],
                    "span_hint": {"start": span_start, "end": span_end},
                    "expected_old_hash": expected_hash
                },
                "op": {"type": "insert_before", "new_text": "# conflicting\n"},
                "preview": {
                    "old_text": "",
                    "new_text": "# conflicting\n",
                    "matched_span": {"start": span_start, "end": span_start}
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["apply"], &changeset.to_string());
    assert!(
        !output.status.success(),
        "apply should reject insert touching replace boundary"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}

#[test]
fn apply_json_rejects_delete_and_insert_on_same_anchor() {
    let file_path = copy_fixture_to_temp_json("example.json");
    let handle = select_first_handle(&file_path, "object", Some("config"));
    let span = &handle["span"];
    let span_start = span["start"].as_u64().expect("span start") as usize;
    let span_end = span["end"].as_u64().expect("span end") as usize;
    let old_text = handle["text"].as_str().expect("text should be string");
    let expected_hash = identedit::changeset::hash_text(old_text);

    let changeset = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "identity": handle["identity"],
                    "kind": handle["kind"],
                    "span_hint": {"start": span_start, "end": span_end},
                    "expected_old_hash": expected_hash
                },
                "op": {"type": "delete"},
                "preview": {
                    "old_text": old_text,
                    "new_text": "",
                    "matched_span": {"start": span_start, "end": span_end}
                }
            },
            {
                "target": {
                    "identity": handle["identity"],
                    "kind": handle["kind"],
                    "span_hint": {"start": span_start, "end": span_end},
                    "expected_old_hash": expected_hash
                },
                "op": {"type": "insert_before", "new_text": "\"mode\": \"safe\", "},
                "preview": {
                    "old_text": "",
                    "new_text": "\"mode\": \"safe\", ",
                    "matched_span": {"start": span_start, "end": span_start}
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["apply"], &changeset.to_string());
    assert!(
        !output.status.success(),
        "apply should reject delete+insert conflict on JSON anchor"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("same anchor")),
        "expected same-anchor conflict message"
    );
}

#[test]
fn apply_json_rejects_overlapping_root_replace_and_nested_insert() {
    let file_path = copy_fixture_to_temp_json("example.json");
    let root_handle = select_root_json_object_handle(&file_path);
    let nested_handle = select_first_handle(&file_path, "object", Some("config"));

    let root_span = &root_handle["span"];
    let root_start = root_span["start"].as_u64().expect("root span start") as usize;
    let root_end = root_span["end"].as_u64().expect("root span end") as usize;
    let nested_span = &nested_handle["span"];
    let nested_start = nested_span["start"].as_u64().expect("nested span start") as usize;
    let nested_end = nested_span["end"].as_u64().expect("nested span end") as usize;

    let root_old_text = root_handle["text"]
        .as_str()
        .expect("root text should be string");
    let root_hash = identedit::changeset::hash_text(root_old_text);
    let nested_old_text = nested_handle["text"]
        .as_str()
        .expect("nested text should be string");
    let nested_hash = identedit::changeset::hash_text(nested_old_text);

    let changeset = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "identity": root_handle["identity"],
                    "kind": root_handle["kind"],
                    "span_hint": {"start": root_start, "end": root_end},
                    "expected_old_hash": root_hash
                },
                "op": {"type": "replace", "new_text": "{}"},
                "preview": {
                    "old_text": root_old_text,
                    "new_text": "{}",
                    "matched_span": {"start": root_start, "end": root_end}
                }
            },
            {
                "target": {
                    "identity": nested_handle["identity"],
                    "kind": nested_handle["kind"],
                    "span_hint": {"start": nested_start, "end": nested_end},
                    "expected_old_hash": nested_hash
                },
                "op": {"type": "insert_before", "new_text": "\"mode\": \"safe\", "},
                "preview": {
                    "old_text": "",
                    "new_text": "\"mode\": \"safe\", ",
                    "matched_span": {"start": nested_start, "end": nested_start}
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["apply"], &changeset.to_string());
    assert!(
        !output.status.success(),
        "apply should reject overlapping root/nested JSON operations"
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
fn apply_json_same_anchor_insert_before_after_is_order_independent() {
    let before_insert = "__json-before__";
    let after_insert = "__json-after__";

    let file_path_a = copy_fixture_to_temp_json("example.json");
    let handle_a = select_first_handle(&file_path_a, "object", Some("config"));
    let span_a = &handle_a["span"];
    let start_a = span_a["start"].as_u64().expect("span start") as usize;
    let end_a = span_a["end"].as_u64().expect("span end") as usize;
    let expected_hash_a =
        identedit::changeset::hash_text(handle_a["text"].as_str().expect("text should be string"));
    let changeset_a = json!({
        "file": file_path_a.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "identity": handle_a["identity"],
                    "kind": handle_a["kind"],
                    "span_hint": {"start": start_a, "end": end_a},
                    "expected_old_hash": expected_hash_a
                },
                "op": {"type": "insert_before", "new_text": before_insert},
                "preview": {
                    "old_text": "",
                    "new_text": before_insert,
                    "matched_span": {"start": start_a, "end": start_a}
                }
            },
            {
                "target": {
                    "identity": handle_a["identity"],
                    "kind": handle_a["kind"],
                    "span_hint": {"start": start_a, "end": end_a},
                    "expected_old_hash": expected_hash_a
                },
                "op": {"type": "insert_after", "new_text": after_insert},
                "preview": {
                    "old_text": "",
                    "new_text": after_insert,
                    "matched_span": {"start": end_a, "end": end_a}
                }
            }
        ]
    });
    let output_a = run_identedit_with_stdin(&["apply"], &changeset_a.to_string());
    assert!(
        output_a.status.success(),
        "first order should succeed: {}",
        String::from_utf8_lossy(&output_a.stderr)
    );
    let result_a = fs::read_to_string(&file_path_a).expect("file should be readable");

    let file_path_b = copy_fixture_to_temp_json("example.json");
    let handle_b = select_first_handle(&file_path_b, "object", Some("config"));
    let span_b = &handle_b["span"];
    let start_b = span_b["start"].as_u64().expect("span start") as usize;
    let end_b = span_b["end"].as_u64().expect("span end") as usize;
    let expected_hash_b =
        identedit::changeset::hash_text(handle_b["text"].as_str().expect("text should be string"));
    let changeset_b = json!({
        "file": file_path_b.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "identity": handle_b["identity"],
                    "kind": handle_b["kind"],
                    "span_hint": {"start": start_b, "end": end_b},
                    "expected_old_hash": expected_hash_b
                },
                "op": {"type": "insert_after", "new_text": after_insert},
                "preview": {
                    "old_text": "",
                    "new_text": after_insert,
                    "matched_span": {"start": end_b, "end": end_b}
                }
            },
            {
                "target": {
                    "identity": handle_b["identity"],
                    "kind": handle_b["kind"],
                    "span_hint": {"start": start_b, "end": end_b},
                    "expected_old_hash": expected_hash_b
                },
                "op": {"type": "insert_before", "new_text": before_insert},
                "preview": {
                    "old_text": "",
                    "new_text": before_insert,
                    "matched_span": {"start": start_b, "end": start_b}
                }
            }
        ]
    });
    let output_b = run_identedit_with_stdin(&["apply"], &changeset_b.to_string());
    assert!(
        output_b.status.success(),
        "second order should succeed: {}",
        String::from_utf8_lossy(&output_b.stderr)
    );
    let result_b = fs::read_to_string(&file_path_b).expect("file should be readable");

    assert_eq!(
        result_a, result_b,
        "JSON insert_before/insert_after should be order-independent"
    );
    assert!(result_a.contains(before_insert));
    assert!(result_a.contains(after_insert));
}

#[test]
fn apply_json_delete_changeset_second_apply_returns_target_missing() {
    let file_path = copy_fixture_to_temp_json("example.json");
    let handle = select_root_json_object_handle(&file_path);
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");

    let transform_output = run_identedit(&[
        "edit",
        "--identity",
        identity,
        "--delete",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        transform_output.status.success(),
        "transform should succeed: {}",
        String::from_utf8_lossy(&transform_output.stderr)
    );

    let changeset_json =
        std::str::from_utf8(&transform_output.stdout).expect("changeset should be utf-8");
    let first_apply = run_identedit_with_stdin(&["apply"], changeset_json);
    assert!(
        first_apply.status.success(),
        "first apply should succeed: {}",
        String::from_utf8_lossy(&first_apply.stderr)
    );
    let after_first = fs::read_to_string(&file_path).expect("file should be readable");

    let second_apply = run_identedit_with_stdin(&["apply"], changeset_json);
    assert!(
        !second_apply.status.success(),
        "second apply should fail after anchor deletion"
    );
    let response: Value =
        serde_json::from_slice(&second_apply.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "target_missing");

    let after_second = fs::read_to_string(&file_path).expect("file should be readable");
    assert_eq!(
        after_second, after_first,
        "failed second apply must not mutate file"
    );
}

#[test]
fn apply_json_replace_changeset_second_apply_returns_target_missing() {
    let file_path = copy_fixture_to_temp_json("example.json");
    let handle = select_first_handle(&file_path, "object", Some("config"));
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");

    let transform_output = run_identedit(&[
        "edit",
        "--identity",
        identity,
        "--replace",
        "{\"enabled\":false,\"retries\":42}",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        transform_output.status.success(),
        "transform should succeed: {}",
        String::from_utf8_lossy(&transform_output.stderr)
    );

    let changeset_json =
        std::str::from_utf8(&transform_output.stdout).expect("changeset should be utf-8");
    let first_apply = run_identedit_with_stdin(&["apply"], changeset_json);
    assert!(
        first_apply.status.success(),
        "first apply should succeed: {}",
        String::from_utf8_lossy(&first_apply.stderr)
    );
    let after_first = fs::read_to_string(&file_path).expect("file should be readable");

    let second_apply = run_identedit_with_stdin(&["apply"], changeset_json);
    assert!(
        !second_apply.status.success(),
        "second apply should fail after replace mutation"
    );
    let response: Value =
        serde_json::from_slice(&second_apply.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "target_missing");

    let after_second = fs::read_to_string(&file_path).expect("file should be readable");
    assert_eq!(
        after_second, after_first,
        "failed second apply must not mutate file"
    );
}

#[test]
fn apply_json_insert_before_changeset_second_apply_returns_span_hint_mismatch() {
    let file_path = copy_fixture_to_temp_json("example.json");
    let handle = select_first_handle(&file_path, "key", Some("name"));

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
                    "new_text": "\"version\": 1,\n  "
                }
            }
        ]
    });
    let transform_output = run_identedit_with_stdin(&["edit", "--json"], &request.to_string());
    assert!(
        transform_output.status.success(),
        "transform should succeed: {}",
        String::from_utf8_lossy(&transform_output.stderr)
    );

    let changeset_json =
        std::str::from_utf8(&transform_output.stdout).expect("changeset should be utf-8");
    let first_apply = run_identedit_with_stdin(&["apply"], changeset_json);
    assert!(
        first_apply.status.success(),
        "first apply should succeed: {}",
        String::from_utf8_lossy(&first_apply.stderr)
    );
    let after_first = fs::read_to_string(&file_path).expect("file should be readable");

    let second_apply = run_identedit_with_stdin(&["apply"], changeset_json);
    assert!(
        !second_apply.status.success(),
        "second apply should fail after anchor span shifts"
    );
    let response: Value =
        serde_json::from_slice(&second_apply.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("span_hint")),
        "expected span_hint mismatch message"
    );

    let after_second = fs::read_to_string(&file_path).expect("file should be readable");
    assert_eq!(
        after_second, after_first,
        "failed second apply must not mutate file"
    );
}

#[test]
fn apply_json_non_overlapping_operations_are_order_independent() {
    let object_replacement = "{\"enabled\": false, \"retries\": 10}";
    let array_replacement = "[3, 2, 1]";

    let file_path_a = copy_fixture_to_temp_json("example.json");
    let object_handle_a = select_first_handle(&file_path_a, "object", Some("config"));
    let array_handle_a = select_first_handle(&file_path_a, "array", Some("items"));
    let object_span_a = &object_handle_a["span"];
    let array_span_a = &array_handle_a["span"];

    let changeset_a = json!({
        "file": file_path_a.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "identity": object_handle_a["identity"],
                    "kind": object_handle_a["kind"],
                    "span_hint": {"start": object_span_a["start"], "end": object_span_a["end"]},
                    "expected_old_hash": identedit::changeset::hash_text(
                        object_handle_a["text"].as_str().expect("text should be string")
                    )
                },
                "op": {"type": "replace", "new_text": object_replacement},
                "preview": {
                    "old_text": object_handle_a["text"],
                    "new_text": object_replacement,
                    "matched_span": {"start": object_span_a["start"], "end": object_span_a["end"]}
                }
            },
            {
                "target": {
                    "identity": array_handle_a["identity"],
                    "kind": array_handle_a["kind"],
                    "span_hint": {"start": array_span_a["start"], "end": array_span_a["end"]},
                    "expected_old_hash": identedit::changeset::hash_text(
                        array_handle_a["text"].as_str().expect("text should be string")
                    )
                },
                "op": {"type": "replace", "new_text": array_replacement},
                "preview": {
                    "old_text": array_handle_a["text"],
                    "new_text": array_replacement,
                    "matched_span": {"start": array_span_a["start"], "end": array_span_a["end"]}
                }
            }
        ]
    });
    let output_a = run_identedit_with_stdin(&["apply"], &changeset_a.to_string());
    assert!(
        output_a.status.success(),
        "first order should succeed: {}",
        String::from_utf8_lossy(&output_a.stderr)
    );
    let result_a = fs::read_to_string(&file_path_a).expect("file should be readable");

    let file_path_b = copy_fixture_to_temp_json("example.json");
    let object_handle_b = select_first_handle(&file_path_b, "object", Some("config"));
    let array_handle_b = select_first_handle(&file_path_b, "array", Some("items"));
    let object_span_b = &object_handle_b["span"];
    let array_span_b = &array_handle_b["span"];

    let changeset_b = json!({
        "file": file_path_b.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "identity": array_handle_b["identity"],
                    "kind": array_handle_b["kind"],
                    "span_hint": {"start": array_span_b["start"], "end": array_span_b["end"]},
                    "expected_old_hash": identedit::changeset::hash_text(
                        array_handle_b["text"].as_str().expect("text should be string")
                    )
                },
                "op": {"type": "replace", "new_text": array_replacement},
                "preview": {
                    "old_text": array_handle_b["text"],
                    "new_text": array_replacement,
                    "matched_span": {"start": array_span_b["start"], "end": array_span_b["end"]}
                }
            },
            {
                "target": {
                    "identity": object_handle_b["identity"],
                    "kind": object_handle_b["kind"],
                    "span_hint": {"start": object_span_b["start"], "end": object_span_b["end"]},
                    "expected_old_hash": identedit::changeset::hash_text(
                        object_handle_b["text"].as_str().expect("text should be string")
                    )
                },
                "op": {"type": "replace", "new_text": object_replacement},
                "preview": {
                    "old_text": object_handle_b["text"],
                    "new_text": object_replacement,
                    "matched_span": {"start": object_span_b["start"], "end": object_span_b["end"]}
                }
            }
        ]
    });
    let output_b = run_identedit_with_stdin(&["apply"], &changeset_b.to_string());
    assert!(
        output_b.status.success(),
        "second order should succeed: {}",
        String::from_utf8_lossy(&output_b.stderr)
    );
    let result_b = fs::read_to_string(&file_path_b).expect("file should be readable");

    assert_eq!(
        result_a, result_b,
        "JSON non-overlapping replace operations should be order-independent"
    );
}

#[test]
fn apply_json_replace_preview_tampering_matrix() {
    for tamper_kind in ["old_text", "new_text", "matched_span"] {
        let file_path = copy_fixture_to_temp_json("example.json");
        let before = fs::read_to_string(&file_path).expect("fixture should be readable");
        let handle = select_first_handle(&file_path, "object", Some("config"));
        let span = &handle["span"];
        let span_start = span["start"].as_u64().expect("span start");
        let span_end = span["end"].as_u64().expect("span end");
        let old_text = handle["text"].as_str().expect("text should be string");
        let replacement = "{\"enabled\": false, \"retries\": 99}";
        let expected_hash = identedit::changeset::hash_text(old_text);

        let mut operation = json!({
            "target": {
                "identity": handle["identity"],
                "kind": handle["kind"],
                "span_hint": {"start": span_start, "end": span_end},
                "expected_old_hash": expected_hash
            },
            "op": {"type": "replace", "new_text": replacement},
            "preview": {
                "old_text": old_text,
                "new_text": replacement,
                "matched_span": {"start": span_start, "end": span_end}
            }
        });

        match tamper_kind {
            "old_text" => {
                operation["preview"]["old_text"] = json!("{}");
            }
            "new_text" => {
                operation["preview"]["new_text"] = json!("{\"enabled\": true}");
            }
            "matched_span" => {
                operation["preview"]["matched_span"]["start"] = json!(span_start + 1);
            }
            _ => unreachable!("unknown tamper kind"),
        }

        let changeset = json!({
            "file": file_path.to_string_lossy().to_string(),
            "operations": [operation]
        });

        let output = run_identedit_with_stdin(&["apply"], &changeset.to_string());
        assert!(
            !output.status.success(),
            "apply should reject JSON preview tampering for {tamper_kind}"
        );

        let response: Value =
            serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
        assert_eq!(response["error"]["type"], "invalid_request");
        assert!(
            response["error"]["message"]
                .as_str()
                .is_some_and(|message| message.contains("preview")),
            "expected preview validation error for {tamper_kind}"
        );

        let after = fs::read_to_string(&file_path).expect("fixture should be readable");
        assert_eq!(
            before, after,
            "apply must remain atomic for JSON preview tamper {tamper_kind}"
        );
    }
}

#[test]
fn apply_same_anchor_insert_before_after_is_order_independent() {
    let before_insert = "# ordered-before\n";
    let after_insert = "\n# ordered-after";

    let file_path_a = copy_fixture_to_temp_python("example.py");
    let handle_a = select_named_handle(&file_path_a, "process_*");
    let span_a = &handle_a["span"];
    let start_a = span_a["start"].as_u64().expect("span start") as usize;
    let end_a = span_a["end"].as_u64().expect("span end") as usize;
    let expected_hash_a =
        identedit::changeset::hash_text(handle_a["text"].as_str().expect("text should be string"));
    let changeset_a = json!({
        "file": file_path_a.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "identity": handle_a["identity"],
                    "kind": handle_a["kind"],
                    "span_hint": {"start": start_a, "end": end_a},
                    "expected_old_hash": expected_hash_a
                },
                "op": {"type": "insert_before", "new_text": before_insert},
                "preview": {
                    "old_text": "",
                    "new_text": before_insert,
                    "matched_span": {"start": start_a, "end": start_a}
                }
            },
            {
                "target": {
                    "identity": handle_a["identity"],
                    "kind": handle_a["kind"],
                    "span_hint": {"start": start_a, "end": end_a},
                    "expected_old_hash": expected_hash_a
                },
                "op": {"type": "insert_after", "new_text": after_insert},
                "preview": {
                    "old_text": "",
                    "new_text": after_insert,
                    "matched_span": {"start": end_a, "end": end_a}
                }
            }
        ]
    });

    let output_a = run_identedit_with_stdin(&["apply"], &changeset_a.to_string());
    assert!(
        output_a.status.success(),
        "apply should succeed for before->after order: {}",
        String::from_utf8_lossy(&output_a.stderr)
    );
    let after_a = fs::read_to_string(&file_path_a).expect("file should be readable");

    let file_path_b = copy_fixture_to_temp_python("example.py");
    let handle_b = select_named_handle(&file_path_b, "process_*");
    let span_b = &handle_b["span"];
    let start_b = span_b["start"].as_u64().expect("span start") as usize;
    let end_b = span_b["end"].as_u64().expect("span end") as usize;
    let expected_hash_b =
        identedit::changeset::hash_text(handle_b["text"].as_str().expect("text should be string"));
    let changeset_b = json!({
        "file": file_path_b.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "identity": handle_b["identity"],
                    "kind": handle_b["kind"],
                    "span_hint": {"start": start_b, "end": end_b},
                    "expected_old_hash": expected_hash_b
                },
                "op": {"type": "insert_after", "new_text": after_insert},
                "preview": {
                    "old_text": "",
                    "new_text": after_insert,
                    "matched_span": {"start": end_b, "end": end_b}
                }
            },
            {
                "target": {
                    "identity": handle_b["identity"],
                    "kind": handle_b["kind"],
                    "span_hint": {"start": start_b, "end": end_b},
                    "expected_old_hash": expected_hash_b
                },
                "op": {"type": "insert_before", "new_text": before_insert},
                "preview": {
                    "old_text": "",
                    "new_text": before_insert,
                    "matched_span": {"start": start_b, "end": start_b}
                }
            }
        ]
    });

    let output_b = run_identedit_with_stdin(&["apply"], &changeset_b.to_string());
    assert!(
        output_b.status.success(),
        "apply should succeed for after->before order: {}",
        String::from_utf8_lossy(&output_b.stderr)
    );
    let after_b = fs::read_to_string(&file_path_b).expect("file should be readable");

    assert_eq!(
        after_a, after_b,
        "same-anchor before/after ordering should not change final content"
    );
}

#[test]
fn apply_insert_after_preserves_crlf_source_segments() {
    let mut temp_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp python file should be created");
    let source = "def process_data(value):\r\n    result = value + 1\r\n    return result\r\n\r\n\r\ndef helper():\r\n    return \"helper\"\r\n";
    temp_file
        .write_all(source.as_bytes())
        .expect("crlf fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let handle = select_named_handle(&file_path, "process_*");
    let span = &handle["span"];
    let start = span["start"].as_u64().expect("span start") as usize;
    let end = span["end"].as_u64().expect("span end") as usize;
    let expected_hash =
        identedit::changeset::hash_text(handle["text"].as_str().expect("text should be string"));

    let changeset = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "identity": handle["identity"],
                    "kind": handle["kind"],
                    "span_hint": {"start": start, "end": end},
                    "expected_old_hash": expected_hash
                },
                "op": {"type": "insert_after", "new_text": "\r\n# inserted-crlf\r\n"},
                "preview": {
                    "old_text": "",
                    "new_text": "\r\n# inserted-crlf\r\n",
                    "matched_span": {"start": end, "end": end}
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["apply"], &changeset.to_string());
    assert!(
        output.status.success(),
        "apply should support insert_after on CRLF source: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("file should be readable");
    assert!(
        modified.contains("return result\r\n# inserted-crlf\r\n"),
        "inserted CRLF text should appear after anchor without normalizing existing CRLF"
    );
    assert!(
        modified.contains("def helper():\r\n    return \"helper\""),
        "existing CRLF segments after the anchor should be preserved"
    );
}

#[test]
fn apply_insert_before_preserves_utf8_bom_prefix() {
    let mut temp_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp python file should be created");
    temp_file
        .write_all(b"\xEF\xBB\xBFdef process_data(value):\n    return value + 1\n")
        .expect("bom python fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let handle = select_named_handle(&file_path, "process_*");
    let span = &handle["span"];
    let start = span["start"].as_u64().expect("span start") as usize;
    let end = span["end"].as_u64().expect("span end") as usize;
    let expected_hash =
        identedit::changeset::hash_text(handle["text"].as_str().expect("text should be string"));

    let changeset = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "identity": handle["identity"],
                    "kind": handle["kind"],
                    "span_hint": {"start": start, "end": end},
                    "expected_old_hash": expected_hash
                },
                "op": {"type": "insert_before", "new_text": "# inserted-bom\n"},
                "preview": {
                    "old_text": "",
                    "new_text": "# inserted-bom\n",
                    "matched_span": {"start": start, "end": start}
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["apply"], &changeset.to_string());
    assert!(
        output.status.success(),
        "apply should support insert_before on BOM source: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let modified_bytes = fs::read(&file_path).expect("file bytes should be readable");
    assert!(
        modified_bytes.starts_with(&[0xEF, 0xBB, 0xBF]),
        "UTF-8 BOM must remain at file start after insert"
    );

    let modified_text = String::from_utf8(modified_bytes).expect("modified file should be utf-8");
    assert!(
        modified_text.starts_with("\u{feff}# inserted-bom\ndef process_data"),
        "insert text should be placed after BOM and before anchor"
    );
}

#[test]
fn apply_insert_operation_returns_parse_failure_for_nul_python_source() {
    let mut temp_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp python file should be created");
    temp_file
        .write_all(b"def process_data(value):\n    return value + 1\x00\n")
        .expect("nul python fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let changeset = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "identity": "placeholder",
                    "kind": "function_definition",
                    "span_hint": {"start": 0, "end": 1},
                    "expected_old_hash": "placeholder-hash"
                },
                "op": {"type": "insert_before", "new_text": "# should not apply\n"},
                "preview": {
                    "old_text": "",
                    "new_text": "# should not apply\n",
                    "matched_span": {"start": 0, "end": 0}
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["apply"], &changeset.to_string());
    assert!(
        !output.status.success(),
        "apply should fail for NUL python source even for insert operation"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "parse_failure");
}

#[test]
fn apply_reports_deterministic_error_for_duplicated_same_target_operations() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_handle(&file_path, "process_*");
    let span = &handle["span"];
    let start = span["start"].as_u64().expect("span start") as usize;
    let end = span["end"].as_u64().expect("span end") as usize;
    let expected_hash =
        identedit::changeset::hash_text(handle["text"].as_str().expect("text should be string"));

    let make_changeset = |first_new_text: &str, second_new_text: &str| {
        json!({
            "file": file_path.to_string_lossy().to_string(),
            "operations": [
                {
                    "target": {
                        "identity": handle["identity"],
                        "kind": handle["kind"],
                        "span_hint": {"start": start, "end": end},
                        "expected_old_hash": expected_hash
                    },
                    "op": {"type": "replace", "new_text": first_new_text},
                    "preview": {
                        "old_text": handle["text"],
                        "new_text": first_new_text,
                        "matched_span": {"start": start, "end": end}
                    }
                },
                {
                    "target": {
                        "identity": handle["identity"],
                        "kind": handle["kind"],
                        "span_hint": {"start": start, "end": end},
                        "expected_old_hash": expected_hash
                    },
                    "op": {"type": "replace", "new_text": second_new_text},
                    "preview": {
                        "old_text": handle["text"],
                        "new_text": second_new_text,
                        "matched_span": {"start": start, "end": end}
                    }
                }
            ]
        })
    };

    let first_output = run_identedit_with_stdin(
        &["apply"],
        &make_changeset(
            "def process_data(value):\n    return value + 10",
            "def process_data(value):\n    return value + 20",
        )
        .to_string(),
    );
    let second_output = run_identedit_with_stdin(
        &["apply"],
        &make_changeset(
            "def process_data(value):\n    return value + 20",
            "def process_data(value):\n    return value + 10",
        )
        .to_string(),
    );

    assert!(
        !first_output.status.success() && !second_output.status.success(),
        "duplicated target operations should fail regardless of operation order"
    );

    let first_response: Value =
        serde_json::from_slice(&first_output.stdout).expect("stdout should be valid JSON");
    let second_response: Value =
        serde_json::from_slice(&second_output.stdout).expect("stdout should be valid JSON");

    assert_eq!(first_response["error"]["type"], "invalid_request");
    assert_eq!(second_response["error"]["type"], "invalid_request");
    assert_eq!(
        first_response["error"]["message"], second_response["error"]["message"],
        "error message should be deterministic for duplicated same-target operations"
    );
}

#[test]
fn apply_reports_deterministic_error_for_three_operation_conflict_permutations() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let process_handle = select_named_handle(&file_path, "process_*");
    let helper_handle = select_named_handle(&file_path, "helper");
    let process_span = &process_handle["span"];
    let helper_span = &helper_handle["span"];
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
        "target": {
            "identity": process_handle["identity"],
            "kind": process_handle["kind"],
            "span_hint": {"start": process_span["start"], "end": process_span["end"]},
            "expected_old_hash": process_hash
        },
        "op": {"type": "replace", "new_text": "def process_data(value):\n    return value + 10"},
        "preview": {
            "old_text": process_handle["text"],
            "new_text": "def process_data(value):\n    return value + 10",
            "matched_span": {"start": process_span["start"], "end": process_span["end"]}
        }
    });
    let process_replace_b = json!({
        "target": {
            "identity": process_handle["identity"],
            "kind": process_handle["kind"],
            "span_hint": {"start": process_span["start"], "end": process_span["end"]},
            "expected_old_hash": process_hash
        },
        "op": {"type": "replace", "new_text": "def process_data(value):\n    return value + 20"},
        "preview": {
            "old_text": process_handle["text"],
            "new_text": "def process_data(value):\n    return value + 20",
            "matched_span": {"start": process_span["start"], "end": process_span["end"]}
        }
    });
    let helper_replace = json!({
        "target": {
            "identity": helper_handle["identity"],
            "kind": helper_handle["kind"],
            "span_hint": {"start": helper_span["start"], "end": helper_span["end"]},
            "expected_old_hash": helper_hash
        },
        "op": {"type": "replace", "new_text": "def helper():\n    return \"changed\""},
        "preview": {
            "old_text": helper_handle["text"],
            "new_text": "def helper():\n    return \"changed\"",
            "matched_span": {"start": helper_span["start"], "end": helper_span["end"]}
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
        let changeset = json!({
            "file": file_path.to_string_lossy().to_string(),
            "operations": ordered_operations
        });
        let output = run_identedit_with_stdin(&["apply"], &changeset.to_string());
        assert!(
            !output.status.success(),
            "apply should fail for conflicting three-operation permutation {permutation:?}"
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
                "error message should remain deterministic across permutations"
            );
        } else {
            assert!(!message.is_empty(), "error message should not be empty");
            first_error_message = Some(message);
        }
    }
}

#[test]
fn apply_rejects_span_hint_with_start_greater_than_end() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_handle(&file_path, "process_*");
    let span = &handle["span"];
    let start = span["start"].as_u64().expect("span start") as usize;
    let end = span["end"].as_u64().expect("span end") as usize;
    let expected_hash =
        identedit::changeset::hash_text(handle["text"].as_str().expect("text should be string"));

    let changeset = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "identity": handle["identity"],
                    "kind": handle["kind"],
                    "span_hint": {"start": end, "end": start},
                    "expected_old_hash": expected_hash
                },
                "op": {"type": "replace", "new_text": "def process_data(value):\n    return value + 99"},
                "preview": {
                    "old_text": handle["text"],
                    "new_text": "def process_data(value):\n    return value + 99",
                    "matched_span": {"start": start, "end": end}
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["apply"], &changeset.to_string());
    assert!(
        !output.status.success(),
        "apply should reject invalid span_hint boundaries"
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
fn apply_rejects_zero_length_span_hint() {
    let fixture = fixture_path("ambiguous.py");
    let handle_output = run_identedit(&[
        "read",
        "--json",
        "--verbose",
        "--kind",
        "function_definition",
        "--name",
        "duplicate",
        fixture.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        handle_output.status.success(),
        "select should succeed: {}",
        String::from_utf8_lossy(&handle_output.stderr)
    );
    let select_response: Value =
        serde_json::from_slice(&handle_output.stdout).expect("stdout should be valid JSON");
    let handle = select_response["handles"][0].clone();
    let start = handle["span"]["start"].as_u64().expect("span start") as usize;
    let expected_hash =
        identedit::changeset::hash_text(handle["text"].as_str().expect("text should be string"));

    let changeset = json!({
        "file": fixture.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "identity": handle["identity"],
                    "kind": handle["kind"],
                    "span_hint": {"start": start, "end": start},
                    "expected_old_hash": expected_hash
                },
                "op": {"type": "replace", "new_text": "def duplicate():\n    return 99"},
                "preview": {
                    "old_text": handle["text"],
                    "new_text": "def duplicate():\n    return 99",
                    "matched_span": {
                        "start": handle["span"]["start"],
                        "end": handle["span"]["end"]
                    }
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["apply"], &changeset.to_string());
    assert!(
        !output.status.success(),
        "apply should reject zero-length span_hint"
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
fn apply_rejects_extreme_span_values_without_panicking() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_handle(&file_path, "process_*");
    let expected_hash =
        identedit::changeset::hash_text(handle["text"].as_str().expect("text should be string"));

    let changeset = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "identity": handle["identity"],
                    "kind": handle["kind"],
                    "span_hint": {"start": usize::MAX - 1, "end": usize::MAX},
                    "expected_old_hash": expected_hash
                },
                "op": {"type": "replace", "new_text": "def process_data(value):\n    return value + 99"},
                "preview": {
                    "old_text": handle["text"],
                    "new_text": "def process_data(value):\n    return value + 99",
                    "matched_span": {
                        "start": handle["span"]["start"],
                        "end": handle["span"]["end"]
                    }
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["apply"], &changeset.to_string());
    assert!(
        !output.status.success(),
        "apply should reject extreme span values"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}

#[test]
fn apply_rejects_non_matching_span_hint_for_unique_target() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_handle(&file_path, "process_*");
    let span = &handle["span"];
    let start = span["start"].as_u64().expect("span start") as usize;
    let end = span["end"].as_u64().expect("span end") as usize;
    let expected_hash =
        identedit::changeset::hash_text(handle["text"].as_str().expect("text should be string"));

    let changeset = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "identity": handle["identity"],
                    "kind": handle["kind"],
                    "span_hint": {"start": 9999, "end": 10000},
                    "expected_old_hash": expected_hash
                },
                "op": {"type": "replace", "new_text": "def process_data(value):\n    return value + 99"},
                "preview": {
                    "old_text": handle["text"],
                    "new_text": "def process_data(value):\n    return value + 99",
                    "matched_span": {"start": start, "end": end}
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["apply"], &changeset.to_string());
    assert!(
        !output.status.success(),
        "apply should reject non-matching span_hint for unique target"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("span_hint")),
        "expected span_hint mismatch message"
    );
}

#[test]
fn apply_preserves_real_newlines_in_replacement_text() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_handle(&file_path, "process_*");
    let span = &handle["span"];
    let replacement = "def process_data(value):\n    total = value + 3\n    return total";
    let expected_hash =
        identedit::changeset::hash_text(handle["text"].as_str().expect("text should be string"));

    let changeset = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "identity": handle["identity"],
                    "kind": handle["kind"],
                    "span_hint": {"start": span["start"], "end": span["end"]},
                    "expected_old_hash": expected_hash
                },
                "op": {"type": "replace", "new_text": replacement},
                "preview": {
                    "old_text": handle["text"],
                    "new_text": replacement,
                    "matched_span": {"start": span["start"], "end": span["end"]}
                }
            }
        ]
    });

    let changeset_file = write_changeset_json(&changeset.to_string());
    let output = run_identedit(&[
        "apply",
        changeset_file
            .path()
            .to_str()
            .expect("changeset path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "apply should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("file should be readable");
    assert!(modified.contains("total = value + 3"));
    assert!(modified.contains("return total"));
    assert!(
        modified.lines().count() > 5,
        "multiline replacement should add lines"
    );
}

#[test]
fn apply_json_mode_rejects_invalid_json_payload() {
    let output = run_identedit_with_stdin(&["apply", "--json"], "{");
    assert!(
        !output.status.success(),
        "apply should fail for malformed JSON request"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}

#[test]
fn apply_changeset_file_invalid_utf8_contents_return_io_error() {
    let mut changeset_file = Builder::new()
        .suffix(".json")
        .tempfile()
        .expect("changeset temp file should be created");
    changeset_file
        .write_all(&[0xFF, 0xFE, 0xFD])
        .expect("invalid utf8 payload write should succeed");

    let output = run_identedit(&[
        "apply",
        changeset_file
            .path()
            .to_str()
            .expect("changeset path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "apply should fail for invalid UTF-8 changeset file"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "io_error");
}

#[test]
fn apply_changeset_file_bom_only_payload_returns_invalid_request() {
    let mut changeset_file = Builder::new()
        .suffix(".json")
        .tempfile()
        .expect("changeset temp file should be created");
    changeset_file
        .write_all(&[0xEF, 0xBB, 0xBF])
        .expect("bom-only payload write should succeed");

    let output = run_identedit(&[
        "apply",
        changeset_file
            .path()
            .to_str()
            .expect("changeset path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "apply should reject BOM-only changeset payload"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}

#[test]
fn apply_changeset_file_empty_or_whitespace_payload_returns_invalid_request() {
    for payload in ["", " ", "\n\t", "\r\n    "] {
        let changeset_file = write_changeset_json(payload);
        let output = run_identedit(&[
            "apply",
            changeset_file
                .path()
                .to_str()
                .expect("changeset path should be utf-8"),
        ]);
        assert!(
            !output.status.success(),
            "apply should reject empty/whitespace file-mode payload"
        );

        let response: Value =
            serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
        assert_eq!(response["error"]["type"], "invalid_request");
    }
}

#[test]
fn apply_changeset_file_duplicate_field_is_deterministic_parse_error() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let file_literal = json_string_literal(&file_path);
    let payload = format!(
        "{{\"files\":[{{\"file\":\"{file_literal}\",\"operations\":[]}}],\"files\":[{{\"file\":\"{file_literal}\",\"operations\":[]}}]}}"
    );

    let changeset_file = write_raw_changeset_json(&payload);
    let output = run_identedit(&[
        "apply",
        changeset_file
            .path()
            .to_str()
            .expect("changeset path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "apply should reject duplicate fields in changeset file"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("duplicate field")),
        "expected deterministic duplicate-field parse message"
    );
}

#[test]
fn apply_changeset_file_unknown_field_rejected_by_strict_mode() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let changeset = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": [],
        "unexpected": true
    });

    let changeset_file = write_changeset_json(&changeset.to_string());
    let output = run_identedit(&[
        "apply",
        changeset_file
            .path()
            .to_str()
            .expect("changeset path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "apply should reject unknown fields in file-mode changeset"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("unknown field `unexpected`")),
        "expected deny_unknown_fields message in file mode"
    );
}

#[test]
fn apply_changeset_file_rejects_wrapped_command_payload() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let payload = json!({
        "command": "apply",
        "file": file_path.to_string_lossy().to_string(),
        "operations": []
    });

    let changeset_file = write_changeset_json(&payload.to_string());
    let output = run_identedit(&[
        "apply",
        changeset_file
            .path()
            .to_str()
            .expect("changeset path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "apply should reject command-wrapped payload in file mode"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("unknown field `command`")),
        "expected strict unknown-field rejection for wrapped command payload"
    );
}

#[test]
fn apply_changeset_file_raw_v1_payload_is_rejected_after_v2_cutover() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let file_literal = json_string_literal(&file_path);
    let payload = format!("{{\"file\":\"{file_literal}\",\"operations\":[]}}");

    let changeset_file = write_raw_changeset_json(&payload);
    let output = run_identedit(&[
        "apply",
        changeset_file
            .path()
            .to_str()
            .expect("changeset path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "apply file mode should reject raw v1 payload post v2 cutover"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("unknown field `file`")),
        "expected explicit v1->v2 parse diagnostic in file mode"
    );
}

#[test]
fn apply_changeset_file_rejects_non_object_top_level_payload() {
    for payload in ["[]", "null", "1"] {
        let changeset_file = write_changeset_json(payload);
        let output = run_identedit(&[
            "apply",
            changeset_file
                .path()
                .to_str()
                .expect("changeset path should be utf-8"),
        ]);
        assert!(
            !output.status.success(),
            "apply should reject non-object file-mode payload"
        );

        let response: Value =
            serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
        assert_eq!(response["error"]["type"], "invalid_request");
    }
}

#[test]
fn apply_changeset_file_utf8_bom_prefixed_payload_returns_invalid_request() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let changeset = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": []
    });

    let mut changeset_file = Builder::new()
        .suffix(".json")
        .tempfile()
        .expect("changeset temp file should be created");
    changeset_file
        .write_all(&[0xEF, 0xBB, 0xBF])
        .expect("bom prefix write should succeed");
    changeset_file
        .write_all(changeset.to_string().as_bytes())
        .expect("changeset payload write should succeed");

    let output = run_identedit(&[
        "apply",
        changeset_file
            .path()
            .to_str()
            .expect("changeset path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "apply should reject BOM-prefixed changeset file payload"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}

#[test]
fn apply_changeset_file_trailing_garbage_returns_invalid_request() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let changeset = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": []
    });
    let payload = format!("{}\ntrailing-garbage", changeset);

    let changeset_file = write_changeset_json(&payload);
    let output = run_identedit(&[
        "apply",
        changeset_file
            .path()
            .to_str()
            .expect("changeset path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "apply should reject trailing garbage in file-mode changeset payload"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}

#[test]
fn apply_changeset_file_trailing_nul_returns_invalid_request() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let changeset = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": []
    });

    let mut changeset_file = Builder::new()
        .suffix(".json")
        .tempfile()
        .expect("changeset temp file should be created");
    changeset_file
        .write_all(changeset.to_string().as_bytes())
        .expect("changeset payload write should succeed");
    changeset_file
        .write_all(&[0x00])
        .expect("trailing nul write should succeed");

    let output = run_identedit(&[
        "apply",
        changeset_file
            .path()
            .to_str()
            .expect("changeset path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "apply should reject file-mode changeset with trailing NUL byte"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}

#[test]
fn apply_changeset_file_nested_duplicate_fields_are_deterministic_parse_errors() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let file_literal = json_string_literal(&file_path);
    let payloads = [
        format!(
            "{{\"files\":[{{\"file\":\"{file_literal}\",\"operations\":[{{\"target\":{{\"identity\":\"a\",\"identity\":\"b\",\"kind\":\"function_definition\",\"expected_old_hash\":\"00\"}},\"op\":{{\"type\":\"replace\",\"new_text\":\"x\"}},\"preview\":{{\"old_text\":\"a\",\"new_text\":\"x\",\"matched_span\":{{\"start\":0,\"end\":1}}}}}}]}}],\"transaction\":{{\"mode\":\"all_or_nothing\"}}}}"
        ),
        format!(
            "{{\"files\":[{{\"file\":\"{file_literal}\",\"operations\":[{{\"target\":{{\"identity\":\"a\",\"kind\":\"function_definition\",\"expected_old_hash\":\"00\"}},\"op\":{{\"type\":\"replace\",\"new_text\":\"x\",\"new_text\":\"y\"}},\"preview\":{{\"old_text\":\"a\",\"new_text\":\"x\",\"matched_span\":{{\"start\":0,\"end\":1}}}}}}]}}],\"transaction\":{{\"mode\":\"all_or_nothing\"}}}}"
        ),
        format!(
            "{{\"files\":[{{\"file\":\"{file_literal}\",\"operations\":[{{\"target\":{{\"identity\":\"a\",\"kind\":\"function_definition\",\"expected_old_hash\":\"00\"}},\"op\":{{\"type\":\"replace\",\"new_text\":\"x\"}},\"preview\":{{\"old_text\":\"a\",\"new_text\":\"x\",\"matched_span\":{{\"start\":0,\"start\":1,\"end\":1}}}}}}]}}],\"transaction\":{{\"mode\":\"all_or_nothing\"}}}}"
        ),
    ];

    for payload in payloads {
        let changeset_file = write_raw_changeset_json(&payload);
        let output = run_identedit(&[
            "apply",
            changeset_file
                .path()
                .to_str()
                .expect("changeset path should be utf-8"),
        ]);
        assert!(
            !output.status.success(),
            "apply should reject nested duplicate fields in file-mode changeset"
        );

        let response: Value =
            serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
        assert_eq!(response["error"]["type"], "invalid_request");
        assert!(
            response["error"]["message"]
                .as_str()
                .is_some_and(|message| message.contains("duplicate field")),
            "expected deterministic duplicate-field parse error message"
        );
    }
}

#[test]
fn apply_changeset_file_duplicate_transaction_mode_key_is_parse_error() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let file_literal = json_string_literal(&file_path);
    let payload = format!(
        "{{\"files\":[{{\"file\":\"{file_literal}\",\"operations\":[]}}],\"transaction\":{{\"mode\":\"all_or_nothing\",\"mode\":\"all_or_nothing\"}}}}"
    );

    let changeset_file = write_raw_changeset_json(&payload);
    let output = run_identedit(&[
        "apply",
        changeset_file
            .path()
            .to_str()
            .expect("changeset path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "apply should reject duplicate transaction.mode in file-mode changeset"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("duplicate field `mode`")),
        "expected deterministic duplicate transaction.mode parse message"
    );
}

#[test]
fn apply_stdin_mode_rejects_unknown_field_in_bare_changeset_payload() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let payload = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": [],
        "unexpected": true
    });

    let output = run_identedit_with_stdin(&["apply"], &payload.to_string());
    assert!(
        !output.status.success(),
        "apply stdin mode should reject unknown fields in bare changeset"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("unknown field `unexpected`")),
        "expected deny_unknown_fields message in stdin mode"
    );
}

#[test]
fn apply_stdin_mode_raw_v1_payload_is_rejected_after_v2_cutover() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let file_literal = json_string_literal(&file_path);
    let payload = format!("{{\"file\":\"{file_literal}\",\"operations\":[]}}");

    let output = run_identedit_with_raw_stdin(&["apply"], payload.as_bytes());
    assert!(
        !output.status.success(),
        "apply stdin mode should reject raw v1 payload post v2 cutover"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("unknown field `file`")),
        "expected explicit v1->v2 parse diagnostic in stdin mode"
    );
}

#[test]
fn apply_stdin_mode_empty_file_path_returns_io_error() {
    let payload = json!({
        "file": "",
        "operations": []
    });

    let output = run_identedit_with_stdin(&["apply"], &payload.to_string());
    assert!(
        !output.status.success(),
        "apply should fail for bare stdin payload with empty file path"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "io_error");
}

#[test]
fn apply_stdin_mode_escaped_nul_file_path_returns_io_error() {
    let output = run_identedit_with_stdin(&["apply"], r#"{"file":"\u0000","operations":[]}"#);
    assert!(
        !output.status.success(),
        "apply should fail for bare stdin payload with escaped NUL file path"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "io_error");
}
