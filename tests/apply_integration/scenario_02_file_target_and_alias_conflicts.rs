use super::*;

#[test]
fn apply_multi_file_boundary_conflict_fails_without_mutating_other_files() {
    let mut conflict_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp python file should be created");
    conflict_file
        .write_all(b"def conflict_target(value):\n    return value + 1\n")
        .expect("conflict fixture write should succeed");
    let conflict_path = conflict_file.keep().expect("temp file should persist").1;
    let conflict_before = fs::read_to_string(&conflict_path).expect("file should be readable");
    let conflict_file_hash = identedit::changeset::hash_text(&conflict_before);
    let conflict_handle = select_first_handle(
        &conflict_path,
        "function_definition",
        Some("conflict_target"),
    );
    let conflict_span = &conflict_handle["span"];
    let conflict_start = conflict_span["start"].as_u64().expect("span start") as usize;
    let conflict_end = conflict_span["end"].as_u64().expect("span end") as usize;
    assert_eq!(
        conflict_start, 0,
        "fixture precondition: conflict function should start at byte 0"
    );

    let valid_path = copy_fixture_to_temp_python("example.py");
    let valid_before = fs::read_to_string(&valid_path).expect("file should be readable");
    let valid_handle = select_named_handle(&valid_path, "process_*");
    let valid_span = &valid_handle["span"];
    let valid_start = valid_span["start"].as_u64().expect("span start") as usize;
    let valid_end = valid_span["end"].as_u64().expect("span end") as usize;
    let valid_old_text = valid_handle["text"]
        .as_str()
        .expect("text should be string");
    let valid_expected_hash = identedit::changeset::hash_text(valid_old_text);
    let valid_replacement = "def process_data(value):\n    return value * 17";

    let changeset = json!({
        "files": [
            {
                "file": conflict_path.to_string_lossy().to_string(),
                "operations": [
                    {
                        "target": {
                            "type": "file_start",
                            "expected_file_hash": conflict_file_hash
                        },
                        "op": {"type": "insert", "new_text": "# file-header\n"},
                        "preview": {
                            "old_text": "",
                            "new_text": "# file-header\n",
                            "matched_span": {"start": 0, "end": 0}
                        }
                    },
                    {
                        "target": {
                            "identity": conflict_handle["identity"],
                            "kind": conflict_handle["kind"],
                            "span_hint": {"start": conflict_start, "end": conflict_end},
                            "expected_old_hash": identedit::changeset::hash_text(
                                conflict_handle["text"].as_str().expect("text should be string")
                            )
                        },
                        "op": {"type": "insert_before", "new_text": "# node-header\n"},
                        "preview": {
                            "old_text": "",
                            "new_text": "# node-header\n",
                            "matched_span": {"start": conflict_start, "end": conflict_start}
                        }
                    }
                ]
            },
            {
                "file": valid_path.to_string_lossy().to_string(),
                "operations": [
                    {
                        "target": {
                            "identity": valid_handle["identity"],
                            "kind": valid_handle["kind"],
                            "span_hint": {"start": valid_start, "end": valid_end},
                            "expected_old_hash": valid_expected_hash
                        },
                        "op": {"type": "replace", "new_text": valid_replacement},
                        "preview": {
                            "old_text": valid_old_text,
                            "new_text": valid_replacement,
                            "matched_span": {"start": valid_start, "end": valid_end}
                        }
                    }
                ]
            }
        ],
        "transaction": {
            "mode": "all_or_nothing"
        }
    });

    let output = run_identedit_with_stdin(&["apply"], &changeset.to_string());
    assert!(
        !output.status.success(),
        "multi-file apply should fail when one file has boundary conflict"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("Overlapping operations")),
        "expected overlap conflict message for conflicting file"
    );

    let conflict_after = fs::read_to_string(&conflict_path).expect("file should be readable");
    let valid_after = fs::read_to_string(&valid_path).expect("file should be readable");
    assert_eq!(
        conflict_after, conflict_before,
        "conflicting file must remain unchanged after failed multi-file apply"
    );
    assert_eq!(
        valid_after, valid_before,
        "other files must remain unchanged after failed multi-file preflight"
    );
}

#[test]
fn apply_multi_file_alias_path_boundary_conflict_fails_without_mutating_other_files() {
    let workspace = tempdir().expect("tempdir should be created");
    fs::create_dir_all(workspace.path().join("nested")).expect("nested dir should be created");

    let conflict_path = workspace.path().join("target.py");
    fs::write(
        &conflict_path,
        "def target_fn(value):\n    return value + 1\n",
    )
    .expect("conflict fixture write should succeed");
    let conflict_before = fs::read_to_string(&conflict_path).expect("file should be readable");
    let conflict_file_hash = identedit::changeset::hash_text(&conflict_before);
    let conflict_handle =
        select_first_handle(&conflict_path, "function_definition", Some("target_fn"));
    let conflict_span = &conflict_handle["span"];
    let conflict_start = conflict_span["start"].as_u64().expect("span start") as usize;
    let conflict_end = conflict_span["end"].as_u64().expect("span end") as usize;

    let safe_path = workspace.path().join("safe.py");
    let safe_fixture = fs::read_to_string(fixture_path("example.py")).expect("fixture read");
    fs::write(&safe_path, safe_fixture).expect("safe fixture write should succeed");
    let safe_before = fs::read_to_string(&safe_path).expect("safe file should be readable");
    let safe_handle = select_named_handle(&safe_path, "process_*");
    let safe_span = &safe_handle["span"];
    let safe_start = safe_span["start"].as_u64().expect("span start") as usize;
    let safe_end = safe_span["end"].as_u64().expect("span end") as usize;
    let safe_old_text = safe_handle["text"]
        .as_str()
        .expect("safe text should be string");
    let safe_expected_hash = identedit::changeset::hash_text(safe_old_text);
    let safe_replacement = "def process_data(value):\n    return value * 55";

    let payload = json!({
        "files": [
            {
                "file": "nested/../target.py",
                "operations": [
                    {
                        "target": {
                            "type": "file_start",
                            "expected_file_hash": conflict_file_hash
                        },
                        "op": {"type": "insert", "new_text": "# alias-header\n"},
                        "preview": {
                            "old_text": "",
                            "new_text": "# alias-header\n",
                            "matched_span": {"start": 0, "end": 0}
                        }
                    },
                    {
                        "target": {
                            "identity": conflict_handle["identity"],
                            "kind": conflict_handle["kind"],
                            "span_hint": {"start": conflict_start, "end": conflict_end},
                            "expected_old_hash": identedit::changeset::hash_text(
                                conflict_handle["text"].as_str().expect("text should be string")
                            )
                        },
                        "op": {"type": "insert_before", "new_text": "# alias-node\n"},
                        "preview": {
                            "old_text": "",
                            "new_text": "# alias-node\n",
                            "matched_span": {"start": conflict_start, "end": conflict_start}
                        }
                    }
                ]
            },
            {
                "file": "safe.py",
                "operations": [
                    {
                        "target": {
                            "identity": safe_handle["identity"],
                            "kind": safe_handle["kind"],
                            "span_hint": {"start": safe_start, "end": safe_end},
                            "expected_old_hash": safe_expected_hash
                        },
                        "op": {"type": "replace", "new_text": safe_replacement},
                        "preview": {
                            "old_text": safe_old_text,
                            "new_text": safe_replacement,
                            "matched_span": {"start": safe_start, "end": safe_end}
                        }
                    }
                ]
            }
        ],
        "transaction": {
            "mode": "all_or_nothing"
        }
    });

    let output =
        run_identedit_with_stdin_in_dir(workspace.path(), &["apply"], &payload.to_string());
    assert!(
        !output.status.success(),
        "multi-file apply should fail on alias boundary conflict"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("Overlapping operations")),
        "expected overlap conflict message for alias boundary conflict"
    );

    let conflict_after = fs::read_to_string(&conflict_path).expect("file should be readable");
    let safe_after = fs::read_to_string(&safe_path).expect("file should be readable");
    assert_eq!(
        conflict_after, conflict_before,
        "conflicting alias target must remain unchanged after failed apply"
    );
    assert_eq!(
        safe_after, safe_before,
        "other files must remain unchanged after failed alias conflict preflight"
    );
}

#[test]
fn apply_multi_file_alias_boundary_conflict_error_is_order_independent() {
    let workspace = tempdir().expect("tempdir should be created");
    fs::create_dir_all(workspace.path().join("nested")).expect("nested dir should be created");

    let conflict_path = workspace.path().join("target.py");
    fs::write(
        &conflict_path,
        "def target_order(value):\n    return value + 1\n",
    )
    .expect("conflict fixture write should succeed");
    let conflict_before = fs::read_to_string(&conflict_path).expect("file should be readable");
    let conflict_file_hash = identedit::changeset::hash_text(&conflict_before);
    let conflict_handle =
        select_first_handle(&conflict_path, "function_definition", Some("target_order"));
    let conflict_span = &conflict_handle["span"];
    let conflict_start = conflict_span["start"].as_u64().expect("span start") as usize;
    let conflict_end = conflict_span["end"].as_u64().expect("span end") as usize;
    let expected_old_hash = identedit::changeset::hash_text(
        conflict_handle["text"]
            .as_str()
            .expect("text should be string"),
    );

    let operation_orders = vec![
        vec![
            json!({
                "target": {"type": "file_start", "expected_file_hash": conflict_file_hash},
                "op": {"type": "insert", "new_text": "# a\n"},
                "preview": {"old_text": "", "new_text": "# a\n", "matched_span": {"start": 0, "end": 0}}
            }),
            json!({
                "target": {
                    "identity": conflict_handle["identity"],
                    "kind": conflict_handle["kind"],
                    "span_hint": {"start": conflict_start, "end": conflict_end},
                    "expected_old_hash": expected_old_hash
                },
                "op": {"type": "insert_before", "new_text": "# b\n"},
                "preview": {"old_text": "", "new_text": "# b\n", "matched_span": {"start": conflict_start, "end": conflict_start}}
            }),
        ],
        vec![
            json!({
                "target": {
                    "identity": conflict_handle["identity"],
                    "kind": conflict_handle["kind"],
                    "span_hint": {"start": conflict_start, "end": conflict_end},
                    "expected_old_hash": expected_old_hash
                },
                "op": {"type": "insert_before", "new_text": "# b\n"},
                "preview": {"old_text": "", "new_text": "# b\n", "matched_span": {"start": conflict_start, "end": conflict_start}}
            }),
            json!({
                "target": {"type": "file_start", "expected_file_hash": conflict_file_hash},
                "op": {"type": "insert", "new_text": "# a\n"},
                "preview": {"old_text": "", "new_text": "# a\n", "matched_span": {"start": 0, "end": 0}}
            }),
        ],
    ];

    let mut messages = Vec::new();
    for operations in operation_orders {
        let payload = json!({
            "files": [
                {
                    "file": "nested/../target.py",
                    "operations": operations
                }
            ],
            "transaction": {"mode": "all_or_nothing"}
        });

        let output =
            run_identedit_with_stdin_in_dir(workspace.path(), &["apply"], &payload.to_string());
        assert!(
            !output.status.success(),
            "apply should fail on alias boundary conflict regardless of operation order"
        );

        let response: Value =
            serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
        assert_eq!(response["error"]["type"], "invalid_request");
        let message = response["error"]["message"]
            .as_str()
            .expect("error message should be string")
            .to_string();
        assert!(
            message.contains("Overlapping operations"),
            "expected overlap conflict message"
        );
        messages.push(message);

        let conflict_after = fs::read_to_string(&conflict_path).expect("file should be readable");
        assert_eq!(
            conflict_after, conflict_before,
            "failed apply must keep alias target unchanged"
        );
    }

    assert_eq!(
        messages[0], messages[1],
        "alias boundary conflict message should be deterministic across operation order"
    );
}

#[test]
fn apply_multi_file_alias_path_file_end_collision_fails_without_mutating_other_files() {
    let workspace = tempdir().expect("tempdir should be created");
    fs::create_dir_all(workspace.path().join("nested")).expect("nested dir should be created");

    let tail_path = workspace.path().join("tail.py");
    fs::write(&tail_path, "def tail_target(value):\n    return value + 1")
        .expect("tail fixture write should succeed");
    let tail_before = fs::read_to_string(&tail_path).expect("tail file should be readable");
    let tail_file_hash = identedit::changeset::hash_text(&tail_before);
    let tail_handle = select_first_handle(&tail_path, "function_definition", Some("tail_target"));
    let tail_span = &tail_handle["span"];
    let tail_start = tail_span["start"].as_u64().expect("span start") as usize;
    let tail_end = tail_span["end"].as_u64().expect("span end") as usize;
    assert_eq!(
        tail_end,
        tail_before.len(),
        "fixture precondition: tail function should end at file boundary"
    );

    let safe_path = workspace.path().join("safe.py");
    let safe_fixture = fs::read_to_string(fixture_path("example.py")).expect("fixture read");
    fs::write(&safe_path, safe_fixture).expect("safe fixture write should succeed");
    let safe_before = fs::read_to_string(&safe_path).expect("safe file should be readable");
    let safe_handle = select_named_handle(&safe_path, "process_*");
    let safe_span = &safe_handle["span"];
    let safe_start = safe_span["start"].as_u64().expect("span start") as usize;
    let safe_end = safe_span["end"].as_u64().expect("span end") as usize;
    let safe_old_text = safe_handle["text"]
        .as_str()
        .expect("safe text should be string");
    let safe_expected_hash = identedit::changeset::hash_text(safe_old_text);
    let safe_replacement = "def process_data(value):\n    return value * 89";

    let payload = json!({
        "files": [
            {
                "file": "./nested/../tail.py",
                "operations": [
                    {
                        "target": {
                            "type": "file_end",
                            "expected_file_hash": tail_file_hash
                        },
                        "op": {"type": "insert", "new_text": "# alias-tail\n"},
                        "preview": {
                            "old_text": "",
                            "new_text": "# alias-tail\n",
                            "matched_span": {"start": tail_end, "end": tail_end}
                        }
                    },
                    {
                        "target": {
                            "identity": tail_handle["identity"],
                            "kind": tail_handle["kind"],
                            "span_hint": {"start": tail_start, "end": tail_end},
                            "expected_old_hash": identedit::changeset::hash_text(
                                tail_handle["text"].as_str().expect("text should be string")
                            )
                        },
                        "op": {"type": "insert_after", "new_text": "# after-tail\n"},
                        "preview": {
                            "old_text": "",
                            "new_text": "# after-tail\n",
                            "matched_span": {"start": tail_end, "end": tail_end}
                        }
                    }
                ]
            },
            {
                "file": "safe.py",
                "operations": [
                    {
                        "target": {
                            "identity": safe_handle["identity"],
                            "kind": safe_handle["kind"],
                            "span_hint": {"start": safe_start, "end": safe_end},
                            "expected_old_hash": safe_expected_hash
                        },
                        "op": {"type": "replace", "new_text": safe_replacement},
                        "preview": {
                            "old_text": safe_old_text,
                            "new_text": safe_replacement,
                            "matched_span": {"start": safe_start, "end": safe_end}
                        }
                    }
                ]
            }
        ],
        "transaction": {
            "mode": "all_or_nothing"
        }
    });

    let output =
        run_identedit_with_stdin_in_dir(workspace.path(), &["apply"], &payload.to_string());
    assert!(
        !output.status.success(),
        "multi-file apply should fail on alias file_end collision"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("Overlapping operations")),
        "expected overlap conflict message for alias file_end collision"
    );

    let tail_after = fs::read_to_string(&tail_path).expect("tail file should be readable");
    let safe_after = fs::read_to_string(&safe_path).expect("safe file should be readable");
    assert_eq!(
        tail_after, tail_before,
        "conflicting alias target must remain unchanged after failed apply"
    );
    assert_eq!(
        safe_after, safe_before,
        "other files must remain unchanged after failed alias conflict preflight"
    );
}

#[test]
fn apply_multi_file_json_mode_alias_boundary_conflict_fails_without_mutating_other_files() {
    let workspace = tempdir().expect("tempdir should be created");
    fs::create_dir_all(workspace.path().join("nested")).expect("nested dir should be created");

    let conflict_path = workspace.path().join("target.py");
    fs::write(
        &conflict_path,
        "def target_json(value):\n    return value + 1\n",
    )
    .expect("conflict fixture write should succeed");
    let conflict_before = fs::read_to_string(&conflict_path).expect("file should be readable");
    let conflict_file_hash = identedit::changeset::hash_text(&conflict_before);
    let conflict_handle =
        select_first_handle(&conflict_path, "function_definition", Some("target_json"));
    let conflict_span = &conflict_handle["span"];
    let conflict_start = conflict_span["start"].as_u64().expect("span start") as usize;
    let conflict_end = conflict_span["end"].as_u64().expect("span end") as usize;

    let safe_path = workspace.path().join("safe.py");
    let safe_fixture = fs::read_to_string(fixture_path("example.py")).expect("fixture read");
    fs::write(&safe_path, safe_fixture).expect("safe fixture write should succeed");
    let safe_before = fs::read_to_string(&safe_path).expect("safe file should be readable");
    let safe_handle = select_named_handle(&safe_path, "process_*");
    let safe_span = &safe_handle["span"];
    let safe_start = safe_span["start"].as_u64().expect("span start") as usize;
    let safe_end = safe_span["end"].as_u64().expect("span end") as usize;
    let safe_old_text = safe_handle["text"]
        .as_str()
        .expect("safe text should be string");
    let safe_expected_hash = identedit::changeset::hash_text(safe_old_text);
    let safe_replacement = "def process_data(value):\n    return value * 66";

    let request = json!({
        "command": "apply",
        "changeset": {
            "files": [
                {
                    "file": "nested/../target.py",
                    "operations": [
                        {
                            "target": {
                                "type": "file_start",
                                "expected_file_hash": conflict_file_hash
                            },
                            "op": {"type": "insert", "new_text": "# json-header\n"},
                            "preview": {
                                "old_text": "",
                                "new_text": "# json-header\n",
                                "matched_span": {"start": 0, "end": 0}
                            }
                        },
                        {
                            "target": {
                                "identity": conflict_handle["identity"],
                                "kind": conflict_handle["kind"],
                                "span_hint": {"start": conflict_start, "end": conflict_end},
                                "expected_old_hash": identedit::changeset::hash_text(
                                    conflict_handle["text"].as_str().expect("text should be string")
                                )
                            },
                            "op": {"type": "insert_before", "new_text": "# json-node\n"},
                            "preview": {
                                "old_text": "",
                                "new_text": "# json-node\n",
                                "matched_span": {"start": conflict_start, "end": conflict_start}
                            }
                        }
                    ]
                },
                {
                    "file": "safe.py",
                    "operations": [
                        {
                            "target": {
                                "identity": safe_handle["identity"],
                                "kind": safe_handle["kind"],
                                "span_hint": {"start": safe_start, "end": safe_end},
                                "expected_old_hash": safe_expected_hash
                            },
                            "op": {"type": "replace", "new_text": safe_replacement},
                            "preview": {
                                "old_text": safe_old_text,
                                "new_text": safe_replacement,
                                "matched_span": {"start": safe_start, "end": safe_end}
                            }
                        }
                    ]
                }
            ],
            "transaction": {
                "mode": "all_or_nothing"
            }
        }
    });

    let output = run_identedit_with_stdin_in_dir(
        workspace.path(),
        &["apply", "--json"],
        &request.to_string(),
    );
    assert!(
        !output.status.success(),
        "apply --json should fail on alias boundary conflict"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("Overlapping operations")),
        "expected overlap conflict message for alias boundary conflict"
    );

    let conflict_after = fs::read_to_string(&conflict_path).expect("file should be readable");
    let safe_after = fs::read_to_string(&safe_path).expect("file should be readable");
    assert_eq!(
        conflict_after, conflict_before,
        "conflicting alias target must remain unchanged after failed apply --json"
    );
    assert_eq!(
        safe_after, safe_before,
        "other files must remain unchanged after failed alias conflict preflight"
    );
}

#[test]
fn apply_multi_file_json_mode_alias_boundary_conflict_error_is_order_independent() {
    let workspace = tempdir().expect("tempdir should be created");
    fs::create_dir_all(workspace.path().join("nested")).expect("nested dir should be created");

    let conflict_path = workspace.path().join("target.py");
    fs::write(
        &conflict_path,
        "def target_json_order(value):\n    return value + 1\n",
    )
    .expect("conflict fixture write should succeed");
    let conflict_before = fs::read_to_string(&conflict_path).expect("file should be readable");
    let conflict_file_hash = identedit::changeset::hash_text(&conflict_before);
    let conflict_handle = select_first_handle(
        &conflict_path,
        "function_definition",
        Some("target_json_order"),
    );
    let conflict_span = &conflict_handle["span"];
    let conflict_start = conflict_span["start"].as_u64().expect("span start") as usize;
    let conflict_end = conflict_span["end"].as_u64().expect("span end") as usize;
    let expected_old_hash = identedit::changeset::hash_text(
        conflict_handle["text"]
            .as_str()
            .expect("text should be string"),
    );

    let operation_orders = vec![
        vec![
            json!({
                "target": {"type": "file_start", "expected_file_hash": conflict_file_hash},
                "op": {"type": "insert", "new_text": "# j1\n"},
                "preview": {"old_text": "", "new_text": "# j1\n", "matched_span": {"start": 0, "end": 0}}
            }),
            json!({
                "target": {
                    "identity": conflict_handle["identity"],
                    "kind": conflict_handle["kind"],
                    "span_hint": {"start": conflict_start, "end": conflict_end},
                    "expected_old_hash": expected_old_hash
                },
                "op": {"type": "insert_before", "new_text": "# j2\n"},
                "preview": {"old_text": "", "new_text": "# j2\n", "matched_span": {"start": conflict_start, "end": conflict_start}}
            }),
        ],
        vec![
            json!({
                "target": {
                    "identity": conflict_handle["identity"],
                    "kind": conflict_handle["kind"],
                    "span_hint": {"start": conflict_start, "end": conflict_end},
                    "expected_old_hash": expected_old_hash
                },
                "op": {"type": "insert_before", "new_text": "# j2\n"},
                "preview": {"old_text": "", "new_text": "# j2\n", "matched_span": {"start": conflict_start, "end": conflict_start}}
            }),
            json!({
                "target": {"type": "file_start", "expected_file_hash": conflict_file_hash},
                "op": {"type": "insert", "new_text": "# j1\n"},
                "preview": {"old_text": "", "new_text": "# j1\n", "matched_span": {"start": 0, "end": 0}}
            }),
        ],
    ];

    let mut messages = Vec::new();
    for operations in operation_orders {
        let request = json!({
            "command": "apply",
            "changeset": {
                "files": [
                    {
                        "file": "nested/../target.py",
                        "operations": operations
                    }
                ],
                "transaction": {"mode": "all_or_nothing"}
            }
        });

        let output = run_identedit_with_stdin_in_dir(
            workspace.path(),
            &["apply", "--json"],
            &request.to_string(),
        );
        assert!(
            !output.status.success(),
            "apply --json should fail on alias boundary conflict regardless of operation order"
        );

        let response: Value =
            serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
        assert_eq!(response["error"]["type"], "invalid_request");
        let message = response["error"]["message"]
            .as_str()
            .expect("error message should be string")
            .to_string();
        assert!(
            message.contains("Overlapping operations"),
            "expected overlap conflict message"
        );
        messages.push(message);

        let conflict_after = fs::read_to_string(&conflict_path).expect("file should be readable");
        assert_eq!(
            conflict_after, conflict_before,
            "failed apply --json must keep alias target unchanged"
        );
    }

    assert_eq!(
        messages[0], messages[1],
        "alias boundary conflict message should be deterministic across operation order in --json mode"
    );
}

#[test]
fn apply_rejects_file_start_and_file_end_inserts_on_bom_only_css_file() {
    let mut temp_file = Builder::new()
        .suffix(".css")
        .tempfile()
        .expect("temp css file should be created");
    temp_file
        .write_all(b"\xEF\xBB\xBF")
        .expect("bom-only css fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let source = fs::read_to_string(&file_path).expect("fixture should be readable");
    let expected_file_hash = identedit::changeset::hash_text(&source);

    let changeset = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "type": "file_start",
                    "expected_file_hash": expected_file_hash
                },
                "op": {"type": "insert", "new_text": "/* start */\n"},
                "preview": {
                    "old_text": "",
                    "new_text": "/* start */\n",
                    "matched_span": {"start": 3, "end": 3}
                }
            },
            {
                "target": {
                    "type": "file_end",
                    "expected_file_hash": expected_file_hash
                },
                "op": {"type": "insert", "new_text": "/* end */\n"},
                "preview": {
                    "old_text": "",
                    "new_text": "/* end */\n",
                    "matched_span": {"start": 3, "end": 3}
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["apply"], &changeset.to_string());
    assert!(
        !output.status.success(),
        "apply should reject file_start/file_end overlap for BOM-only css file"
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

    let after_bytes = fs::read(&file_path).expect("file bytes should be readable");
    assert_eq!(after_bytes, vec![0xEF, 0xBB, 0xBF]);
}

#[test]
fn apply_multi_file_css_conflict_rolls_back_unrelated_html_file() {
    let mut conflict_temp = Builder::new()
        .suffix(".css")
        .tempfile()
        .expect("temp css file should be created");
    conflict_temp
        .write_all(b".token{color:red}\n")
        .expect("conflict css fixture write should succeed");
    let conflict_path = conflict_temp.keep().expect("temp file should persist").1;
    let conflict_before = fs::read_to_string(&conflict_path).expect("css file should be readable");
    let conflict_hash = identedit::changeset::hash_text(&conflict_before);
    let conflict_handle = select_first_handle(&conflict_path, "rule_set", None);
    let conflict_span = &conflict_handle["span"];
    let conflict_start = conflict_span["start"].as_u64().expect("span start") as usize;
    let conflict_end = conflict_span["end"].as_u64().expect("span end") as usize;

    let html_path = {
        let mut html_temp = Builder::new()
            .suffix(".html")
            .tempfile()
            .expect("temp html file should be created");
        let html_fixture =
            fs::read_to_string(fixture_path("minified_dashboard.html")).expect("fixture read");
        html_temp
            .write_all(html_fixture.as_bytes())
            .expect("html fixture write should succeed");
        html_temp.keep().expect("temp file should persist").1
    };
    let html_before = fs::read_to_string(&html_path).expect("html file should be readable");
    let html_handle = select_first_handle(&html_path, "start_tag", None);
    let html_span = &html_handle["span"];
    let html_start = html_span["start"].as_u64().expect("span start") as usize;
    let html_end = html_span["end"].as_u64().expect("span end") as usize;
    let html_old_text = html_handle["text"]
        .as_str()
        .expect("html text should be string");
    let html_expected_hash = identedit::changeset::hash_text(html_old_text);

    let payload = json!({
        "files": [
            {
                "file": conflict_path.to_string_lossy().to_string(),
                "operations": [
                    {
                        "target": {
                            "type": "file_start",
                            "expected_file_hash": conflict_hash
                        },
                        "op": {"type": "insert", "new_text": "/* head */\n"},
                        "preview": {
                            "old_text": "",
                            "new_text": "/* head */\n",
                            "matched_span": {"start": 0, "end": 0}
                        }
                    },
                    {
                        "target": {
                            "identity": conflict_handle["identity"],
                            "kind": conflict_handle["kind"],
                            "span_hint": {"start": conflict_start, "end": conflict_end},
                            "expected_old_hash": identedit::changeset::hash_text(
                                conflict_handle["text"].as_str().expect("text should be string")
                            )
                        },
                        "op": {"type": "insert_before", "new_text": "/* node */\n"},
                        "preview": {
                            "old_text": "",
                            "new_text": "/* node */\n",
                            "matched_span": {"start": conflict_start, "end": conflict_start}
                        }
                    }
                ]
            },
            {
                "file": html_path.to_string_lossy().to_string(),
                "operations": [
                    {
                        "target": {
                            "identity": html_handle["identity"],
                            "kind": html_handle["kind"],
                            "span_hint": {"start": html_start, "end": html_end},
                            "expected_old_hash": html_expected_hash
                        },
                        "op": {"type": "replace", "new_text": "<html lang=\"en\" data-safe=\"1\">"},
                        "preview": {
                            "old_text": html_old_text,
                            "new_text": "<html lang=\"en\" data-safe=\"1\">",
                            "matched_span": {"start": html_start, "end": html_end}
                        }
                    }
                ]
            }
        ],
        "transaction": {"mode": "all_or_nothing"}
    });

    let output = run_identedit_with_stdin(&["apply"], &payload.to_string());
    assert!(
        !output.status.success(),
        "multi-file apply should fail when css file has boundary conflict"
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

    let conflict_after = fs::read_to_string(&conflict_path).expect("css file should be readable");
    let html_after = fs::read_to_string(&html_path).expect("html file should be readable");
    assert_eq!(
        conflict_after, conflict_before,
        "conflicting css file should remain unchanged"
    );
    assert_eq!(
        html_after, html_before,
        "unrelated html file should remain unchanged after failed transaction"
    );
}

#[test]
fn apply_multi_file_json_mode_alias_file_end_conflict_error_is_order_independent() {
    let workspace = tempdir().expect("tempdir should be created");
    fs::create_dir_all(workspace.path().join("nested")).expect("nested dir should be created");

    let tail_path = workspace.path().join("tail.py");
    fs::write(
        &tail_path,
        "def tail_json_order(value):\n    return value + 1",
    )
    .expect("tail fixture write should succeed");
    let tail_before = fs::read_to_string(&tail_path).expect("tail file should be readable");
    let tail_file_hash = identedit::changeset::hash_text(&tail_before);
    let tail_handle =
        select_first_handle(&tail_path, "function_definition", Some("tail_json_order"));
    let tail_span = &tail_handle["span"];
    let tail_start = tail_span["start"].as_u64().expect("span start") as usize;
    let tail_end = tail_span["end"].as_u64().expect("span end") as usize;
    assert_eq!(
        tail_end,
        tail_before.len(),
        "fixture precondition: tail function should end at file boundary"
    );
    let expected_old_hash = identedit::changeset::hash_text(
        tail_handle["text"].as_str().expect("text should be string"),
    );

    let operation_orders = vec![
        vec![
            json!({
                "target": {"type": "file_end", "expected_file_hash": tail_file_hash},
                "op": {"type": "insert", "new_text": "# end-a\n"},
                "preview": {"old_text": "", "new_text": "# end-a\n", "matched_span": {"start": tail_end, "end": tail_end}}
            }),
            json!({
                "target": {
                    "identity": tail_handle["identity"],
                    "kind": tail_handle["kind"],
                    "span_hint": {"start": tail_start, "end": tail_end},
                    "expected_old_hash": expected_old_hash
                },
                "op": {"type": "insert_after", "new_text": "# end-b\n"},
                "preview": {"old_text": "", "new_text": "# end-b\n", "matched_span": {"start": tail_end, "end": tail_end}}
            }),
        ],
        vec![
            json!({
                "target": {
                    "identity": tail_handle["identity"],
                    "kind": tail_handle["kind"],
                    "span_hint": {"start": tail_start, "end": tail_end},
                    "expected_old_hash": expected_old_hash
                },
                "op": {"type": "insert_after", "new_text": "# end-b\n"},
                "preview": {"old_text": "", "new_text": "# end-b\n", "matched_span": {"start": tail_end, "end": tail_end}}
            }),
            json!({
                "target": {"type": "file_end", "expected_file_hash": tail_file_hash},
                "op": {"type": "insert", "new_text": "# end-a\n"},
                "preview": {"old_text": "", "new_text": "# end-a\n", "matched_span": {"start": tail_end, "end": tail_end}}
            }),
        ],
    ];

    let mut messages = Vec::new();
    for operations in operation_orders {
        let request = json!({
            "command": "apply",
            "changeset": {
                "files": [
                    {
                        "file": "nested/../tail.py",
                        "operations": operations
                    }
                ],
                "transaction": {"mode": "all_or_nothing"}
            }
        });

        let output = run_identedit_with_stdin_in_dir(
            workspace.path(),
            &["apply", "--json"],
            &request.to_string(),
        );
        assert!(
            !output.status.success(),
            "apply --json should fail on alias file_end conflict regardless of operation order"
        );

        let response: Value =
            serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
        assert_eq!(response["error"]["type"], "invalid_request");
        let message = response["error"]["message"]
            .as_str()
            .expect("error message should be string")
            .to_string();
        assert!(
            message.contains("Overlapping operations"),
            "expected overlap conflict message"
        );
        messages.push(message);

        let tail_after = fs::read_to_string(&tail_path).expect("tail file should be readable");
        assert_eq!(
            tail_after, tail_before,
            "failed apply --json must keep alias target unchanged"
        );
    }

    assert_eq!(
        messages[0], messages[1],
        "alias file_end conflict message should be deterministic across operation order in --json mode"
    );
}

#[test]
fn apply_multi_file_hardlink_alias_entries_are_rejected_as_duplicates_without_mutation() {
    let workspace = tempdir().expect("tempdir should be created");
    let target_path = workspace.path().join("target.py");
    let alias_path = workspace.path().join("alias.py");
    let source =
        fs::read_to_string(fixture_path("example.py")).expect("fixture should be readable");
    fs::write(&target_path, source).expect("target fixture write should succeed");
    fs::hard_link(&target_path, &alias_path).expect("hardlink alias should be created");

    let before_target = fs::read_to_string(&target_path).expect("target should be readable");
    let before_alias = fs::read_to_string(&alias_path).expect("alias should be readable");

    let handle = select_named_handle(&target_path, "process_*");
    let span = &handle["span"];
    let start = span["start"].as_u64().expect("span start") as usize;
    let end = span["end"].as_u64().expect("span end") as usize;
    let replacement = "def process_data(value):\n    return value * 71";
    let expected_hash =
        identedit::changeset::hash_text(handle["text"].as_str().expect("text should be string"));

    let payload = json!({
        "files": [
            {
                "file": target_path.to_string_lossy().to_string(),
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
                            "old_text": handle["text"],
                            "new_text": replacement,
                            "matched_span": {"start": start, "end": end}
                        }
                    }
                ]
            },
            {
                "file": alias_path.to_string_lossy().to_string(),
                "operations": []
            }
        ],
        "transaction": {
            "mode": "all_or_nothing"
        }
    });

    let output = run_identedit_with_stdin(&["apply"], &payload.to_string());
    assert!(
        !output.status.success(),
        "apply should reject hardlink alias duplicate entries before lock contention"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| { message.contains("Duplicate file entry in changeset.files") }),
        "expected duplicate file entry diagnostic for hardlink alias entries"
    );

    let after_target = fs::read_to_string(&target_path).expect("target should be readable");
    let after_alias = fs::read_to_string(&alias_path).expect("alias should be readable");
    assert_eq!(
        after_target, before_target,
        "target content should remain unchanged after duplicate alias rejection"
    );
    assert_eq!(
        after_alias, before_alias,
        "alias content should remain unchanged after duplicate alias rejection"
    );
}

#[test]
fn apply_multi_file_hardlink_alias_with_middle_file_is_rejected_without_mutation() {
    let workspace = tempdir().expect("tempdir should be created");
    let target_path = workspace.path().join("a_target.py");
    let middle_path = workspace.path().join("m_middle.py");
    let alias_path = workspace.path().join("z_alias.py");
    let source =
        fs::read_to_string(fixture_path("example.py")).expect("fixture should be readable");
    fs::write(&target_path, &source).expect("target fixture write should succeed");
    fs::write(&middle_path, &source).expect("middle fixture write should succeed");
    fs::hard_link(&target_path, &alias_path).expect("hardlink alias should be created");

    let before_target = fs::read_to_string(&target_path).expect("target should be readable");
    let before_middle = fs::read_to_string(&middle_path).expect("middle should be readable");
    let before_alias = fs::read_to_string(&alias_path).expect("alias should be readable");

    let target_handle = select_named_handle(&target_path, "process_*");
    let target_span = &target_handle["span"];
    let target_start = target_span["start"].as_u64().expect("target span start") as usize;
    let target_end = target_span["end"].as_u64().expect("target span end") as usize;
    let target_expected_hash = identedit::changeset::hash_text(
        target_handle["text"]
            .as_str()
            .expect("target text should be string"),
    );

    let middle_handle = select_named_handle(&middle_path, "process_*");
    let middle_span = &middle_handle["span"];
    let middle_start = middle_span["start"].as_u64().expect("middle span start") as usize;
    let middle_end = middle_span["end"].as_u64().expect("middle span end") as usize;
    let middle_expected_hash = identedit::changeset::hash_text(
        middle_handle["text"]
            .as_str()
            .expect("middle text should be string"),
    );

    let payload = json!({
        "files": [
            {
                "file": middle_path.to_string_lossy().to_string(),
                "operations": [
                    {
                        "target": {
                            "identity": middle_handle["identity"],
                            "kind": middle_handle["kind"],
                            "span_hint": {"start": middle_start, "end": middle_end},
                            "expected_old_hash": middle_expected_hash
                        },
                        "op": {"type": "replace", "new_text": "def process_data(value):\n    return value * 80"},
                        "preview": {
                            "old_text": middle_handle["text"],
                            "new_text": "def process_data(value):\n    return value * 80",
                            "matched_span": {"start": middle_start, "end": middle_end}
                        }
                    }
                ]
            },
            {
                "file": alias_path.to_string_lossy().to_string(),
                "operations": []
            },
            {
                "file": target_path.to_string_lossy().to_string(),
                "operations": [
                    {
                        "target": {
                            "identity": target_handle["identity"],
                            "kind": target_handle["kind"],
                            "span_hint": {"start": target_start, "end": target_end},
                            "expected_old_hash": target_expected_hash
                        },
                        "op": {"type": "replace", "new_text": "def process_data(value):\n    return value * 81"},
                        "preview": {
                            "old_text": target_handle["text"],
                            "new_text": "def process_data(value):\n    return value * 81",
                            "matched_span": {"start": target_start, "end": target_end}
                        }
                    }
                ]
            }
        ],
        "transaction": {
            "mode": "all_or_nothing"
        }
    });

    let output = run_identedit_with_stdin(&["apply"], &payload.to_string());
    assert!(
        !output.status.success(),
        "apply should reject non-adjacent hardlink alias duplicates"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| { message.contains("Duplicate file entry in changeset.files") }),
        "expected duplicate file entry diagnostic for non-adjacent hardlink aliases"
    );

    let after_target = fs::read_to_string(&target_path).expect("target should be readable");
    let after_middle = fs::read_to_string(&middle_path).expect("middle should be readable");
    let after_alias = fs::read_to_string(&alias_path).expect("alias should be readable");
    assert_eq!(
        after_target, before_target,
        "target content should remain unchanged after duplicate alias rejection"
    );
    assert_eq!(
        after_middle, before_middle,
        "middle content should remain unchanged after duplicate alias rejection"
    );
    assert_eq!(
        after_alias, before_alias,
        "alias content should remain unchanged after duplicate alias rejection"
    );
}

#[test]
fn apply_rejects_file_end_insert_and_insert_after_on_bom_css_source() {
    let mut temp_file = Builder::new()
        .suffix(".css")
        .tempfile()
        .expect("temp css file should be created");
    temp_file
        .write_all(b"\xEF\xBB\xBF.token{color:red}")
        .expect("bom css fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let source = fs::read_to_string(&file_path).expect("fixture should be readable");
    let expected_file_hash = identedit::changeset::hash_text(&source);
    let handle = select_first_handle(&file_path, "rule_set", None);
    let span = &handle["span"];
    let span_start = span["start"].as_u64().expect("span start") as usize;
    let span_end = span["end"].as_u64().expect("span end") as usize;
    assert_eq!(
        span_end,
        source.len(),
        "fixture precondition: css rule should end at file boundary"
    );

    let changeset = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "type": "file_end",
                    "expected_file_hash": expected_file_hash
                },
                "op": {"type": "insert", "new_text": "/* end */"},
                "preview": {
                    "old_text": "",
                    "new_text": "/* end */",
                    "matched_span": {"start": span_end, "end": span_end}
                }
            },
            {
                "target": {
                    "identity": handle["identity"],
                    "kind": handle["kind"],
                    "span_hint": {"start": span_start, "end": span_end},
                    "expected_old_hash": identedit::changeset::hash_text(
                        handle["text"].as_str().expect("text should be string")
                    )
                },
                "op": {"type": "insert_after", "new_text": "/* after */"},
                "preview": {
                    "old_text": "",
                    "new_text": "/* after */",
                    "matched_span": {"start": span_end, "end": span_end}
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["apply"], &changeset.to_string());
    assert!(
        !output.status.success(),
        "apply should reject file_end insert colliding with insert_after on BOM css source"
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

    let after = fs::read(&file_path).expect("file should remain readable");
    assert_eq!(
        after, b"\xEF\xBB\xBF.token{color:red}",
        "failed apply must leave BOM css source untouched"
    );
}

#[test]
fn apply_json_mode_rejects_unknown_file_level_target_type() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let request = json!({
        "command": "apply",
        "changeset": {
            "files": [
                {
                    "file": file_path.to_string_lossy().to_string(),
                    "operations": [
                        {
                            "target": {
                                "type": "file_middle",
                                "expected_file_hash": "irrelevant"
                            },
                            "op": {"type": "insert", "new_text": "# invalid target type\n"},
                            "preview": {
                                "old_text": "",
                                "new_text": "# invalid target type\n",
                                "matched_span": {"start": 0, "end": 0}
                            }
                        }
                    ]
                }
            ],
            "transaction": {
                "mode": "all_or_nothing"
            }
        }
    });

    let output = run_identedit_with_stdin(&["apply", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "apply --json should reject unknown file-level target variant"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("unknown variant")),
        "expected unknown variant parse message"
    );
}

#[test]
fn apply_rejects_file_start_insert_preview_span_mismatch_on_bom_file() {
    let mut temp_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp python file should be created");
    temp_file
        .write_all(b"\xEF\xBB\xBFdef value(x):\n    return x + 1\n")
        .expect("bom fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    let source = fs::read_to_string(&file_path).expect("fixture should be readable");
    let expected_file_hash = identedit::changeset::hash_text(&source);

    let changeset = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "type": "file_start",
                    "expected_file_hash": expected_file_hash
                },
                "op": {"type": "insert", "new_text": "# header\n"},
                "preview": {
                    "old_text": "",
                    "new_text": "# header\n",
                    "matched_span": {"start": 0, "end": 0}
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["apply"], &changeset.to_string());
    assert!(
        !output.status.success(),
        "apply should reject BOM file_start preview span mismatch"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("preview.matched_span")),
        "expected preview span mismatch diagnostic"
    );
}

#[test]
fn apply_rejects_file_end_insert_preview_span_mismatch_on_bom_file() {
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

    let changeset = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "type": "file_end",
                    "expected_file_hash": expected_file_hash
                },
                "op": {"type": "insert", "new_text": "# tail\n"},
                "preview": {
                    "old_text": "",
                    "new_text": "# tail\n",
                    "matched_span": {"start": 0, "end": 0}
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["apply"], &changeset.to_string());
    assert!(
        !output.status.success(),
        "apply should reject BOM file_end preview span mismatch"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("preview.matched_span")),
        "expected preview span mismatch diagnostic"
    );
}

#[test]
fn apply_rejects_file_start_insert_preview_old_text_tamper() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let source = fs::read_to_string(&file_path).expect("fixture should be readable");
    let expected_file_hash = identedit::changeset::hash_text(&source);

    let changeset = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "type": "file_start",
                    "expected_file_hash": expected_file_hash
                },
                "op": {"type": "insert", "new_text": "# header\n"},
                "preview": {
                    "old_text": "tampered",
                    "new_text": "# header\n",
                    "matched_span": {"start": 0, "end": 0}
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["apply"], &changeset.to_string());
    assert!(
        !output.status.success(),
        "apply should reject file_start preview old_text tampering"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("preview.old_text")),
        "expected preview old_text mismatch diagnostic"
    );
}

#[test]
fn apply_rejects_file_end_insert_preview_old_text_tamper() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let source = fs::read_to_string(&file_path).expect("fixture should be readable");
    let expected_file_hash = identedit::changeset::hash_text(&source);
    let file_end = source.len();

    let changeset = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "type": "file_end",
                    "expected_file_hash": expected_file_hash
                },
                "op": {"type": "insert", "new_text": "# tail\n"},
                "preview": {
                    "old_text": "tampered",
                    "new_text": "# tail\n",
                    "matched_span": {"start": file_end, "end": file_end}
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["apply"], &changeset.to_string());
    assert!(
        !output.status.success(),
        "apply should reject file_end preview old_text tampering"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("preview.old_text")),
        "expected preview old_text mismatch diagnostic"
    );
}

#[test]
fn apply_rejects_file_end_insert_when_file_hash_is_stale() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let before = fs::read_to_string(&file_path).expect("file should be readable");
    let insert_text = "\n# stale-append\n";

    let changeset = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "type": "file_end",
                    "expected_file_hash": "stale-file-hash"
                },
                "op": {"type": "insert", "new_text": insert_text},
                "preview": {
                    "old_text": "",
                    "new_text": insert_text,
                    "matched_span": {"start": before.len(), "end": before.len()}
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["apply"], &changeset.to_string());
    assert!(
        !output.status.success(),
        "apply should reject stale file hash for file_end insert"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "precondition_failed");
}

#[test]
fn apply_rejects_file_end_replace_combo() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let before = fs::read_to_string(&file_path).expect("file should be readable");
    let expected_file_hash = identedit::changeset::hash_text(&before);

    let changeset = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "type": "file_end",
                    "expected_file_hash": expected_file_hash
                },
                "op": {"type": "replace", "new_text": "invalid"},
                "preview": {
                    "old_text": "",
                    "new_text": "invalid",
                    "matched_span": {"start": before.len(), "end": before.len()}
                }
            }
        ]
    });

    let output = run_identedit_with_stdin(&["apply"], &changeset.to_string());
    assert!(
        !output.status.success(),
        "apply should reject file target + replace combo"
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
fn apply_supports_delete_for_anchor_span() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_handle(&file_path, "process_*");
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
            }
        ]
    });

    let output = run_identedit_with_stdin(&["apply"], &changeset.to_string());
    assert!(
        output.status.success(),
        "apply should support delete operation: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["summary"]["operations_applied"], 1);
    assert_eq!(response["summary"]["operations_failed"], 0);

    let modified = fs::read_to_string(&file_path).expect("file should be readable");
    assert!(
        !modified.contains(old_text),
        "delete should remove the matched anchor text"
    );
    assert!(
        modified.contains("def helper():"),
        "delete should not remove unrelated nodes"
    );
}

#[test]
fn apply_delete_and_empty_replace_are_semantically_equivalent() {
    let file_path_delete = copy_fixture_to_temp_python("example.py");
    let handle_delete = select_named_handle(&file_path_delete, "process_*");
    let span_delete = &handle_delete["span"];
    let start_delete = span_delete["start"].as_u64().expect("span start") as usize;
    let end_delete = span_delete["end"].as_u64().expect("span end") as usize;
    let old_text_delete = handle_delete["text"]
        .as_str()
        .expect("text should be string");
    let expected_hash_delete = identedit::changeset::hash_text(old_text_delete);

    let delete_changeset = json!({
        "file": file_path_delete.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "identity": handle_delete["identity"],
                    "kind": handle_delete["kind"],
                    "span_hint": {"start": start_delete, "end": end_delete},
                    "expected_old_hash": expected_hash_delete
                },
                "op": {"type": "delete"},
                "preview": {
                    "old_text": old_text_delete,
                    "new_text": "",
                    "matched_span": {"start": start_delete, "end": end_delete}
                }
            }
        ]
    });

    let delete_output = run_identedit_with_stdin(&["apply"], &delete_changeset.to_string());
    assert!(
        delete_output.status.success(),
        "delete apply should succeed: {}",
        String::from_utf8_lossy(&delete_output.stderr)
    );
    let delete_response: Value =
        serde_json::from_slice(&delete_output.stdout).expect("stdout should be valid JSON");

    let file_path_replace = copy_fixture_to_temp_python("example.py");
    let handle_replace = select_named_handle(&file_path_replace, "process_*");
    let span_replace = &handle_replace["span"];
    let start_replace = span_replace["start"].as_u64().expect("span start") as usize;
    let end_replace = span_replace["end"].as_u64().expect("span end") as usize;
    let old_text_replace = handle_replace["text"]
        .as_str()
        .expect("text should be string");
    let expected_hash_replace = identedit::changeset::hash_text(old_text_replace);

    let replace_changeset = json!({
        "file": file_path_replace.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "identity": handle_replace["identity"],
                    "kind": handle_replace["kind"],
                    "span_hint": {"start": start_replace, "end": end_replace},
                    "expected_old_hash": expected_hash_replace
                },
                "op": {"type": "replace", "new_text": ""},
                "preview": {
                    "old_text": old_text_replace,
                    "new_text": "",
                    "matched_span": {"start": start_replace, "end": end_replace}
                }
            }
        ]
    });

    let replace_output = run_identedit_with_stdin(&["apply"], &replace_changeset.to_string());
    assert!(
        replace_output.status.success(),
        "empty replace apply should succeed: {}",
        String::from_utf8_lossy(&replace_output.stderr)
    );
    let replace_response: Value =
        serde_json::from_slice(&replace_output.stdout).expect("stdout should be valid JSON");

    assert_eq!(
        delete_response["summary"]["operations_applied"],
        replace_response["summary"]["operations_applied"]
    );
    assert_eq!(
        delete_response["summary"]["operations_failed"],
        replace_response["summary"]["operations_failed"]
    );

    let after_delete = fs::read_to_string(&file_path_delete).expect("file should be readable");
    let after_replace = fs::read_to_string(&file_path_replace).expect("file should be readable");
    assert_eq!(
        after_delete, after_replace,
        "delete and empty replace should produce identical file contents"
    );
}

#[cfg(unix)]
#[test]
fn apply_delete_whole_file_span_results_in_empty_file_and_preserves_mode() {
    let mut temp_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp python file should be created");
    let source = "def only_function(value):\n    return value + 1";
    temp_file
        .write_all(source.as_bytes())
        .expect("fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    fs::set_permissions(&file_path, fs::Permissions::from_mode(0o640))
        .expect("permissions should be set");

    let handle = select_named_handle(&file_path, "only_*");
    let span = &handle["span"];
    let span_start = span["start"].as_u64().expect("span start") as usize;
    let span_end = span["end"].as_u64().expect("span end") as usize;
    let source_len = fs::read(&file_path).expect("file should be readable").len();
    assert_eq!(
        span_start, 0,
        "whole-file function span should start at byte 0"
    );
    assert_eq!(
        span_end, source_len,
        "whole-file function span should cover all bytes"
    );

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
            }
        ]
    });

    let output = run_identedit_with_stdin(&["apply"], &changeset.to_string());
    assert!(
        output.status.success(),
        "whole-file delete should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("file should be readable");
    assert_eq!(modified, "", "whole-file delete should leave empty file");

    let mode = fs::metadata(&file_path)
        .expect("metadata should be readable")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o640, "whole-file delete should preserve mode bits");
}

#[cfg(unix)]
#[test]
fn apply_delete_whole_file_span_with_crlf_results_in_empty_file_and_preserves_mode() {
    let mut temp_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp python file should be created");
    let source = "def only_function(value):\r\n    return value + 1";
    temp_file
        .write_all(source.as_bytes())
        .expect("fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    fs::set_permissions(&file_path, fs::Permissions::from_mode(0o640))
        .expect("permissions should be set");

    let handle = select_named_handle(&file_path, "only_*");
    let span = &handle["span"];
    let span_start = span["start"].as_u64().expect("span start") as usize;
    let span_end = span["end"].as_u64().expect("span end") as usize;
    let source_len = fs::read(&file_path).expect("file should be readable").len();
    assert_eq!(
        span_start, 0,
        "whole-file CRLF function span should start at byte 0"
    );
    assert_eq!(
        span_end, source_len,
        "whole-file CRLF function span should cover all bytes"
    );

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
            }
        ]
    });

    let output = run_identedit_with_stdin(&["apply"], &changeset.to_string());
    assert!(
        output.status.success(),
        "whole-file CRLF delete should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("file should be readable");
    assert_eq!(
        modified, "",
        "whole-file CRLF delete should leave empty file"
    );

    let mode = fs::metadata(&file_path)
        .expect("metadata should be readable")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(
        mode, 0o640,
        "whole-file CRLF delete should preserve mode bits"
    );
}

#[cfg(unix)]
#[test]
fn apply_delete_single_function_in_bom_prefixed_file_preserves_bom_prefix() {
    let mut temp_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp python file should be created");
    let mut source = vec![0xEF, 0xBB, 0xBF];
    source.extend_from_slice(b"def only_function(value):\n    return value + 1");
    temp_file
        .write_all(&source)
        .expect("fixture write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;
    fs::set_permissions(&file_path, fs::Permissions::from_mode(0o640))
        .expect("permissions should be set");

    let handle = select_named_handle(&file_path, "only_*");
    let span = &handle["span"];
    let span_start = span["start"].as_u64().expect("span start") as usize;
    let span_end = span["end"].as_u64().expect("span end") as usize;
    assert_eq!(
        span_start, 3,
        "BOM-prefixed function span should begin after BOM bytes"
    );
    assert_eq!(
        span_end,
        source.len(),
        "BOM-prefixed function span should cover bytes after BOM"
    );

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
            }
        ]
    });

    let output = run_identedit_with_stdin(&["apply"], &changeset.to_string());
    assert!(
        output.status.success(),
        "delete on BOM-prefixed function should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let modified = fs::read(&file_path).expect("file should be readable");
    assert_eq!(
        modified,
        vec![0xEF, 0xBB, 0xBF],
        "delete should preserve BOM bytes when anchor starts after BOM"
    );

    let mode = fs::metadata(&file_path)
        .expect("metadata should be readable")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o640, "delete should preserve mode bits in BOM files");
}

#[test]
fn apply_rejects_delete_and_insert_on_same_anchor() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_handle(&file_path, "process_*");
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
        "apply should reject delete+insert conflict on the same anchor"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}
