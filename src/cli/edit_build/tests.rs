use std::fs;

use serde_json::json;
use tempfile::tempdir;

use super::*;

#[test]
fn builder_reread_revalidates_the_guard_after_query_and_regex_lowering() {
    let root = tempdir().unwrap();
    let file = root.path().join("example.py");
    let original = "def target():\n    return 1\n";
    let changed = "def target():\n    return 2\n";

    for op in [
        json!({"type":"replace","new_text":"def target():\n    return 3"}),
        json!({"type":"scoped_regex","pattern":"return 1","replacement":"return 3"}),
    ] {
        fs::write(&file, original).unwrap();
        let context = ExecutionContext::new();
        let request = serde_json::from_value(json!({"command":"edit","file":file,"operations":[{
            "target":{"type":"symbol","name":"target"},"op":op
        }]}))
        .unwrap();
        let entries = parse_stdin_edit_shape(request).unwrap();
        let bucket = normalize_edit_file_requests(entries, &context)
            .unwrap()
            .remove(0);
        let original_target = bucket.instructions[0].operation.target().clone();
        let instructions = bucket
            .instructions
            .into_iter()
            .map(|item| item.operation)
            .collect();

        fs::write(&file, changed).unwrap();
        let failure =
            build_changeset_with_context_indexed(&file, instructions, None, &context).unwrap_err();
        assert!(matches!(
            failure.error,
            IdenteditError::PreconditionFailed { .. }
        ));
        assert_eq!(failure.operation_indices, vec![0]);
        let TransformTarget::Node {
            expected_old_hash, ..
        } = original_target
        else {
            panic!("query must lower to a guarded node")
        };
        assert_eq!(expected_old_hash, hash_text("def target():\n    return 1"));
        assert_eq!(fs::read_to_string(&file).unwrap(), changed);
    }
}

#[test]
fn selection_observation_keeps_diagnostic_source_and_handle_spans_together() {
    let root = tempdir().unwrap();
    let file = root.path().join("example.py");
    let before =
        "\u{feff}# λ\r\n\r\ndef target():\r\n    return 1\r\n\r\ndef target():\r\n    return 2\r\n";
    fs::write(&file, before).unwrap();
    let context = ExecutionContext::new();
    let mut state = NormalizeState::new(&context);
    let error = state
        .resolve_query(&file, NodeTargetSelector::Symbol("target".to_string()))
        .unwrap_err();
    let IdenteditError::AmbiguousTargetSelector {
        candidates,
        candidate_contexts,
        ..
    } = error
    else {
        panic!("expected contextual ambiguity")
    };
    assert_eq!(candidates, 2);
    assert_eq!(
        candidate_contexts
            .iter()
            .map(|candidate| candidate.line)
            .collect::<Vec<_>>(),
        vec![3, 6]
    );
    for candidate in candidate_contexts {
        let selected = &before[candidate.span.start..candidate.span.end];
        assert_eq!(hash_text(selected).to_string(), candidate.expected_old_hash);
    }
}

#[test]
fn config_lowering_retains_the_source_condition_for_the_builders_actual_reread() {
    let root = tempdir().unwrap();
    let file = root.path().join("settings.json");
    let original = "{\"inside\":{\"a\":1,\"b\":2},\"outside\":0}";
    let changed = "{\"inside\":{\"a\":1,\"b\":2},\"outside\":9}";
    fs::write(&file, original).unwrap();
    let context = ExecutionContext::new();
    let request = serde_json::from_value(json!({"command":"edit","file":file,"operations":[
        {"target":{"type":"config_path","path":"inside.a"},"op":{"type":"set","new_text":"10"}},
        {"target":{"type":"config_path","path":"inside.b"},"op":{"type":"set","new_text":"20"}}
    ]}))
    .unwrap();
    let entries = parse_stdin_edit_shape(request).unwrap();
    let bucket = normalize_edit_file_requests(entries, &context)
        .unwrap()
        .remove(0);
    assert_eq!(bucket.expected_file_hash, Some(hash_text(original)));
    let instructions = bucket
        .instructions
        .into_iter()
        .map(|item| item.operation)
        .collect();

    fs::write(&file, changed).unwrap();
    let failure = build_changeset_with_context_indexed(
        &file,
        instructions,
        bucket.expected_file_hash,
        &context,
    )
    .unwrap_err();
    assert!(matches!(
        failure.error,
        IdenteditError::FilePreconditionFailed { .. }
    ));
    assert!(failure.operation_indices.is_empty());
    assert_eq!(fs::read_to_string(&file).unwrap(), changed);
}

#[test]
fn config_context_replaces_an_inferred_origin_for_builder_file_failures() {
    let root = tempdir().unwrap();
    let file = root.path().join("settings.json");
    let original = "{\"inside\":{\"a\":1},\"outside\":0}";
    let changed = "{\"inside\":{\"a\":1},\"outside\":9}";
    fs::write(&file, original).unwrap();
    let context = ExecutionContext::new();
    let mut state = NormalizeState::new(&context);
    let target = state
        .resolve_query(
            &file,
            NodeTargetSelector::Selector {
                kind: "number".to_string(),
                name_pattern: "outside".to_string(),
            },
        )
        .unwrap();
    let source_move = OperationOrigin {
        file_index: Some(0),
        operation_index: 0,
    };
    let config_origin = OperationOrigin {
        file_index: Some(1),
        operation_index: 0,
    };
    let pending = PendingFileInstructions {
        file: file.clone(),
        instructions: vec![
            PendingInstruction::Addressed(LocatedInstruction {
                operation: EditOperation::try_new(
                    target,
                    OpKind::InsertBefore {
                        new_text: String::new(),
                    },
                )
                .unwrap(),
                origin: source_move,
            }),
            PendingInstruction::Config {
                request: ConfigPathRequest {
                    path: "inside.a".to_string(),
                    expected_file_hash: None,
                    document_index: None,
                    operation: ConfigPathOperation::Set {
                        new_text: "10".to_string(),
                        missing_path: MissingPathPolicy::Reject,
                    },
                },
                origin: config_origin,
            },
        ],
        request_pointer: source_move.pointer(),
    };
    let bucket = compile_file_bucket(pending).unwrap();
    assert_eq!(bucket.request_pointer, "/files/1");
    let instructions = bucket
        .instructions
        .into_iter()
        .map(|item| item.operation)
        .collect();
    fs::write(&file, changed).unwrap();
    let failure = build_changeset_with_context_indexed(
        &file,
        instructions,
        bucket.expected_file_hash,
        &context,
    )
    .unwrap_err();
    assert!(matches!(
        failure.error,
        IdenteditError::FilePreconditionFailed { .. }
    ));
    assert!(failure.operation_indices.is_empty());
    assert_eq!(fs::read_to_string(&file).unwrap(), changed);
}
