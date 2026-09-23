use super::*;

fn line_anchor(line: usize, content: &str) -> String {
    format!("{line}:{}", common::compute_line_hash(content))
}

fn text_file(source: &str) -> PathBuf {
    let file = Builder::new().suffix(".txt").tempfile().unwrap();
    fs::write(file.path(), source).unwrap();
    file.keep().unwrap().1
}

fn apply_plan(plan: &Value) -> std::process::Output {
    run_identedit_with_stdin(&["apply"], &plan.to_string())
}

#[test]
fn flag_line_edits_match_patch_line_layout() {
    let cases = [
        (
            "set lf",
            "alpha\nbeta\ngamma\n",
            2,
            "beta",
            "--set-line",
            "BETA",
            None,
            "alpha\nBETA\ngamma\n",
        ),
        (
            "set crlf",
            "alpha\r\nbeta\r\ngamma\r\n",
            2,
            "beta",
            "--set-line",
            "BETA",
            None,
            "alpha\r\nBETA\r\ngamma\r\n",
        ),
        (
            "set cr",
            "alpha\rbeta\rgamma\r",
            2,
            "beta",
            "--set-line",
            "BETA",
            None,
            "alpha\rBETA\rgamma\r",
        ),
        (
            "blank line",
            "alpha\nbeta\ngamma\n",
            2,
            "beta",
            "--set-line",
            "",
            None,
            "alpha\n\ngamma\n",
        ),
        (
            "set eof",
            "alpha\nbeta",
            2,
            "beta",
            "--set-line",
            "BETA",
            None,
            "alpha\nBETA",
        ),
        (
            "delete range",
            "alpha\nbeta\ngamma\n",
            2,
            "beta",
            "--replace-range",
            "",
            None,
            "alpha\ngamma\n",
        ),
        (
            "delete final line",
            "alpha\nbeta",
            2,
            "beta",
            "--replace-range",
            "",
            None,
            "alpha",
        ),
        (
            "replace range",
            "alpha\r\nbeta\r\ngamma\r\ndelta\r\n",
            2,
            "beta",
            "--replace-range",
            "BETA\nGAMMA",
            Some((3, "gamma")),
            "alpha\r\nBETA\r\nGAMMA\r\ndelta\r\n",
        ),
        (
            "insert lf",
            "alpha\nbeta\ngamma\n",
            2,
            "beta",
            "--insert-after-line",
            "X",
            None,
            "alpha\nbeta\nX\ngamma\n",
        ),
        (
            "insert crlf",
            "alpha\r\nbeta\r\ngamma\r\n",
            2,
            "beta",
            "--insert-after-line",
            "X",
            None,
            "alpha\r\nbeta\r\nX\r\ngamma\r\n",
        ),
        (
            "insert cr",
            "alpha\rbeta\rgamma\r",
            2,
            "beta",
            "--insert-after-line",
            "X",
            None,
            "alpha\rbeta\rX\rgamma\r",
        ),
        (
            "insert eof",
            "alpha\nbeta",
            2,
            "beta",
            "--insert-after-line",
            "X",
            None,
            "alpha\nbeta\nX",
        ),
        (
            "insert terminated eof",
            "alpha\nbeta\n",
            2,
            "beta",
            "--insert-after-line",
            "X",
            None,
            "alpha\nbeta\nX\n",
        ),
        (
            "mixed endings",
            "alpha\r\nbeta\ngamma\rdelta",
            2,
            "beta",
            "--insert-after-line",
            "X\nY",
            None,
            "alpha\r\nbeta\nX\nY\ngamma\rdelta",
        ),
        (
            "multiline set",
            "alpha\r\nbeta\r\ngamma\r\n",
            2,
            "beta",
            "--set-line",
            "BETA\nMORE",
            None,
            "alpha\r\nBETA\r\nMORE\r\ngamma\r\n",
        ),
        (
            "unicode",
            "α\nβ\nγ\n",
            2,
            "β",
            "--set-line",
            "βeta",
            None,
            "α\nβeta\nγ\n",
        ),
        (
            "delete sole line",
            "beta",
            1,
            "beta",
            "--replace-range",
            "",
            None,
            "",
        ),
    ];

    for (name, source, line, content, operation, payload, end, expected) in cases {
        let anchor = line_anchor(line, content);
        let patch_file = text_file(source);
        let edit_file = text_file(source);

        let mut patch_args = vec!["patch", "--at", anchor.as_str(), operation, payload];
        let mut edit_args = vec!["edit", "--at", anchor.as_str(), operation, payload];
        let end_anchor = end.map(|(line, content)| line_anchor(line, content));
        if let Some(end_anchor) = end_anchor.as_deref() {
            patch_args.extend(["--end-anchor", end_anchor]);
            edit_args.extend(["--end-anchor", end_anchor]);
        }
        patch_args.push(patch_file.to_str().unwrap());
        edit_args.push(edit_file.to_str().unwrap());

        let patch = run_identedit(&patch_args);
        assert!(
            patch.status.success(),
            "{name} patch: {}",
            String::from_utf8_lossy(&patch.stdout)
        );
        assert_eq!(
            fs::read_to_string(&patch_file).unwrap(),
            expected,
            "{name} patch oracle"
        );

        let edit = run_identedit(&edit_args);
        assert!(
            edit.status.success(),
            "{name} edit: {}",
            String::from_utf8_lossy(&edit.stdout)
        );
        assert_eq!(
            fs::read_to_string(&edit_file).unwrap(),
            source,
            "{name} edit must be dry-run"
        );
        let plan: Value = serde_json::from_slice(&edit.stdout).unwrap();
        let apply = apply_plan(&plan);
        assert!(
            apply.status.success(),
            "{name} apply: {}",
            String::from_utf8_lossy(&apply.stdout)
        );
        assert_eq!(
            fs::read_to_string(&edit_file).unwrap(),
            expected,
            "{name} edit/apply"
        );
    }
}

#[test]
fn stale_line_plan_and_tampered_preview_do_not_write() {
    let source = "alpha\r\nbeta\r\ngamma\r\n";
    let file = text_file(source);
    let edit = run_identedit(&[
        "edit",
        "--at",
        &line_anchor(2, "beta"),
        "--set-line",
        "BETA",
        file.to_str().unwrap(),
    ]);
    assert!(edit.status.success());
    let plan: Value = serde_json::from_slice(&edit.stdout).unwrap();

    let mut tampered = plan.clone();
    tampered["files"][0]["operations"][0]["preview"]["new_text"] = json!("SURPRISE\r\n");
    let rejected = apply_plan(&tampered);
    assert!(!rejected.status.success());
    assert_eq!(fs::read_to_string(&file).unwrap(), source);

    let changed = "alpha\r\nbeta2\r\ngamma\r\n";
    fs::write(&file, changed).unwrap();
    let rejected = apply_plan(&plan);
    assert!(!rejected.status.success());
    assert_eq!(fs::read_to_string(&file).unwrap(), changed);
}

#[test]
fn repair_does_not_hide_tampered_preview_when_line_is_unchanged() {
    let source = "alpha\r\nbeta\r\ngamma\r\n";
    let file = text_file(source);
    let edit = run_identedit(&[
        "edit",
        "--at",
        &line_anchor(2, "beta"),
        "--set-line",
        "BETA",
        file.to_str().unwrap(),
    ]);
    assert!(edit.status.success());
    let mut plan: Value = serde_json::from_slice(&edit.stdout).unwrap();
    plan["files"][0]["operations"][0]["preview"]["new_text"] = json!("SURPRISE\r\n");

    let rejected = run_identedit_with_stdin(&["apply", "--repair"], &plan.to_string());
    assert!(!rejected.status.success());
    assert_eq!(fs::read_to_string(&file).unwrap(), source);
}

#[test]
fn repair_refreshes_line_layout_when_terminators_change() {
    let source = "alpha\r\nbeta\r\ngamma\r\n";
    let file = text_file(source);
    let edit = run_identedit(&[
        "edit",
        "--at",
        &line_anchor(2, "beta"),
        "--insert-after-line",
        "X",
        file.to_str().unwrap(),
    ]);
    assert!(edit.status.success());
    let plan: Value = serde_json::from_slice(&edit.stdout).unwrap();

    let changed = "alpha\nbeta\ngamma\n";
    fs::write(&file, changed).unwrap();
    let rejected = apply_plan(&plan);
    assert!(
        !rejected.status.success(),
        "strict apply must detect changed line layout"
    );
    assert_eq!(fs::read_to_string(&file).unwrap(), changed);

    let repaired = run_identedit_with_stdin(&["apply", "--repair"], &plan.to_string());
    assert!(
        repaired.status.success(),
        "repair failed: {}",
        String::from_utf8_lossy(&repaired.stdout)
    );
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        "alpha\nbeta\nX\ngamma\n"
    );
}

#[test]
fn stale_second_file_prevents_first_line_edit_in_batch() {
    let first_source = "one\ntwo\n";
    let second_source = "three\nfour\n";
    let first = text_file(first_source);
    let second = text_file(second_source);
    let request = json!({
        "command": "edit",
        "files": [
            { "file": first, "operations": [{
                "target": { "type": "line", "anchor": line_anchor(2, "two") },
                "op": { "type": "set_line", "new_text": "TWO" }
            }] },
            { "file": second, "operations": [{
                "target": { "type": "line", "anchor": line_anchor(2, "four") },
                "op": { "type": "insert_after_line", "text": "FIVE" }
            }] }
        ]
    });
    let edit = run_identedit_with_stdin(&["edit", "--json"], &request.to_string());
    assert!(
        edit.status.success(),
        "batch edit failed: {}",
        String::from_utf8_lossy(&edit.stdout)
    );
    let plan: Value = serde_json::from_slice(&edit.stdout).unwrap();

    fs::write(&second, "three\nFOUR\n").unwrap();
    let apply = apply_plan(&plan);
    assert!(!apply.status.success());
    assert_eq!(fs::read_to_string(&first).unwrap(), first_source);
    assert_eq!(fs::read_to_string(&second).unwrap(), "three\nFOUR\n");
}

#[test]
fn commit_failure_rolls_back_line_edits_in_both_files() {
    let first_source = "one\r\ntwo\r\n";
    let second_source = "three\nfour\n";
    let first = text_file(first_source);
    let second = text_file(second_source);
    let request = json!({
        "command": "edit",
        "files": [
            { "file": first, "operations": [{
                "target": { "type": "line", "anchor": line_anchor(2, "two") },
                "op": { "type": "set_line", "new_text": "TWO" }
            }] },
            { "file": second, "operations": [{
                "target": { "type": "line", "anchor": line_anchor(2, "four") },
                "op": { "type": "insert_after_line", "text": "FIVE" }
            }] }
        ]
    });
    let edit = run_identedit_with_stdin(&["edit", "--json"], &request.to_string());
    assert!(edit.status.success());
    let plan: Value = serde_json::from_slice(&edit.stdout).unwrap();

    let plan_file = Builder::new().suffix(".json").tempfile().unwrap();
    fs::write(plan_file.path(), plan.to_string()).unwrap();
    let failed = Command::new(env!("CARGO_BIN_EXE_identedit"))
        .args(["apply", "--inject-failure-after-writes", "1"])
        .arg(plan_file.path())
        .env("IDENTEDIT_EXPERIMENTAL", "1")
        .output()
        .unwrap();
    assert!(!failed.status.success());
    let response: Value = serde_json::from_slice(&failed.stdout).unwrap();
    let message = response["error"]["message"].as_str().unwrap();
    assert!(message.contains("Injected apply failure for rollback rehearsal"));
    assert!(message.contains("after 1 committed writes"));
    assert_eq!(fs::read_to_string(&first).unwrap(), first_source);
    assert_eq!(fs::read_to_string(&second).unwrap(), second_source);
}

#[test]
fn edit_json_rejects_end_anchor_for_non_range_line_operations() {
    let source = "alpha\nbeta\ngamma\n";
    for op in [
        json!({ "type": "set_line", "new_text": "BETA" }),
        json!({ "type": "insert_after_line", "text": "X" }),
    ] {
        let file = text_file(source);
        let request = json!({
            "command": "edit",
            "file": file,
            "operations": [{
                "target": {
                    "type": "line",
                    "anchor": line_anchor(2, "beta"),
                    "end_anchor": line_anchor(3, "gamma"),
                },
                "op": op,
            }],
        });
        let edit = run_identedit_with_stdin(&["edit", "--json"], &request.to_string());
        assert!(!edit.status.success());
        let response: Value = serde_json::from_slice(&edit.stdout).unwrap();
        assert!(
            response["error"]["message"]
                .as_str()
                .unwrap()
                .contains("end_anchor")
        );
        assert_eq!(fs::read_to_string(&file).unwrap(), source);
    }
}

#[test]
fn empty_line_insertion_is_rejected_before_apply() {
    let source = "alpha\nbeta\n";
    let file = text_file(source);
    let edit = run_identedit(&[
        "edit",
        "--at",
        &line_anchor(1, "alpha"),
        "--insert-after-line",
        "",
        file.to_str().unwrap(),
    ]);
    assert!(!edit.status.success());
    assert_eq!(fs::read_to_string(&file).unwrap(), source);
}

#[test]
fn json_line_edits_preserve_operation_semantics_and_layout() {
    let cases = [
        (
            "set_line",
            "new_text",
            "",
            "alpha\r\nbeta\r\ngamma\r\n",
            "alpha\r\n\r\ngamma\r\n",
        ),
        (
            "replace_lines",
            "new_text",
            "",
            "alpha\r\nbeta\r\ngamma\r\n",
            "alpha\r\ngamma\r\n",
        ),
        (
            "insert_after_line",
            "text",
            "X",
            "alpha\r\nbeta\r\ngamma\r\n",
            "alpha\r\nbeta\r\nX\r\ngamma\r\n",
        ),
    ];

    for (operation, field, payload, source, expected) in cases {
        let file = text_file(source);
        let mut op = json!({ "type": operation });
        op[field] = json!(payload);
        let request = json!({
            "command": "edit",
            "file": file,
            "operations": [{
                "target": { "type": "line", "anchor": line_anchor(2, "beta") },
                "op": op
            }]
        });
        let edit = run_identedit_with_stdin(&["edit", "--json"], &request.to_string());
        assert!(
            edit.status.success(),
            "{operation}: {}",
            String::from_utf8_lossy(&edit.stdout)
        );
        let plan: Value = serde_json::from_slice(&edit.stdout).unwrap();
        assert_eq!(plan["files"][0]["operations"][0]["op"]["type"], operation);
        let apply = apply_plan(&plan);
        assert!(
            apply.status.success(),
            "{operation}: {}",
            String::from_utf8_lossy(&apply.stdout)
        );
        assert_eq!(fs::read_to_string(&file).unwrap(), expected, "{operation}");
    }
}
