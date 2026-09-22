use std::fs;
use std::path::Path;

use serde_json::Value;
use tempfile::tempdir;

mod common;

fn read_json(file: &Path, options: &[&str]) -> Value {
    let mut args = vec!["read", "--json"];
    args.extend_from_slice(options);
    args.push(file.to_str().unwrap());
    let output = common::run_identedit(&args);
    assert!(
        output.status.success(),
        "{} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn bounded_lines_preserve_original_anchors_and_can_patch() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("example.txt");
    let source = "same\r\nsame\rsame\nlast";
    fs::write(&file, source).unwrap();
    let result = read_json(&file, &["--mode", "line", "--offset", "2", "--limit", "1"]);
    let lines = result["handles"].as_array().unwrap();
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["line"], 2);
    assert_eq!(
        lines[0]["anchor"],
        format!("2:{}", common::compute_line_hash("same"))
    );
    assert_eq!(result["windows"][0]["omitted_before"], 1);
    assert_eq!(result["windows"][0]["omitted_after"], 2);
    assert_eq!(
        result["file_preconditions"][0]["expected_file_hash"],
        common::hash_text(source)
    );
    assert_eq!(fs::read_to_string(&file).unwrap(), source);

    let output = common::run_identedit(&[
        "patch",
        file.to_str().unwrap(),
        "--at",
        lines[0]["anchor"].as_str().unwrap(),
        "--set-line",
        "changed",
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert_eq!(
        fs::read_to_string(file).unwrap(),
        "same\r\nchanged\rsame\nlast"
    );
}

#[test]
fn symbol_context_separates_complete_target_and_original_lines() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("example.py");
    let source = "# header\nclass Box:\n    def get(self):\n        return '\u{d55c}\u{ae00}'\n\n# trailer\nx = 1\n";
    fs::write(&file, source).unwrap();
    let result = read_json(&file, &["--symbol", "Box.get", "--context", "1"]);
    assert_eq!(result["handles"].as_array().unwrap().len(), 1);
    let node = &result["handles"][0];
    assert_eq!(node["name"], "get");
    assert!(node.get("text").is_none());
    let window = &result["windows"][0];
    assert_eq!(window["kind"], "symbol");
    assert_eq!(window["start_line"], 2);
    assert_eq!(window["end_line"], 5);
    assert_eq!(
        window["target_lines"],
        serde_json::json!({"start":3,"end":4})
    );
    assert_eq!(window["lines"][1]["text"], "    def get(self):");
    assert_eq!(
        window["lines"][1]["anchor"],
        format!("3:{}", common::compute_line_hash("    def get(self):"))
    );
    let start = node["span"]["start"].as_u64().unwrap() as usize;
    let end = node["span"]["end"].as_u64().unwrap() as usize;
    assert_eq!(
        &source[start..end],
        "def get(self):\n        return '\u{d55c}\u{ae00}'"
    );

    let output = common::run_identedit(&[
        "patch",
        file.to_str().unwrap(),
        "--at",
        node["identity"].as_str().unwrap(),
        "--replace",
        "def get(self):\n        return 42",
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert_eq!(
        fs::read_to_string(file).unwrap(),
        source.replace("return '\u{d55c}\u{ae00}'", "return 42")
    );
}

#[test]
fn symbol_text_renders_source_once_without_added_indentation() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("example.py");
    fs::write(
        &file,
        "class Box:\n    def get(self):\n        return 42\n# after\n",
    )
    .unwrap();
    let output = common::run_identedit(&[
        "read",
        file.to_str().unwrap(),
        "--symbol",
        "Box.get",
        "--context",
        "1",
        "--verbose",
    ]);
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    assert_eq!(text.matches("def get(self):").count(), 1);
    assert!(text.contains(&format!(
        "2:{}|    def get(self):",
        common::compute_line_hash("    def get(self):")
    )));
    assert!(text.contains("# target lines 2-3"));
    assert!(text.contains("# context"));
    let json = read_json(&file, &["--symbol", "Box.get"]);
    assert!(text.contains(&format!(
        "# node --at {}",
        json["handles"][0]["identity"].as_str().unwrap()
    )));
    assert!(text.contains("2:5..3:18 (1-based byte columns; end exclusive)"));
}

#[test]
fn bounded_empty_and_past_eof_reads_report_coverage() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("empty.txt");
    let huge_offset = usize::MAX.to_string();
    for (source, offset, total) in [
        ("", "1", 0),
        ("a\n", "2", 1),
        ("a\n", huge_offset.as_str(), 1),
    ] {
        fs::write(&file, source).unwrap();
        let result = read_json(
            &file,
            &["--mode", "line", "--offset", offset, "--limit", "2"],
        );
        assert!(result["handles"].as_array().unwrap().is_empty());
        assert_eq!(result["windows"][0]["total_lines"], total);
        assert!(result["windows"][0]["start_line"].is_null());
        assert!(result["windows"][0]["end_line"].is_null());
        assert_eq!(result["windows"][0]["omitted_before"], total);
        assert_eq!(result["windows"][0]["omitted_after"], 0);
    }
}

#[test]
fn invalid_read_option_combinations_are_not_ignored() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("example.py");
    fs::write(&file, "def f():\n    pass\n").unwrap();
    let cases: &[&[&str]] = &[
        &["--context", "1"],
        &["--offset", "1"],
        &["--limit", "1"],
        &["--symbol", "f", "--kind", "function_definition"],
        &["--symbol", "f", "--name", "f"],
        &["--symbol", "f", "--exclude-kind", "identifier"],
        &["--mode", "line", "--symbol", "f"],
        &["--mode", "line", "--context", "1"],
        &["--mode", "line", "--offset", "0"],
        &["--mode", "line", "--limit", "0"],
        &["--symbol", "   "],
    ];
    for options in cases {
        let mut args = vec!["read", "--json", file.to_str().unwrap()];
        args.extend_from_slice(options);
        let output = common::run_identedit(&args);
        assert!(!output.status.success(), "accepted {options:?}");
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("invalid_request"),
            "{options:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn symbol_ambiguity_matches_patch_diagnostics() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("example.py");
    fs::write(
        &file,
        "class A:\n    def get(self):\n        pass\nclass B:\n    def get(self):\n        pass\n",
    )
    .unwrap();
    let read =
        common::run_identedit(&["read", file.to_str().unwrap(), "--symbol", "get", "--json"]);
    let patch = common::run_identedit(&[
        "patch",
        file.to_str().unwrap(),
        "--symbol",
        "get",
        "--delete",
    ]);
    assert!(!read.status.success());
    assert!(!patch.status.success());
    let read: Value = serde_json::from_slice(&read.stdout).unwrap();
    let patch: Value = serde_json::from_slice(&patch.stdout).unwrap();
    assert_eq!(read["error"], patch["error"]);
}

#[test]
fn symbol_and_line_windows_agree_on_mixed_newlines_bom_and_unicode() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("example.js");
    let huge_context = usize::MAX.to_string();
    for (source, target_start, target_end) in [
        (
            "\u{feff}// \u{3b1}\r\nfunction target() {\r  return '\u{d55c}\u{ae00}';\n}\r\n// end",
            2,
            4,
        ),
        ("function target() {}", 1, 1),
        ("function target() {}\r\n", 1, 1),
        ("const \u{3b1} = 1; function target() {} // same line", 1, 1),
    ] {
        fs::write(&file, source).unwrap();
        let result = read_json(
            &file,
            &[
                "--symbol",
                "target",
                "--context",
                &huge_context,
                "--verbose",
            ],
        );
        let all = read_json(&file, &["--mode", "line"]);
        let window = &result["windows"][0];
        assert_eq!(
            window["target_lines"],
            serde_json::json!({"start":target_start,"end":target_end})
        );
        assert_eq!(window["omitted_before"], 0);
        assert_eq!(window["omitted_after"], 0);
        let lines = window["lines"].as_array().unwrap();
        assert_eq!(lines.len(), all["handles"].as_array().unwrap().len());
        for (context, line) in lines.iter().zip(all["handles"].as_array().unwrap()) {
            assert_eq!(context["anchor"], line["anchor"]);
            assert_eq!(context["text"], line["text"]);
            assert_eq!(context["line"], line["line"]);
        }
        let handle = &result["handles"][0];
        let start = handle["span"]["start"].as_u64().unwrap() as usize;
        let end = handle["span"]["end"].as_u64().unwrap() as usize;
        assert_eq!(handle["text"], &source[start..end]);
        assert_eq!(
            handle["expected_old_hash"],
            common::hash_text(&source[start..end])
        );
    }
}

#[test]
fn symbol_read_defaults_to_complete_target_without_context() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("example.py");
    let source = format!(
        "# before\ndef target():\n{}# after\n",
        "    x = 1\n".repeat(300)
    );
    fs::write(&file, source).unwrap();
    let result = read_json(&file, &["--symbol", "target"]);
    let window = &result["windows"][0];
    assert_eq!(window["start_line"], 2);
    assert_eq!(window["end_line"], 302);
    assert_eq!(window["lines"].as_array().unwrap().len(), 301);
    assert_eq!(window["omitted_before"], 1);
    assert_eq!(window["omitted_after"], 1);
}

#[test]
fn symbol_context_line_anchor_is_usable_and_stale_anchor_is_rejected() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("example.py");
    fs::write(&file, "# outside\ndef target():\n    return 1\n").unwrap();
    let result = read_json(&file, &["--symbol", "target", "--context", "1"]);
    let anchor = result["windows"][0]["lines"][0]["anchor"].as_str().unwrap();
    let first = common::run_identedit(&[
        "patch",
        file.to_str().unwrap(),
        "--at",
        anchor,
        "--set-line",
        "# updated",
    ]);
    assert!(first.status.success());
    let second = common::run_identedit(&[
        "patch",
        file.to_str().unwrap(),
        "--at",
        anchor,
        "--set-line",
        "# should not apply",
    ]);
    assert!(!second.status.success());
    assert_eq!(
        fs::read_to_string(file).unwrap(),
        "# updated\ndef target():\n    return 1\n"
    );
}

#[test]
fn symbol_read_handles_can_compile_and_apply_without_context_in_target() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("example.py");
    fs::write(&file, "# before\ndef target():\n    return 1\n# after\n").unwrap();
    let result = read_json(&file, &["--symbol", "target", "--context", "1"]);
    let node = &result["handles"][0];
    let request = serde_json::json!({
        "command":"edit", "file":file,
        "operations":[{"target":{"type":"node", "identity":node["identity"], "kind":node["kind"], "expected_old_hash":node["expected_old_hash"], "span_hint":node["span"]},
        "op":{"type":"replace", "new_text":"def target():\n    return 2"}}]
    });
    let edit = common::run_identedit_with_stdin(&["edit", "--json"], &request.to_string());
    assert!(
        edit.status.success(),
        "{}",
        String::from_utf8_lossy(&edit.stdout)
    );
    let apply =
        common::run_identedit_with_stdin(&["apply"], &String::from_utf8(edit.stdout).unwrap());
    assert!(
        apply.status.success(),
        "{}",
        String::from_utf8_lossy(&apply.stdout)
    );
    assert_eq!(
        fs::read_to_string(file).unwrap(),
        "# before\ndef target():\n    return 2\n# after\n"
    );
}

#[test]
fn symbol_multi_file_reads_fail_without_partial_results() {
    let dir = tempdir().unwrap();
    let first = dir.path().join("first.py");
    let second = dir.path().join("second.py");
    fs::write(&first, "def target():\n    pass\n").unwrap();
    fs::write(&second, "def target():\n    return 1\n").unwrap();
    let args = [
        "read",
        "--json",
        "--symbol",
        "target",
        first.to_str().unwrap(),
        second.to_str().unwrap(),
    ];
    let success = common::run_identedit(&args);
    assert!(success.status.success());
    let response: Value = serde_json::from_slice(&success.stdout).unwrap();
    assert_eq!(response["summary"]["files_scanned"], 2);
    assert_eq!(response["summary"]["matches"], 2);
    assert_eq!(response["windows"].as_array().unwrap().len(), 2);

    fs::write(&second, "def other():\n    pass\n").unwrap();
    let failure = common::run_identedit(&args);
    assert!(!failure.status.success());
    let response: Value = serde_json::from_slice(&failure.stdout).unwrap();
    assert_eq!(response["error"]["type"], "target_missing");
    assert!(response.get("handles").is_none());
    assert!(response.get("windows").is_none());
}

#[test]
fn new_flags_are_rejected_in_stdin_selector_mode_instead_of_ignored() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("example.py");
    fs::write(&file, "def target():\n    pass\n").unwrap();
    let request = serde_json::json!({"command":"read", "file":file, "selector":{"kind":"function_definition"}}).to_string();
    for flag in ["--symbol", "--context", "--offset", "--limit"] {
        let output = common::run_identedit_with_stdin(&["read", "--json", flag, "1"], &request);
        assert!(!output.status.success(), "ignored {flag}");
        let result: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(result["error"]["type"], "invalid_request");
        assert!(
            result["error"]["message"]
                .as_str()
                .unwrap()
                .contains("FILE")
        );
    }
}

#[test]
fn paging_matches_unbounded_slice_for_varied_line_layouts() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("example.txt");
    let huge = usize::MAX.to_string();
    for source in [
        "",
        "\n",
        "\r\n\r",
        "\u{feff}a\r\n\r\u{3b2}\n\nlast",
        "same\nsame\nsame",
    ] {
        fs::write(&file, source).unwrap();
        let all = read_json(&file, &["--mode", "line"]);
        assert!(all.get("windows").is_none());
        let handles = all["handles"].as_array().unwrap();
        for (offset, limit) in [(1, "1"), (2, "2"), (2, huge.as_str())] {
            let offset_arg = offset.to_string();
            let result = read_json(
                &file,
                &["--mode", "line", "--offset", &offset_arg, "--limit", limit],
            );
            let start = (offset - 1).min(handles.len());
            let end = start
                .saturating_add(limit.parse::<usize>().unwrap())
                .min(handles.len());
            assert_eq!(result["handles"], serde_json::json!(&handles[start..end]));
            assert_eq!(result["summary"]["matches"], end - start);
            assert_eq!(result["windows"][0]["omitted_before"], start);
            assert_eq!(result["windows"][0]["omitted_after"], handles.len() - end);
        }
    }
}

#[test]
fn paging_reports_empty_files_in_multi_file_text_output() {
    let dir = tempdir().unwrap();
    let empty = dir.path().join("empty.txt");
    let full = dir.path().join("full.txt");
    fs::write(&empty, "").unwrap();
    fs::write(&full, "first\nsecond\nthird\n").unwrap();
    let output = common::run_identedit(&[
        "read",
        "--mode",
        "line",
        "--offset",
        "2",
        "--limit",
        "1",
        empty.to_str().unwrap(),
        full.to_str().unwrap(),
    ]);
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains(&format!("## {}", empty.display())));
    assert!(text.contains("# window lines none of 0; omitted before=0 after=0"));
    assert!(text.contains("# window lines 2-2 of 3; omitted before=1 after=1"));
    assert!(text.contains(&format!("2:{}|second", common::compute_line_hash("second"))));
    assert!(!text.contains("|first"));
    assert!(!text.contains("|third"));
}

#[test]
fn omitted_paging_bounds_and_zero_context_have_explicit_defaults() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("example.py");
    fs::write(&file, "# before\ndef target():\n\treturn 1\n# after\n").unwrap();
    let all = read_json(&file, &["--mode", "line"]);
    let handles = all["handles"].as_array().unwrap();
    let suffix = read_json(&file, &["--mode", "line", "--offset", "2"]);
    let prefix = read_json(&file, &["--mode", "line", "--limit", "2"]);
    assert_eq!(suffix["handles"], serde_json::json!(&handles[1..]));
    assert_eq!(prefix["handles"], serde_json::json!(&handles[..2]));
    let symbol = read_json(&file, &["--symbol", "target"]);
    let zero = read_json(&file, &["--symbol", " target ", "--context", "0"]);
    assert_eq!(symbol, zero);
    assert_eq!(symbol["windows"][0]["lines"][1]["text"], "\treturn 1");
}

#[test]
fn bounded_reads_keep_duplicate_file_rejection() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("example.py");
    fs::write(&file, "def target():\n    pass\n").unwrap();
    for flags in [
        vec!["--symbol", "target"],
        vec!["--mode", "line", "--limit", "1"],
    ] {
        let mut args = vec![
            "read",
            "--json",
            file.to_str().unwrap(),
            file.to_str().unwrap(),
        ];
        args.extend(flags);
        let output = common::run_identedit(&args);
        assert!(!output.status.success());
        let error: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(error["error"]["type"], "invalid_request");
        assert!(
            error["error"]["message"]
                .as_str()
                .unwrap()
                .contains("Duplicate file")
        );
    }
}
