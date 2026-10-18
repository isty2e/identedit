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
