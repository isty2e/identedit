use proptest::prelude::*;

use super::*;

fn diff(before: &str, after: &str) -> String {
    render_file_diff(Path::new("sample.txt"), before, after, ColorMode::Never)
}

// Independent unified-patch consumer: validate coordinates, counts, old bytes,
// context and EOF markers, not the renderer's internal operation sequence.
fn replay(before: &str, patch: &str) -> String {
    if patch.is_empty() {
        return before.to_owned();
    }
    let source: Vec<_> = before.split_inclusive('\n').collect();
    let lines: Vec<_> = patch.split_inclusive('\n').collect();
    assert!(lines[0].starts_with("--- "));
    assert!(lines[1].starts_with("+++ "));
    let mut result = String::new();
    let mut cursor = 0;
    let mut index = 2;
    let mut new_cursor = 0;
    while index < lines.len() {
        let header: Vec<_> = lines[index].split_whitespace().collect();
        assert_eq!(header[0], "@@");
        let parse_range = |value: &str| {
            let (start, count) = value[1..].split_once(',').unwrap();
            (
                start.parse::<usize>().unwrap(),
                count.parse::<usize>().unwrap(),
            )
        };
        let (old_start, old_count) = parse_range(header[1]);
        let (new_start, new_count) = parse_range(header[2]);
        let start = old_start - usize::from(old_count > 0);
        assert!(start >= cursor);
        result.extend(source[cursor..start].iter().copied());
        new_cursor += start - cursor;
        cursor = start;
        assert_eq!(new_start - usize::from(new_count > 0), new_cursor);
        index += 1;
        let (mut consumed, mut produced) = (0, 0);
        while index < lines.len() && !lines[index].starts_with("@@ ") {
            let prefix = lines[index].as_bytes()[0];
            assert!(matches!(prefix, b' ' | b'-' | b'+'));
            let mut text = &lines[index][1..];
            index += 1;
            if lines.get(index) == Some(&"\\ No newline at end of file\n") {
                text = text.strip_suffix('\n').unwrap();
                index += 1;
            }
            if prefix != b'+' {
                assert_eq!(source[cursor], text);
                cursor += 1;
                consumed += 1;
            }
            if prefix != b'-' {
                result.push_str(text);
                produced += 1;
            }
        }
        assert_eq!((consumed, produced), (old_count, new_count));
        new_cursor += produced;
    }
    result.extend(source[cursor..].iter().copied());
    result
}

#[test]
fn newline_and_empty_file_matrix_replays_exactly() {
    let texts = [
        "",
        "a",
        "a\n",
        "a\r",
        "a\r\n",
        "a\rb\nc\r\n",
        "\n\n",
        "\u{feff}é\n終",
        "--- a\n+++ b\n@@\n\\ No newline at end of file\n",
    ];
    for before in texts {
        for after in texts {
            assert_eq!(
                replay(before, &diff(before, after)),
                after,
                "{before:?} -> {after:?}"
            );
        }
    }
}

#[test]
fn distant_edits_in_a_large_file_stay_in_separate_compact_hunks() {
    let before: String = (0..10_000).map(|n| format!("line {n}\n")).collect();
    let after = before
        .replace("line 10\n", "changed 10\n")
        .replace("line 9900\n", "changed 9900\n");
    let patch = diff(&before, &after);
    assert_eq!(patch.matches("@@ -").count(), 2);
    assert!(patch.lines().count() < 25);
    assert_eq!(replay(&before, &patch), after);
}

#[test]
fn context_merges_at_six_unchanged_lines_and_splits_at_seven() {
    for gap in [6, 7] {
        let middle = "context\n".repeat(gap);
        let before = format!("old a\n{middle}old b\n");
        let after = format!("new a\n{middle}new b\n");
        let patch = diff(&before, &after);
        assert_eq!(patch.matches("@@ -").count(), if gap == 6 { 1 } else { 2 });
        assert_eq!(replay(&before, &patch), after);
    }
}

#[test]
fn fallback_limits_are_inclusive_and_preserve_all_bytes() {
    let half = MAX_ALIGNMENT_LINES / 2;
    assert!(alignment_within_budget(
        &vec!["a\n"; half],
        &vec!["b\n"; half]
    ));
    assert!(!alignment_within_budget(
        &vec!["a\n"; half + 1],
        &vec!["b\n"; half]
    ));
    let long_line = "x".repeat(MAX_ALIGNMENT_BYTES);
    assert!(alignment_within_budget(&[&long_line], &[]));
    assert!(!alignment_within_budget(&[&long_line], &["y"]));

    let before = format!("prefix\n{}suffix\n", "a\n".repeat(half + 1));
    let after = format!("prefix\n{}suffix\n", "b\r\n".repeat(half));
    let patch = diff(&before, &after);
    assert_eq!(patch, diff(&before, &after));
    assert_eq!(patch.matches("@@ -").count(), 1);
    assert_eq!(replay(&before, &patch), after);
}

#[test]
fn bare_cr_is_data_and_does_not_increment_hunk_coordinates() {
    let before = "a\rb\nc\nd\ne\nf\ng\nh\ni\n";
    let after = before.replace("i\n", "I\n");
    let patch = diff(before, &after);
    assert!(patch.contains("@@ -5,4 +5,4 @@"), "{patch}");
    assert_eq!(replay(before, &patch), after);
}

#[test]
fn color_is_optional_without_changing_payload_bytes() {
    let before = "old\r\n";
    let after = "new\r\n";
    let plain = diff(before, after);
    let color = render_file_diff(Path::new("sample.txt"), before, after, ColorMode::Always);
    let stripped = [RED, GREEN, CYAN, RESET]
        .iter()
        .fold(color, |s, ansi| s.replace(ansi, ""));
    assert_eq!(stripped, plain);
}

#[test]
fn color_reset_precedes_the_physical_line_terminator() {
    for (text, expected) in [
        ("+new\n", "\x1b[32m+new\x1b[0m\n"),
        ("+new\r\n", "\x1b[32m+new\x1b[0m\r\n"),
        ("+new", "\x1b[32m+new\x1b[0m"),
        ("+new\r", "\x1b[32m+new\r\x1b[0m"),
    ] {
        let mut output = String::new();
        render_colored(&mut output, text, GREEN, true);
        assert_eq!(output, expected);
    }
}

#[test]
fn largest_admitted_disjoint_rewrite_replays_exactly() {
    let before: String = (0..MAX_ALIGNMENT_LINES / 2)
        .map(|n| format!("old {n}\n"))
        .collect();
    let after: String = (0..MAX_ALIGNMENT_LINES / 2)
        .map(|n| format!("new {n}\n"))
        .collect();
    let patch = diff(&before, &after);
    assert_eq!(replay(&before, &patch), after);
}

fn arbitrary_text() -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop::sample::select(vec![
            "a", "b", " ", "\t", "\r", "\n", "é", "終", "\u{feff}", "@@", "-", "+", "\\",
        ]),
        0..180,
    )
    .prop_map(|parts| parts.concat())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]
    #[test]
    fn arbitrary_text_diff_replays_byte_exactly(before in arbitrary_text(), after in arbitrary_text()) {
        let patch = diff(&before, &after);
        prop_assert_eq!(replay(&before, &patch), after.as_str());
        prop_assert_eq!(patch, diff(&before, &after));
    }

    #[test]
    fn edits_among_repeated_lines_keep_context_and_offsets(
        mut lines in prop::collection::vec(prop::sample::select(vec!["same\n", "\n", "a\r\n", "a\rb\n", "é\n"]), 1..120),
        positions in prop::collection::vec(0usize..120, 1..10),
    ) {
        let before = lines.concat();
        for position in positions {
            let index = position % lines.len();
            lines[index] = "changed\r\n";
        }
        let after = lines.concat();
        prop_assert_eq!(replay(&before, &diff(&before, &after)), after);
    }
}
