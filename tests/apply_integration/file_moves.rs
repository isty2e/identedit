use super::*;

#[test]
fn apply_json_mode_rejects_multiple_move_operations_per_file() {
    let workspace = tempdir().expect("tempdir should be created");
    let source_path = workspace.path().join("source.py");
    fs::write(&source_path, "def keep():\n    return 1\n").expect("fixture write should succeed");
    let destination_a = workspace.path().join("renamed_a.py");
    let destination_b = workspace.path().join("renamed_b.py");

    let request = json!({
        "command": "apply",
        "changeset": {
            "files": [
                {
                    "file": source_path.to_string_lossy().to_string(),
                    "operations": [
                        {
                            "target": file_move_target(&source_path),
                            "op": {
                                "type": "move",
                                "to": destination_a.to_string_lossy().to_string()
                            },
                            "preview": file_move_preview(source_path.to_string_lossy().to_string(), destination_a.to_string_lossy().to_string())
                        },
                        {
                            "target": file_move_target(&source_path),
                            "op": {
                                "type": "move",
                                "to": destination_b.to_string_lossy().to_string()
                            },
                            "preview": file_move_preview(source_path.to_string_lossy().to_string(), destination_b.to_string_lossy().to_string())
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
        "apply should reject multiple move operations in a single file change"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| {
                message.contains("Only one move operation is allowed per file")
            }),
        "expected explicit multiple-move validation error"
    );
}
#[test]
fn apply_json_mode_rejects_move_mixed_with_content_edits_for_same_file() {
    let workspace = tempdir().expect("tempdir should be created");
    let source_path = workspace.path().join("source.py");
    fs::write(&source_path, "def keep():\n    return 1\n").expect("fixture write should succeed");
    let destination = workspace.path().join("renamed.py");

    let request = json!({
        "command": "apply",
        "changeset": {
            "files": [
                {
                    "file": source_path.to_string_lossy().to_string(),
                    "operations": [
                        {
                            "target": file_move_target(&source_path),
                            "op": {
                                "type": "move",
                                "to": destination.to_string_lossy().to_string()
                            },
                            "preview": file_move_preview(source_path.to_string_lossy().to_string(), destination.to_string_lossy().to_string())
                        },
                        {
                            "target": {
                                "identity": "unused-identity-edit",
                                "kind": "function_definition",
                                "expected_old_hash": "ffffffffffffffff"
                            },
                            "op": {
                                "type": "replace",
                                "new_text": "def keep():\n    return 2\n"
                            },
                            "preview": {
                                "old_text": "def keep():\n    return 1\n",
                                "new_text": "def keep():\n    return 2\n",
                                "matched_span": {
                                    "start": 0,
                                    "end": 1
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
        "apply should reject move + content-edit mix within the same file change"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| {
                message.contains(
                    "Move cannot be combined with content-edit operations for the same file",
                )
            }),
        "expected move/edit mix validation error"
    );
}
#[test]
fn apply_json_mode_executes_single_move_operation() {
    let workspace = tempdir().expect("tempdir should be created");
    let source_path = workspace.path().join("source.py");
    fs::write(&source_path, "def keep():\n    return 1\n").expect("fixture write should succeed");
    let destination = workspace.path().join("renamed.py");

    let request = json!({
        "command": "apply",
        "changeset": {
            "files": [
                {
                    "file": source_path.to_string_lossy().to_string(),
                    "operations": [
                        {
                            "target": file_move_target(&source_path),
                            "op": {
                                "type": "move",
                                "to": destination.to_string_lossy().to_string()
                            },
                            "preview": file_move_preview(source_path.to_string_lossy().to_string(), destination.to_string_lossy().to_string())
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
        output.status.success(),
        "single move should execute successfully: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["transaction"]["status"], "committed");
    assert_eq!(response["summary"]["files_modified"], 1);
    assert_eq!(response["summary"]["operations_applied"], 1);
    assert_eq!(response["summary"]["operations_failed"], 0);
    assert!(
        !source_path.exists(),
        "source path should be moved away after successful move apply"
    );
    let destination_text =
        fs::read_to_string(&destination).expect("destination should contain moved source content");
    assert!(
        destination_text.contains("def keep():"),
        "destination file should contain original source text"
    );
}
#[test]
fn apply_json_mode_rejects_legacy_move_preview_without_mutating_files() {
    let workspace = tempdir().expect("tempdir should be created");
    let source_path = workspace.path().join("source.py");
    let source_text = "def keep():\n    return 1\n";
    fs::write(&source_path, source_text).expect("fixture write should succeed");
    let destination = workspace.path().join("renamed.py");

    let request = json!({
        "command": "apply",
        "changeset": {
            "files": [
                {
                    "file": source_path.to_string_lossy().to_string(),
                    "operations": [
                        {
                            "target": file_move_target(&source_path),
                            "op": {
                                "type": "move",
                                "to": destination.to_string_lossy().to_string()
                            },
                            "preview": {
                                "old_text": "",
                                "new_text": "",
                                "matched_span": { "start": 0, "end": 0 }
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
        "legacy move preview must be rejected at the CLI boundary"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let message = response["error"]["message"]
        .as_str()
        .expect("error should include a diagnostic message");
    assert!(message.contains("requires 'move' with 'from' and 'to'"));
    assert!(message.contains("regenerate the changeset"));
    assert_eq!(
        fs::read_to_string(&source_path).expect("source should remain readable"),
        source_text
    );
    assert!(
        !destination.exists(),
        "rejected legacy input must not create the destination"
    );
}
#[test]
fn apply_json_mode_move_graph_rejects_self_move() {
    let workspace = tempdir().expect("tempdir should be created");
    let source_path = workspace.path().join("source.py");
    fs::write(&source_path, "def keep():\n    return 1\n").expect("fixture write should succeed");

    let request = json!({
        "command": "apply",
        "changeset": {
            "files": [
                {
                    "file": source_path.to_string_lossy().to_string(),
                    "operations": [
                        {
                            "target": file_move_target(&source_path),
                            "op": {
                                "type": "move",
                                "to": source_path.to_string_lossy().to_string()
                            },
                            "preview": file_move_preview(source_path.to_string_lossy().to_string(), source_path.to_string_lossy().to_string())
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
        "self-move should be rejected during move graph validation"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("self-move")),
        "expected self-move validation error"
    );
}
#[test]
fn apply_json_mode_move_graph_rejects_duplicate_destination_paths() {
    let workspace = tempdir().expect("tempdir should be created");
    let source_a = workspace.path().join("a.py");
    let source_b = workspace.path().join("b.py");
    fs::write(&source_a, "def a():\n    return 1\n").expect("fixture write should succeed");
    fs::write(&source_b, "def b():\n    return 2\n").expect("fixture write should succeed");
    let destination = workspace.path().join("renamed.py");

    let request = json!({
        "command": "apply",
        "changeset": {
            "files": [
                {
                    "file": source_a.to_string_lossy().to_string(),
                    "operations": [
                        {
                            "target": file_move_target(&source_a),
                            "op": {
                                "type": "move",
                                "to": destination.to_string_lossy().to_string()
                            },
                            "preview": file_move_preview(source_a.to_string_lossy().to_string(), destination.to_string_lossy().to_string())
                        }
                    ]
                },
                {
                    "file": source_b.to_string_lossy().to_string(),
                    "operations": [
                        {
                            "target": file_move_target(&source_b),
                            "op": {
                                "type": "move",
                                "to": destination.to_string_lossy().to_string()
                            },
                            "preview": file_move_preview(source_b.to_string_lossy().to_string(), destination.to_string_lossy().to_string())
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
        "duplicate move destinations should be rejected"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("Duplicate move destination")),
        "expected duplicate destination validation error"
    );
}
#[test]
fn apply_json_mode_move_graph_rejects_existing_destination_when_not_chain() {
    let workspace = tempdir().expect("tempdir should be created");
    let source = workspace.path().join("source.py");
    let destination = workspace.path().join("existing.py");
    fs::write(&source, "def source():\n    return 1\n").expect("fixture write should succeed");
    fs::write(&destination, "def existing():\n    return 2\n")
        .expect("fixture write should succeed");

    let request = json!({
        "command": "apply",
        "changeset": {
            "files": [
                {
                    "file": source.to_string_lossy().to_string(),
                    "operations": [
                        {
                            "target": file_move_target(&source),
                            "op": {
                                "type": "move",
                                "to": destination.to_string_lossy().to_string()
                            },
                            "preview": file_move_preview(source.to_string_lossy().to_string(), destination.to_string_lossy().to_string())
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
        "existing destination without chain source should be rejected"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| { message.contains("Destination path already exists") }),
        "expected overwrite-policy validation error"
    );
}
#[test]
fn apply_json_mode_move_graph_rejects_cycle() {
    let workspace = tempdir().expect("tempdir should be created");
    let source_a = workspace.path().join("a.py");
    let source_b = workspace.path().join("b.py");
    fs::write(&source_a, "def a():\n    return 1\n").expect("fixture write should succeed");
    fs::write(&source_b, "def b():\n    return 2\n").expect("fixture write should succeed");

    let request = json!({
        "command": "apply",
        "changeset": {
            "files": [
                {
                    "file": source_a.to_string_lossy().to_string(),
                    "operations": [
                        {
                            "target": file_move_target(&source_a),
                            "op": {
                                "type": "move",
                                "to": source_b.to_string_lossy().to_string()
                            },
                            "preview": file_move_preview(source_a.to_string_lossy().to_string(), source_b.to_string_lossy().to_string())
                        }
                    ]
                },
                {
                    "file": source_b.to_string_lossy().to_string(),
                    "operations": [
                        {
                            "target": file_move_target(&source_b),
                            "op": {
                                "type": "move",
                                "to": source_a.to_string_lossy().to_string()
                            },
                            "preview": file_move_preview(source_b.to_string_lossy().to_string(), source_a.to_string_lossy().to_string())
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
        "move cycles should be rejected by graph validation"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| {
                message.contains(
                    "Move graph contains a cycle; move operations must form an acyclic chain",
                )
            }),
        "expected explicit cycle validation error"
    );
}
#[test]
fn apply_json_mode_move_graph_executes_chain_in_reverse_topological_order() {
    let workspace = tempdir().expect("tempdir should be created");
    let source_a = workspace.path().join("a.py");
    let source_b = workspace.path().join("b.py");
    let destination_c = workspace.path().join("c.py");
    fs::write(&source_a, "def from_a():\n    return 'a'\n").expect("fixture write should succeed");
    fs::write(&source_b, "def from_b():\n    return 'b'\n").expect("fixture write should succeed");

    let request = json!({
        "command": "apply",
        "changeset": {
            "files": [
                {
                    "file": source_a.to_string_lossy().to_string(),
                    "operations": [
                        {
                            "target": file_move_target(&source_a),
                            "op": {
                                "type": "move",
                                "to": source_b.to_string_lossy().to_string()
                            },
                            "preview": file_move_preview(source_a.to_string_lossy().to_string(), source_b.to_string_lossy().to_string())
                        }
                    ]
                },
                {
                    "file": source_b.to_string_lossy().to_string(),
                    "operations": [
                        {
                            "target": file_move_target(&source_b),
                            "op": {
                                "type": "move",
                                "to": destination_c.to_string_lossy().to_string()
                            },
                            "preview": file_move_preview(source_b.to_string_lossy().to_string(), destination_c.to_string_lossy().to_string())
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
        output.status.success(),
        "move chain should execute successfully: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["transaction"]["status"], "committed");
    assert_eq!(response["summary"]["files_modified"], 2);
    assert_eq!(response["summary"]["operations_applied"], 2);
    assert_eq!(response["summary"]["operations_failed"], 0);

    assert!(
        !source_a.exists(),
        "first chain source should no longer exist after successful move commit"
    );
    assert!(
        source_b.exists(),
        "intermediate chain path should be recreated as destination of the second move"
    );
    assert!(
        destination_c.exists(),
        "final chain destination should exist after successful move commit"
    );

    let moved_to_b =
        fs::read_to_string(&source_b).expect("intermediate destination should be readable");
    let moved_to_c =
        fs::read_to_string(&destination_c).expect("final destination should be readable");
    assert!(
        moved_to_b.contains("from_a"),
        "a->b should happen after b->c so b ends with source_a content"
    );
    assert!(
        moved_to_c.contains("from_b"),
        "b->c should run first so c ends with original source_b content"
    );
}
#[test]
fn apply_json_mode_move_rejects_duplicate_source_alias_paths() {
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
                            "target": file_move_target(&source_path),
                            "op": {
                                "type": "move",
                                "to": "renamed_a.py"
                            },
                            "preview": file_move_preview("source.py", "renamed_a.py")
                        }
                    ]
                },
                {
                    "file": "./source.py",
                    "operations": [
                        {
                            "target": file_move_target(&source_path),
                            "op": {
                                "type": "move",
                                "to": "renamed_b.py"
                            },
                            "preview": file_move_preview("./source.py", "renamed_b.py")
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
        "duplicate canonical source paths should be rejected"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("Duplicate move source path")),
        "expected duplicate move source validation error"
    );
    assert!(source_path.exists(), "source file should remain untouched");
    assert!(
        !workspace.path().join("renamed_a.py").exists(),
        "no destination should be created on validation failure"
    );
    assert!(
        !workspace.path().join("renamed_b.py").exists(),
        "no destination should be created on validation failure"
    );
}
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
                            "target": file_move_target(&source_path),
                            "op": {
                                "type": "move",
                                "to": symlink_destination.to_string_lossy().to_string()
                            },
                            "preview": file_move_preview(source_path.to_string_lossy().to_string(), symlink_destination.to_string_lossy().to_string())
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
                            "target": file_move_target(&source_path),
                            "op": {
                                "type": "move",
                                "to": "./renamed.py"
                            },
                            "preview": file_move_preview("source.py", "./renamed.py")
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
    let source_path = workspace.path().join("source.py");
    fs::write(&source_path, "def keep():\n    return 1\n").expect("fixture write should succeed");

    let request = json!({
        "command": "apply",
        "changeset": {
            "files": [
                {
                    "file": "./source.py",
                    "operations": [
                        {
                            "target": file_move_target(&source_path),
                            "op": {
                                "type": "move",
                                "to": "nested/../source.py"
                            },
                            "preview": file_move_preview("./source.py", "nested/../source.py")
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
                            "target": file_move_target(&source_path),
                            "op": {
                                "type": "move",
                                "to": "nested/renamed.py"
                            },
                            "preview": file_move_preview("source.py", "nested/renamed.py")
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
                            "target": file_move_target(&source_path),
                            "op": {
                                "type": "move",
                                "to": workspace.path().join("not_a_directory/renamed.py").to_string_lossy().to_string()
                            },
                            "preview": file_move_preview(source_path.to_string_lossy().to_string(), workspace.path().join("not_a_directory/renamed.py").to_string_lossy().to_string())
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
    let source_a = workspace.path().join("a.py");
    let source_b = workspace.path().join("b.py");
    fs::write(&source_a, "def from_a():\n    return 'a'\n").expect("fixture write should succeed");
    fs::write(&source_b, "def from_b():\n    return 'b'\n").expect("fixture write should succeed");

    let request = json!({
        "command": "apply",
        "changeset": {
            "files": [
                {
                    "file": "a.py",
                    "operations": [
                        {
                            "target": file_move_target(&source_a),
                            "op": {
                                "type": "move",
                                "to": "renamed.py"
                            },
                            "preview": file_move_preview("a.py", "renamed.py")
                        }
                    ]
                },
                {
                    "file": "b.py",
                    "operations": [
                        {
                            "target": file_move_target(&source_b),
                            "op": {
                                "type": "move",
                                "to": "./renamed.py"
                            },
                            "preview": file_move_preview("b.py", "./renamed.py")
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
    let source_a = workspace.path().join("a.py");
    let source_b = workspace.path().join("b.py");
    fs::write(&source_a, "def from_a():\n    return 'a'\n").expect("fixture write should succeed");
    fs::write(&source_b, "def from_b():\n    return 'b'\n").expect("fixture write should succeed");

    let request = json!({
        "command": "apply",
        "changeset": {
            "files": [
                {
                    "file": "a.py",
                    "operations": [
                        {
                            "target": file_move_target(&source_a),
                            "op": {
                                "type": "move",
                                "to": "./b.py"
                            },
                            "preview": file_move_preview("a.py", "./b.py")
                        }
                    ]
                },
                {
                    "file": "b.py",
                    "operations": [
                        {
                            "target": file_move_target(&source_b),
                            "op": {
                                "type": "move",
                                "to": "nested/../a.py"
                            },
                            "preview": file_move_preview("b.py", "nested/../a.py")
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
    let source_a = workspace.path().join("a.py");
    let source_b = workspace.path().join("b.py");
    fs::write(&source_a, "def from_a():\n    return 'a'\n").expect("fixture write should succeed");
    fs::write(&source_b, "def from_b():\n    return 'b'\n").expect("fixture write should succeed");

    let request = json!({
        "command": "apply",
        "changeset": {
            "files": [
                {
                    "file": "a.py",
                    "operations": [
                        {
                            "target": file_move_target(&source_a),
                            "op": {
                                "type": "move",
                                "to": "./b.py"
                            },
                            "preview": file_move_preview("a.py", "./b.py")
                        }
                    ]
                },
                {
                    "file": "b.py",
                    "operations": [
                        {
                            "target": file_move_target(&source_b),
                            "op": {
                                "type": "move",
                                "to": "./c.py"
                            },
                            "preview": file_move_preview("b.py", "./c.py")
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
                            "target": file_move_target(&source_path),
                            "op": {
                                "type": "move",
                                "to": "nested/../renamed.py"
                            },
                            "preview": file_move_preview("source.py", "nested/../renamed.py")
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
