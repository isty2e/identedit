use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use serde_json::{Value, json};
use tempfile::tempdir;

mod common;

fn report(output: &Output) -> Value {
    assert!(
        output.status.success(),
        "{} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    let report = &response["structure_changes"];
    assert!(report.is_object(), "successful JSON must carry the report");
    report.clone()
}

fn node_target(file: &Path, kind: &str, name: Option<&str>) -> Value {
    let handle = common::select_first_handle(file, kind, name);
    json!({
        "type": "node", "identity": handle["identity"], "kind": handle["kind"],
        "expected_old_hash": handle["expected_old_hash"], "span_hint": handle["span"]
    })
}

fn compile_plan(files: Value) -> Value {
    let request = json!({"command": "edit", "files": files});
    let output = common::run_identedit_with_stdin(&["edit", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn body_only_edit_is_checked_with_an_empty_difference() {
    let root = tempdir().unwrap();
    let file = root.path().join("example.py");
    fs::write(&file, "def target():\n    return 1\n").unwrap();

    let output = common::run_identedit(&[
        "patch",
        file.to_str().unwrap(),
        "--symbol",
        "target",
        "--replace",
        "def target():\n    return 2",
    ]);
    let report = report(&output);

    assert_eq!(report["summary"]["checked_files"], 1);
    assert_eq!(report["summary"]["unavailable_files"], 0);
    assert_eq!(report["summary"]["added"], 0);
    assert_eq!(report["summary"]["removed"], 0);
    assert_eq!(report["files"]["entries"][0]["status"], "checked");
    assert_eq!(report["changes"]["total"], 0);
    assert!(report["changes"]["entries"].as_array().unwrap().is_empty());
}

#[test]
fn default_preview_and_commit_show_decreases_before_increases() {
    let root = tempdir().unwrap();
    let file = root.path().join("example.py");
    let source = "def z_target():\n    return 1\n";
    let mut previous_report = None;

    for mode in [vec!["--dry-run"], vec!["--dry-run", "--verbose"], vec![]] {
        fs::write(&file, source).unwrap();
        let mut arguments = vec![
            "patch",
            file.to_str().unwrap(),
            "--symbol",
            "z_target",
            "--replace",
            "def a_target():\n    return 2",
        ];
        arguments.extend(&mode);

        let output = common::run_identedit(&arguments);
        let report = report(&output);
        let changes = report["changes"]["entries"].as_array().unwrap();

        assert_eq!(changes.len(), 2);
        assert_eq!(changes[0]["qualified_name"], "z_target");
        assert_eq!(changes[0]["before_count"], 1);
        assert_eq!(changes[0]["after_count"], 0);
        assert_eq!(changes[1]["qualified_name"], "a_target");
        assert_eq!(changes[1]["before_count"], 0);
        assert_eq!(changes[1]["after_count"], 1);
        assert_eq!(report["summary"]["added"], 1);
        assert_eq!(report["summary"]["removed"], 1);
        if let Some(previous) = previous_report {
            assert_eq!(report, previous);
        }
        previous_report = Some(report);
        if mode.contains(&"--dry-run") {
            assert_eq!(fs::read_to_string(&file).unwrap(), source);
        } else {
            assert_eq!(
                fs::read_to_string(&file).unwrap(),
                "def a_target():\n    return 2\n"
            );
        }
    }
}

#[test]
fn limits_keep_a_removed_name_and_the_tail_of_long_names() {
    let root = tempdir().unwrap();
    let file = root.path().join("example.py");
    fs::write(&file, "def z_target():\n    return 1\n").unwrap();
    let module = common::select_first_handle(&file, "module", None);
    let container = "A".repeat(150);
    let mut replacement = format!("class {container}:\n    def target(self):\n        pass\n");
    for index in 0..20 {
        replacement.push_str(&format!("\ndef added_{index}():\n    pass\n"));
    }

    let output = common::run_identedit(&[
        "patch",
        file.to_str().unwrap(),
        "--at",
        module["identity"].as_str().unwrap(),
        "--replace",
        &replacement,
        "--dry-run",
    ]);
    let report = report(&output);
    let changes = report["changes"]["entries"].as_array().unwrap();

    assert_eq!(changes.len(), 16);
    assert_eq!(report["changes"]["total"], 23);
    assert_eq!(report["changes"]["omitted"], 7);
    assert_eq!(changes[0]["qualified_name"], "z_target");
    assert_eq!(report["summary"]["removed"], 1);
    assert_eq!(report["summary"]["added"], 22);

    let method = changes
        .iter()
        .find(|change| {
            change["kind"] == "function_definition"
                && change["qualified_name"]
                    .as_str()
                    .unwrap()
                    .ends_with(".target")
        })
        .unwrap();
    let displayed = method["qualified_name"].as_str().unwrap();
    assert!(displayed.starts_with('…'));
    assert_eq!(displayed.chars().count(), 120);
    assert_eq!(method["qualified_name_truncated"], true);
}

#[test]
fn nested_target_is_reported_without_inventing_a_rejection_or_rename() {
    let root = tempdir().unwrap();
    let file = root.path().join("example.py");
    let source = "class Example:\n    def first(self):\n        return 1\n\n    def target(self):\n        return 2\n";
    fs::write(&file, source).unwrap();
    let first = node_target(&file, "function_definition", Some("first"));
    let target = node_target(&file, "function_definition", Some("target"));
    let plan = compile_plan(json!([{"file": file, "operations": [
        {"target": first, "op": {"type": "replace", "new_text": "def first(self):\n        def target(self):\n            return 2\n        return 1"}},
        {"target": target, "op": {"type": "delete"}}
    ]}]));

    let preview = common::run_identedit_with_stdin(&["apply", "--dry-run"], &plan.to_string());
    let preview_report = report(&preview);
    assert_eq!(fs::read_to_string(&file).unwrap(), source);
    let committed = common::run_identedit_with_stdin(&["apply"], &plan.to_string());
    assert_eq!(report(&committed), preview_report);

    let changes = preview_report["changes"]["entries"].as_array().unwrap();
    assert_eq!(changes.len(), 2);
    assert_eq!(changes[0]["qualified_name"], "Example.target");
    assert_eq!(changes[0]["after_count"], 0);
    assert_eq!(changes[1]["qualified_name"], "Example.first.target");
    assert_eq!(changes[1]["before_count"], 0);
}

#[test]
fn co_resident_boundary_edit_is_observed_in_the_complete_proposed_file() {
    let root = tempdir().unwrap();
    let file = root.path().join("example.py");
    let source = "def target():\n    return 1\n";
    fs::write(&file, source).unwrap();
    let target = node_target(&file, "function_definition", Some("target"));
    let plan = compile_plan(json!([{"file": file, "operations": [
        {"target": target, "op": {"type": "replace", "new_text": "def target():\n    return 1"}},
        {"target": {"type": "file_end", "expected_file_hash": common::hash_text(source)},
         "op": {"type": "insert", "new_text": "\ndef added():\n    return 2\n"}}
    ]}]));
    let output = common::run_identedit_with_stdin(&["apply", "--dry-run"], &plan.to_string());
    let report = report(&output);

    assert_eq!(report["summary"]["added"], 1);
    assert_eq!(report["summary"]["removed"], 0);
    assert_eq!(report["changes"]["entries"][0]["qualified_name"], "added");
    assert_eq!(fs::read_to_string(&file).unwrap(), source);
}

#[test]
fn json_patch_and_wrapped_apply_share_the_default_report() {
    let root = tempdir().unwrap();
    let file = root.path().join("example.js");
    let source = "function target() { return 1; }\n";
    fs::write(&file, source).unwrap();
    let target = node_target(&file, "function_declaration", Some("target"));
    let operation = json!({"target": target, "op": {"type": "replace", "new_text": "function renamed() { return 2; }"}});
    let plan = compile_plan(json!([{"file": file, "operations": [operation.clone()]}]));
    let patch = json!({"command": "patch", "file": file, "target": operation["target"], "op": operation["op"], "options": {"dry_run": true}});
    let output = common::run_identedit_with_stdin(&["patch", "--json"], &patch.to_string());
    let expected = report(&output);
    let apply = json!({"command": "apply", "changeset": plan});
    let output =
        common::run_identedit_with_stdin(&["apply", "--json", "--dry-run"], &apply.to_string());

    assert_eq!(report(&output), expected);
    assert_eq!(fs::read_to_string(&file).unwrap(), source);
}

#[test]
fn fallback_is_unavailable_without_changing_its_edit_acceptance() {
    let root = tempdir().unwrap();
    let file = root.path().join("example.unknown");
    fs::write(&file, "function target() { return 1; }\n").unwrap();
    let output = common::run_identedit(&[
        "patch",
        file.to_str().unwrap(),
        "--symbol",
        "target",
        "--replace",
        "function target() {",
    ]);
    let report = report(&output);

    assert_eq!(report["summary"]["checked_files"], 0);
    assert_eq!(report["summary"]["unavailable_files"], 1);
    assert_eq!(report["files"]["entries"][0]["status"], "unavailable");
    assert_eq!(
        report["files"]["entries"][0]["reason"],
        "unsupported_provider"
    );
    assert_eq!(report["changes"]["total"], 0);
    assert!(
        fs::read_to_string(&file)
            .unwrap()
            .contains("function target() {")
    );
}

#[test]
fn line_and_boundary_only_edits_remain_out_of_scope_even_for_invalid_syntax() {
    let root = tempdir().unwrap();
    let file = root.path().join("invalid.py");
    let source = "def broken(:\n";
    let anchor = format!("1:{}", common::compute_line_hash("def broken(:"));
    for arguments in [
        vec!["--at", anchor.as_str(), "--replace", "still invalid"],
        vec!["--at", "file-end", "--insert", "more invalid"],
    ] {
        fs::write(&file, source).unwrap();
        let mut command = vec!["patch", file.to_str().unwrap(), "--dry-run"];
        command.extend(arguments);
        let output = common::run_identedit(&command);
        let report = report(&output);
        assert_eq!(report["summary"]["checked_files"], 0);
        assert_eq!(report["summary"]["out_of_scope_files"], 1);
        assert_eq!(report["files"]["entries"][0]["status"], "out_of_scope");
        assert_eq!(fs::read_to_string(&file).unwrap(), source);
    }
}

#[test]
fn diff_stays_text_only_and_silent_on_stderr() {
    let root = tempdir().unwrap();
    let file = root.path().join("example.py");
    let source = "def target():\n    return 1\n";
    fs::write(&file, source).unwrap();
    let output = common::run_identedit(&[
        "patch",
        file.to_str().unwrap(),
        "--symbol",
        "target",
        "--replace",
        "def renamed():\n    return 2",
        "--dry-run",
        "--diff",
        "--color",
        "never",
    ]);
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let diff = String::from_utf8(output.stdout).unwrap();
    assert!(diff.starts_with("--- "));
    assert!(diff.contains("\n@@ "));
    assert!(diff.contains("+def renamed():"));
    assert!(!diff.contains("structure_changes"));
    assert_eq!(fs::read_to_string(&file).unwrap(), source);
}

#[test]
fn syntax_rejection_and_rollback_never_emit_a_success_report() {
    let root = tempdir().unwrap();
    let first = root.path().join("a.py");
    let second = root.path().join("b.py");
    let source = "def target():\n    return 1\n";
    for file in [&first, &second] {
        fs::write(file, source).unwrap();
    }
    let invalid = common::run_identedit(&[
        "patch",
        first.to_str().unwrap(),
        "--symbol",
        "target",
        "--replace",
        "def broken(:",
    ]);
    assert!(!invalid.status.success());
    let error: Value = serde_json::from_slice(&invalid.stdout).unwrap();
    assert_eq!(error["error"]["stage"], "proposed_result");
    assert!(error.get("structure_changes").is_none());
    assert_eq!(fs::read_to_string(&first).unwrap(), source);

    let files = [&second, &first]
        .into_iter()
        .map(|file| {
            json!({"file": file, "operations": [{
                "target": node_target(file, "function_definition", Some("target")),
                "op": {"type": "replace", "new_text": "def renamed():\n    return 2"}
            }]})
        })
        .collect::<Vec<_>>();
    let plan = compile_plan(json!(files));
    let plan_file = root.path().join("plan.json");
    fs::write(&plan_file, plan.to_string()).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_identedit"))
        .env("IDENTEDIT_EXPERIMENTAL", "1")
        .args(["apply", "--inject-failure-after-writes", "1"])
        .arg(plan_file)
        .output()
        .unwrap();
    assert!(!output.status.success());
    let error: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(error["error"]["type"].is_string());
    assert!(error.get("structure_changes").is_none());
    assert_eq!(fs::read_to_string(&first).unwrap(), source);
    assert_eq!(fs::read_to_string(&second).unwrap(), source);
}

#[test]
fn mixed_batch_keeps_per_file_coverage_and_deterministic_file_order() {
    let root = tempdir().unwrap();
    let checked = root.path().join("a.py");
    let fallback = root.path().join("b.unknown");
    let boundary = root.path().join("c.py");
    fs::write(&checked, "def target():\n    return 1\n").unwrap();
    fs::write(&fallback, "function target() { return 1; }\n").unwrap();
    fs::write(&boundary, "def broken(:\n").unwrap();
    let requests = [
        (
            &boundary,
            vec!["--at", "file-end", "--insert", "\n# tail\n"],
        ),
        (
            &fallback,
            vec![
                "--symbol",
                "target",
                "--replace",
                "function renamed() { return 2; }",
            ],
        ),
        (
            &checked,
            vec![
                "--symbol",
                "target",
                "--replace",
                "def renamed():\n    return 2",
            ],
        ),
    ];
    let mut files = Vec::new();
    for (file, arguments) in requests {
        let mut command = vec!["edit", file.to_str().unwrap()];
        command.extend(arguments);
        let output = common::run_identedit(&command);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        let part: Value = serde_json::from_slice(&output.stdout).unwrap();
        files.extend(part["files"].as_array().unwrap().iter().cloned());
    }
    let plan = json!({"files": files, "transaction": {"mode": "all_or_nothing"}});
    let output = common::run_identedit_with_stdin(&["apply", "--dry-run"], &plan.to_string());
    let report = report(&output);
    let files = report["files"]["entries"].as_array().unwrap();

    assert_eq!(report["summary"]["checked_files"], 1);
    assert_eq!(report["summary"]["unavailable_files"], 1);
    assert_eq!(report["summary"]["out_of_scope_files"], 1);
    assert_eq!(report["summary"]["added"], 1);
    assert_eq!(report["summary"]["removed"], 1);
    assert_eq!(files[0]["file"], checked.to_str().unwrap());
    assert_eq!(files[0]["status"], "checked");
    assert_eq!(files[1]["file"], fallback.to_str().unwrap());
    assert_eq!(files[1]["status"], "unavailable");
    assert_eq!(files[1]["reason"], "unsupported_provider");
    assert_eq!(files[2]["status"], "out_of_scope");
}

#[test]
fn decorator_and_body_changes_do_not_create_named_declaration_churn() {
    let root = tempdir().unwrap();
    let file = root.path().join("example.py");
    fs::write(&file, "@staticmethod\ndef target():\n    return 1\n").unwrap();
    let module = common::select_first_handle(&file, "module", None);
    let output = common::run_identedit(&[
        "patch",
        file.to_str().unwrap(),
        "--at",
        module["identity"].as_str().unwrap(),
        "--replace",
        "@classmethod\ndef target():\n    return 2\n",
    ]);
    let report = report(&output);
    assert_eq!(report["summary"]["checked_files"], 1);
    assert_eq!(report["changes"]["total"], 0);
    assert!(fs::read_to_string(file).unwrap().contains("@classmethod"));
}

#[test]
fn getter_setter_count_change_preserves_multiplicity() {
    let root = tempdir().unwrap();
    let file = root.path().join("example.js");
    fs::write(
        &file,
        "class Example { get value() { return 1; } set value(x) {} }\n",
    )
    .unwrap();
    let program = common::select_first_handle(&file, "program", None);
    let output = common::run_identedit(&[
        "patch",
        file.to_str().unwrap(),
        "--at",
        program["identity"].as_str().unwrap(),
        "--replace",
        "class Example { get value() { return 1; } }\n",
    ]);
    let report = report(&output);
    let changes = report["changes"]["entries"].as_array().unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0]["qualified_name"], "Example.value");
    assert_eq!(changes[0]["before_count"], 2);
    assert_eq!(changes[0]["after_count"], 1);
    assert_eq!(report["summary"]["removed"], 1);
}

#[test]
fn whole_file_move_reports_its_source_as_out_of_scope() {
    let root = tempdir().unwrap();
    let source = root.path().join("source.py");
    let destination = root.path().join("renamed.py");
    let text = "def target():\n    return 1\n";
    fs::write(&source, text).unwrap();
    let plan = json!({"files": [{"file": source, "operations": [{
        "target": {"type": "file", "expected_file_hash": common::hash_text(text)},
        "op": {"type": "move", "to": destination},
        "preview": {"move": {"from": source, "to": destination}}
    }]}], "transaction": {"mode": "all_or_nothing"}});

    let preview = common::run_identedit_with_stdin(&["apply", "--dry-run"], &plan.to_string());
    let preview_report = report(&preview);
    assert_eq!(preview_report["summary"]["out_of_scope_files"], 1);
    assert_eq!(preview_report["summary"]["checked_files"], 0);
    let canonical_source = source.canonicalize().unwrap();
    assert_eq!(
        preview_report["files"]["entries"][0]["file"],
        canonical_source.to_str().unwrap()
    );
    assert_eq!(
        preview_report["files"]["entries"][0]["status"],
        "out_of_scope"
    );
    assert_eq!(fs::read_to_string(&source).unwrap(), text);
    assert!(!destination.exists());

    let committed = common::run_identedit_with_stdin(&["apply"], &plan.to_string());
    assert_eq!(report(&committed), preview_report);
    assert!(!source.exists());
    assert_eq!(fs::read_to_string(destination).unwrap(), text);
}
