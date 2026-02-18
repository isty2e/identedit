use std::path::{Path, PathBuf};

use crate::changeset::{OpKind, hash_text};

use super::{
    MatchedChange, TransformTarget, reject_move_operation, resolve_target_in_handles,
    validate_change_conflicts,
};
use crate::error::IdenteditError;
use crate::handle::{SelectionHandle, Span};

fn handle(span: Span, kind: &str, name: Option<&str>, text: &str) -> SelectionHandle {
    SelectionHandle::from_parts(
        PathBuf::from("fixture.py"),
        span,
        kind.to_string(),
        name.map(ToString::to_string),
        text.to_string(),
    )
}

fn matched_change(
    index: usize,
    op: OpKind,
    matched_span: Span,
    anchor_span: Span,
    anchor_kind: &str,
) -> MatchedChange {
    MatchedChange {
        index,
        target: TransformTarget::node(
            format!("id-{index}"),
            anchor_kind.to_string(),
            Some(anchor_span),
            "hash".to_string(),
        ),
        op,
        expected_hash: "hash".to_string(),
        old_text: String::new(),
        matched_span,
        move_insert_at: None,
        anchor_kind: anchor_kind.to_string(),
        anchor_span,
    }
}

#[test]
fn resolve_target_returns_target_missing_when_span_hint_hits_wrong_kind_only() {
    let handles = vec![handle(
        Span { start: 4, end: 28 },
        "class_definition",
        Some("Processor"),
        "class Processor:\n    pass",
    )];
    let target = TransformTarget::node(
        "missing-identity".to_string(),
        "function_definition".to_string(),
        Some(Span { start: 4, end: 28 }),
        "irrelevant".to_string(),
    );

    let error = resolve_target_in_handles(Path::new("fixture.py"), &handles, &target)
        .expect_err("wrong-kind stale hint should not resolve");

    match error {
        IdenteditError::TargetMissing { identity, file } => {
            assert_eq!(identity, "missing-identity");
            assert_eq!(file, "fixture.py");
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn resolve_target_returns_precondition_failed_for_unique_stale_span_hint_candidate() {
    let stale = handle(
        Span { start: 0, end: 24 },
        "function_definition",
        Some("process_data"),
        "def process_data(): pass",
    );
    let handles = vec![stale.clone()];
    let target = TransformTarget::node(
        "stale-identity".to_string(),
        "function_definition".to_string(),
        Some(stale.span),
        "stale-hash".to_string(),
    );

    let error = resolve_target_in_handles(Path::new("fixture.py"), &handles, &target)
        .expect_err("stale span hint should return precondition failure");

    match error {
        IdenteditError::PreconditionFailed {
            expected_hash,
            actual_hash,
        } => {
            assert_eq!(expected_hash, "stale-hash");
            assert_eq!(actual_hash, hash_text(&stale.text));
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn resolve_target_returns_ambiguous_when_stale_span_hint_matches_multiple_candidates() {
    let first = handle(
        Span { start: 0, end: 20 },
        "function_definition",
        Some("first"),
        "def first():\n    pass",
    );
    let mut second = handle(
        Span { start: 30, end: 52 },
        "function_definition",
        Some("second"),
        "def second():\n    pass",
    );
    second.span = first.span;
    let handles = vec![first, second];
    let target = TransformTarget::node(
        "missing-identity".to_string(),
        "function_definition".to_string(),
        Some(Span { start: 0, end: 20 }),
        "irrelevant".to_string(),
    );

    let error = resolve_target_in_handles(Path::new("fixture.py"), &handles, &target)
        .expect_err("multiple stale candidates should stay ambiguous");

    match error {
        IdenteditError::AmbiguousTarget {
            identity,
            file,
            candidates,
        } => {
            assert_eq!(identity, "missing-identity");
            assert_eq!(file, "fixture.py");
            assert_eq!(candidates, 2);
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn resolve_target_falls_back_to_unique_kind_and_expected_hash_without_span_hint() {
    let candidate = handle(
        Span { start: 40, end: 76 },
        "function_definition",
        Some("process_data"),
        "def process_data(value):\n    return value + 1",
    );
    let target = TransformTarget::node(
        "stale-identity".to_string(),
        "function_definition".to_string(),
        None,
        hash_text(&candidate.text),
    );

    let resolved = resolve_target_in_handles(
        Path::new("fixture.py"),
        std::slice::from_ref(&candidate),
        &target,
    )
    .expect("unique kind+hash fallback should resolve stale identity");

    assert_eq!(resolved.identity, candidate.identity);
}

#[test]
fn resolve_target_falls_back_to_unique_kind_and_expected_hash_with_stale_span_hint() {
    let candidate = handle(
        Span {
            start: 120,
            end: 168,
        },
        "function_definition",
        Some("compute"),
        "def compute(value):\n    return value * 2",
    );
    let target = TransformTarget::node(
        "stale-identity".to_string(),
        "function_definition".to_string(),
        Some(Span { start: 0, end: 20 }),
        hash_text(&candidate.text),
    );

    let resolved = resolve_target_in_handles(
        Path::new("fixture.py"),
        std::slice::from_ref(&candidate),
        &target,
    )
    .expect("unique kind+hash fallback should ignore stale span_hint");

    assert_eq!(resolved.identity, candidate.identity);
}

#[test]
fn resolve_target_allows_stale_span_hint_for_unique_identity_match() {
    let candidate = handle(
        Span { start: 40, end: 76 },
        "function_definition",
        Some("process_data"),
        "def process_data(value):\n    return value + 1",
    );
    let target = TransformTarget::node(
        candidate.identity.clone(),
        candidate.kind.clone(),
        Some(Span { start: 0, end: 20 }),
        hash_text(&candidate.text),
    );

    let resolved = resolve_target_in_handles(
        Path::new("fixture.py"),
        std::slice::from_ref(&candidate),
        &target,
    )
    .expect("stale span_hint should not block unique identity+hash resolution");

    assert_eq!(resolved.identity, candidate.identity);
}

#[test]
fn resolve_target_kind_and_hash_fallback_rejects_ambiguous_candidates() {
    let shared_text = "def process_data(value):\n    return value + 1";
    let first = handle(
        Span { start: 10, end: 58 },
        "function_definition",
        Some("process_a"),
        shared_text,
    );
    let second = handle(
        Span {
            start: 120,
            end: 168,
        },
        "function_definition",
        Some("process_b"),
        shared_text,
    );
    let target = TransformTarget::node(
        "stale-identity".to_string(),
        "function_definition".to_string(),
        None,
        hash_text(shared_text),
    );

    let error = resolve_target_in_handles(Path::new("fixture.py"), &[first, second], &target)
        .expect_err("multiple kind+hash candidates should be ambiguous");

    match error {
        IdenteditError::AmbiguousTarget {
            identity,
            file,
            candidates,
        } => {
            assert_eq!(identity, "stale-identity");
            assert_eq!(file, "fixture.py");
            assert_eq!(candidates, 2);
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn resolve_target_rejects_zero_length_span_hint_before_matching() {
    let candidate = handle(
        Span { start: 0, end: 24 },
        "function_definition",
        Some("process_data"),
        "def process_data(): pass",
    );
    let target = TransformTarget::node(
        candidate.identity.clone(),
        candidate.kind.clone(),
        Some(Span { start: 4, end: 4 }),
        hash_text(&candidate.text),
    );

    let error = resolve_target_in_handles(Path::new("fixture.py"), &[candidate], &target)
        .expect_err("zero-length span_hint should be rejected");

    match error {
        IdenteditError::InvalidRequest { message } => {
            assert!(message.contains("zero-length spans are not supported"));
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn validate_change_conflicts_rejects_delete_and_insert_on_same_anchor() {
    let anchor = Span { start: 20, end: 40 };
    let changes = vec![
        matched_change(0, OpKind::Delete, anchor, anchor, "function_definition"),
        matched_change(
            1,
            OpKind::InsertBefore {
                new_text: "# before\n".to_string(),
            },
            Span {
                start: anchor.start,
                end: anchor.start,
            },
            anchor,
            "function_definition",
        ),
    ];

    let error = validate_change_conflicts(&changes)
        .expect_err("delete + insert on same anchor should conflict");

    match error {
        IdenteditError::InvalidRequest { message } => {
            assert!(message.contains("Conflicting operations on the same anchor"));
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn validate_change_conflicts_allows_insert_before_and_after_on_same_anchor() {
    let anchor = Span { start: 10, end: 30 };
    let changes = vec![
        matched_change(
            0,
            OpKind::InsertBefore {
                new_text: "# before\n".to_string(),
            },
            Span {
                start: anchor.start,
                end: anchor.start,
            },
            anchor,
            "function_definition",
        ),
        matched_change(
            1,
            OpKind::InsertAfter {
                new_text: "\n# after".to_string(),
            },
            Span {
                start: anchor.end,
                end: anchor.end,
            },
            anchor,
            "function_definition",
        ),
    ];

    validate_change_conflicts(&changes)
        .expect("insert before/after on same anchor should be allowed");
}

#[test]
fn validate_change_conflicts_rejects_multiple_inserts_at_same_position() {
    let first_anchor = Span { start: 4, end: 8 };
    let second_anchor = Span { start: 20, end: 24 };
    let insertion_point = 12;

    let changes = vec![
        matched_change(
            0,
            OpKind::InsertAfter {
                new_text: "A".to_string(),
            },
            Span {
                start: insertion_point,
                end: insertion_point,
            },
            first_anchor,
            "function_definition",
        ),
        matched_change(
            1,
            OpKind::InsertBefore {
                new_text: "B".to_string(),
            },
            Span {
                start: insertion_point,
                end: insertion_point,
            },
            second_anchor,
            "function_definition",
        ),
    ];

    let error = validate_change_conflicts(&changes)
        .expect_err("multiple inserts at same byte position should conflict");

    match error {
        IdenteditError::InvalidRequest { message } => {
            assert!(message.contains("Overlapping operations"));
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn validate_change_conflicts_allows_adjacent_delete_and_replace_on_different_anchors() {
    let first_anchor = Span { start: 0, end: 10 };
    let second_anchor = Span { start: 10, end: 20 };
    let changes = vec![
        matched_change(
            0,
            OpKind::Delete,
            first_anchor,
            first_anchor,
            "function_definition",
        ),
        matched_change(
            1,
            OpKind::Replace {
                new_text: "replacement".to_string(),
            },
            second_anchor,
            second_anchor,
            "function_definition",
        ),
    ];

    validate_change_conflicts(&changes).expect("adjacent delete/replace ranges should be allowed");
}

#[test]
fn validate_change_conflicts_reports_deterministic_overlap_message_for_permutations() {
    let anchor_a = Span { start: 10, end: 20 };
    let anchor_b = Span { start: 20, end: 30 };
    let first = matched_change(
        0,
        OpKind::Replace {
            new_text: "A".to_string(),
        },
        anchor_a,
        anchor_a,
        "function_definition",
    );
    let second = matched_change(
        1,
        OpKind::InsertBefore {
            new_text: "B".to_string(),
        },
        Span { start: 20, end: 20 },
        anchor_b,
        "function_definition",
    );

    let first_error = validate_change_conflicts(&[first.clone(), second.clone()])
        .expect_err("first permutation should conflict");
    let second_error = validate_change_conflicts(&[second, first])
        .expect_err("second permutation should conflict");

    match (first_error, second_error) {
        (
            IdenteditError::InvalidRequest { message: message_a },
            IdenteditError::InvalidRequest { message: message_b },
        ) => {
            assert_eq!(message_a, message_b);
        }
        other => panic!("unexpected error variants: {other:?}"),
    }
}

#[test]
fn validate_change_conflicts_reports_deterministic_equal_start_overlap_message() {
    let anchor_rewrite = Span { start: 12, end: 24 };
    let anchor_insert = Span { start: 12, end: 18 };
    let rewrite = matched_change(
        0,
        OpKind::Replace {
            new_text: "rewritten".to_string(),
        },
        anchor_rewrite,
        anchor_rewrite,
        "function_definition",
    );
    let insert = matched_change(
        1,
        OpKind::InsertBefore {
            new_text: "prefix_".to_string(),
        },
        Span { start: 12, end: 12 },
        anchor_insert,
        "expression_statement",
    );

    let first_error = validate_change_conflicts(&[rewrite.clone(), insert.clone()])
        .expect_err("first permutation should conflict");
    let second_error = validate_change_conflicts(&[insert, rewrite])
        .expect_err("second permutation should conflict");

    match (first_error, second_error) {
        (
            IdenteditError::InvalidRequest { message: message_a },
            IdenteditError::InvalidRequest { message: message_b },
        ) => {
            assert_eq!(message_a, message_b);
            assert!(
                message_a.contains("[12, 12)"),
                "error should mention zero-width insert span"
            );
            assert!(
                message_a.contains("[12, 24)"),
                "error should mention rewrite span"
            );
        }
        other => panic!("unexpected error variants: {other:?}"),
    }
}

#[test]
fn validate_change_conflicts_reports_deterministic_anchor_mix_message_for_permutations() {
    let anchor = Span { start: 5, end: 25 };
    let rewrite = matched_change(0, OpKind::Delete, anchor, anchor, "function_definition");
    let insert = matched_change(
        1,
        OpKind::InsertAfter {
            new_text: "# after".to_string(),
        },
        Span {
            start: anchor.end,
            end: anchor.end,
        },
        anchor,
        "function_definition",
    );

    let first_error = validate_change_conflicts(&[rewrite.clone(), insert.clone()])
        .expect_err("first permutation should conflict");
    let second_error = validate_change_conflicts(&[insert, rewrite])
        .expect_err("second permutation should conflict");

    match (first_error, second_error) {
        (
            IdenteditError::InvalidRequest { message: message_a },
            IdenteditError::InvalidRequest { message: message_b },
        ) => {
            assert_eq!(message_a, message_b);
        }
        other => panic!("unexpected error variants: {other:?}"),
    }
}

#[test]
fn reject_move_operation_returns_invalid_request() {
    let error = reject_move_operation(
        &OpKind::Move {
            to: PathBuf::from("renamed.py"),
        },
        2,
    )
    .expect_err("move operation should be rejected in transform");

    match error {
        IdenteditError::InvalidRequest { message } => {
            assert!(message.contains("Operation 2 uses move"));
            assert!(message.contains("not supported by transform"));
        }
        other => panic!("unexpected error: {other}"),
    }
}
