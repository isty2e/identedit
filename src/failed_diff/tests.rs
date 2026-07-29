use serde_json::json;

use crate::changeset::TransformTarget;

use super::model::FailedDiffCandidate;
use super::{analyze_failed_diff, parse_failed_diff};

fn candidate_start_line(candidate: &FailedDiffCandidate) -> usize {
    match &candidate.target {
        TransformTarget::Line { anchor, .. } => anchor.line(),
        TransformTarget::FileStart { .. } => 0,
        TransformTarget::FileEnd { .. } => usize::MAX,
        target => panic!(
            "failed-diff candidate should use a line boundary target, got {}",
            target.kind_name()
        ),
    }
}

fn candidate_end_line(candidate: &FailedDiffCandidate) -> usize {
    match &candidate.target {
        TransformTarget::Line { anchor, end_anchor } => {
            end_anchor.as_ref().unwrap_or(anchor).line()
        }
        _ => candidate_start_line(candidate),
    }
}

#[test]
fn parser_splits_separated_change_runs_inside_one_numbered_hunk() {
    let parsed = parse_failed_diff(
        "@@ -1,4 +1,4 @@\n keep\n-old one\n+new one\n middle\n-old two\n+new two\n",
    )
    .expect("diff should parse");

    assert_eq!(parsed.changes.len(), 2);
    assert_eq!(parsed.source_hunk_count, 1);
    assert_eq!(parsed.changes[0].source_hunk_index, 0);
    assert_eq!(parsed.changes[0].block_index, 0);
    assert_eq!(parsed.changes[1].source_hunk_index, 0);
    assert_eq!(parsed.changes[1].block_index, 1);
    assert_eq!(parsed.changes[0].old_lines, ["old one"]);
    assert_eq!(parsed.changes[0].new_lines, ["new one"]);
    assert_eq!(parsed.changes[0].before_context, ["keep"]);
    assert_eq!(parsed.changes[0].after_context, ["middle"]);
    assert_eq!(parsed.changes[1].before_context, ["middle"]);
    assert_eq!(parsed.changes[1].old_lines, ["old two"]);
}

#[test]
fn parser_rejects_numbered_hunks_with_incorrect_counts() {
    let error =
        parse_failed_diff("@@ -1,2 +1,2 @@\n-old\n+new\n").expect_err("invalid counts should fail");

    assert!(error.message.contains("line counts"));
}

#[test]
fn parser_accepts_git_headers_and_strips_conventional_prefixes() {
    let parsed = parse_failed_diff(
        "diff --git a/src/file.rs b/src/file.rs\n--- a/src/file.rs\n+++ b/src/file.rs\n@@ -1 +1 @@\n-old\n+new\n",
    )
    .expect("git diff should parse");

    assert_eq!(parsed.header_file.as_deref(), Some("src/file.rs"));
}

#[test]
fn parser_rejects_different_old_and_new_paths() {
    let error = parse_failed_diff("--- old.rs\n+++ new.rs\n@@ -1 +1 @@\n-old\n+new\n")
        .expect_err("rename should fail");

    assert!(error.message.contains("rename"));
}

#[test]
fn parser_rejects_unpaired_no_final_newline_semantics() {
    let error = parse_failed_diff("@@ -1 +1 @@\n-old\n\\ No newline at end of file\n+new\n")
        .expect_err("newline-state change should fail");

    assert!(error.message.contains("final-newline"));
}

#[test]
fn resolver_preserves_overlapping_matches() {
    let parsed = parse_failed_diff("@@\n-same\n-same\n+new\n").expect("diff should parse");
    let analysis = analyze_failed_diff("same\nsame\nsame\n", parsed).expect("diff should analyze");

    assert_eq!(analysis.summary.ambiguous, 1);
    assert_eq!(analysis.summary.candidates, 2);
    assert_eq!(candidate_start_line(&analysis.changes[0].candidates[0]), 1);
    assert_eq!(candidate_end_line(&analysis.changes[0].candidates[0]), 2);
    assert_eq!(candidate_start_line(&analysis.changes[0].candidates[1]), 2);
    assert_eq!(candidate_end_line(&analysis.changes[0].candidates[1]), 3);
}

#[test]
fn resolver_uses_both_sides_of_insertion_context() {
    let parsed = parse_failed_diff("@@\n repeated\n+new\n target\n").expect("diff should parse");
    let analysis = analyze_failed_diff("repeated\nother\ntarget\nrepeated\ntarget\n", parsed)
        .expect("diff should analyze");

    assert_eq!(analysis.summary.unique, 1);
    assert_eq!(candidate_start_line(&analysis.changes[0].candidates[0]), 4);
}

#[test]
fn resolver_reports_context_free_insertion_into_nonempty_file_as_missing() {
    let parsed = parse_failed_diff("@@\n+new\n").expect("diff should parse");
    let analysis = analyze_failed_diff("existing\n", parsed).expect("diff should analyze");

    assert_eq!(analysis.summary.missing, 1);
    assert!(analysis.changes[0].candidates.is_empty());
}

#[test]
fn resolver_allows_context_free_insertion_into_empty_file() {
    let parsed = parse_failed_diff("@@\n+new\n").expect("diff should parse");
    let analysis = analyze_failed_diff("", parsed).expect("diff should analyze");
    let candidate = &analysis.changes[0].candidates[0];
    let serialized = serde_json::to_value(candidate).expect("candidate should serialize");

    assert_eq!(analysis.summary.unique, 1);
    assert_eq!(serialized["target"]["type"], "file_start");
    assert_eq!(
        serialized["op"],
        json!({"type": "insert", "new_text": "new"})
    );
}

#[test]
fn candidate_preview_is_bounded_for_large_matches() {
    let old_lines = (0..20)
        .map(|index| format!("-line {index}"))
        .collect::<Vec<_>>()
        .join("\n");
    let source = (0..20)
        .map(|index| format!("line {index}"))
        .collect::<Vec<_>>()
        .join("\n");
    let parsed =
        parse_failed_diff(&format!("@@\n{old_lines}\n+replacement\n")).expect("diff should parse");
    let analysis = analyze_failed_diff(&source, parsed).expect("diff should analyze");
    let preview = &analysis.changes[0].candidates[0].preview;

    assert_eq!(preview.matched.len(), 4);
    assert_eq!(preview.matched_lines_omitted, 16);
}

#[test]
fn apply_patch_wrapper_preserves_a_real_a_directory_prefix() {
    let parsed = parse_failed_diff("*** Update File: a/config.toml\n@@\n-old\n+new\n")
        .expect("wrapper diff should parse");

    assert_eq!(parsed.header_file.as_deref(), Some("a/config.toml"));
}

#[test]
fn ordinary_unified_headers_preserve_matching_a_directory_prefixes() {
    let parsed =
        parse_failed_diff("--- a/config.toml\n+++ a/config.toml\n@@ -1 +1 @@\n-old\n+new\n")
            .expect("ordinary unified diff should parse");

    assert_eq!(parsed.header_file.as_deref(), Some("a/config.toml"));
}

#[test]
fn unnumbered_hunk_can_replace_lines_that_look_like_file_headers() {
    let parsed =
        parse_failed_diff("@@\n--- old label\n+++ new label\n").expect("hunk should parse");

    assert_eq!(parsed.header_file, None);
    assert_eq!(parsed.changes[0].old_lines, ["-- old label"]);
    assert_eq!(parsed.changes[0].new_lines, ["++ new label"]);
}

#[test]
fn numbered_hunk_rejects_body_lines_beyond_declared_counts() {
    let error = parse_failed_diff("@@ -1 +1 @@\n-old\n+new\n+extra\n")
        .expect_err("extra body line should fail");

    assert!(error.message.contains("counts") || error.message.contains("extra"));
}

#[test]
fn resolver_matches_embedded_nul_as_ordinary_line_content() {
    let parsed = parse_failed_diff("@@\n-old\0value\n+new\0value\n").expect("diff should parse");
    let analysis =
        analyze_failed_diff("before\nold\0value\nafter\n", parsed).expect("diff should analyze");

    assert_eq!(analysis.summary.unique, 1);
    assert_eq!(candidate_start_line(&analysis.changes[0].candidates[0]), 2);
}

#[test]
fn repeated_insertion_context_preserves_every_exact_boundary() {
    let parsed = parse_failed_diff("@@\n before\n+new\n after\n").expect("diff should parse");
    let analysis = analyze_failed_diff("before\nafter\nmiddle\nbefore\nafter\n", parsed)
        .expect("diff should analyze");

    assert_eq!(analysis.summary.ambiguous, 1);
    assert_eq!(analysis.changes[0].candidates.len(), 2);
    assert_eq!(candidate_start_line(&analysis.changes[0].candidates[0]), 1);
    assert_eq!(candidate_start_line(&analysis.changes[0].candidates[1]), 4);
}

#[test]
fn multiple_numbered_hunks_keep_deterministic_input_order() {
    let parsed =
        parse_failed_diff("@@ -1 +1 @@\n-old one\n+new one\n@@ -3 +3 @@\n-old two\n+new two\n")
            .expect("multiple hunks should parse");
    let analysis = analyze_failed_diff("old one\nkeep\nold two\n", parsed)
        .expect("multiple hunks should analyze");

    assert_eq!(analysis.changes.len(), 2);
    assert_eq!(analysis.summary.source_hunks, 2);
    assert_eq!(analysis.changes[0].source_hunk_index, 0);
    assert_eq!(analysis.changes[0].block_index, 0);
    assert_eq!(candidate_start_line(&analysis.changes[0].candidates[0]), 1);
    assert_eq!(analysis.changes[1].source_hunk_index, 1);
    assert_eq!(analysis.changes[1].block_index, 0);
    assert_eq!(candidate_start_line(&analysis.changes[1].candidates[0]), 3);
}

#[test]
fn insertion_at_end_uses_the_last_line_anchor() {
    let parsed = parse_failed_diff("@@\n last\n+new\n").expect("insertion should parse");
    let analysis = analyze_failed_diff("first\nlast\n", parsed).expect("insertion should analyze");
    let serialized = serde_json::to_value(&analysis.changes[0].candidates[0])
        .expect("candidate should serialize");

    assert_eq!(serialized["target"]["type"], "line");
    assert_eq!(candidate_start_line(&analysis.changes[0].candidates[0]), 2);
    assert_eq!(serialized["op"]["type"], "insert_after");
}

#[test]
fn insertion_candidates_can_require_different_canonical_operation_kinds() {
    let parsed = parse_failed_diff("@@\n+new\n target\n").expect("insertion should parse");
    let analysis =
        analyze_failed_diff("target\nmiddle\ntarget\n", parsed).expect("insertion should analyze");
    let candidates = &analysis.changes[0].candidates;
    let first = serde_json::to_value(&candidates[0]).expect("candidate should serialize");
    let second = serde_json::to_value(&candidates[1]).expect("candidate should serialize");

    assert_eq!(analysis.summary.ambiguous, 1);
    assert_eq!(first["target"]["type"], "file_start");
    assert_eq!(first["op"]["type"], "insert");
    assert_eq!(second["target"]["type"], "line");
    assert_eq!(second["op"]["type"], "insert_after");
}

#[test]
fn numbered_zero_count_hunk_supports_file_insertion() {
    let parsed = parse_failed_diff("@@ -0,0 +1,2 @@\n+first\n+second\n")
        .expect("zero-count insertion should parse");
    let analysis = analyze_failed_diff("", parsed).expect("empty-file insertion should analyze");
    let serialized = serde_json::to_value(&analysis.changes[0].candidates[0])
        .expect("candidate should serialize");

    assert_eq!(analysis.summary.unique, 1);
    assert_eq!(serialized["op"]["new_text"], "first\nsecond");
}

#[test]
fn resolver_treats_tabs_and_trailing_spaces_as_significant() {
    let parsed = parse_failed_diff("@@\n-\tvalue  \n+\tnew  \n").expect("diff should parse");
    let analysis =
        analyze_failed_diff("\tvalue\n\tvalue  \n", parsed).expect("diff should analyze");

    assert_eq!(analysis.summary.unique, 1);
    assert_eq!(candidate_start_line(&analysis.changes[0].candidates[0]), 2);
}

#[test]
fn resolver_does_not_unicode_normalize_visually_equal_lines() {
    let composed = "café";
    let decomposed = "cafe\u{301}";
    let parsed =
        parse_failed_diff(&format!("@@\n-{composed}\n+updated\n")).expect("diff should parse");
    let analysis = analyze_failed_diff(&format!("{decomposed}\n{composed}\n"), parsed)
        .expect("diff should analyze");

    assert_eq!(analysis.summary.unique, 1);
    assert_eq!(candidate_start_line(&analysis.changes[0].candidates[0]), 2);
}

#[test]
fn apply_patch_wrapper_rejects_a_second_file_even_after_a_valid_hunk() {
    let error = parse_failed_diff(
        "*** Update File: first.txt\n@@\n-old\n+new\n*** Update File: second.txt\n@@\n-old\n+new\n",
    )
    .expect_err("multi-file wrapper should fail");

    assert!(error.message.contains("one file"));
}

#[test]
fn quoted_git_paths_fail_instead_of_being_misresolved() {
    let error = parse_failed_diff(
        "--- \"a/path with space\"\n+++ \"b/path with space\"\n@@ -1 +1 @@\n-old\n+new\n",
    )
    .expect_err("quoted paths should fail");

    assert!(error.message.contains("quoted"));
}

#[test]
fn unified_header_timestamps_do_not_become_part_of_the_path() {
    let parsed = parse_failed_diff(
        "--- path.txt\t2026-07-28 01:02:03\n+++ path.txt\t2026-07-28 01:02:04\n@@ -1 +1 @@\n-old\n+new\n",
    )
    .expect("timestamped paths should parse");

    assert_eq!(parsed.header_file.as_deref(), Some("path.txt"));
}

#[test]
fn empty_removed_line_matches_only_an_actual_blank_line() {
    let parsed = parse_failed_diff("@@\n-\n+not blank\n").expect("blank-line diff should parse");
    let analysis =
        analyze_failed_diff("value\n\nother\n", parsed).expect("blank line should analyze");

    assert_eq!(analysis.summary.unique, 1);
    assert_eq!(candidate_start_line(&analysis.changes[0].candidates[0]), 2);
}

#[test]
fn byte_order_mark_is_preserved_as_first_line_content() {
    let parsed =
        parse_failed_diff("@@\n-\u{feff}old\n+\u{feff}new\n").expect("BOM diff should parse");
    let analysis =
        analyze_failed_diff("\u{feff}old\nother\n", parsed).expect("BOM line should analyze");

    assert_eq!(analysis.summary.unique, 1);
    assert_eq!(candidate_start_line(&analysis.changes[0].candidates[0]), 1);
}

#[test]
fn mixed_source_newlines_still_use_logical_line_boundaries() {
    let parsed = parse_failed_diff("@@\n-old\n+new\n").expect("mixed-newline diff should parse");
    let analysis = analyze_failed_diff("first\r\nold\nlast\r", parsed)
        .expect("mixed-newline source should analyze");

    assert_eq!(analysis.summary.unique, 1);
    assert_eq!(candidate_start_line(&analysis.changes[0].candidates[0]), 2);
}

#[test]
fn no_newline_marker_after_context_is_rejected() {
    let error = parse_failed_diff("@@\n context\n\\ No newline at end of file\n-old\n+new\n")
        .expect_err("misplaced marker should fail");

    assert!(error.message.contains("must follow"));
}

#[test]
fn final_context_line_can_carry_a_shared_no_newline_marker() {
    let parsed = parse_failed_diff(
        "@@ -1,2 +1,2 @@\n-old\n+new\n final context\n\\ No newline at end of file\n",
    )
    .expect("shared final context marker should parse");

    assert_eq!(parsed.changes.len(), 1);
    assert_eq!(parsed.changes[0].old_lines, ["old"]);
    assert_eq!(parsed.changes[0].new_lines, ["new"]);
    assert_eq!(parsed.changes[0].after_context, ["final context"]);
}

#[test]
fn overflowing_hunk_counts_fail_without_panicking() {
    let huge = "9".repeat(100);
    let error = parse_failed_diff(&format!("@@ -1,{huge} +1,1 @@\n-old\n+new\n"))
        .expect_err("overflowing count should fail");

    assert!(error.message.contains("fit"));
}

#[test]
fn repeated_wrapper_sections_for_the_same_file_remain_one_file() {
    let parsed = parse_failed_diff(
        "*** Update File: same.txt\n@@\n-old one\n+new one\n*** Update File: same.txt\n@@\n-old two\n+new two\n",
    )
    .expect("same-file wrapper sections should parse");

    assert_eq!(parsed.header_file.as_deref(), Some("same.txt"));
    assert_eq!(parsed.changes.len(), 2);
}

#[test]
fn consecutive_blank_lines_remain_ambiguous_candidates() {
    let parsed = parse_failed_diff("@@\n-\n+filled\n").expect("blank-line diff should parse");
    let analysis = analyze_failed_diff("\n\nvalue\n", parsed).expect("blank lines should analyze");

    assert_eq!(analysis.summary.ambiguous, 1);
    assert_eq!(analysis.changes[0].candidates.len(), 2);
    assert_eq!(candidate_start_line(&analysis.changes[0].candidates[0]), 1);
    assert_eq!(candidate_start_line(&analysis.changes[0].candidates[1]), 2);
}

#[test]
fn a_source_line_equal_to_the_no_newline_marker_is_editable() {
    let parsed = parse_failed_diff("@@\n-\\ No newline at end of file\n+ordinary content\n")
        .expect("marker-like content should parse");
    let analysis = analyze_failed_diff("\\ No newline at end of file\n", parsed)
        .expect("marker-like content should analyze");

    assert_eq!(analysis.summary.unique, 1);
}

#[test]
fn long_repeated_insertion_context_preserves_all_boundaries() {
    let source = std::iter::repeat_n("same", 4_000)
        .collect::<Vec<_>>()
        .join("\n");
    let context = std::iter::repeat_n(" same", 2_000)
        .collect::<Vec<_>>()
        .join("\n");
    let parsed = parse_failed_diff(&format!("@@\n{context}\n+new\n"))
        .expect("large insertion diff should parse");
    let analysis = analyze_failed_diff(&source, parsed).expect("large input should analyze");

    assert_eq!(analysis.summary.ambiguous, 1);
    assert_eq!(analysis.changes[0].candidates.len(), 2_001);
    assert_eq!(
        candidate_start_line(&analysis.changes[0].candidates[0]),
        2_000
    );
    assert_eq!(
        analysis.changes[0]
            .candidates
            .last()
            .map(candidate_start_line),
        Some(4_000)
    );
}
