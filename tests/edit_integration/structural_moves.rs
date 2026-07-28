use super::*;

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
                    "expected_old_hash": crate::common::hash_text(
                        source_handle["text"].as_str().expect("source text should be present")
                    )
                },
                "op": {
                    "type": "move_before",
                    "destination": {
                        "identity": destination_handle["identity"],
                        "kind": destination_handle["kind"],
                        "span_hint": destination_handle["span"],
                        "expected_old_hash": crate::common::hash_text(
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
                    "expected_old_hash": crate::common::hash_text(
                        source_handle["text"].as_str().expect("source text should be present")
                    )
                },
                "op": {
                    "type": "move_before",
                    "destination": {
                        "identity": source_handle["identity"],
                        "kind": source_handle["kind"],
                        "span_hint": source_handle["span"],
                        "expected_old_hash": crate::common::hash_text(
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
                    "expected_old_hash": crate::common::hash_text("def missing():\n    pass")
                },
                "op": {
                    "type": "move_before",
                    "destination": {
                        "identity": destination_handle["identity"],
                        "kind": destination_handle["kind"],
                        "span_hint": destination_handle["span"],
                        "expected_old_hash": crate::common::hash_text(
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
                    "expected_old_hash": crate::common::hash_text(
                        source_handle["text"].as_str().expect("source text should be present")
                    )
                },
                "op": {
                    "type": "move_before",
                    "destination": {
                        "identity": "missing-destination",
                        "kind": "function_definition",
                        "expected_old_hash": crate::common::hash_text(
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
                    "expected_old_hash": crate::common::hash_text(source_text)
                },
                "op": {
                    "type": "move_before",
                    "destination": {
                        "identity": source_handle["identity"],
                        "kind": source_handle["kind"],
                        "expected_old_hash": crate::common::hash_text(source_text)
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
                    "expected_old_hash": crate::common::hash_text(source_text)
                },
                "op": {
                    "type": "move_to_before",
                    "destination_file": destination_file.to_string_lossy().to_string(),
                    "destination": {
                        "identity": destination_handle["identity"],
                        "kind": destination_handle["kind"],
                        "span_hint": destination_handle["span"],
                        "expected_old_hash": crate::common::hash_text(
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
                    "expected_old_hash": crate::common::hash_text(source_text)
                },
                "op": {
                    "type": "move_to_before",
                    "destination_file": destination_file.to_string_lossy().to_string(),
                    "destination": {
                        "identity": "missing-destination-target",
                        "kind": "function_definition",
                        "expected_old_hash": crate::common::hash_text("def missing():\n    return 0\n")
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
                    "expected_old_hash": crate::common::hash_text(source_text)
                },
                "op": {
                    "type": "move_to_before",
                    "destination_file": destination_file.to_string_lossy().to_string(),
                    "destination": {
                        "identity": destination_handle["identity"],
                        "kind": destination_handle["kind"],
                        "span_hint": destination_handle["span"],
                        "expected_old_hash": crate::common::hash_text(
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
                    "expected_old_hash": crate::common::hash_text(source_text)
                },
                "op": {
                    "type": "move_to_before",
                    "destination_file": file_path.to_string_lossy().to_string(),
                    "destination": {
                        "identity": destination_handle["identity"],
                        "kind": destination_handle["kind"],
                        "span_hint": destination_handle["span"],
                        "expected_old_hash": crate::common::hash_text(
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
