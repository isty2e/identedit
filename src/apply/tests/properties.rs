#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use proptest::prelude::*;
use tempfile::tempdir;

use crate::error::IdenteditError;

use super::super::{
    AtomicWritePhase, ResolvedReplacement, apply_replacements_to_text, ensure_non_overlapping,
    write_text_atomically_with_hook,
};
use super::fail_on_phase;

fn replacement(
    index: usize,
    expected_hash: &str,
    old_text: String,
    start: usize,
    end: usize,
    new_text: String,
) -> ResolvedReplacement {
    ResolvedReplacement {
        index,
        expected_hash: expected_hash.to_string(),
        old_text,
        start,
        end,
        new_text,
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn prop_overlap_detection_matches_interval_math(
        first_start in 0usize..128,
        first_len in 1usize..32,
        second_start in 0usize..128,
        second_len in 1usize..32,
    ) {
        let first_end = first_start + first_len;
        let second_end = second_start + second_len;
        let mut replacements = vec![
            replacement(
                0,
                "hash-a",
                "x".repeat(first_len),
                first_start,
                first_end,
                "A".to_string(),
            ),
            replacement(
                1,
                "hash-b",
                "x".repeat(second_len),
                second_start,
                second_end,
                "B".to_string(),
            ),
        ];
        replacements.sort_by_key(|replacement| replacement.start);

        let expected_overlap = replacements[0].end > replacements[1].start;
        let result = ensure_non_overlapping(&replacements);
        prop_assert_eq!(result.is_err(), expected_overlap);
    }

    #[test]
    fn prop_touching_intervals_are_not_treated_as_overlapping(
        first_start in 0usize..128,
        first_len in 1usize..32,
        second_len in 1usize..32,
    ) {
        let first_end = first_start + first_len;
        let second_start = first_end;
        let second_end = second_start + second_len;
        let replacements = vec![
            replacement(
                0,
                "hash-a",
                "x".repeat(first_len),
                first_start,
                first_end,
                "A".to_string(),
            ),
            replacement(
                1,
                "hash-b",
                "x".repeat(second_len),
                second_start,
                second_end,
                "B".to_string(),
            ),
        ];

        let result = ensure_non_overlapping(&replacements);
        prop_assert!(result.is_ok());
    }

    #[test]
    fn prop_identical_spans_are_rejected_as_overlapping(
        start in 0usize..128,
        len in 1usize..32,
    ) {
        let end = start + len;
        let replacements = vec![
            replacement(
                0,
                "hash-a",
                "x".repeat(len),
                start,
                end,
                "A".to_string(),
            ),
            replacement(
                1,
                "hash-b",
                "x".repeat(len),
                start,
                end,
                "B".to_string(),
            ),
        ];

        let result = ensure_non_overlapping(&replacements);
        prop_assert!(result.is_err());
    }

    #[test]
    fn prop_replacement_order_is_deterministic_for_non_overlapping_spans(
        source in "[a-z]{16,96}",
        first_start in 0usize..32,
        first_len in 1usize..8,
        gap in 1usize..16,
        second_len in 1usize..8,
    ) {
        let second_start = first_start + first_len + gap;
        prop_assume!(second_start + second_len <= source.len());

        let first_end = first_start + first_len;
        let second_end = second_start + second_len;
        let first_old = source[first_start..first_end].to_string();
        let second_old = source[second_start..second_end].to_string();

        let first_op = replacement(
            0,
            "hash-a",
            first_old,
            first_start,
            first_end,
            "A".repeat(first_len + 1),
        );
        let second_op = replacement(
            1,
            "hash-b",
            second_old,
            second_start,
            second_end,
            "B".repeat(second_len + 1),
        );

        let forward_output = apply_replacements_to_text(
            Path::new("fixture.py"),
            source.clone(),
            vec![first_op.clone(), second_op.clone()],
        );
        let reverse_output = apply_replacements_to_text(
            Path::new("fixture.py"),
            source,
            vec![second_op, first_op],
        );

        prop_assert!(forward_output.is_ok());
        prop_assert!(reverse_output.is_ok());
        prop_assert_eq!(forward_output.unwrap(), reverse_output.unwrap());
    }

    #[test]
    fn prop_atomic_write_failure_never_partially_writes(
        original in "[ -~]{0,80}",
        replacement in "[ -~]{0,80}",
    ) {
        let directory = tempdir().expect("tempdir should be created");
        let file_path = directory.path().join("target.txt");
        std::fs::write(&file_path, &original).expect("fixture write should succeed");

        let mut hook = fail_on_phase(AtomicWritePhase::TempSynced);
        let result = write_text_atomically_with_hook(&file_path, &replacement, &mut hook);
        prop_assert!(result.is_err());

        let actual = std::fs::read_to_string(&file_path).expect("target should be readable");
        prop_assert_eq!(actual, original);
    }

    #[cfg(unix)]
    #[test]
    fn prop_atomic_write_preserves_mode_bits_across_rwx_space(
        mode in 0u32..=0o777u32,
        contents in "[ -~]{0,64}",
        replacement in "[ -~]{0,64}",
    ) {
        let directory = tempdir().expect("tempdir should be created");
        let file_path = directory.path().join("target.txt");
        std::fs::write(&file_path, contents).expect("fixture write should succeed");
        std::fs::set_permissions(&file_path, std::fs::Permissions::from_mode(mode))
            .expect("fixture permissions should be set");

        let write_result = write_text_atomically_with_hook(&file_path, &replacement, |_| Ok(()));
        prop_assert!(write_result.is_ok());

        let actual_mode = std::fs::metadata(&file_path)
            .expect("metadata should be readable")
            .permissions()
            .mode()
            & 0o777;
        prop_assert_eq!(actual_mode, mode);
    }
}

#[test]
fn ensure_non_overlapping_rejects_insert_touching_replace_boundary() {
    let replacements = vec![
        replacement(0, "hash-a", "abc".to_string(), 10, 13, "XYZ".to_string()),
        replacement(1, "hash-b", String::new(), 13, 13, "insert".to_string()),
    ];

    let result = ensure_non_overlapping(&replacements);
    assert!(
        result.is_err(),
        "insert touching replace boundary should be rejected"
    );
}

#[test]
fn ensure_non_overlapping_rejects_multiple_inserts_same_position() {
    let replacements = vec![
        replacement(0, "hash-a", String::new(), 32, 32, "first".to_string()),
        replacement(1, "hash-b", String::new(), 32, 32, "second".to_string()),
    ];

    let result = ensure_non_overlapping(&replacements);
    assert!(
        result.is_err(),
        "multiple inserts at same position should be rejected"
    );
}

#[test]
fn ensure_non_overlapping_allows_adjacent_rewrite_ranges() {
    let replacements = vec![
        replacement(0, "hash-a", "abc".to_string(), 0, 3, "A".to_string()),
        replacement(1, "hash-b", "def".to_string(), 3, 6, "B".to_string()),
    ];

    let result = ensure_non_overlapping(&replacements);
    assert!(
        result.is_ok(),
        "adjacent non-insert ranges should be allowed"
    );
}

#[test]
fn apply_replacements_rejects_span_start_inside_multibyte_codepoint() {
    let source = "a😀b".to_string();
    let replacements = vec![replacement(
        0,
        "hash-a",
        String::new(),
        2,
        5,
        "X".to_string(),
    )];

    let error = apply_replacements_to_text(Path::new("fixture.py"), source, replacements)
        .expect_err("span start inside a multibyte codepoint must fail");
    match error {
        IdenteditError::InvalidRequest { message } => {
            assert!(
                message.contains("not a valid UTF-8 boundary range"),
                "unexpected message: {message}"
            );
        }
        other => panic!("unexpected error variant: {other}"),
    }
}

#[test]
fn apply_replacements_rejects_span_end_inside_multibyte_codepoint() {
    let source = "a😀b".to_string();
    let replacements = vec![replacement(
        0,
        "hash-a",
        String::new(),
        1,
        3,
        "X".to_string(),
    )];

    let error = apply_replacements_to_text(Path::new("fixture.py"), source, replacements)
        .expect_err("span end inside a multibyte codepoint must fail");
    match error {
        IdenteditError::InvalidRequest { message } => {
            assert!(
                message.contains("not a valid UTF-8 boundary range"),
                "unexpected message: {message}"
            );
        }
        other => panic!("unexpected error variant: {other}"),
    }
}

#[test]
fn apply_replacements_reports_deterministic_equal_start_overlap_message() {
    let source = "abcdefghijklmnopqrstuvwxyz".to_string();

    let first_error = apply_replacements_to_text(
        Path::new("fixture.py"),
        source.clone(),
        vec![
            replacement(
                0,
                "hash-rewrite",
                "klmnopqrst".to_string(),
                10,
                20,
                "X".to_string(),
            ),
            replacement(1, "hash-insert", String::new(), 10, 10, "Y".to_string()),
        ],
    )
    .expect_err("first permutation should conflict");

    let second_error = apply_replacements_to_text(
        Path::new("fixture.py"),
        source,
        vec![
            replacement(0, "hash-insert", String::new(), 10, 10, "Y".to_string()),
            replacement(
                1,
                "hash-rewrite",
                "klmnopqrst".to_string(),
                10,
                20,
                "X".to_string(),
            ),
        ],
    )
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
