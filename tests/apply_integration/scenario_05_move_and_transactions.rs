use super::*;

#[cfg(unix)]
#[test]
fn apply_json_mode_move_rejects_existing_symlink_destination() {
    use std::os::unix::fs::symlink;

    let workspace = tempdir().expect("tempdir should be created");
    let source_path = workspace.path().join("source.py");
    let real_destination = workspace.path().join("existing_target.py");
    let symlink_destination = workspace.path().join("existing_link.py");
    fs::write(&source_path, "def keep():\n    return 1\n")
        .expect("source fixture write should succeed");
    fs::write(&real_destination, "def already_here():\n    return 9\n")
        .expect("destination fixture write should succeed");
    symlink(&real_destination, &symlink_destination).expect("symlink should be created");

    let request = json!({
        "command": "apply",
        "changeset": {
            "files": [
                {
                    "file": source_path.to_string_lossy().to_string(),
                    "operations": [
                        {
                            "target": {
                                "identity": "unused-identity-symlink",
                                "kind": "function_definition",
                                "expected_old_hash": "unused-hash-symlink"
                            },
                            "op": {
                                "type": "move",
                                "to": symlink_destination.to_string_lossy().to_string()
                            },
                            "preview": {
                                "old_text": "",
                                "new_text": "",
                                "matched_span": {
                                    "start": 0,
                                    "end": 0
                                }
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
        "existing symlink destination should be treated as occupied path"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| { message.contains("Destination path already exists") }),
        "expected destination-exists rejection for symlink path"
    );
    assert!(
        source_path.exists(),
        "source should not be moved on rejection"
    );
}

#[test]
fn apply_json_mode_executes_move_with_relative_paths_in_json_mode() {
    let workspace = tempdir().expect("tempdir should be created");
    let source_path = workspace.path().join("source.py");
    fs::write(&source_path, "def keep():\n    return 1\n").expect("fixture write should succeed");

    let request = json!({
        "command": "apply",
        "changeset": {
            "files": [
                {
                    "file": "source.py",
                    "operations": [
                        {
                            "target": {
                                "identity": "unused-identity-rel",
                                "kind": "function_definition",
                                "expected_old_hash": "unused-hash-rel"
                            },
                            "op": {
                                "type": "move",
                                "to": "./renamed.py"
                            },
                            "preview": {
                                "old_text": "",
                                "new_text": "",
                                "matched_span": {
                                    "start": 0,
                                    "end": 0
                                }
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
        output.status.success(),
        "relative-path move should succeed in json mode: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["transaction"]["status"], "committed");
    assert!(
        !workspace.path().join("source.py").exists(),
        "relative source path should be moved away"
    );
    assert!(
        workspace.path().join("renamed.py").exists(),
        "relative destination should be created in current directory"
    );
}

#[test]
fn apply_json_mode_move_rejects_dot_segment_self_move_in_relative_mode() {
    let workspace = tempdir().expect("tempdir should be created");
    fs::write(
        workspace.path().join("source.py"),
        "def keep():\n    return 1\n",
    )
    .expect("fixture write should succeed");

    let request = json!({
        "command": "apply",
        "changeset": {
            "files": [
                {
                    "file": "./source.py",
                    "operations": [
                        {
                            "target": {
                                "identity": "unused-identity-dot",
                                "kind": "function_definition",
                                "expected_old_hash": "unused-hash-dot"
                            },
                            "op": {
                                "type": "move",
                                "to": "nested/../source.py"
                            },
                            "preview": {
                                "old_text": "",
                                "new_text": "",
                                "matched_span": {
                                    "start": 0,
                                    "end": 0
                                }
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
        "dot-segment alias that resolves to same path should be rejected as self-move"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("self-move")),
        "expected self-move rejection after dot-segment normalization"
    );
}

#[test]
fn apply_json_mode_executes_move_to_nested_existing_directory() {
    let workspace = tempdir().expect("tempdir should be created");
    let source_path = workspace.path().join("source.py");
    let nested_dir = workspace.path().join("nested");
    fs::create_dir_all(&nested_dir).expect("nested directory should be created");
    fs::write(&source_path, "def keep():\n    return 1\n").expect("fixture write should succeed");

    let request = json!({
        "command": "apply",
        "changeset": {
            "files": [
                {
                    "file": "source.py",
                    "operations": [
                        {
                            "target": {
                                "identity": "unused-identity-nested",
                                "kind": "function_definition",
                                "expected_old_hash": "unused-hash-nested"
                            },
                            "op": {
                                "type": "move",
                                "to": "nested/renamed.py"
                            },
                            "preview": {
                                "old_text": "",
                                "new_text": "",
                                "matched_span": {
                                    "start": 0,
                                    "end": 0
                                }
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
        output.status.success(),
        "move into existing nested directory should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(
        !workspace.path().join("source.py").exists(),
        "source should be moved away on successful nested move"
    );
    assert!(
        workspace.path().join("nested/renamed.py").exists(),
        "nested destination file should exist after move"
    );
}

#[test]
fn apply_json_mode_move_to_path_under_file_parent_returns_io_error() {
    let workspace = tempdir().expect("tempdir should be created");
    let source_path = workspace.path().join("source.py");
    let parent_file = workspace.path().join("not_a_directory");
    fs::write(&source_path, "def keep():\n    return 1\n")
        .expect("source fixture write should succeed");
    fs::write(&parent_file, "occupied by file\n")
        .expect("parent-file fixture write should succeed");

    let request = json!({
        "command": "apply",
        "changeset": {
            "files": [
                {
                    "file": source_path.to_string_lossy().to_string(),
                    "operations": [
                        {
                            "target": {
                                "identity": "unused-identity-parent-file",
                                "kind": "function_definition",
                                "expected_old_hash": "unused-hash-parent-file"
                            },
                            "op": {
                                "type": "move",
                                "to": workspace.path().join("not_a_directory/renamed.py").to_string_lossy().to_string()
                            },
                            "preview": {
                                "old_text": "",
                                "new_text": "",
                                "matched_span": {
                                    "start": 0,
                                    "end": 0
                                }
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
        "destination under non-directory parent should fail with io error"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "io_error");
    assert!(
        source_path.exists(),
        "source should remain in place on io failure"
    );
}

#[test]
fn apply_json_mode_move_rejects_duplicate_destination_alias_paths() {
    let workspace = tempdir().expect("tempdir should be created");
    fs::write(
        workspace.path().join("a.py"),
        "def from_a():\n    return 'a'\n",
    )
    .expect("fixture write should succeed");
    fs::write(
        workspace.path().join("b.py"),
        "def from_b():\n    return 'b'\n",
    )
    .expect("fixture write should succeed");

    let request = json!({
        "command": "apply",
        "changeset": {
            "files": [
                {
                    "file": "a.py",
                    "operations": [
                        {
                            "target": {
                                "identity": "unused-identity-a",
                                "kind": "function_definition",
                                "expected_old_hash": "unused-hash-a"
                            },
                            "op": {
                                "type": "move",
                                "to": "renamed.py"
                            },
                            "preview": {
                                "old_text": "",
                                "new_text": "",
                                "matched_span": {
                                    "start": 0,
                                    "end": 0
                                }
                            }
                        }
                    ]
                },
                {
                    "file": "b.py",
                    "operations": [
                        {
                            "target": {
                                "identity": "unused-identity-b",
                                "kind": "function_definition",
                                "expected_old_hash": "unused-hash-b"
                            },
                            "op": {
                                "type": "move",
                                "to": "./renamed.py"
                            },
                            "preview": {
                                "old_text": "",
                                "new_text": "",
                                "matched_span": {
                                    "start": 0,
                                    "end": 0
                                }
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
        "destination alias collision should be rejected"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("Duplicate move destination")),
        "expected duplicate destination rejection for alias paths"
    );
    assert!(workspace.path().join("a.py").exists());
    assert!(workspace.path().join("b.py").exists());
    assert!(
        !workspace.path().join("renamed.py").exists(),
        "no destination should be created on validation failure"
    );
}

#[test]
fn apply_json_mode_move_graph_rejects_cycle_with_relative_aliases() {
    let workspace = tempdir().expect("tempdir should be created");
    fs::write(
        workspace.path().join("a.py"),
        "def from_a():\n    return 'a'\n",
    )
    .expect("fixture write should succeed");
    fs::write(
        workspace.path().join("b.py"),
        "def from_b():\n    return 'b'\n",
    )
    .expect("fixture write should succeed");

    let request = json!({
        "command": "apply",
        "changeset": {
            "files": [
                {
                    "file": "a.py",
                    "operations": [
                        {
                            "target": {
                                "identity": "unused-identity-a",
                                "kind": "function_definition",
                                "expected_old_hash": "unused-hash-a"
                            },
                            "op": {
                                "type": "move",
                                "to": "./b.py"
                            },
                            "preview": {
                                "old_text": "",
                                "new_text": "",
                                "matched_span": {
                                    "start": 0,
                                    "end": 0
                                }
                            }
                        }
                    ]
                },
                {
                    "file": "b.py",
                    "operations": [
                        {
                            "target": {
                                "identity": "unused-identity-b",
                                "kind": "function_definition",
                                "expected_old_hash": "unused-hash-b"
                            },
                            "op": {
                                "type": "move",
                                "to": "nested/../a.py"
                            },
                            "preview": {
                                "old_text": "",
                                "new_text": "",
                                "matched_span": {
                                    "start": 0,
                                    "end": 0
                                }
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
        "relative alias cycle should be rejected"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("Move graph contains a cycle")),
        "expected cycle rejection for alias-based cycle"
    );
}

#[test]
fn apply_json_mode_move_chain_executes_with_relative_alias_destinations() {
    let workspace = tempdir().expect("tempdir should be created");
    fs::write(
        workspace.path().join("a.py"),
        "def from_a():\n    return 'a'\n",
    )
    .expect("fixture write should succeed");
    fs::write(
        workspace.path().join("b.py"),
        "def from_b():\n    return 'b'\n",
    )
    .expect("fixture write should succeed");

    let request = json!({
        "command": "apply",
        "changeset": {
            "files": [
                {
                    "file": "a.py",
                    "operations": [
                        {
                            "target": {
                                "identity": "unused-identity-a",
                                "kind": "function_definition",
                                "expected_old_hash": "unused-hash-a"
                            },
                            "op": {
                                "type": "move",
                                "to": "./b.py"
                            },
                            "preview": {
                                "old_text": "",
                                "new_text": "",
                                "matched_span": {
                                    "start": 0,
                                    "end": 0
                                }
                            }
                        }
                    ]
                },
                {
                    "file": "b.py",
                    "operations": [
                        {
                            "target": {
                                "identity": "unused-identity-b",
                                "kind": "function_definition",
                                "expected_old_hash": "unused-hash-b"
                            },
                            "op": {
                                "type": "move",
                                "to": "./c.py"
                            },
                            "preview": {
                                "old_text": "",
                                "new_text": "",
                                "matched_span": {
                                    "start": 0,
                                    "end": 0
                                }
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
        output.status.success(),
        "relative-alias move chain should execute successfully: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(!workspace.path().join("a.py").exists());
    assert!(workspace.path().join("b.py").exists());
    assert!(workspace.path().join("c.py").exists());
}

#[test]
fn apply_json_mode_executes_move_with_non_self_dot_segment_destination() {
    let workspace = tempdir().expect("tempdir should be created");
    fs::write(
        workspace.path().join("source.py"),
        "def keep():\n    return 1\n",
    )
    .expect("fixture write should succeed");

    let request = json!({
        "command": "apply",
        "changeset": {
            "files": [
                {
                    "file": "source.py",
                    "operations": [
                        {
                            "target": {
                                "identity": "unused-identity-dot-nonself",
                                "kind": "function_definition",
                                "expected_old_hash": "unused-hash-dot-nonself"
                            },
                            "op": {
                                "type": "move",
                                "to": "nested/../renamed.py"
                            },
                            "preview": {
                                "old_text": "",
                                "new_text": "",
                                "matched_span": {
                                    "start": 0,
                                    "end": 0
                                }
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
        output.status.success(),
        "non-self dot-segment destination should still execute move: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!workspace.path().join("source.py").exists());
    assert!(workspace.path().join("renamed.py").exists());
}

#[test]
fn apply_json_mode_rejects_unknown_span_field() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_handle(&file_path, "process_*");
    let span = &handle["span"];
    let old_text = handle["text"].as_str().expect("text should be string");
    let expected_hash = identedit::changeset::hash_text(old_text);
    let request = json!({
        "command": "apply",
        "changeset": {
            "file": file_path.to_string_lossy().to_string(),
            "operations": [
                {
                    "target": {
                        "identity": handle["identity"],
                        "kind": handle["kind"],
                        "expected_old_hash": expected_hash,
                        "span_hint": {
                            "start": span["start"],
                            "end": span["end"],
                            "unexpected": 0
                        }
                    },
                    "op": {
                        "type": "replace",
                        "new_text": "def process_data(value):\n    return value + 3"
                    },
                    "preview": {
                        "old_text": old_text,
                        "new_text": "def process_data(value):\n    return value + 3",
                        "matched_span": {
                            "start": span["start"],
                            "end": span["end"]
                        }
                    }
                }
            ]
        }
    });

    let output = run_identedit_with_stdin(&["apply", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "apply should reject unknown span fields"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("unknown field `unexpected`")),
        "expected unknown span field message"
    );
}

#[test]
fn apply_json_mode_rejects_operations_null_type() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let request = json!({
        "command": "apply",
        "changeset": {
            "file": file_path.to_string_lossy().to_string(),
            "operations": Value::Null
        }
    });

    let output = run_identedit_with_stdin(&["apply", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "apply should reject null operations payload"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}

#[test]
fn apply_json_mode_treats_env_token_file_path_as_literal() {
    let request = json!({
        "command": "apply",
        "changeset": {
            "file": format!("${{IDENTEDIT_APPLY_JSON_PATH_{}}}/example.py", std::process::id()),
            "operations": []
        }
    });

    let output = run_identedit_with_stdin(&["apply", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "json-mode apply path should not expand env tokens"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "io_error");
}

#[test]
fn apply_json_mode_rejects_non_apply_command() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let request = json!({
        "command": "transform",
        "changeset": {
            "file": file_path.to_string_lossy().to_string(),
            "operations": []
        }
    });

    let output = run_identedit_with_stdin(&["apply", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "apply should reject command mismatch in JSON mode"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("expected 'apply'")),
        "expected command mismatch message"
    );
}

#[test]
fn apply_json_mode_rejects_missing_command_field() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let request = json!({
        "changeset": {
            "file": file_path.to_string_lossy().to_string(),
            "operations": []
        }
    });

    let output = run_identedit_with_stdin(&["apply", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "apply should reject missing command field in JSON mode"
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
fn apply_json_mode_rejects_non_string_command_type() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let payloads = [
        json!({
            "command": 123,
            "changeset": {
                "file": file_path.to_string_lossy().to_string(),
                "operations": []
            }
        }),
        json!({
            "command": null,
            "changeset": {
                "file": file_path.to_string_lossy().to_string(),
                "operations": []
            }
        }),
        json!({
            "command": {"value": "apply"},
            "changeset": {
                "file": file_path.to_string_lossy().to_string(),
                "operations": []
            }
        }),
    ];

    for payload in payloads {
        let output = run_identedit_with_stdin(&["apply", "--json"], &payload.to_string());
        assert!(
            !output.status.success(),
            "apply should reject non-string command in JSON mode: {payload}"
        );

        let response: Value =
            serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
        assert_eq!(response["error"]["type"], "invalid_request");
        assert!(
            response["error"]["message"]
                .as_str()
                .is_some_and(|message| message.contains("invalid type")),
            "expected invalid type command diagnostic"
        );
    }
}

#[test]
fn apply_json_mode_rejects_command_with_trailing_whitespace() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let request = json!({
        "command": "apply ",
        "changeset": {
            "file": file_path.to_string_lossy().to_string(),
            "operations": []
        }
    });

    let output = run_identedit_with_stdin(&["apply", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "apply should reject trailing-whitespace command token"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}

#[test]
fn apply_json_mode_rejects_uppercase_command_token() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let request = json!({
        "command": "APPLY",
        "changeset": {
            "file": file_path.to_string_lossy().to_string(),
            "operations": []
        }
    });

    let output = run_identedit_with_stdin(&["apply", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "apply should reject uppercase command token"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}

#[test]
fn apply_changeset_argument_directory_returns_io_error() {
    let directory = tempdir().expect("tempdir should be created");
    let output = run_identedit(&[
        "apply",
        directory.path().to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "apply should fail when CHANGESET argument is a directory"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "io_error");
}

#[test]
fn apply_json_mode_directory_target_returns_io_error() {
    let directory = tempdir().expect("tempdir should be created");
    let request = json!({
        "command": "apply",
        "changeset": {
            "file": directory.path().to_string_lossy().to_string(),
            "operations": []
        }
    });

    let output = run_identedit_with_stdin(&["apply", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "apply JSON mode should fail when changeset file target is a directory"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "io_error");
}

#[test]
fn apply_multi_file_stdin_mode_applies_all_files_in_deterministic_order() {
    let file_a = copy_fixture_to_temp_python("example.py");
    let file_b = copy_fixture_to_temp_python("example.py");
    let before_a = fs::read_to_string(&file_a).expect("file_a should be readable");
    let before_b = fs::read_to_string(&file_b).expect("file_b should be readable");

    let handle_a = select_named_handle(&file_a, "process_*");
    let handle_b = select_named_handle(&file_b, "process_*");
    let span_a = &handle_a["span"];
    let span_b = &handle_b["span"];
    let new_text_a = "def process_data(value):\n    return value * 99";
    let new_text_b = "def process_data(value):\n    return value * 98";
    let expected_hash_a =
        identedit::changeset::hash_text(handle_a["text"].as_str().expect("text should be string"));
    let expected_hash_b =
        identedit::changeset::hash_text(handle_b["text"].as_str().expect("text should be string"));

    let payload = json!({
        "files": [
            {
                "file": file_b.to_string_lossy().to_string(),
                "operations": [
                    {
                        "target": {
                            "identity": handle_b["identity"],
                            "kind": handle_b["kind"],
                            "span_hint": {"start": span_b["start"], "end": span_b["end"]},
                            "expected_old_hash": expected_hash_b
                        },
                        "op": {"type": "replace", "new_text": new_text_b},
                        "preview": {
                            "old_text": handle_b["text"],
                            "new_text": new_text_b,
                            "matched_span": {"start": span_b["start"], "end": span_b["end"]}
                        }
                    }
                ]
            },
            {
                "file": file_a.to_string_lossy().to_string(),
                "operations": [
                    {
                        "target": {
                            "identity": handle_a["identity"],
                            "kind": handle_a["kind"],
                            "span_hint": {"start": span_a["start"], "end": span_a["end"]},
                            "expected_old_hash": expected_hash_a
                        },
                        "op": {"type": "replace", "new_text": new_text_a},
                        "preview": {
                            "old_text": handle_a["text"],
                            "new_text": new_text_a,
                            "matched_span": {"start": span_a["start"], "end": span_a["end"]}
                        }
                    }
                ]
            }
        ],
        "transaction": {
            "mode": "all_or_nothing"
        }
    });

    let payload_json = payload.to_string();
    let output = run_identedit_with_raw_stdin(&["apply", "--verbose"], payload_json.as_bytes());
    assert!(
        output.status.success(),
        "bare apply stdin should apply multi-file payload: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["summary"]["files_modified"], 2);
    assert_eq!(response["summary"]["operations_applied"], 2);
    assert_eq!(response["summary"]["operations_failed"], 0);
    assert_eq!(response["transaction"]["mode"], "all_or_nothing");
    assert_eq!(response["transaction"]["status"], "committed");

    let applied_files = response["applied"]
        .as_array()
        .expect("applied should be array")
        .iter()
        .map(|entry| {
            entry["file"]
                .as_str()
                .expect("applied.file should be string")
                .to_string()
        })
        .collect::<Vec<_>>();
    let mut expected_order = vec![
        file_a.to_string_lossy().to_string(),
        file_b.to_string_lossy().to_string(),
    ];
    expected_order.sort();
    assert_eq!(
        applied_files, expected_order,
        "applied files should follow deterministic path order"
    );

    let after_a = fs::read_to_string(&file_a).expect("file_a should be readable");
    let after_b = fs::read_to_string(&file_b).expect("file_b should be readable");
    assert_ne!(before_a, after_a, "file_a should be modified");
    assert_ne!(before_b, after_b, "file_b should be modified");
    assert!(after_a.contains("return value * 99"));
    assert!(after_b.contains("return value * 98"));
}

#[test]
fn apply_multi_file_json_mode_applies_cross_language_changes() {
    let file_py = copy_fixture_to_temp_python("example.py");
    let file_json = copy_fixture_to_temp_json("example.json");
    let before_py = fs::read_to_string(&file_py).expect("python fixture should be readable");
    let before_json = fs::read_to_string(&file_json).expect("json fixture should be readable");

    let py_handle = select_named_handle(&file_py, "process_*");
    let py_span = &py_handle["span"];
    let py_new_text = "def process_data(value):\n    return value * 77";
    let py_expected_hash =
        identedit::changeset::hash_text(py_handle["text"].as_str().expect("text should be string"));
    let json_handle = select_root_json_object_handle(&file_json);
    let json_span = &json_handle["span"];
    let json_new_text = "{\n  \"enabled\": true,\n  \"version\": 2,\n  \"tool\": \"identedit\"\n}";
    let json_expected_hash = identedit::changeset::hash_text(
        json_handle["text"].as_str().expect("text should be string"),
    );

    let request = json!({
        "command": "apply",
        "changeset": {
            "files": [
                {
                    "file": file_json.to_string_lossy().to_string(),
                    "operations": [
                        {
                            "target": {
                                "identity": json_handle["identity"],
                                "kind": json_handle["kind"],
                                "span_hint": {"start": json_span["start"], "end": json_span["end"]},
                                "expected_old_hash": json_expected_hash
                            },
                            "op": {"type": "replace", "new_text": json_new_text},
                            "preview": {
                                "old_text": json_handle["text"],
                                "new_text": json_new_text,
                                "matched_span": {"start": json_span["start"], "end": json_span["end"]}
                            }
                        }
                    ]
                },
                {
                    "file": file_py.to_string_lossy().to_string(),
                    "operations": [
                        {
                            "target": {
                                "identity": py_handle["identity"],
                                "kind": py_handle["kind"],
                                "span_hint": {"start": py_span["start"], "end": py_span["end"]},
                                "expected_old_hash": py_expected_hash
                            },
                            "op": {"type": "replace", "new_text": py_new_text},
                            "preview": {
                                "old_text": py_handle["text"],
                                "new_text": py_new_text,
                                "matched_span": {"start": py_span["start"], "end": py_span["end"]}
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

    let request_json = request.to_string();
    let output =
        run_identedit_with_raw_stdin(&["apply", "--json", "--verbose"], request_json.as_bytes());
    assert!(
        output.status.success(),
        "apply --json should apply cross-language multi-file payload: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["summary"]["files_modified"], 2);
    assert_eq!(response["summary"]["operations_applied"], 2);
    assert_eq!(response["summary"]["operations_failed"], 0);
    assert_eq!(response["transaction"]["mode"], "all_or_nothing");
    assert_eq!(response["transaction"]["status"], "committed");

    let mut expected_order = vec![
        file_json.to_string_lossy().to_string(),
        file_py.to_string_lossy().to_string(),
    ];
    expected_order.sort();
    let applied_files = response["applied"]
        .as_array()
        .expect("applied should be array")
        .iter()
        .map(|entry| {
            entry["file"]
                .as_str()
                .expect("applied.file should be string")
                .to_string()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        applied_files, expected_order,
        "applied entries should follow deterministic sorted order"
    );

    let after_py = fs::read_to_string(&file_py).expect("python file should be readable");
    let after_json = fs::read_to_string(&file_json).expect("json file should be readable");
    assert_ne!(before_py, after_py, "python file should be modified");
    assert_ne!(before_json, after_json, "json file should be modified");
    assert!(after_py.contains("return value * 77"));
    assert!(after_json.contains("\"tool\": \"identedit\""));
}

#[test]
fn apply_same_file_stale_identity_resolves_by_unique_kind_and_expected_hash() {
    let source =
        "def alpha(value):\n    return value + 1\n\n\ndef beta(value):\n    return value + 2\n";
    let mut file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp python file should be created");
    file.write_all(source.as_bytes())
        .expect("temp python source write should succeed");
    let file_path = file.keep().expect("temp file should persist").1;

    let alpha_handle = select_first_handle(&file_path, "function_definition", Some("alpha"));
    let beta_handle = select_first_handle(&file_path, "function_definition", Some("beta"));
    let alpha_span = &alpha_handle["span"];
    let beta_span = &beta_handle["span"];
    let alpha_new_text =
        "def alpha(value):\n    if value > 0:\n        return value + 10\n    return value";
    let beta_new_text = "def beta(value):\n    return value + 20";
    let alpha_expected_hash = identedit::changeset::hash_text(
        alpha_handle["text"]
            .as_str()
            .expect("alpha text should be string"),
    );
    let beta_expected_hash = identedit::changeset::hash_text(
        beta_handle["text"]
            .as_str()
            .expect("beta text should be string"),
    );

    let alpha_changeset = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "identity": alpha_handle["identity"],
                    "kind": alpha_handle["kind"],
                    "span_hint": {"start": alpha_span["start"], "end": alpha_span["end"]},
                    "expected_old_hash": alpha_expected_hash
                },
                "op": {"type": "replace", "new_text": alpha_new_text},
                "preview": {
                    "old_text": alpha_handle["text"],
                    "new_text": alpha_new_text,
                    "matched_span": {"start": alpha_span["start"], "end": alpha_span["end"]}
                }
            }
        ]
    });

    let alpha_output = run_identedit_with_stdin(&["apply"], &alpha_changeset.to_string());
    assert!(
        alpha_output.status.success(),
        "first apply should succeed: {}",
        String::from_utf8_lossy(&alpha_output.stderr)
    );

    let stale_beta_changeset = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": [
            {
                "target": {
                    "identity": beta_handle["identity"],
                    "kind": beta_handle["kind"],
                    "span_hint": {"start": beta_span["start"], "end": beta_span["end"]},
                    "expected_old_hash": beta_expected_hash
                },
                "op": {"type": "replace", "new_text": beta_new_text},
                "preview": {
                    "old_text": beta_handle["text"],
                    "new_text": beta_new_text,
                    "matched_span": {"start": beta_span["start"], "end": beta_span["end"]}
                }
            }
        ]
    });

    let beta_output = run_identedit_with_stdin(&["apply"], &stale_beta_changeset.to_string());
    assert!(
        beta_output.status.success(),
        "second apply should recover stale identity via kind+expected_old_hash fallback: {}",
        String::from_utf8_lossy(&beta_output.stderr)
    );

    let beta_response: Value =
        serde_json::from_slice(&beta_output.stdout).expect("stdout should be valid JSON");
    assert_eq!(beta_response["summary"]["operations_applied"], 1);
    assert_eq!(beta_response["summary"]["operations_failed"], 0);

    let after = fs::read_to_string(&file_path).expect("updated file should be readable");
    assert!(
        after.contains("if value > 0"),
        "alpha replacement should remain applied"
    );
    assert!(
        after.contains("return value + 20"),
        "beta replacement should apply after identity fallback"
    );
}

#[test]
fn apply_multi_file_stale_target_fails_without_writing_other_files() {
    let file_a = copy_fixture_to_temp_python("example.py");
    let file_b = copy_fixture_to_temp_python("example.py");
    let before_a = fs::read_to_string(&file_a).expect("file_a should be readable");
    let before_b = fs::read_to_string(&file_b).expect("file_b should be readable");

    let handle_a = select_named_handle(&file_a, "process_*");
    let handle_b = select_named_handle(&file_b, "process_*");
    let span_a = &handle_a["span"];
    let span_b = &handle_b["span"];
    let new_text_a = "def process_data(value):\n    return value * 201";
    let new_text_b = "def process_data(value):\n    return value * 202";
    let expected_hash_a =
        identedit::changeset::hash_text(handle_a["text"].as_str().expect("text should be string"));
    let stale_hash_b = "deadbeef";

    let payload = json!({
        "files": [
            {
                "file": file_a.to_string_lossy().to_string(),
                "operations": [
                    {
                        "target": {
                            "identity": handle_a["identity"],
                            "kind": handle_a["kind"],
                            "span_hint": {"start": span_a["start"], "end": span_a["end"]},
                            "expected_old_hash": expected_hash_a
                        },
                        "op": {"type": "replace", "new_text": new_text_a},
                        "preview": {
                            "old_text": handle_a["text"],
                            "new_text": new_text_a,
                            "matched_span": {"start": span_a["start"], "end": span_a["end"]}
                        }
                    }
                ]
            },
            {
                "file": file_b.to_string_lossy().to_string(),
                "operations": [
                    {
                        "target": {
                            "identity": handle_b["identity"],
                            "kind": handle_b["kind"],
                            "span_hint": {"start": span_b["start"], "end": span_b["end"]},
                            "expected_old_hash": stale_hash_b
                        },
                        "op": {"type": "replace", "new_text": new_text_b},
                        "preview": {
                            "old_text": handle_b["text"],
                            "new_text": new_text_b,
                            "matched_span": {"start": span_b["start"], "end": span_b["end"]}
                        }
                    }
                ]
            }
        ],
        "transaction": {
            "mode": "all_or_nothing"
        }
    });

    let output = run_identedit_with_raw_stdin(&["apply"], payload.to_string().as_bytes());
    assert!(
        !output.status.success(),
        "stale target should fail multi-file apply in preflight"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "precondition_failed");

    let after_a = fs::read_to_string(&file_a).expect("file_a should remain readable");
    let after_b = fs::read_to_string(&file_b).expect("file_b should remain readable");
    assert_eq!(after_a, before_a, "preflight failure must not write file_a");
    assert_eq!(after_b, before_b, "preflight failure must not write file_b");
}

#[test]
fn apply_inject_failure_flag_requires_experimental_env_gate() {
    let file_a = copy_fixture_to_temp_python("example.py");
    let file_b = copy_fixture_to_temp_python("example.py");
    let handle_a = select_named_handle(&file_a, "process_*");
    let handle_b = select_named_handle(&file_b, "process_*");
    let span_a = &handle_a["span"];
    let span_b = &handle_b["span"];
    let expected_hash_a =
        identedit::changeset::hash_text(handle_a["text"].as_str().expect("text should be string"));
    let expected_hash_b =
        identedit::changeset::hash_text(handle_b["text"].as_str().expect("text should be string"));

    let payload = json!({
        "files": [
            {
                "file": file_a.to_string_lossy().to_string(),
                "operations": [
                    {
                        "target": {
                            "identity": handle_a["identity"],
                            "kind": handle_a["kind"],
                            "span_hint": {"start": span_a["start"], "end": span_a["end"]},
                            "expected_old_hash": expected_hash_a
                        },
                        "op": {"type": "replace", "new_text": "def process_data(value):\n    return value * 501"},
                        "preview": {
                            "old_text": handle_a["text"],
                            "new_text": "def process_data(value):\n    return value * 501",
                            "matched_span": {"start": span_a["start"], "end": span_a["end"]}
                        }
                    }
                ]
            },
            {
                "file": file_b.to_string_lossy().to_string(),
                "operations": [
                    {
                        "target": {
                            "identity": handle_b["identity"],
                            "kind": handle_b["kind"],
                            "span_hint": {"start": span_b["start"], "end": span_b["end"]},
                            "expected_old_hash": expected_hash_b
                        },
                        "op": {"type": "replace", "new_text": "def process_data(value):\n    return value * 502"},
                        "preview": {
                            "old_text": handle_b["text"],
                            "new_text": "def process_data(value):\n    return value * 502",
                            "matched_span": {"start": span_b["start"], "end": span_b["end"]}
                        }
                    }
                ]
            }
        ],
        "transaction": {
            "mode": "all_or_nothing"
        }
    });

    let output = run_identedit_with_raw_stdin(
        &["apply", "--inject-failure-after-writes", "1"],
        payload.to_string().as_bytes(),
    );
    assert!(
        !output.status.success(),
        "inject-failure flag should be rejected without experimental env gate"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| {
                message.contains("IDENTEDIT_EXPERIMENTAL=1")
                    && message.contains("--inject-failure-after-writes")
            }),
        "expected gate error message to reference env gate and hidden flag"
    );
}

#[test]
fn apply_inject_failure_after_one_write_rolls_back_prior_commits() {
    let file_a = copy_fixture_to_temp_python("example.py");
    let file_b = copy_fixture_to_temp_python("example.py");
    let before_a = fs::read_to_string(&file_a).expect("file_a should be readable");
    let before_b = fs::read_to_string(&file_b).expect("file_b should be readable");

    let handle_a = select_named_handle(&file_a, "process_*");
    let handle_b = select_named_handle(&file_b, "process_*");
    let span_a = &handle_a["span"];
    let span_b = &handle_b["span"];
    let expected_hash_a =
        identedit::changeset::hash_text(handle_a["text"].as_str().expect("text should be string"));
    let expected_hash_b =
        identedit::changeset::hash_text(handle_b["text"].as_str().expect("text should be string"));
    let new_text_a = "def process_data(value):\n    return value * 601";
    let new_text_b = "def process_data(value):\n    return value * 602";

    let payload = json!({
        "files": [
            {
                "file": file_a.to_string_lossy().to_string(),
                "operations": [
                    {
                        "target": {
                            "identity": handle_a["identity"],
                            "kind": handle_a["kind"],
                            "span_hint": {"start": span_a["start"], "end": span_a["end"]},
                            "expected_old_hash": expected_hash_a
                        },
                        "op": {"type": "replace", "new_text": new_text_a},
                        "preview": {
                            "old_text": handle_a["text"],
                            "new_text": new_text_a,
                            "matched_span": {"start": span_a["start"], "end": span_a["end"]}
                        }
                    }
                ]
            },
            {
                "file": file_b.to_string_lossy().to_string(),
                "operations": [
                    {
                        "target": {
                            "identity": handle_b["identity"],
                            "kind": handle_b["kind"],
                            "span_hint": {"start": span_b["start"], "end": span_b["end"]},
                            "expected_old_hash": expected_hash_b
                        },
                        "op": {"type": "replace", "new_text": new_text_b},
                        "preview": {
                            "old_text": handle_b["text"],
                            "new_text": new_text_b,
                            "matched_span": {"start": span_b["start"], "end": span_b["end"]}
                        }
                    }
                ]
            }
        ],
        "transaction": {
            "mode": "all_or_nothing"
        }
    });

    let output = run_identedit_with_raw_stdin_and_env(
        &["apply", "--inject-failure-after-writes", "1"],
        payload.to_string().as_bytes(),
        &[("IDENTEDIT_EXPERIMENTAL", "1")],
    );
    assert!(
        !output.status.success(),
        "injected commit-stage failure should trigger rollback"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| {
                message.contains("Injected apply failure for rollback rehearsal")
                    && message.contains("after 1 committed writes")
            }),
        "expected deterministic injected failure message"
    );

    let after_a = fs::read_to_string(&file_a).expect("file_a should remain readable");
    let after_b = fs::read_to_string(&file_b).expect("file_b should remain readable");
    assert_eq!(after_a, before_a, "rollback should restore file_a");
    assert_eq!(after_b, before_b, "rollback should restore file_b");
}

#[test]
fn apply_inject_failure_after_one_write_rolls_back_cross_file_structural_move() {
    let workspace = tempdir().expect("tempdir should be created");
    let source_file = workspace.path().join("a_source.py");
    let destination_file = workspace.path().join("b_destination.py");
    fs::write(
        &source_file,
        "def source_fn(value):\n    return value + 1\n\n\ndef keep_source(value):\n    return value + 2\n",
    )
    .expect("source fixture write should succeed");
    fs::write(
        &destination_file,
        "def destination_anchor(value):\n    return value * 2\n",
    )
    .expect("destination fixture write should succeed");

    let before_source = fs::read_to_string(&source_file).expect("source should be readable");
    let before_destination =
        fs::read_to_string(&destination_file).expect("destination should be readable");

    let source_handle = select_first_handle(&source_file, "function_definition", Some("source_fn"));
    let destination_handle = select_first_handle(
        &destination_file,
        "function_definition",
        Some("destination_anchor"),
    );

    let transform_request = json!({
        "command": "transform",
        "file": source_file.to_string_lossy().to_string(),
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

    let transform_output =
        run_identedit_with_stdin(&["transform", "--json"], &transform_request.to_string());
    assert!(
        transform_output.status.success(),
        "transform should build cross-file structural move changeset: {}",
        String::from_utf8_lossy(&transform_output.stderr)
    );

    let transformed_changeset: Value =
        serde_json::from_slice(&transform_output.stdout).expect("stdout should be valid JSON");
    assert_eq!(
        transformed_changeset["files"].as_array().map(Vec::len),
        Some(2),
        "cross-file move should normalize to source delete + destination insert"
    );

    let apply_output = run_identedit_with_raw_stdin_and_env(
        &["apply", "--inject-failure-after-writes", "1"],
        &transform_output.stdout,
        &[("IDENTEDIT_EXPERIMENTAL", "1")],
    );
    assert!(
        !apply_output.status.success(),
        "injected commit-stage failure should trigger rollback for cross-file move"
    );

    let response: Value =
        serde_json::from_slice(&apply_output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| {
                message.contains("Injected apply failure for rollback rehearsal")
                    && message.contains("after 1 committed writes")
            }),
        "expected deterministic injected failure diagnostic"
    );

    let after_source = fs::read_to_string(&source_file).expect("source should remain readable");
    let after_destination =
        fs::read_to_string(&destination_file).expect("destination should remain readable");
    assert_eq!(
        after_source, before_source,
        "rollback should restore source file after cross-file move failure"
    );
    assert_eq!(
        after_destination, before_destination,
        "rollback should restore destination file after cross-file move failure"
    );
}

#[test]
fn apply_inject_failure_after_one_write_rolls_back_in_json_mode() {
    let file_a = copy_fixture_to_temp_python("example.py");
    let file_b = copy_fixture_to_temp_python("example.py");
    let before_a = fs::read_to_string(&file_a).expect("file_a should be readable");
    let before_b = fs::read_to_string(&file_b).expect("file_b should be readable");

    let handle_a = select_named_handle(&file_a, "process_*");
    let handle_b = select_named_handle(&file_b, "process_*");
    let span_a = &handle_a["span"];
    let span_b = &handle_b["span"];
    let expected_hash_a =
        identedit::changeset::hash_text(handle_a["text"].as_str().expect("text should be string"));
    let expected_hash_b =
        identedit::changeset::hash_text(handle_b["text"].as_str().expect("text should be string"));
    let new_text_a = "def process_data(value):\n    return value * 611";
    let new_text_b = "def process_data(value):\n    return value * 612";

    let request = json!({
        "command": "apply",
        "changeset": {
            "files": [
                {
                    "file": file_a.to_string_lossy().to_string(),
                    "operations": [
                        {
                            "target": {
                                "identity": handle_a["identity"],
                                "kind": handle_a["kind"],
                                "span_hint": {"start": span_a["start"], "end": span_a["end"]},
                                "expected_old_hash": expected_hash_a
                            },
                            "op": {"type": "replace", "new_text": new_text_a},
                            "preview": {
                                "old_text": handle_a["text"],
                                "new_text": new_text_a,
                                "matched_span": {"start": span_a["start"], "end": span_a["end"]}
                            }
                        }
                    ]
                },
                {
                    "file": file_b.to_string_lossy().to_string(),
                    "operations": [
                        {
                            "target": {
                                "identity": handle_b["identity"],
                                "kind": handle_b["kind"],
                                "span_hint": {"start": span_b["start"], "end": span_b["end"]},
                                "expected_old_hash": expected_hash_b
                            },
                            "op": {"type": "replace", "new_text": new_text_b},
                            "preview": {
                                "old_text": handle_b["text"],
                                "new_text": new_text_b,
                                "matched_span": {"start": span_b["start"], "end": span_b["end"]}
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

    let output = run_identedit_with_raw_stdin_and_env(
        &["apply", "--json", "--inject-failure-after-writes", "1"],
        request.to_string().as_bytes(),
        &[("IDENTEDIT_EXPERIMENTAL", "1")],
    );
    assert!(
        !output.status.success(),
        "json-mode injected commit-stage failure should trigger rollback"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| {
                message.contains("Injected apply failure for rollback rehearsal")
                    && message.contains("after 1 committed writes")
            }),
        "expected deterministic injected failure message in json mode"
    );

    let after_a = fs::read_to_string(&file_a).expect("file_a should remain readable");
    let after_b = fs::read_to_string(&file_b).expect("file_b should remain readable");
    assert_eq!(after_a, before_a, "rollback should restore file_a");
    assert_eq!(after_b, before_b, "rollback should restore file_b");
}

#[test]
fn apply_inject_failure_count_above_commit_count_is_noop() {
    let file_a = copy_fixture_to_temp_python("example.py");
    let file_b = copy_fixture_to_temp_python("example.py");
    let before_a = fs::read_to_string(&file_a).expect("file_a should be readable");
    let before_b = fs::read_to_string(&file_b).expect("file_b should be readable");

    let handle_a = select_named_handle(&file_a, "process_*");
    let handle_b = select_named_handle(&file_b, "process_*");
    let span_a = &handle_a["span"];
    let span_b = &handle_b["span"];
    let expected_hash_a =
        identedit::changeset::hash_text(handle_a["text"].as_str().expect("text should be string"));
    let expected_hash_b =
        identedit::changeset::hash_text(handle_b["text"].as_str().expect("text should be string"));
    let new_text_a = "def process_data(value):\n    return value * 701";
    let new_text_b = "def process_data(value):\n    return value * 702";

    let payload = json!({
        "files": [
            {
                "file": file_a.to_string_lossy().to_string(),
                "operations": [
                    {
                        "target": {
                            "identity": handle_a["identity"],
                            "kind": handle_a["kind"],
                            "span_hint": {"start": span_a["start"], "end": span_a["end"]},
                            "expected_old_hash": expected_hash_a
                        },
                        "op": {"type": "replace", "new_text": new_text_a},
                        "preview": {
                            "old_text": handle_a["text"],
                            "new_text": new_text_a,
                            "matched_span": {"start": span_a["start"], "end": span_a["end"]}
                        }
                    }
                ]
            },
            {
                "file": file_b.to_string_lossy().to_string(),
                "operations": [
                    {
                        "target": {
                            "identity": handle_b["identity"],
                            "kind": handle_b["kind"],
                            "span_hint": {"start": span_b["start"], "end": span_b["end"]},
                            "expected_old_hash": expected_hash_b
                        },
                        "op": {"type": "replace", "new_text": new_text_b},
                        "preview": {
                            "old_text": handle_b["text"],
                            "new_text": new_text_b,
                            "matched_span": {"start": span_b["start"], "end": span_b["end"]}
                        }
                    }
                ]
            }
        ],
        "transaction": {
            "mode": "all_or_nothing"
        }
    });

    let output = run_identedit_with_raw_stdin_and_env(
        &["apply", "--inject-failure-after-writes", "99"],
        payload.to_string().as_bytes(),
        &[("IDENTEDIT_EXPERIMENTAL", "1")],
    );
    assert!(
        output.status.success(),
        "injection count above committed write count should not fail apply: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["transaction"]["status"], "committed");
    assert_eq!(response["summary"]["operations_applied"], 2);
    assert_eq!(response["summary"]["operations_failed"], 0);

    let after_a = fs::read_to_string(&file_a).expect("file_a should remain readable");
    let after_b = fs::read_to_string(&file_b).expect("file_b should remain readable");
    assert_ne!(after_a, before_a, "file_a should be updated");
    assert_ne!(after_b, before_b, "file_b should be updated");
    assert!(after_a.contains("return value * 701"));
    assert!(after_b.contains("return value * 702"));
}

#[test]
fn apply_multi_file_mid_commit_guard_failure_rolls_back_already_committed_files() {
    let mut saw_expected_failure = false;
    let mut last_attempt_message = String::new();

    for attempt in 1..=6 {
        let file_a = copy_fixture_to_temp_python("example.py");
        let file_b = copy_fixture_to_temp_python("example.py");
        let before_a = fs::read_to_string(&file_a).expect("file_a should be readable");
        let before_b = fs::read_to_string(&file_b).expect("file_b should be readable");
        let original_mtime_b = fs::metadata(&file_b)
            .expect("file_b metadata should be readable")
            .modified()
            .expect("file_b mtime should be readable");

        let handle_a = select_named_handle(&file_a, "process_*");
        let handle_b = select_named_handle(&file_b, "process_*");
        let span_a = &handle_a["span"];
        let span_b = &handle_b["span"];
        let new_text_a =
            "def process_data(value):\n    return value * 401\n# rollback_probe_mid_commit";
        let new_text_b = "def process_data(value):\n    return value * 402";
        let expected_hash_a = identedit::changeset::hash_text(
            handle_a["text"].as_str().expect("text should be string"),
        );
        let expected_hash_b = identedit::changeset::hash_text(
            handle_b["text"].as_str().expect("text should be string"),
        );

        let payload = json!({
            "files": [
                {
                    "file": file_a.to_string_lossy().to_string(),
                    "operations": [
                        {
                            "target": {
                                "identity": handle_a["identity"],
                                "kind": handle_a["kind"],
                                "span_hint": {"start": span_a["start"], "end": span_a["end"]},
                                "expected_old_hash": expected_hash_a
                            },
                            "op": {"type": "replace", "new_text": new_text_a},
                            "preview": {
                                "old_text": handle_a["text"],
                                "new_text": new_text_a,
                                "matched_span": {"start": span_a["start"], "end": span_a["end"]}
                            }
                        }
                    ]
                },
                {
                    "file": file_b.to_string_lossy().to_string(),
                    "operations": [
                        {
                            "target": {
                                "identity": handle_b["identity"],
                                "kind": handle_b["kind"],
                                "span_hint": {"start": span_b["start"], "end": span_b["end"]},
                                "expected_old_hash": expected_hash_b
                            },
                            "op": {"type": "replace", "new_text": new_text_b},
                            "preview": {
                                "old_text": handle_b["text"],
                                "new_text": new_text_b,
                                "matched_span": {"start": span_b["start"], "end": span_b["end"]}
                            }
                        }
                    ]
                }
            ],
            "transaction": {
                "mode": "all_or_nothing"
            }
        });
        let payload_bytes = payload.to_string().into_bytes();

        let stale_text_b = before_b.replace("value + 1", "value + 2");
        assert_eq!(
            stale_text_b.len(),
            before_b.len(),
            "stale mutation should preserve byte length to exercise hash guard path"
        );
        let stale_text_for_thread = stale_text_b.clone();
        let file_a_for_thread = file_a.clone();
        let file_b_for_thread = file_b.clone();
        let sabotage = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(3);
            while Instant::now() < deadline {
                if let Ok(contents) = fs::read_to_string(&file_a_for_thread)
                    && contents.contains("rollback_probe_mid_commit")
                {
                    fs::write(&file_b_for_thread, &stale_text_for_thread)
                        .expect("sabotage should rewrite second file");
                    let handle = OpenOptions::new()
                        .read(true)
                        .write(true)
                        .open(&file_b_for_thread)
                        .expect("sabotage should reopen second file");
                    handle
                        .set_times(FileTimes::new().set_modified(original_mtime_b))
                        .expect("sabotage should restore second file mtime");
                    return true;
                }
                thread::sleep(Duration::from_millis(2));
            }
            false
        });

        let output = run_identedit_with_raw_stdin(&["apply"], &payload_bytes);
        let sabotage_triggered = sabotage.join().expect("sabotage thread should not panic");

        if !sabotage_triggered {
            last_attempt_message = format!("attempt {attempt}: sabotage did not trigger");
            continue;
        }

        if output.status.success() {
            last_attempt_message = format!("attempt {attempt}: apply succeeded unexpectedly");
            continue;
        }

        let response: Value =
            serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
        let error_type = response["error"]["type"]
            .as_str()
            .expect("error.type should be string");
        if error_type != "precondition_failed" && error_type != "path_changed" {
            last_attempt_message = format!(
                "attempt {attempt}: expected precondition/path guard failure, got {error_type}"
            );
            continue;
        }

        let after_a = fs::read_to_string(&file_a).expect("file_a should remain readable");
        let after_b = fs::read_to_string(&file_b).expect("file_b should remain readable");
        if after_a != before_a {
            last_attempt_message =
                format!("attempt {attempt}: file_a rollback did not restore original text");
            continue;
        }
        if after_b != stale_text_b {
            last_attempt_message = format!(
                "attempt {attempt}: file_b expected external stale content after guard failure"
            );
            continue;
        }

        saw_expected_failure = true;
        break;
    }

    assert!(
        saw_expected_failure,
        "mid-commit guard rollback scenario did not materialize within retry budget: {last_attempt_message}"
    );
}

#[test]
fn apply_multi_file_lock_contention_fails_without_writing_unlocked_files() {
    let file_a = copy_fixture_to_temp_python("example.py");
    let file_b = copy_fixture_to_temp_python("example.py");
    let before_a = fs::read_to_string(&file_a).expect("file_a should be readable");
    let before_b = fs::read_to_string(&file_b).expect("file_b should be readable");

    let handle_a = select_named_handle(&file_a, "process_*");
    let handle_b = select_named_handle(&file_b, "process_*");
    let span_a = &handle_a["span"];
    let span_b = &handle_b["span"];
    let new_text_a = "def process_data(value):\n    return value * 211";
    let new_text_b = "def process_data(value):\n    return value * 212";
    let expected_hash_a =
        identedit::changeset::hash_text(handle_a["text"].as_str().expect("text should be string"));
    let expected_hash_b =
        identedit::changeset::hash_text(handle_b["text"].as_str().expect("text should be string"));

    let contention_lock = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&file_b)
        .expect("file_b should be lockable for contention setup");
    contention_lock
        .try_lock_exclusive()
        .expect("contention lock should be held");

    let payload = json!({
        "files": [
            {
                "file": file_a.to_string_lossy().to_string(),
                "operations": [
                    {
                        "target": {
                            "identity": handle_a["identity"],
                            "kind": handle_a["kind"],
                            "span_hint": {"start": span_a["start"], "end": span_a["end"]},
                            "expected_old_hash": expected_hash_a
                        },
                        "op": {"type": "replace", "new_text": new_text_a},
                        "preview": {
                            "old_text": handle_a["text"],
                            "new_text": new_text_a,
                            "matched_span": {"start": span_a["start"], "end": span_a["end"]}
                        }
                    }
                ]
            },
            {
                "file": file_b.to_string_lossy().to_string(),
                "operations": [
                    {
                        "target": {
                            "identity": handle_b["identity"],
                            "kind": handle_b["kind"],
                            "span_hint": {"start": span_b["start"], "end": span_b["end"]},
                            "expected_old_hash": expected_hash_b
                        },
                        "op": {"type": "replace", "new_text": new_text_b},
                        "preview": {
                            "old_text": handle_b["text"],
                            "new_text": new_text_b,
                            "matched_span": {"start": span_b["start"], "end": span_b["end"]}
                        }
                    }
                ]
            }
        ],
        "transaction": {
            "mode": "all_or_nothing"
        }
    });

    let output = run_identedit_with_raw_stdin(&["apply"], payload.to_string().as_bytes());
    drop(contention_lock);
    assert!(
        !output.status.success(),
        "lock contention should fail multi-file apply"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "resource_busy");

    let after_a = fs::read_to_string(&file_a).expect("file_a should remain readable");
    let after_b = fs::read_to_string(&file_b).expect("file_b should remain readable");
    assert_eq!(after_a, before_a, "lock contention must not write file_a");
    assert_eq!(after_b, before_b, "lock contention must not write file_b");
}

#[test]
fn apply_multi_file_commit_io_failure_rolls_back_already_committed_files() {
    let mut saw_expected_failure = false;
    let mut last_attempt_message = String::new();

    for attempt in 1..=12 {
        let file_a = copy_fixture_to_temp_python("example.py");
        let file_b = copy_fixture_to_temp_python("example.py");
        let before_a = fs::read_to_string(&file_a).expect("file_a should be readable");

        let handle_a = select_named_handle(&file_a, "process_*");
        let handle_b = select_named_handle(&file_b, "process_*");
        let span_a = &handle_a["span"];
        let span_b = &handle_b["span"];
        let new_text_a = "def process_data(value):\n    return value * 301\n# rollback_probe";
        let new_text_b = "def process_data(value):\n    return value * 302";
        let expected_hash_a = identedit::changeset::hash_text(
            handle_a["text"].as_str().expect("text should be string"),
        );
        let expected_hash_b = identedit::changeset::hash_text(
            handle_b["text"].as_str().expect("text should be string"),
        );

        let payload = json!({
            "files": [
                {
                    "file": file_a.to_string_lossy().to_string(),
                    "operations": [
                        {
                            "target": {
                                "identity": handle_a["identity"],
                                "kind": handle_a["kind"],
                                "span_hint": {"start": span_a["start"], "end": span_a["end"]},
                                "expected_old_hash": expected_hash_a
                            },
                            "op": {"type": "replace", "new_text": new_text_a},
                            "preview": {
                                "old_text": handle_a["text"],
                                "new_text": new_text_a,
                                "matched_span": {"start": span_a["start"], "end": span_a["end"]}
                            }
                        }
                    ]
                },
                {
                    "file": file_b.to_string_lossy().to_string(),
                    "operations": [
                        {
                            "target": {
                                "identity": handle_b["identity"],
                                "kind": handle_b["kind"],
                                "span_hint": {"start": span_b["start"], "end": span_b["end"]},
                                "expected_old_hash": expected_hash_b
                            },
                            "op": {"type": "replace", "new_text": new_text_b},
                            "preview": {
                                "old_text": handle_b["text"],
                                "new_text": new_text_b,
                                "matched_span": {"start": span_b["start"], "end": span_b["end"]}
                            }
                        }
                    ]
                }
            ],
            "transaction": {
                "mode": "all_or_nothing"
            }
        });
        let payload_bytes = payload.to_string().into_bytes();

        let file_a_for_thread = file_a.clone();
        let file_b_for_thread = file_b.clone();
        let sabotage = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(4);
            while Instant::now() < deadline {
                if let Ok(contents) = fs::read_to_string(&file_a_for_thread)
                    && contents.contains("rollback_probe")
                {
                    fs::remove_file(&file_b_for_thread)
                        .expect("sabotage should remove second file before second commit");
                    return true;
                }
                thread::sleep(Duration::from_millis(2));
            }
            false
        });

        let output = run_identedit_with_raw_stdin(&["apply"], &payload_bytes);
        let sabotage_triggered = sabotage.join().expect("sabotage thread should not panic");

        if !sabotage_triggered {
            last_attempt_message = format!("attempt {attempt}: sabotage did not trigger");
            continue;
        }

        if output.status.success() {
            last_attempt_message = format!("attempt {attempt}: apply succeeded unexpectedly");
            continue;
        }

        let response: Value =
            serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
        if response["error"]["type"] != "io_error" {
            last_attempt_message = format!(
                "attempt {attempt}: expected io_error, got {}",
                response["error"]["type"]
            );
            continue;
        }

        let after_a = fs::read_to_string(&file_a).expect("file_a should remain readable");
        if after_a != before_a {
            last_attempt_message =
                format!("attempt {attempt}: file_a rollback did not restore original text");
            continue;
        }

        if file_b.exists() {
            last_attempt_message = format!(
                "attempt {attempt}: file_b still exists; sabotage/commit timing unexpected"
            );
            continue;
        }

        saw_expected_failure = true;
        break;
    }

    assert!(
        saw_expected_failure,
        "commit-stage io failure scenario did not materialize within retry budget: {last_attempt_message}"
    );
}

#[test]
fn apply_multi_file_same_logical_path_variants_still_reject_without_mutation() {
    let workspace = tempdir().expect("tempdir should be created");
    let target = workspace.path().join("target.py");
    let source =
        fs::read_to_string(fixture_path("example.py")).expect("fixture should be readable");
    fs::write(&target, source).expect("target fixture write should succeed");
    let before = fs::read_to_string(&target).expect("target should be readable");

    let handle = select_named_handle(&target, "process_*");
    let span = &handle["span"];
    let new_text = "def process_data(value):\n    return value * 33";
    let expected_hash =
        identedit::changeset::hash_text(handle["text"].as_str().expect("text should be string"));

    let payload = json!({
        "files": [
            {
                "file": "target.py",
                "operations": [
                    {
                        "target": {
                            "identity": handle["identity"],
                            "kind": handle["kind"],
                            "span_hint": {"start": span["start"], "end": span["end"]},
                            "expected_old_hash": expected_hash
                        },
                        "op": {"type": "replace", "new_text": new_text},
                        "preview": {
                            "old_text": handle["text"],
                            "new_text": new_text,
                            "matched_span": {"start": span["start"], "end": span["end"]}
                        }
                    }
                ]
            },
            {
                "file": target.to_string_lossy().to_string(),
                "operations": []
            }
        ],
        "transaction": {
            "mode": "all_or_nothing"
        }
    });
    let payload_json = payload.to_string();

    let output = run_identedit_with_stdin_in_dir(workspace.path(), &["apply"], &payload_json);
    assert!(
        !output.status.success(),
        "apply should reject duplicate logical path variants"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("Duplicate file entry in changeset.files")),
        "expected duplicate logical-path diagnostic"
    );

    let after = fs::read_to_string(&target).expect("target should remain readable");
    assert_eq!(before, after, "target content should remain unchanged");
}

#[test]
fn apply_accepts_empty_changeset_as_noop() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");

    let empty_changeset = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": []
    });
    let output = run_identedit_with_stdin(&["apply"], &empty_changeset.to_string());

    assert!(
        output.status.success(),
        "apply should allow empty changeset: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["summary"]["files_modified"], 0);
    assert_eq!(response["summary"]["operations_applied"], 0);
    assert_eq!(response["summary"]["operations_failed"], 0);

    let after = fs::read_to_string(&file_path).expect("fixture should be readable");
    assert_eq!(before, after, "empty apply must not modify file contents");
}

#[test]
fn apply_response_omits_applied_by_default() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let empty_changeset = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": []
    });

    let output = run_identedit_with_stdin(&["apply"], &empty_changeset.to_string());
    assert!(
        output.status.success(),
        "empty apply should still return structured success response"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["transaction"]["mode"], "all_or_nothing");
    assert_eq!(response["transaction"]["status"], "committed");
    assert!(
        response.get("applied").is_none(),
        "compact apply response should omit detailed per-file entries"
    );
}

#[test]
fn apply_response_verbose_includes_transaction_and_file_status_fields() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let empty_changeset = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": []
    });

    let output = run_identedit_with_stdin(&["apply", "--verbose"], &empty_changeset.to_string());
    assert!(
        output.status.success(),
        "empty apply should still return structured success response"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["transaction"]["mode"], "all_or_nothing");
    assert_eq!(response["transaction"]["status"], "committed");
    assert_eq!(response["applied"][0]["status"], "applied");
}

#[test]
fn apply_empty_changeset_missing_file_returns_io_error() {
    let missing_path =
        std::env::temp_dir().join(format!("identedit-missing-noop-{}.py", std::process::id()));
    if missing_path.exists() {
        fs::remove_file(&missing_path).expect("existing stale temp file should be removable");
    }

    let empty_changeset = json!({
        "file": missing_path.to_string_lossy().to_string(),
        "operations": []
    });
    let output = run_identedit_with_stdin(&["apply"], &empty_changeset.to_string());
    assert!(
        !output.status.success(),
        "apply should fail for missing files even when operations are empty"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "io_error");
}

#[test]
fn apply_relative_changeset_path_depends_on_current_working_directory() {
    let workspace = tempdir().expect("tempdir should be created");
    let nested = workspace.path().join("nested");
    fs::create_dir_all(&nested).expect("nested directory should be created");
    let target = workspace.path().join("target.py");

    let source =
        fs::read_to_string(fixture_path("example.py")).expect("fixture should be readable");
    fs::write(&target, source).expect("target fixture write should succeed");

    let changeset = json!({
        "file": "../target.py",
        "operations": []
    });
    let changeset_json = changeset.to_string();

    let nested_output = run_identedit_with_stdin_in_dir(&nested, &["apply"], &changeset_json);
    assert!(
        nested_output.status.success(),
        "apply should resolve relative changeset path from nested cwd: {}",
        String::from_utf8_lossy(&nested_output.stderr)
    );

    let nested_response: Value =
        serde_json::from_slice(&nested_output.stdout).expect("stdout should be valid JSON");
    assert_eq!(nested_response["summary"]["files_modified"], 0);

    let workspace_output =
        run_identedit_with_stdin_in_dir(workspace.path(), &["apply"], &changeset_json);
    assert!(
        !workspace_output.status.success(),
        "apply should fail when cwd changes and relative path no longer resolves"
    );

    let workspace_response: Value =
        serde_json::from_slice(&workspace_output.stdout).expect("stdout should be valid JSON");
    assert_eq!(workspace_response["error"]["type"], "io_error");
}

#[test]
fn apply_file_mode_relative_changeset_target_depends_on_caller_cwd() {
    let workspace = tempdir().expect("tempdir should be created");
    let nested = workspace.path().join("nested");
    fs::create_dir_all(&nested).expect("nested directory should be created");
    let target = workspace.path().join("target.py");
    fs::write(&target, "def value():\n    return 1\n").expect("target file should be written");

    let changeset_path = workspace.path().join("changeset.json");
    let changeset = json!({
        "files": [
            {
                "file": "target.py",
                "operations": []
            }
        ],
        "transaction": {
            "mode": "all_or_nothing"
        }
    });
    fs::write(&changeset_path, changeset.to_string()).expect("changeset write should succeed");

    let workspace_output = run_identedit_in_dir(workspace.path(), &["apply", "changeset.json"]);
    assert!(
        workspace_output.status.success(),
        "apply should resolve relative target from workspace cwd: {}",
        String::from_utf8_lossy(&workspace_output.stderr)
    );

    let nested_output = run_identedit_in_dir(&nested, &["apply", "../changeset.json"]);
    assert!(
        !nested_output.status.success(),
        "apply should fail when cwd changes and relative target no longer resolves"
    );

    let nested_response: Value =
        serde_json::from_slice(&nested_output.stdout).expect("stdout should be valid JSON");
    assert_eq!(nested_response["error"]["type"], "io_error");
}

#[test]
fn apply_empty_changeset_unsupported_extension_is_noop_success() {
    let mut temp_file = Builder::new()
        .suffix(".txt")
        .tempfile()
        .expect("temp txt file should be created");
    temp_file
        .write_all(b"plain text body\n")
        .expect("temp txt write should succeed");
    let file_path = temp_file.path().to_path_buf();

    let empty_changeset = json!({
        "file": file_path.to_string_lossy().to_string(),
        "operations": []
    });
    let output = run_identedit_with_stdin(&["apply"], &empty_changeset.to_string());
    assert!(
        output.status.success(),
        "fallback should allow no-op apply for unsupported extension: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["summary"]["files_modified"], 0);
    assert_eq!(response["summary"]["operations_applied"], 0);
    assert_eq!(response["summary"]["operations_failed"], 0);
}
