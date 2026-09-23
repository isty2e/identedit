use super::*;

fn line_anchor(line: usize, content: &str) -> String {
    format!("{line}:{}", common::compute_line_hash(content))
}

fn text_file(source: &str) -> PathBuf {
    let file = Builder::new().suffix(".txt").tempfile().unwrap();
    fs::write(file.path(), source).unwrap();
    file.keep().unwrap().1
}

#[test]
fn common_line_verbs_are_logical_in_edit_and_apply() {
    let cases = [
        (
            "replace",
            json!({ "type": "replace", "new_text": "B" }),
            None,
            "a\r\nB\r\nc\r\n",
            "set_line",
        ),
        (
            "blank",
            json!({ "type": "replace", "new_text": "" }),
            None,
            "a\r\n\r\nc\r\n",
            "set_line",
        ),
        (
            "range blank",
            json!({ "type": "replace", "new_text": "" }),
            Some(3),
            "a\r\n\r\n",
            "blank_lines",
        ),
        (
            "delete",
            json!({ "type": "delete" }),
            None,
            "a\r\nc\r\n",
            "delete_lines",
        ),
        (
            "range delete",
            json!({ "type": "delete" }),
            Some(3),
            "a\r\n",
            "delete_lines",
        ),
        (
            "insert",
            json!({ "type": "insert_after", "new_text": "X" }),
            None,
            "a\r\nb\r\nX\r\nc\r\n",
            "insert_after_line",
        ),
    ];

    for (name, op, end, expected, plan_op) in cases {
        let file = text_file("a\r\nb\r\nc\r\n");
        let mut target = json!({ "type": "line", "anchor": line_anchor(2, "b") });
        if end.is_some() {
            target["end_anchor"] = json!(line_anchor(3, "c"));
        }
        let request = json!({ "command": "edit", "file": file, "operations": [{ "target": target, "op": op }] });

        let edit = run_identedit_with_stdin(&["edit", "--json"], &request.to_string());
        assert!(
            edit.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&edit.stdout)
        );
        let plan: Value = serde_json::from_slice(&edit.stdout).unwrap();
        assert_eq!(
            plan["files"][0]["operations"][0]["op"]["type"], plan_op,
            "{name}"
        );

        let apply = run_identedit_with_stdin(&["apply"], &plan.to_string());
        assert!(
            apply.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&apply.stdout)
        );
        assert_eq!(fs::read_to_string(&file).unwrap(), expected, "{name}");
    }
}

#[test]
fn common_line_verbs_are_logical_in_patch_flag_and_json_modes() {
    for mode in ["flag", "json"] {
        let file = text_file("a\nb\nc\n");
        let anchor = line_anchor(2, "b");
        let output = if mode == "flag" {
            run_identedit(&[
                "patch",
                "--at",
                &anchor,
                "--insert-after",
                "X",
                file.to_str().unwrap(),
            ])
        } else {
            let request = json!({
                "command": "patch", "file": file, "target": {"type": "line", "anchor": anchor},
                "op": {"type": "insert_after", "new_text": "X"}
            });
            run_identedit_with_stdin(&["patch", "--json"], &request.to_string())
        };
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stdout)
        );
        assert_eq!(fs::read_to_string(&file).unwrap(), "a\nb\nX\nc\n", "{mode}");
    }
}

#[test]
fn legacy_line_verbs_and_raw_line_changesets_fail_explicitly() {
    let file = text_file("a\nb\n");
    let anchor = line_anchor(2, "b");
    for old_flag in ["--set-line", "--replace-range", "--insert-after-line"] {
        let legacy = run_identedit(&[
            "patch",
            "--at",
            &anchor,
            old_flag,
            "B",
            file.to_str().unwrap(),
        ]);
        assert!(!legacy.status.success(), "{old_flag} should be removed");
        assert_eq!(fs::read_to_string(&file).unwrap(), "a\nb\n");
    }

    let request = json!({ "command": "edit", "file": file, "operations": [{
        "target": { "type": "line", "anchor": anchor },
        "op": { "type": "set_line", "new_text": "B" }
    }] });
    let edit = run_identedit_with_stdin(&["edit", "--json"], &request.to_string());
    assert!(!edit.status.success());

    for op in [
        json!({ "type": "set_line", "new_text": "B" }),
        json!({ "type": "replace_lines", "new_text": "B" }),
        json!({ "type": "insert_after_line", "text": "X" }),
    ] {
        let patch_request = json!({
            "command": "patch", "file": file,
            "target": { "type": "line", "anchor": anchor },
            "op": op
        });
        let rejected = run_identedit_with_stdin(&["patch", "--json"], &patch_request.to_string());
        assert!(!rejected.status.success(), "old line op must fail: {op}");
        let response: Value = serde_json::from_slice(&rejected.stdout).unwrap();
        assert_eq!(response["error"]["type"], "invalid_request");
    }

    for op in [
        json!({ "type": "replace", "new_text": "B\n" }),
        json!({ "type": "insert_after", "new_text": "X\n" }),
    ] {
        let plan = json!({ "files": [{ "file": file, "operations": [{
            "target": { "type": "line", "anchor": anchor },
            "op": op,
            "preview": { "old_text": "b\n", "new_text": "B\n", "matched_span": { "start": 2, "end": 4 } }
        }] }] });
        let apply = run_identedit_with_stdin(&["apply"], &plan.to_string());
        assert!(
            !apply.status.success(),
            "legacy raw line plan must be refused"
        );
        let response: Value = serde_json::from_slice(&apply.stdout).unwrap();
        assert_eq!(response["error"]["type"], "invalid_request");
    }

    let old_empty_range_plan = json!({ "files": [{ "file": file, "operations": [{
        "target": { "type": "line", "anchor": anchor, "end_anchor": anchor },
        "op": { "type": "replace_lines", "new_text": "" },
        "preview": { "old_text": "b\n", "new_text": "", "matched_span": { "start": 2, "end": 4 } }
    }] }] });
    let old_plan = run_identedit_with_stdin(&["apply"], &old_empty_range_plan.to_string());
    assert!(
        !old_plan.status.success(),
        "legacy empty range plan must be refused"
    );
    let response: Value = serde_json::from_slice(&old_plan.stdout).unwrap();
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .unwrap()
            .contains("empty legacy range replacement")
    );

    assert_eq!(fs::read_to_string(&file).unwrap(), "a\nb\n");
}

#[test]
fn empty_range_replace_and_delete_have_distinct_patch_results() {
    let source = "a\rb\rc\r";
    let start = line_anchor(2, "b");
    let end = line_anchor(3, "c");

    for (op, expected) in [
        (json!({ "type": "replace", "new_text": "" }), "a\r\r"),
        (json!({ "type": "delete" }), "a\r"),
    ] {
        let file = text_file(source);
        let request = json!({
            "command": "patch", "file": file,
            "target": { "type": "line", "anchor": start, "end_anchor": end },
            "op": op
        });
        let result = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
        assert!(
            result.status.success(),
            "{op}: {}",
            String::from_utf8_lossy(&result.stdout)
        );
        assert_eq!(fs::read_to_string(&file).unwrap(), expected, "{op}");
    }
}

#[test]
fn line_replace_at_eof_distinguishes_payload_newline_from_file_terminator() {
    for (source, replacement, expected) in [
        ("solo", "", ""),
        ("a\nb", "", "a\n"),
        ("a\nb", "B", "a\nB"),
        ("a\nb", "B\n", "a\nB\n"),
        ("a\r\nb", "B\n", "a\r\nB\r\n"),
        ("a\nb\n", "B\n", "a\nB\n\n"),
    ] {
        let file = text_file(source);
        let content = source
            .trim_end_matches(['\n', '\r'])
            .rsplit(['\n', '\r'])
            .next()
            .unwrap();
        let anchor = line_anchor(if source == "solo" { 1 } else { 2 }, content);
        let request = json!({
            "command": "patch", "file": file,
            "target": { "type": "line", "anchor": anchor },
            "op": { "type": "replace", "new_text": replacement }
        });

        let result = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
        assert!(
            result.status.success(),
            "{source:?}: {}",
            String::from_utf8_lossy(&result.stdout)
        );
        assert_eq!(fs::read_to_string(&file).unwrap(), expected, "{source:?}");
    }
}

#[test]
fn line_delete_repair_remaps_stale_end_anchor_without_partial_write() {
    let source = "a\nb\nc\nd\n";
    let file = text_file(source);
    let request = json!({ "command": "edit", "file": file, "operations": [{
        "target": {
            "type": "line", "anchor": line_anchor(2, "b"),
            "end_anchor": line_anchor(3, "c")
        },
        "op": { "type": "delete" }
    }] });
    let edit = run_identedit_with_stdin(&["edit", "--json"], &request.to_string());
    assert!(edit.status.success());
    let plan: Value = serde_json::from_slice(&edit.stdout).unwrap();

    fs::write(&file, "a\nb\nx\nc\nd\n").unwrap();
    let strict = run_identedit_with_stdin(&["apply"], &plan.to_string());
    assert!(!strict.status.success());
    assert_eq!(fs::read_to_string(&file).unwrap(), "a\nb\nx\nc\nd\n");

    let repaired = run_identedit_with_stdin(&["apply", "--repair"], &plan.to_string());
    assert!(
        repaired.status.success(),
        "{}",
        String::from_utf8_lossy(&repaired.stdout)
    );
    assert_eq!(fs::read_to_string(&file).unwrap(), "a\nd\n");
}
