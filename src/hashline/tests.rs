use super::{
    HASHLINE_DEFAULT_HEX_LEN, HASHLINE_MIN_HEX_LEN, HashlineApplyError, HashlineApplyMode,
    HashlineEdit, HashlineMismatchStatus, apply_hashline_edits, apply_hashline_edits_with_mode,
    check_hashline_edits, check_hashline_refs, compute_line_hash, format_hashed_lines,
    format_line_ref, parse_line_ref,
};

fn line_ref(source: &str, line: usize) -> String {
    let line_text = source
        .lines()
        .nth(line - 1)
        .expect("line should exist for anchor");
    format_line_ref(line, &compute_line_hash(line_text))
}

fn hashline_display(line: usize, content: &str) -> String {
    format!("{line}:{}|{content}", compute_line_hash(content))
}

#[test]
fn compute_line_hash_uses_fixed_hex_length() {
    let hash = compute_line_hash("project = \"identedit\"");
    assert_eq!(hash.len(), HASHLINE_DEFAULT_HEX_LEN);
}

#[test]
fn format_hashed_lines_emits_line_hash_content_triplets() {
    let rendered = format_hashed_lines("alpha\nbeta");
    let mut lines = rendered.lines();

    let first = lines.next().expect("first line should exist");
    assert!(first.starts_with("1:"));
    assert!(first.ends_with("|alpha"));

    let second = lines.next().expect("second line should exist");
    assert!(second.starts_with("2:"));
    assert!(second.ends_with("|beta"));
    assert!(lines.next().is_none());
}

#[test]
fn show_hashed_lines_strips_cr_from_crlf_content() {
    let lines = super::show_hashed_lines("alpha\r\nbeta\r\n");
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].content, "alpha");
    assert_eq!(lines[1].content, "beta");
}

#[test]
fn show_hashed_lines_returns_empty_for_empty_source() {
    let lines = super::show_hashed_lines("");
    assert!(lines.is_empty());
}

#[test]
fn parse_line_ref_accepts_display_suffix_and_upper_hex() {
    let parsed = parse_line_ref("12:ABCDEF123456|source text").expect("anchor should parse");
    assert_eq!(parsed.line, 12);
    assert_eq!(parsed.hash, "abcdef123456");
}

#[test]
fn parse_line_ref_rejects_short_hash() {
    let error = parse_line_ref("3:abcd").expect_err("short hash should fail");
    let message = error.to_string();
    assert!(message.contains(&HASHLINE_MIN_HEX_LEN.to_string()));
}

#[test]
fn parse_line_ref_rejects_zero_and_non_numeric_line_number() {
    let valid_hash = "a".repeat(HASHLINE_DEFAULT_HEX_LEN);
    let zero_error = parse_line_ref(&format!("0:{valid_hash}")).expect_err("line 0 should fail");
    assert!(
        zero_error.to_string().contains("line number must be >= 1"),
        "unexpected error: {zero_error}"
    );

    let text_error =
        parse_line_ref(&format!("x:{valid_hash}")).expect_err("non numeric line should fail");
    assert!(
        text_error
            .to_string()
            .contains("line number must be a positive integer"),
        "unexpected error: {text_error}"
    );
}

#[test]
fn parse_line_ref_rejects_hash_longer_than_max() {
    let too_long_hash = "a".repeat(super::HASHLINE_MAX_HEX_LEN + 1);
    let anchor = format!("1:{too_long_hash}");
    let error = parse_line_ref(&anchor).expect_err("too-long hash should fail");
    assert!(
        error.to_string().contains("at most") || error.to_string().contains("exactly"),
        "unexpected error: {error}"
    );
}

#[test]
fn parse_line_ref_accepts_public_hash_length() {
    let hash = "a".repeat(super::HASHLINE_MAX_HEX_LEN);
    let anchor = format!("7:{hash}");
    let parsed = parse_line_ref(&anchor).expect("public hash length should parse");
    assert_eq!(parsed.line, 7);
    assert_eq!(parsed.hash, hash);
}

#[test]
fn parse_line_ref_rejects_non_hex_hash_at_max_length() {
    let mut hash = "a".repeat(super::HASHLINE_MAX_HEX_LEN);
    hash.replace_range(10..11, "z");
    let anchor = format!("7:{hash}");
    let error = parse_line_ref(&anchor).expect_err("non-hex hash should fail");
    assert!(
        error.to_string().contains("only hex characters"),
        "unexpected error: {error}"
    );
}

#[test]
fn check_hashline_refs_reports_remappable_and_ambiguous_statuses() {
    let source = "alpha\nbeta\nalpha";

    let remappable_hash = compute_line_hash("beta");
    let ambiguous_hash = compute_line_hash("alpha");
    let refs = vec![
        format!("1:{remappable_hash}"),
        format!("2:{ambiguous_hash}"),
    ];

    let result = check_hashline_refs(source, &refs).expect("check should succeed");
    assert!(!result.ok);
    assert_eq!(result.summary.total, 2);
    assert_eq!(result.summary.mismatched, 2);
    assert_eq!(result.summary.remappable, 1);
    assert_eq!(result.summary.ambiguous, 1);

    assert_eq!(
        result.mismatches[0].status,
        HashlineMismatchStatus::Remappable
    );
    assert_eq!(result.mismatches[0].remaps.len(), 1);
    assert_eq!(
        result.mismatches[1].status,
        HashlineMismatchStatus::Ambiguous
    );
    assert_eq!(result.mismatches[1].remaps.len(), 2);
    assert_eq!(result.mismatches[1].remaps[0].line, 1);
    assert_eq!(result.mismatches[1].remaps[1].line, 3);
}

#[test]
fn check_hashline_refs_with_empty_input_is_ok() {
    let result = check_hashline_refs("alpha\nbeta", &[]).expect("empty refs should be valid");
    assert!(result.ok);
    assert_eq!(result.summary.total, 0);
    assert_eq!(result.summary.matched, 0);
    assert_eq!(result.summary.mismatched, 0);
    assert!(result.mismatches.is_empty());
}

#[test]
fn check_hashline_edits_collects_edit_index_per_anchor() {
    let source = "alpha\nbeta\ngamma";
    let alpha = line_ref(source, 1);
    let beta = line_ref(source, 2);
    let stale_alpha = format_line_ref(1, &compute_line_hash("stale-alpha"));

    let payload = format!(
        r#"
[
  {{ "set_line": {{ "anchor": "{alpha}", "new_text": "A" }} }},
  {{ "replace_lines": {{ "start_anchor": "{beta}", "end_anchor": "{stale_alpha}", "new_text": "B" }} }}
]
"#
    );
    let edits: Vec<HashlineEdit> =
        serde_json::from_str(&payload).expect("hashline edit payload should parse");

    let result = check_hashline_edits(source, &edits).expect("check should succeed");
    assert!(!result.ok);
    assert_eq!(result.mismatches.len(), 1);
    assert_eq!(result.mismatches[0].edit_index, 1);
    assert_eq!(result.mismatches[0].anchor, stale_alpha);
}

#[test]
fn hashline_edit_deserialize_rejects_unknown_fields() {
    let payload = r#"[{ "set_line": { "anchor": "1:abcdef12", "new_text": "A", "extra": true } }]"#;
    let error = serde_json::from_str::<Vec<HashlineEdit>>(payload)
        .expect_err("unknown fields should be rejected");
    assert!(
        error
            .to_string()
            .contains("did not match any variant of untagged enum HashlineEdit"),
        "error should reject invalid hashline shape, got: {error}"
    );
}

#[test]
fn hashline_edit_deserialize_rejects_multi_variant_object() {
    let payload = r#"
[
  {
    "set_line": { "anchor": "1:abcdef12", "new_text": "A" },
    "insert_after": { "anchor": "1:abcdef12", "text": "B" }
  }
]
"#;

    let error = serde_json::from_str::<Vec<HashlineEdit>>(payload)
        .expect_err("multi-variant object should fail");
    assert!(
        error
            .to_string()
            .contains("did not match any variant of untagged enum HashlineEdit"),
        "unexpected error: {error}"
    );
}

#[test]
fn apply_set_line_replaces_single_line() {
    let source = "alpha\nbeta\ngamma";
    let payload = format!(
        r#"[
  {{ "set_line": {{ "anchor": "{}", "new_text": "BETA" }} }}
]"#,
        line_ref(source, 2)
    );
    let edits: Vec<HashlineEdit> = serde_json::from_str(&payload).expect("edits should parse");

    let applied = apply_hashline_edits(source, &edits).expect("apply should succeed");
    assert_eq!(applied.content, "alpha\nBETA\ngamma");
    assert_eq!(applied.operations_total, 1);
    assert_eq!(applied.operations_applied, 1);
}

#[test]
fn apply_repair_remaps_unique_mismatch_and_succeeds() {
    let source = "a\nb\na";
    let stale_anchor = format_line_ref(1, &compute_line_hash("b"));
    let payload = format!(
        r#"[
  {{ "set_line": {{ "anchor": "{stale_anchor}", "new_text": "B" }} }}
]"#
    );
    let edits: Vec<HashlineEdit> = serde_json::from_str(&payload).expect("edits should parse");

    let strict_error = apply_hashline_edits_with_mode(source, &edits, HashlineApplyMode::Strict)
        .expect_err("strict mode should reject stale anchor");
    assert!(matches!(
        strict_error,
        HashlineApplyError::PreconditionFailed { .. }
    ));

    let repaired = apply_hashline_edits_with_mode(source, &edits, HashlineApplyMode::Repair)
        .expect("repair mode should remap unique mismatch");
    assert_eq!(repaired.content, "a\nB\na");
}

#[test]
fn apply_repair_remaps_replace_lines_start_and_end_anchors() {
    let source = "h\nstart\nmid\nend\n";
    let stale_start = format_line_ref(1, &compute_line_hash("start"));
    let stale_end = format_line_ref(3, &compute_line_hash("end"));
    let payload = format!(
        r#"[
  {{
    "replace_lines": {{
      "start_anchor": "{stale_start}",
      "end_anchor": "{stale_end}",
      "new_text": "S\nE"
    }}
  }}
]"#
    );
    let edits: Vec<HashlineEdit> = serde_json::from_str(&payload).expect("edits should parse");

    let repaired = apply_hashline_edits_with_mode(source, &edits, HashlineApplyMode::Repair)
        .expect("repair mode should remap both anchors");
    assert_eq!(repaired.content, "h\nS\nE\n");
}

#[test]
fn apply_repair_remaps_insert_after_anchor() {
    let source = "top\nmiddle\nbottom";
    let stale_anchor = format_line_ref(1, &compute_line_hash("middle"));
    let payload = format!(
        r#"[
  {{ "insert_after": {{ "anchor": "{stale_anchor}", "text": "M2" }} }}
]"#
    );
    let edits: Vec<HashlineEdit> = serde_json::from_str(&payload).expect("edits should parse");

    let repaired = apply_hashline_edits_with_mode(source, &edits, HashlineApplyMode::Repair)
        .expect("repair insert should remap unique anchor");
    assert_eq!(repaired.content, "top\nmiddle\nM2\nbottom");
}

#[test]
fn apply_repair_is_order_independent_for_multiple_unique_remaps() {
    let source = "head\na\nb\ntail";
    let stale_a = format_line_ref(99, &compute_line_hash("a"));
    let stale_b = format_line_ref(98, &compute_line_hash("b"));
    let forward_payload = format!(
        r#"[
  {{ "set_line": {{ "anchor": "{stale_a}", "new_text": "A" }} }},
  {{ "set_line": {{ "anchor": "{stale_b}", "new_text": "B" }} }}
]"#
    );
    let reverse_payload = format!(
        r#"[
  {{ "set_line": {{ "anchor": "{stale_b}", "new_text": "B" }} }},
  {{ "set_line": {{ "anchor": "{stale_a}", "new_text": "A" }} }}
]"#
    );
    let edits_forward: Vec<HashlineEdit> =
        serde_json::from_str(&forward_payload).expect("forward edits should parse");
    let edits_reverse: Vec<HashlineEdit> =
        serde_json::from_str(&reverse_payload).expect("reverse edits should parse");

    let result_forward =
        apply_hashline_edits_with_mode(source, &edits_forward, HashlineApplyMode::Repair)
            .expect("forward repair should succeed");
    let result_reverse =
        apply_hashline_edits_with_mode(source, &edits_reverse, HashlineApplyMode::Repair)
            .expect("reverse repair should succeed");

    assert_eq!(result_forward.content, "head\nA\nB\ntail");
    assert_eq!(result_reverse.content, "head\nA\nB\ntail");
}

#[test]
fn apply_repair_rejects_ambiguous_insert_after_remap() {
    let source = "top\nmiddle\ntop";
    let stale_anchor = format_line_ref(2, &compute_line_hash("top"));
    let payload = format!(
        r#"[
  {{ "insert_after": {{ "anchor": "{stale_anchor}", "text": "X" }} }}
]"#
    );
    let edits: Vec<HashlineEdit> = serde_json::from_str(&payload).expect("edits should parse");

    let error = apply_hashline_edits_with_mode(source, &edits, HashlineApplyMode::Repair)
        .expect_err("ambiguous insert remap should fail");
    let HashlineApplyError::PreconditionFailed { check } = error else {
        panic!("expected precondition failure");
    };
    assert_eq!(check.summary.ambiguous, 1);
}

#[test]
fn apply_repair_remaps_only_stale_end_anchor_for_replace_lines() {
    let source = "A\nB\nC\nD";
    let stale_end = format_line_ref(3, &compute_line_hash("D"));
    let payload = format!(
        r#"[
  {{
    "replace_lines": {{
      "start_anchor": "{}",
      "end_anchor": "{stale_end}",
      "new_text": "X"
    }}
  }}
]"#,
        line_ref(source, 2)
    );
    let edits: Vec<HashlineEdit> = serde_json::from_str(&payload).expect("edits should parse");

    let repaired = apply_hashline_edits_with_mode(source, &edits, HashlineApplyMode::Repair)
        .expect("repair should remap stale end anchor only");
    assert_eq!(repaired.content, "A\nX");
}

#[test]
fn apply_repair_rejects_overlap_introduced_by_remap() {
    let source = "w\nx\ny\nz";
    let stale_start = format_line_ref(1, &compute_line_hash("x"));
    let stale_end = format_line_ref(2, &compute_line_hash("y"));
    let payload = format!(
        r#"[
  {{
    "replace_lines": {{
      "start_anchor": "{stale_start}",
      "end_anchor": "{stale_end}",
      "new_text": "XY"
    }}
  }},
  {{
    "set_line": {{
      "anchor": "{}",
      "new_text": "Y!"
    }}
  }}
]"#,
        line_ref(source, 3)
    );
    let edits: Vec<HashlineEdit> = serde_json::from_str(&payload).expect("edits should parse");

    let error = apply_hashline_edits_with_mode(source, &edits, HashlineApplyMode::Repair)
        .expect_err("overlap after remap should fail");
    assert!(matches!(error, HashlineApplyError::Overlap { .. }));
}

#[test]
fn apply_repair_rejects_when_any_anchor_is_non_remappable() {
    let source = "one\ntwo\nthree";
    let stale_unique = format_line_ref(1, &compute_line_hash("two"));
    let stale_missing = format_line_ref(2, &compute_line_hash("does-not-exist"));
    let payload = format!(
        r#"[
  {{ "set_line": {{ "anchor": "{stale_unique}", "new_text": "TWO" }} }},
  {{ "set_line": {{ "anchor": "{stale_missing}", "new_text": "?" }} }}
]"#
    );
    let edits: Vec<HashlineEdit> = serde_json::from_str(&payload).expect("edits should parse");

    let error = apply_hashline_edits_with_mode(source, &edits, HashlineApplyMode::Repair)
        .expect_err("repair must fail if any anchor is non-remappable");
    let HashlineApplyError::PreconditionFailed { check } = error else {
        panic!("expected precondition failure");
    };
    assert_eq!(check.summary.total, 2);
    assert_eq!(check.summary.mismatched, 2);
    assert_eq!(check.summary.remappable, 1);
    assert_eq!(check.summary.ambiguous, 0);
}

#[test]
fn apply_repair_reports_invalid_reverse_range_after_remap() {
    let source = "a\nb\nc";
    let stale_start = format_line_ref(1, &compute_line_hash("c"));
    let stale_end = format_line_ref(3, &compute_line_hash("a"));
    let payload = format!(
        r#"[
  {{
    "replace_lines": {{
      "start_anchor": "{stale_start}",
      "end_anchor": "{stale_end}",
      "new_text": "x"
    }}
  }}
]"#
    );
    let edits: Vec<HashlineEdit> = serde_json::from_str(&payload).expect("edits should parse");

    let error = apply_hashline_edits_with_mode(source, &edits, HashlineApplyMode::Repair)
        .expect_err("reverse range after remap should fail");
    let HashlineApplyError::Check(check_error) = error else {
        panic!("expected check error");
    };
    assert!(
        check_error.to_string().contains("must be >= start line"),
        "unexpected error: {check_error}"
    );
}

#[test]
fn apply_repair_remaps_anchor_with_display_suffix() {
    let source = "a\nb\nc";
    let stale_anchor = format!("1:{}|copied display", compute_line_hash("b").to_uppercase());
    let payload = format!(
        r#"[
  {{ "set_line": {{ "anchor": "{stale_anchor}", "new_text": "B" }} }}
]"#
    );
    let edits: Vec<HashlineEdit> = serde_json::from_str(&payload).expect("edits should parse");

    let repaired = apply_hashline_edits_with_mode(source, &edits, HashlineApplyMode::Repair)
        .expect("repair should remap anchor with display suffix");
    assert_eq!(repaired.content, "a\nB\nc");
}

#[test]
fn apply_repair_rejects_ambiguous_remap() {
    let source = "x\ny\nx";
    let stale_anchor = format_line_ref(2, &compute_line_hash("x"));
    let payload = format!(
        r#"[
  {{ "set_line": {{ "anchor": "{stale_anchor}", "new_text": "X" }} }}
]"#
    );
    let edits: Vec<HashlineEdit> = serde_json::from_str(&payload).expect("edits should parse");

    let error = apply_hashline_edits_with_mode(source, &edits, HashlineApplyMode::Repair)
        .expect_err("ambiguous remap must fail");
    let HashlineApplyError::PreconditionFailed { check } = error else {
        panic!("expected precondition failure");
    };
    assert_eq!(check.summary.ambiguous, 1);
}

#[test]
fn apply_repair_strips_hashline_prefixes_in_replacement_text() {
    let source = "a\nb";
    let first = hashline_display(1, "x");
    let second = hashline_display(2, "y");
    let payload = format!(
        r#"[
  {{ "set_line": {{ "anchor": "{}", "new_text": "{first}\n{second}" }} }}
]"#,
        line_ref(source, 2)
    );
    let edits: Vec<HashlineEdit> = serde_json::from_str(&payload).expect("edits should parse");

    let strict = apply_hashline_edits_with_mode(source, &edits, HashlineApplyMode::Strict)
        .expect("strict apply should succeed");
    assert_eq!(strict.content, format!("a\n{first}\n{second}"));

    let repaired = apply_hashline_edits_with_mode(source, &edits, HashlineApplyMode::Repair)
        .expect("repair apply should succeed");
    assert_eq!(repaired.content, "a\nx\ny");
}

#[test]
fn apply_repair_strips_diff_plus_prefixes_in_replacement_text() {
    let source = "a\nb";
    let payload = format!(
        r#"[
  {{ "set_line": {{ "anchor": "{}", "new_text": "+x\n+y" }} }}
]"#,
        line_ref(source, 2)
    );
    let edits: Vec<HashlineEdit> = serde_json::from_str(&payload).expect("edits should parse");

    let strict = apply_hashline_edits_with_mode(source, &edits, HashlineApplyMode::Strict)
        .expect("strict apply should succeed");
    assert_eq!(strict.content, "a\n+x\n+y");

    let repaired = apply_hashline_edits_with_mode(source, &edits, HashlineApplyMode::Repair)
        .expect("repair apply should succeed");
    assert_eq!(repaired.content, "a\nx\ny");
}

#[test]
fn apply_repair_strips_combined_plus_and_hash_prefixes() {
    let source = "a\nb";
    let first = hashline_display(1, "x");
    let second = hashline_display(2, "y");
    let payload = format!(
        r#"[
  {{
    "set_line": {{
      "anchor": "{}",
      "new_text": "+{first}\n+{second}"
    }}
  }}
]"#,
        line_ref(source, 2)
    );
    let edits: Vec<HashlineEdit> = serde_json::from_str(&payload).expect("edits should parse");

    let repaired = apply_hashline_edits_with_mode(source, &edits, HashlineApplyMode::Repair)
        .expect("repair apply should succeed");
    assert_eq!(repaired.content, "a\nx\ny");
}

#[test]
fn apply_repair_preserves_crlf_when_stripping_prefixes() {
    let source = "a\r\nb\r\n";
    let first = hashline_display(1, "x");
    let second = hashline_display(2, "y");
    let payload = format!(
        r#"[
  {{
    "set_line": {{
      "anchor": "{}",
      "new_text": "+{first}\n+{second}"
    }}
  }}
]"#,
        line_ref(source, 2)
    );
    let edits: Vec<HashlineEdit> = serde_json::from_str(&payload).expect("edits should parse");

    let repaired = apply_hashline_edits_with_mode(source, &edits, HashlineApplyMode::Repair)
        .expect("repair apply should succeed");
    assert_eq!(repaired.content, "a\r\nx\r\ny\r\n");
}

#[test]
fn apply_repair_preserves_cplusplus_increment_prefix() {
    let source = "a\nb";
    let payload = format!(
        r#"[
  {{ "set_line": {{ "anchor": "{}", "new_text": "++i\n++j" }} }}
]"#,
        line_ref(source, 2)
    );
    let edits: Vec<HashlineEdit> = serde_json::from_str(&payload).expect("edits should parse");

    let repaired = apply_hashline_edits_with_mode(source, &edits, HashlineApplyMode::Repair)
        .expect("repair apply should succeed");
    assert_eq!(repaired.content, "a\n++i\n++j");
}

#[test]
fn apply_repair_preserves_non_hex_hashline_like_prefix() {
    let source = "a\nb";
    let payload = format!(
        r#"[
  {{ "set_line": {{ "anchor": "{}", "new_text": "1:deadbeeg|x\n2:deadbeeg|y" }} }}
]"#,
        line_ref(source, 2)
    );
    let edits: Vec<HashlineEdit> = serde_json::from_str(&payload).expect("edits should parse");

    let repaired = apply_hashline_edits_with_mode(source, &edits, HashlineApplyMode::Repair)
        .expect("repair apply should succeed");
    assert_eq!(repaired.content, "a\n1:deadbeeg|x\n2:deadbeeg|y");
}

#[test]
fn apply_repair_strips_prefixes_with_empty_lines() {
    let source = "a\nb";
    let first = hashline_display(1, "x");
    let second = hashline_display(2, "y");
    let payload = format!(
        r#"[
  {{
    "set_line": {{
      "anchor": "{}",
      "new_text": "+{first}\n\n+{second}"
    }}
  }}
]"#,
        line_ref(source, 2)
    );
    let edits: Vec<HashlineEdit> = serde_json::from_str(&payload).expect("edits should parse");

    let repaired = apply_hashline_edits_with_mode(source, &edits, HashlineApplyMode::Repair)
        .expect("repair apply should succeed");
    assert_eq!(repaired.content, "a\nx\n\ny");
}

#[test]
fn apply_repair_expands_single_line_merge_with_next_line() {
    let source = "foo &&\nbar\nbaz";
    let payload = format!(
        r#"[
  {{ "set_line": {{ "anchor": "{}", "new_text": "foo && bar" }} }}
]"#,
        line_ref(source, 1)
    );
    let edits: Vec<HashlineEdit> = serde_json::from_str(&payload).expect("edits should parse");

    let strict = apply_hashline_edits_with_mode(source, &edits, HashlineApplyMode::Strict)
        .expect("strict apply should succeed");
    assert_eq!(strict.content, "foo && bar\nbar\nbaz");

    let repaired = apply_hashline_edits_with_mode(source, &edits, HashlineApplyMode::Repair)
        .expect("repair apply should succeed");
    assert_eq!(repaired.content, "foo && bar\nbaz");
}

#[test]
fn apply_repair_expands_merge_for_comma_continuation() {
    let source = "items,\nnext\nend";
    let payload = format!(
        r#"[
  {{ "set_line": {{ "anchor": "{}", "new_text": "items, next" }} }}
]"#,
        line_ref(source, 1)
    );
    let edits: Vec<HashlineEdit> = serde_json::from_str(&payload).expect("edits should parse");

    let repaired = apply_hashline_edits_with_mode(source, &edits, HashlineApplyMode::Repair)
        .expect("repair apply should succeed");
    assert_eq!(repaired.content, "items, next\nend");
}

#[test]
fn apply_repair_expands_merge_for_double_pipe_continuation() {
    let source = "cond ||\nnext\nend";
    let payload = format!(
        r#"[
  {{ "set_line": {{ "anchor": "{}", "new_text": "cond || next" }} }}
]"#,
        line_ref(source, 1)
    );
    let edits: Vec<HashlineEdit> = serde_json::from_str(&payload).expect("edits should parse");

    let repaired = apply_hashline_edits_with_mode(source, &edits, HashlineApplyMode::Repair)
        .expect("repair apply should succeed");
    assert_eq!(repaired.content, "cond || next\nend");
}

#[test]
fn apply_repair_expands_merge_for_nullish_continuation() {
    let source = "lhs ??\nrhs\ntail";
    let payload = format!(
        r#"[
  {{ "set_line": {{ "anchor": "{}", "new_text": "lhs ?? rhs" }} }}
]"#,
        line_ref(source, 1)
    );
    let edits: Vec<HashlineEdit> = serde_json::from_str(&payload).expect("edits should parse");

    let repaired = apply_hashline_edits_with_mode(source, &edits, HashlineApplyMode::Repair)
        .expect("repair apply should succeed");
    assert_eq!(repaired.content, "lhs ?? rhs\ntail");
}

#[test]
fn apply_repair_does_not_expand_for_single_pipe_table_style() {
    let source = "A |\nB\nC";
    let payload = format!(
        r#"[
  {{ "set_line": {{ "anchor": "{}", "new_text": "A | B" }} }}
]"#,
        line_ref(source, 1)
    );
    let edits: Vec<HashlineEdit> = serde_json::from_str(&payload).expect("edits should parse");

    let repaired = apply_hashline_edits_with_mode(source, &edits, HashlineApplyMode::Repair)
        .expect("repair apply should succeed");
    assert_eq!(repaired.content, "A | B\nB\nC");
}

#[test]
fn apply_repair_does_not_expand_for_single_amp_table_style() {
    let source = "A &\nB\nC";
    let payload = format!(
        r#"[
  {{ "set_line": {{ "anchor": "{}", "new_text": "A & B" }} }}
]"#,
        line_ref(source, 1)
    );
    let edits: Vec<HashlineEdit> = serde_json::from_str(&payload).expect("edits should parse");

    let repaired = apply_hashline_edits_with_mode(source, &edits, HashlineApplyMode::Repair)
        .expect("repair apply should succeed");
    assert_eq!(repaired.content, "A & B\nB\nC");
}

#[test]
fn apply_repair_does_not_expand_at_end_of_file() {
    let source = "tail &&";
    let payload = format!(
        r#"[
  {{ "set_line": {{ "anchor": "{}", "new_text": "tail && done" }} }}
]"#,
        line_ref(source, 1)
    );
    let edits: Vec<HashlineEdit> = serde_json::from_str(&payload).expect("edits should parse");

    let repaired = apply_hashline_edits_with_mode(source, &edits, HashlineApplyMode::Repair)
        .expect("repair apply should succeed without eof expansion");
    assert_eq!(repaired.content, "tail && done");
}

#[test]
fn apply_repair_does_not_expand_without_continuation_hint() {
    let source = "foo\nbar\nbaz";
    let payload = format!(
        r#"[
  {{ "set_line": {{ "anchor": "{}", "new_text": "foo bar" }} }}
]"#,
        line_ref(source, 1)
    );
    let edits: Vec<HashlineEdit> = serde_json::from_str(&payload).expect("edits should parse");

    let repaired = apply_hashline_edits_with_mode(source, &edits, HashlineApplyMode::Repair)
        .expect("repair apply should succeed");
    assert_eq!(repaired.content, "foo bar\nbar\nbaz");
}

#[test]
fn apply_repair_expands_merge_with_remapped_anchor() {
    let source = "header\nfoo &&\nbar\ntail";
    let stale_anchor = format_line_ref(1, &compute_line_hash("foo &&"));
    let payload = format!(
        r#"[
  {{ "set_line": {{ "anchor": "{stale_anchor}", "new_text": "foo && bar" }} }}
]"#
    );
    let edits: Vec<HashlineEdit> = serde_json::from_str(&payload).expect("edits should parse");

    let repaired = apply_hashline_edits_with_mode(source, &edits, HashlineApplyMode::Repair)
        .expect("repair apply should succeed with remap + merge");
    assert_eq!(repaired.content, "header\nfoo && bar\ntail");
}

#[test]
fn apply_repair_does_not_remap_nfc_and_nfd_variants() {
    let source = "x\ncafe\u{301}\ny";
    let stale_anchor = format_line_ref(1, &compute_line_hash("café"));
    let payload = format!(
        r#"[
  {{ "set_line": {{ "anchor": "{stale_anchor}", "new_text": "CAFE" }} }}
]"#
    );
    let edits: Vec<HashlineEdit> = serde_json::from_str(&payload).expect("edits should parse");

    let error = apply_hashline_edits_with_mode(source, &edits, HashlineApplyMode::Repair)
        .expect_err("repair should not remap across distinct unicode byte sequences");
    let HashlineApplyError::PreconditionFailed { check } = error else {
        panic!("expected precondition failure");
    };
    assert_eq!(check.summary.remappable, 0);
    assert_eq!(check.mismatches[0].status, HashlineMismatchStatus::Mismatch);
}

#[test]
fn apply_repair_does_not_expand_when_line_ends_with_double_amp_in_comment() {
    let source = "note: &&\nnext\nend";
    let payload = format!(
        r#"[
  {{ "set_line": {{ "anchor": "{}", "new_text": "note: && next" }} }}
]"#,
        line_ref(source, 1)
    );
    let edits: Vec<HashlineEdit> = serde_json::from_str(&payload).expect("edits should parse");

    let repaired = apply_hashline_edits_with_mode(source, &edits, HashlineApplyMode::Repair)
        .expect("repair apply should succeed");
    assert_eq!(repaired.content, "note: && next\nnext\nend");
}

#[test]
fn apply_repair_does_not_strip_timestamp_like_single_line_prefix() {
    let source = "a\nb";
    let payload = format!(
        r#"[
  {{
    "set_line": {{
      "anchor": "{}",
      "new_text": "2024:deadbeef|literal payload"
    }}
  }}
]"#,
        line_ref(source, 2)
    );
    let edits: Vec<HashlineEdit> = serde_json::from_str(&payload).expect("edits should parse");

    let repaired = apply_hashline_edits_with_mode(source, &edits, HashlineApplyMode::Repair)
        .expect("repair apply should succeed");
    assert_eq!(repaired.content, "a\n2024:deadbeef|literal payload");
}

#[test]
fn apply_repair_does_not_strip_timestamp_like_multiline_prefixes() {
    let source = "a\nb";
    let payload = format!(
        r#"[
  {{
    "set_line": {{
      "anchor": "{}",
      "new_text": "2024:deadbeef|first\n2025:deadbeef|second"
    }}
  }}
]"#,
        line_ref(source, 2)
    );
    let edits: Vec<HashlineEdit> = serde_json::from_str(&payload).expect("edits should parse");

    let repaired = apply_hashline_edits_with_mode(source, &edits, HashlineApplyMode::Repair)
        .expect("repair apply should succeed");
    assert_eq!(
        repaired.content,
        "a\n2024:deadbeef|first\n2025:deadbeef|second"
    );
}

#[test]
fn apply_repair_does_not_strip_shape_valid_but_hash_mismatched_prefixes() {
    let source = "a\nb";
    let payload = format!(
        r#"[
  {{
    "set_line": {{
      "anchor": "{}",
      "new_text": "1:deadbeef|x\n2:deadbeef|y"
    }}
  }}
]"#,
        line_ref(source, 2)
    );
    let edits: Vec<HashlineEdit> = serde_json::from_str(&payload).expect("edits should parse");

    let repaired = apply_hashline_edits_with_mode(source, &edits, HashlineApplyMode::Repair)
        .expect("repair apply should succeed");
    assert_eq!(repaired.content, "a\n1:deadbeef|x\n2:deadbeef|y");
}

#[test]
fn apply_repair_strips_hashline_prefixes_with_uppercase_hashes() {
    let source = "a\nb";
    let first = format!("1:{}|x", compute_line_hash("x").to_uppercase());
    let second = format!("2:{}|y", compute_line_hash("y").to_uppercase());
    let payload = format!(
        r#"[
  {{ "set_line": {{ "anchor": "{}", "new_text": "{first}\n{second}" }} }}
]"#,
        line_ref(source, 2)
    );
    let edits: Vec<HashlineEdit> = serde_json::from_str(&payload).expect("edits should parse");

    let repaired = apply_hashline_edits_with_mode(source, &edits, HashlineApplyMode::Repair)
        .expect("repair apply should strip uppercase hashline prefixes");
    assert_eq!(repaired.content, "a\nx\ny");
}

#[test]
fn apply_repair_strips_hashline_prefixes_with_16_char_hashes() {
    let source = "a\nb";
    let first_hash = blake3::hash("x".as_bytes()).to_hex().to_string();
    let second_hash = blake3::hash("y".as_bytes()).to_hex().to_string();
    let first = format!("1:{}|x", &first_hash[..16]);
    let second = format!("2:{}|y", &second_hash[..16]);
    let payload = format!(
        r#"[
  {{ "set_line": {{ "anchor": "{}", "new_text": "{first}\n{second}" }} }}
]"#,
        line_ref(source, 2)
    );
    let edits: Vec<HashlineEdit> = serde_json::from_str(&payload).expect("edits should parse");

    let repaired = apply_hashline_edits_with_mode(source, &edits, HashlineApplyMode::Repair)
        .expect("repair apply should strip 16-char hashline prefixes");
    assert_eq!(repaired.content, "a\nx\ny");
}

#[test]
fn apply_repair_strips_hashline_prefixes_with_64_char_hashes() {
    let source = "a\nb";
    let first = format!("1:{}|x", blake3::hash("x".as_bytes()).to_hex());
    let second = format!("2:{}|y", blake3::hash("y".as_bytes()).to_hex());
    let payload = format!(
        r#"[
  {{ "set_line": {{ "anchor": "{}", "new_text": "{first}\n{second}" }} }}
]"#,
        line_ref(source, 2)
    );
    let edits: Vec<HashlineEdit> = serde_json::from_str(&payload).expect("edits should parse");

    let repaired = apply_hashline_edits_with_mode(source, &edits, HashlineApplyMode::Repair)
        .expect("repair apply should strip 64-char hashline prefixes");
    assert_eq!(repaired.content, "a\nx\ny");
}

#[test]
fn apply_repair_strips_hashline_prefixes_with_uppercase_64_char_hashes() {
    let source = "a\nb";
    let first = format!(
        "1:{}|x",
        blake3::hash("x".as_bytes())
            .to_hex()
            .to_string()
            .to_uppercase()
    );
    let second = format!(
        "2:{}|y",
        blake3::hash("y".as_bytes())
            .to_hex()
            .to_string()
            .to_uppercase()
    );
    let payload = format!(
        r#"[
  {{ "set_line": {{ "anchor": "{}", "new_text": "{first}\n{second}" }} }}
]"#,
        line_ref(source, 2)
    );
    let edits: Vec<HashlineEdit> = serde_json::from_str(&payload).expect("edits should parse");

    let repaired = apply_hashline_edits_with_mode(source, &edits, HashlineApplyMode::Repair)
        .expect("repair apply should strip uppercase 64-char hashline prefixes");
    assert_eq!(repaired.content, "a\nx\ny");
}

#[test]
fn apply_repair_does_not_strip_hashline_prefixes_with_wrong_64_char_hashes() {
    let source = "a\nb";
    let payload = format!(
        r#"[
  {{
    "set_line": {{
      "anchor": "{}",
      "new_text": "1:{}|x\n2:{}|y"
    }}
  }}
]"#,
        line_ref(source, 2),
        "0".repeat(64),
        "f".repeat(64),
    );
    let edits: Vec<HashlineEdit> = serde_json::from_str(&payload).expect("edits should parse");

    let repaired = apply_hashline_edits_with_mode(source, &edits, HashlineApplyMode::Repair)
        .expect("repair apply should succeed");
    assert_eq!(
        repaired.content,
        format!("a\n1:{}|x\n2:{}|y", "0".repeat(64), "f".repeat(64))
    );
}

#[test]
fn check_hashline_refs_reports_deterministic_ambiguous_candidates_for_many_duplicates() {
    let source = (0..120).map(|_| "dup").collect::<Vec<_>>().join("\n");
    let dup_hash = compute_line_hash("dup");
    let refs = vec![format!("121:{dup_hash}")];

    let check = check_hashline_refs(&source, &refs).expect("check should succeed");
    assert!(!check.ok);
    assert_eq!(check.summary.ambiguous, 1);
    let remaps = &check.mismatches[0].remaps;
    assert_eq!(remaps.len(), 120);
    assert_eq!(remaps[0].line, 1);
    assert_eq!(remaps[119].line, 120);
}

#[test]
fn check_hashline_refs_reports_ambiguous_candidates_in_source_order() {
    let source = "a\nx\nb\nc\nd\ne\nf\ng\nh\nx";
    let x_hash = compute_line_hash("x");
    let refs = vec![format!("1:{x_hash}")];

    let check = check_hashline_refs(source, &refs).expect("check should succeed");
    assert!(!check.ok);
    assert_eq!(check.summary.ambiguous, 1);
    let remaps = &check.mismatches[0].remaps;
    assert_eq!(remaps.len(), 2);
    assert_eq!(remaps[0].line, 2);
    assert_eq!(remaps[1].line, 10);
}

#[test]
fn show_hashed_lines_handles_carriage_return_only_newlines() {
    let lines = super::show_hashed_lines("a\rb\rc");
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[0].content, "a");
    assert_eq!(lines[1].content, "b");
    assert_eq!(lines[2].content, "c");
}

#[test]
fn show_hashed_lines_keeps_empty_line_before_trailing_cr_newline() {
    let lines = super::show_hashed_lines("a\rb\r\r");
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[0].content, "a");
    assert_eq!(lines[1].content, "b");
    assert_eq!(lines[2].content, "");
}

#[test]
fn check_hashline_refs_accepts_cr_only_source() {
    let source = "a\rb\rc";
    let refs = vec![format_line_ref(2, &compute_line_hash("b"))];
    let check = check_hashline_refs(source, &refs).expect("check should succeed");
    assert!(check.ok);
    assert_eq!(check.summary.total, 1);
    assert_eq!(check.summary.matched, 1);
}

#[test]
fn apply_set_line_preserves_cr_only_newlines() {
    let source = "a\rb\rc\r";
    let anchor = format_line_ref(2, &compute_line_hash("b"));
    let payload = format!(
        r#"[
  {{ "set_line": {{ "anchor": "{anchor}", "new_text": "B" }} }}
]"#
    );
    let edits: Vec<HashlineEdit> = serde_json::from_str(&payload).expect("edits should parse");

    let applied = apply_hashline_edits(source, &edits).expect("apply should succeed");
    assert_eq!(applied.content, "a\rB\rc\r");
}

#[test]
fn apply_set_line_normalizes_mixed_crlf_and_cr_source_to_lf() {
    let source = "a\r\nb\rc\r\n";
    let anchor = format_line_ref(2, &compute_line_hash("b"));
    let payload = format!(
        r#"[
  {{ "set_line": {{ "anchor": "{anchor}", "new_text": "B" }} }}
]"#
    );
    let edits: Vec<HashlineEdit> = serde_json::from_str(&payload).expect("edits should parse");

    let applied = apply_hashline_edits(source, &edits).expect("apply should succeed");
    assert_eq!(applied.content, "a\nB\nc\n");
}

#[test]
fn apply_repair_preserves_zero_width_characters() {
    let source = "a\nb";
    let payload = format!(
        r#"[
  {{ "set_line": {{ "anchor": "{}", "new_text": "x\u200By" }} }}
]"#,
        line_ref(source, 2)
    );
    let edits: Vec<HashlineEdit> = serde_json::from_str(&payload).expect("edits should parse");

    let repaired = apply_hashline_edits_with_mode(source, &edits, HashlineApplyMode::Repair)
        .expect("repair apply should succeed");
    assert_eq!(repaired.content, "a\nx\u{200b}y");
}

#[test]
fn apply_repair_does_not_strip_hashline_prefix_when_only_half_prefixed() {
    let source = "a\nb";
    let first = hashline_display(1, "x");
    let payload = format!(
        r#"[
  {{ "set_line": {{ "anchor": "{}", "new_text": "{first}\ny" }} }}
]"#,
        line_ref(source, 2)
    );
    let edits: Vec<HashlineEdit> = serde_json::from_str(&payload).expect("edits should parse");

    let repaired = apply_hashline_edits_with_mode(source, &edits, HashlineApplyMode::Repair)
        .expect("repair apply should succeed");
    assert_eq!(repaired.content, format!("a\n{first}\ny"));
}

#[test]
fn apply_repair_does_not_strip_hashline_prefix_when_exactly_half_prefixed() {
    let source = "a\nb";
    let first = hashline_display(1, "x");
    let second = hashline_display(2, "z");
    let payload = format!(
        r#"[
  {{ "set_line": {{ "anchor": "{}", "new_text": "{first}\ny\n{second}\nw" }} }}
]"#,
        line_ref(source, 2)
    );
    let edits: Vec<HashlineEdit> = serde_json::from_str(&payload).expect("edits should parse");

    let repaired = apply_hashline_edits_with_mode(source, &edits, HashlineApplyMode::Repair)
        .expect("repair apply should succeed");
    assert_eq!(repaired.content, format!("a\n{first}\ny\n{second}\nw"));
}

#[test]
fn apply_repair_does_not_strip_diff_plus_when_only_half_prefixed() {
    let source = "a\nb";
    let payload = format!(
        r#"[
  {{ "set_line": {{ "anchor": "{}", "new_text": "+x\ny" }} }}
]"#,
        line_ref(source, 2)
    );
    let edits: Vec<HashlineEdit> = serde_json::from_str(&payload).expect("edits should parse");

    let repaired = apply_hashline_edits_with_mode(source, &edits, HashlineApplyMode::Repair)
        .expect("repair apply should succeed");
    assert_eq!(repaired.content, "a\n+x\ny");
}

#[test]
fn apply_repair_does_not_strip_plus_prefix_when_exactly_half_prefixed() {
    let source = "a\nb";
    let payload = format!(
        r#"[
  {{ "set_line": {{ "anchor": "{}", "new_text": "+x\ny\n+z\nw" }} }}
]"#,
        line_ref(source, 2)
    );
    let edits: Vec<HashlineEdit> = serde_json::from_str(&payload).expect("edits should parse");

    let repaired = apply_hashline_edits_with_mode(source, &edits, HashlineApplyMode::Repair)
        .expect("repair apply should succeed");
    assert_eq!(repaired.content, "a\n+x\ny\n+z\nw");
}

#[test]
fn apply_repair_does_not_expand_merge_when_replacement_is_not_join_variant() {
    let source = "lhs &&\nrhs\ntail";
    let payload = format!(
        r#"[
  {{ "set_line": {{ "anchor": "{}", "new_text": "lhs && rhs // extra" }} }}
]"#,
        line_ref(source, 1)
    );
    let edits: Vec<HashlineEdit> = serde_json::from_str(&payload).expect("edits should parse");

    let repaired = apply_hashline_edits_with_mode(source, &edits, HashlineApplyMode::Repair)
        .expect("repair apply should succeed");
    assert_eq!(repaired.content, "lhs && rhs // extra\nrhs\ntail");
}

#[test]
fn apply_repair_does_not_strip_short_hashline_like_text() {
    let source = "a\nb";
    let payload = format!(
        r#"[
  {{ "set_line": {{ "anchor": "{}", "new_text": "1:a|literal" }} }}
]"#,
        line_ref(source, 2)
    );
    let edits: Vec<HashlineEdit> = serde_json::from_str(&payload).expect("edits should parse");

    let repaired = apply_hashline_edits_with_mode(source, &edits, HashlineApplyMode::Repair)
        .expect("repair apply should succeed");
    assert_eq!(repaired.content, "a\n1:a|literal");
}

#[test]
fn apply_replace_lines_can_delete_range() {
    let source = "a\nb\nc\nd";
    let payload = format!(
        r#"[
  {{ "replace_lines": {{ "start_anchor": "{}", "end_anchor": "{}", "new_text": "" }} }}
]"#,
        line_ref(source, 2),
        line_ref(source, 3)
    );
    let edits: Vec<HashlineEdit> = serde_json::from_str(&payload).expect("edits should parse");

    let applied = apply_hashline_edits(source, &edits).expect("apply should succeed");
    assert_eq!(applied.content, "a\nd");
}

#[test]
fn apply_insert_after_inserts_multiline_text() {
    let source = "a\nb";
    let payload = format!(
        r#"[
  {{ "insert_after": {{ "anchor": "{}", "text": "x\ny" }} }}
]"#,
        line_ref(source, 1)
    );
    let edits: Vec<HashlineEdit> = serde_json::from_str(&payload).expect("edits should parse");

    let applied = apply_hashline_edits(source, &edits).expect("apply should succeed");
    assert_eq!(applied.content, "a\nx\ny\nb");
}

#[test]
fn apply_insert_after_rejects_empty_text() {
    let source = "a\nb";
    let payload = format!(
        r#"[
  {{ "insert_after": {{ "anchor": "{}", "text": "" }} }}
]"#,
        line_ref(source, 1)
    );
    let edits: Vec<HashlineEdit> = serde_json::from_str(&payload).expect("edits should parse");

    let error = apply_hashline_edits(source, &edits).expect_err("empty insert text should fail");
    let HashlineApplyError::Check(check_error) = error else {
        panic!("expected check error");
    };
    assert!(
        check_error.to_string().contains("text must not be empty"),
        "unexpected error: {check_error}"
    );
}

#[test]
fn apply_preserves_existing_trailing_newline() {
    let source = "alpha\nbeta\n";
    let payload = format!(
        r#"[
  {{ "set_line": {{ "anchor": "{}", "new_text": "BETA" }} }}
]"#,
        line_ref(source, 2)
    );
    let edits: Vec<HashlineEdit> = serde_json::from_str(&payload).expect("edits should parse");

    let applied = apply_hashline_edits(source, &edits).expect("apply should succeed");
    assert_eq!(applied.content, "alpha\nBETA\n");
}

#[test]
fn apply_with_no_edits_is_noop() {
    let source = "alpha\nbeta\n";
    let edits = Vec::<HashlineEdit>::new();

    let applied = apply_hashline_edits(source, &edits).expect("empty apply should succeed");
    assert_eq!(applied.content, source);
    assert_eq!(applied.operations_total, 0);
    assert_eq!(applied.operations_applied, 0);
}

#[test]
fn apply_with_no_edits_on_empty_source_is_noop() {
    let source = "";
    let edits = Vec::<HashlineEdit>::new();

    let applied = apply_hashline_edits(source, &edits).expect("empty apply should succeed");
    assert_eq!(applied.content, "");
    assert_eq!(applied.operations_total, 0);
    assert_eq!(applied.operations_applied, 0);
}

#[test]
fn apply_replace_lines_rejects_end_before_start() {
    let source = "a\nb\nc";
    let payload = format!(
        r#"[
  {{ "replace_lines": {{ "start_anchor": "{}", "end_anchor": "{}", "new_text": "x" }} }}
]"#,
        line_ref(source, 3),
        line_ref(source, 1)
    );
    let edits: Vec<HashlineEdit> = serde_json::from_str(&payload).expect("edits should parse");

    let error = apply_hashline_edits(source, &edits).expect_err("reverse range should fail");
    let HashlineApplyError::Check(check_error) = error else {
        panic!("expected check error");
    };
    assert!(
        check_error.to_string().contains("must be >= start line"),
        "unexpected error: {check_error}"
    );
}

#[test]
fn apply_invalid_anchor_format_returns_check_error() {
    let source = "a\nb\nc";
    let payload = r#"
[
  { "set_line": { "anchor": "oops", "new_text": "x" } }
]
"#;
    let edits: Vec<HashlineEdit> = serde_json::from_str(payload).expect("edits should parse");

    let error = apply_hashline_edits(source, &edits).expect_err("invalid anchor should fail");
    let HashlineApplyError::Check(check_error) = error else {
        panic!("expected check error");
    };
    assert!(
        check_error.to_string().contains("expected format"),
        "unexpected error: {check_error}"
    );
}

#[test]
fn apply_rejects_duplicate_insert_after_same_anchor() {
    let source = "a\nb\nc";
    let payload = format!(
        r#"[
  {{ "insert_after": {{ "anchor": "{}", "text": "x" }} }},
  {{ "insert_after": {{ "anchor": "{}", "text": "y" }} }}
]"#,
        line_ref(source, 2),
        line_ref(source, 2)
    );
    let edits: Vec<HashlineEdit> = serde_json::from_str(&payload).expect("edits should parse");

    let error = apply_hashline_edits(source, &edits).expect_err("duplicate inserts should fail");
    let HashlineApplyError::Overlap {
        first_edit_index,
        second_edit_index,
        ..
    } = error
    else {
        panic!("expected overlap error");
    };
    assert_eq!(first_edit_index, 0);
    assert_eq!(second_edit_index, 1);
}

#[test]
fn apply_preserves_crlf_line_endings() {
    let source = "a\r\nb\r\nc\r\n";
    let payload = format!(
        r#"[
  {{ "set_line": {{ "anchor": "{}", "new_text": "B" }} }}
]"#,
        line_ref(source, 2)
    );
    let edits: Vec<HashlineEdit> = serde_json::from_str(&payload).expect("edits should parse");

    let applied = apply_hashline_edits(source, &edits).expect("apply should succeed");
    assert_eq!(applied.content, "a\r\nB\r\nc\r\n");
}

#[test]
fn apply_preserves_utf8_bom_when_not_editing_first_line() {
    let source = "\u{feff}a\nb\nc\n";
    let payload = format!(
        r#"[
  {{ "set_line": {{ "anchor": "{}", "new_text": "B" }} }}
]"#,
        line_ref(source, 2)
    );
    let edits: Vec<HashlineEdit> = serde_json::from_str(&payload).expect("edits should parse");

    let applied = apply_hashline_edits(source, &edits).expect("apply should succeed");
    assert_eq!(applied.content, "\u{feff}a\nB\nc\n");
}

#[test]
fn apply_insert_after_uses_source_crlf_for_inserted_multiline_text() {
    let source = "a\r\nb\r\n";
    let payload = format!(
        r#"[
  {{ "insert_after": {{ "anchor": "{}", "text": "x\ny" }} }}
]"#,
        line_ref(source, 1)
    );
    let edits: Vec<HashlineEdit> = serde_json::from_str(&payload).expect("edits should parse");

    let applied = apply_hashline_edits(source, &edits).expect("apply should succeed");
    assert_eq!(applied.content, "a\r\nx\r\ny\r\nb\r\n");
}

#[test]
fn apply_insert_after_last_line_preserves_no_trailing_newline() {
    let source = "a\nb";
    let payload = format!(
        r#"[
  {{ "insert_after": {{ "anchor": "{}", "text": "x" }} }}
]"#,
        line_ref(source, 2)
    );
    let edits: Vec<HashlineEdit> = serde_json::from_str(&payload).expect("edits should parse");

    let applied = apply_hashline_edits(source, &edits).expect("apply should succeed");
    assert_eq!(applied.content, "a\nb\nx");
}

#[test]
fn apply_set_line_allows_empty_line_content() {
    let source = "a\nb\nc";
    let payload = format!(
        r#"[
  {{ "set_line": {{ "anchor": "{}", "new_text": "" }} }}
]"#,
        line_ref(source, 2)
    );
    let edits: Vec<HashlineEdit> = serde_json::from_str(&payload).expect("edits should parse");

    let applied = apply_hashline_edits(source, &edits).expect("apply should succeed");
    assert_eq!(applied.content, "a\n\nc");
}

#[test]
fn apply_replace_lines_normalizes_crlf_payload_text() {
    let source = "a\r\nb\r\n";
    let payload = format!(
        r#"[
  {{ "replace_lines": {{ "start_anchor": "{}", "new_text": "x\r\ny" }} }}
]"#,
        line_ref(source, 2)
    );
    let edits: Vec<HashlineEdit> = serde_json::from_str(&payload).expect("edits should parse");

    let applied = apply_hashline_edits(source, &edits).expect("apply should succeed");
    assert_eq!(applied.content, "a\r\nx\r\ny\r\n");
}

#[test]
fn apply_returns_precondition_failed_with_check_diagnostics() {
    let source = "alpha\nbeta";
    let stale_anchor = format_line_ref(2, &compute_line_hash("BETA"));
    let payload = format!(
        r#"[
  {{ "set_line": {{ "anchor": "{stale_anchor}", "new_text": "new" }} }}
]"#
    );
    let edits: Vec<HashlineEdit> = serde_json::from_str(&payload).expect("edits should parse");

    let error = apply_hashline_edits(source, &edits).expect_err("apply should fail");
    let HashlineApplyError::PreconditionFailed { check } = error else {
        panic!("expected precondition failure");
    };

    assert!(!check.ok);
    assert_eq!(check.summary.total, 1);
    assert_eq!(check.summary.mismatched, 1);
    assert_eq!(check.mismatches.len(), 1);
    assert_eq!(check.mismatches[0].status, HashlineMismatchStatus::Mismatch);
}

#[test]
fn apply_mixed_valid_and_stale_edits_fails_precondition_for_all() {
    let source = "alpha\nbeta\ngamma";
    let stale_beta = format_line_ref(2, &compute_line_hash("BETA"));
    let payload = format!(
        r#"[
  {{ "set_line": {{ "anchor": "{}", "new_text": "ALPHA" }} }},
  {{ "set_line": {{ "anchor": "{stale_beta}", "new_text": "BETA" }} }}
]"#,
        line_ref(source, 1),
    );
    let edits: Vec<HashlineEdit> = serde_json::from_str(&payload).expect("edits should parse");

    let error = apply_hashline_edits(source, &edits).expect_err("stale edit should fail batch");
    let HashlineApplyError::PreconditionFailed { check } = error else {
        panic!("expected precondition failure");
    };
    assert!(!check.ok);
    assert_eq!(check.summary.total, 2);
    assert_eq!(check.summary.matched, 1);
    assert_eq!(check.summary.mismatched, 1);
    assert_eq!(check.mismatches.len(), 1);
    assert_eq!(check.mismatches[0].edit_index, 1);
}

#[test]
fn apply_out_of_range_anchor_is_reported_as_precondition_mismatch() {
    let source = "alpha\nbeta";
    let payload = r#"
[
  { "set_line": { "anchor": "99:abcdef123456", "new_text": "x" } }
]
"#;
    let edits: Vec<HashlineEdit> = serde_json::from_str(payload).expect("edits should parse");

    let error = apply_hashline_edits(source, &edits).expect_err("apply should fail");
    let HashlineApplyError::PreconditionFailed { check } = error else {
        panic!("expected precondition failure");
    };
    assert_eq!(check.summary.total, 1);
    assert_eq!(check.summary.mismatched, 1);
    assert_eq!(check.mismatches[0].line, 99);
    assert!(check.mismatches[0].actual_hash.is_none());
}

#[test]
fn apply_rejects_overlapping_replace_ranges() {
    let source = "a\nb\nc\nd";
    let payload = format!(
        r#"[
  {{ "replace_lines": {{ "start_anchor": "{}", "end_anchor": "{}", "new_text": "B" }} }},
  {{ "replace_lines": {{ "start_anchor": "{}", "end_anchor": "{}", "new_text": "C" }} }}
]"#,
        line_ref(source, 1),
        line_ref(source, 3),
        line_ref(source, 3),
        line_ref(source, 4)
    );
    let edits: Vec<HashlineEdit> = serde_json::from_str(&payload).expect("edits should parse");

    let error = apply_hashline_edits(source, &edits).expect_err("overlap should fail");
    let HashlineApplyError::Overlap {
        first_edit_index,
        second_edit_index,
        ..
    } = error
    else {
        panic!("expected overlap error");
    };

    assert_eq!(first_edit_index, 0);
    assert_eq!(second_edit_index, 1);
}

#[test]
fn apply_rejects_insert_at_replace_boundary() {
    let source = "a\nb\nc";
    let payload = format!(
        r#"[
  {{ "replace_lines": {{ "start_anchor": "{}", "new_text": "B" }} }},
  {{ "insert_after": {{ "anchor": "{}", "text": "X" }} }}
]"#,
        line_ref(source, 2),
        line_ref(source, 1)
    );
    let edits: Vec<HashlineEdit> = serde_json::from_str(&payload).expect("edits should parse");

    let error = apply_hashline_edits(source, &edits).expect_err("boundary overlap should fail");
    assert!(matches!(error, HashlineApplyError::Overlap { .. }));
}

#[test]
fn apply_uses_bottom_up_order_for_stable_offsets() {
    let source = "line1\nline2\nline3\nline4";
    let payload = format!(
        r#"[
  {{ "replace_lines": {{ "start_anchor": "{}", "new_text": "LINE2" }} }},
  {{ "replace_lines": {{ "start_anchor": "{}", "new_text": "LINE4" }} }}
]"#,
        line_ref(source, 2),
        line_ref(source, 4)
    );
    let edits: Vec<HashlineEdit> = serde_json::from_str(&payload).expect("edits should parse");

    let applied = apply_hashline_edits(source, &edits).expect("apply should succeed");
    assert_eq!(applied.content, "line1\nLINE2\nline3\nLINE4");
}

#[test]
fn apply_multiline_set_then_lower_set_keeps_anchor_stable() {
    let source = "a\nb\nc\nd";
    let payload = format!(
        r#"[
  {{ "set_line": {{ "anchor": "{}", "new_text": "B1\nB2" }} }},
  {{ "set_line": {{ "anchor": "{}", "new_text": "D!" }} }}
]"#,
        line_ref(source, 2),
        line_ref(source, 4)
    );
    let edits: Vec<HashlineEdit> = serde_json::from_str(&payload).expect("edits should parse");

    let applied = apply_hashline_edits(source, &edits).expect("apply should succeed");
    assert_eq!(applied.content, "a\nB1\nB2\nc\nD!");
}

#[test]
fn apply_rejects_insert_after_touching_replace_end_boundary() {
    let source = "a\nb\nc\nd";
    let payload = format!(
        r#"[
  {{ "replace_lines": {{ "start_anchor": "{}", "end_anchor": "{}", "new_text": "BC" }} }},
  {{ "insert_after": {{ "anchor": "{}", "text": "X" }} }}
]"#,
        line_ref(source, 2),
        line_ref(source, 3),
        line_ref(source, 3)
    );
    let edits: Vec<HashlineEdit> = serde_json::from_str(&payload).expect("edits should parse");

    let error = apply_hashline_edits(source, &edits).expect_err("boundary-touch should fail");
    assert!(matches!(error, HashlineApplyError::Overlap { .. }));
}

#[test]
fn apply_rejects_insert_after_touching_replace_start_boundary() {
    let source = "a\nb\nc\nd";
    let payload = format!(
        r#"[
  {{ "replace_lines": {{ "start_anchor": "{}", "end_anchor": "{}", "new_text": "BC" }} }},
  {{ "insert_after": {{ "anchor": "{}", "text": "X" }} }}
]"#,
        line_ref(source, 2),
        line_ref(source, 3),
        line_ref(source, 1)
    );
    let edits: Vec<HashlineEdit> = serde_json::from_str(&payload).expect("edits should parse");

    let error = apply_hashline_edits(source, &edits).expect_err("boundary-touch should fail");
    assert!(matches!(error, HashlineApplyError::Overlap { .. }));
}

#[test]
fn check_hashline_edits_counts_multiple_stale_anchors_in_one_edit() {
    let source = "a\nb\nc";
    let stale_a = format_line_ref(1, &compute_line_hash("stale-a"));
    let stale_b = format_line_ref(2, &compute_line_hash("stale-b"));
    let payload = format!(
        r#"[
  {{ "replace_lines": {{ "start_anchor": "{stale_a}", "end_anchor": "{stale_b}", "new_text": "x" }} }}
]"#
    );
    let edits: Vec<HashlineEdit> = serde_json::from_str(&payload).expect("edits should parse");

    let check = check_hashline_edits(source, &edits).expect("check should succeed");
    assert!(!check.ok);
    assert_eq!(check.summary.total, 2);
    assert_eq!(check.summary.mismatched, 2);
    assert_eq!(check.mismatches.len(), 2);
    assert_eq!(check.mismatches[0].edit_index, 0);
    assert_eq!(check.mismatches[1].edit_index, 0);
}

#[test]
fn apply_mixed_newline_source_normalizes_to_lf() {
    let source = "a\r\nb\nc\r\n";
    let payload = format!(
        r#"[
  {{ "set_line": {{ "anchor": "{}", "new_text": "B" }} }}
]"#,
        line_ref(source, 2)
    );
    let edits: Vec<HashlineEdit> = serde_json::from_str(&payload).expect("edits should parse");

    let applied = apply_hashline_edits(source, &edits).expect("apply should succeed");
    assert_eq!(applied.content, "a\nB\nc\n");
}
