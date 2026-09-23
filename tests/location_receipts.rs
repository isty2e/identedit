mod common;

use std::fs;
use std::path::Path;
use std::process::Output;

use common::{compute_line_hash, run_identedit, run_identedit_with_stdin};
use serde_json::{Value, json};
use tempfile::tempdir;

fn success(output: Output) -> Value {
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn text_location(
    file: &Path,
    index: usize,
    start: usize,
    end: usize,
    first: usize,
    last: usize,
) -> Value {
    json!({"kind":"text", "file":file, "operation_index":index,
        "span":{"start":start,"end":end}, "start_line":first,"end_line":last})
}

fn assert_locations(response: &Value, entries: Vec<Value>, total: usize) {
    assert_eq!(
        response["locations"],
        json!({
            "basis":"pre_edit", "total":total, "omitted":total-entries.len(), "entries":entries
        })
    );
}

#[test]
fn node_preview_and_commit_report_original_resolved_extent() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("module.py");
    let source = "# header\ndef chosen():\n    return 1\n\ndef other():\n    pass\n";
    fs::write(&file, source).unwrap();
    let args = [
        "patch",
        file.to_str().unwrap(),
        "--symbol",
        "chosen",
        "--replace",
        "def chosen():\n    return 222\n    # extra",
    ];
    let preview = success(run_identedit(&[&args[..], &["--dry-run"]].concat()));
    assert_eq!(fs::read_to_string(&file).unwrap(), source);
    let entry = text_location(&file, 0, 9, 35, 2, 3);
    assert_locations(&preview, vec![entry.clone()], 1);
    let applied = success(run_identedit(&args));
    assert_locations(&applied, vec![entry], 1);
    assert_eq!(applied["transaction"]["status"], "committed");
    assert!(fs::read_to_string(&file).unwrap().contains("return 222"));
}

#[test]
fn repaired_line_receipt_uses_actual_line_not_requested_line() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("notes.txt");
    fs::write(&file, "new\r\nalpha\r\nbeta\r\n").unwrap();
    let anchor = format!("1:{}", compute_line_hash("alpha"));
    let response = success(run_identedit(&[
        "patch",
        file.to_str().unwrap(),
        "--at",
        &anchor,
        "--replace",
        "updated",
        "--auto-repair",
    ]));
    assert_locations(&response, vec![text_location(&file, 0, 5, 12, 2, 2)], 1);
    assert_eq!(
        fs::read_to_string(file).unwrap(),
        "new\r\nupdated\r\nbeta\r\n"
    );
}

#[test]
fn file_end_insert_is_a_point_in_the_original_snapshot() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("notes.txt");
    for (source, line) in [("", 1), ("a", 1), ("a\r\n", 2), ("a\r", 2)] {
        fs::write(&file, source).unwrap();
        let response = success(run_identedit(&[
            "patch",
            file.to_str().unwrap(),
            "--at",
            "file-end",
            "--insert",
            "tail",
        ]));
        assert_locations(
            &response,
            vec![text_location(
                &file,
                0,
                source.len(),
                source.len(),
                line,
                line,
            )],
            1,
        );
        assert_eq!(fs::read_to_string(&file).unwrap(), format!("{source}tail"));
    }
}

#[test]
fn batch_receipt_is_globally_bounded_without_limiting_execution() {
    let dir = tempdir().unwrap();
    let mut files = Vec::new();
    for index in 0..20 {
        let file = dir.path().join(format!("{index:02}.txt"));
        fs::write(&file, "original").unwrap();
        files.push(json!({"file":file,"operations":[{
            "target":{"type":"file_end","expected_file_hash":common::hash_text("original")},
            "op":{"type":"insert","new_text":"!"}
        }]}));
    }
    let request = json!({"command":"edit","files":files});
    let plan = success(run_identedit_with_stdin(
        &["edit", "--json"],
        &request.to_string(),
    ));
    let response = success(run_identedit_with_stdin(&["apply"], &plan.to_string()));
    let entries = (0..16)
        .map(|i| text_location(&dir.path().join(format!("{i:02}.txt")), 0, 8, 8, 1, 1))
        .collect();
    assert_locations(&response, entries, 20);
    assert_eq!(response["summary"]["operations_applied"], 20);
    for file in files {
        assert_eq!(
            fs::read_to_string(file["file"].as_str().unwrap()).unwrap(),
            "original!"
        );
    }
}

#[test]
fn config_patch_reports_the_resolved_container_not_the_entire_document() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("settings.json");
    let source = "{\n  \"nested\": {\"enabled\": false},\n  \"enabled\": false\n}\n";
    fs::write(&file, source).unwrap();
    let response = success(run_identedit(&[
        "patch",
        file.to_str().unwrap(),
        "--config-path",
        "nested.enabled",
        "--set-value",
        "true",
    ]));
    let start = source.find("{\"enabled\"").unwrap();
    let end = start + "{\"enabled\": false}".len();
    assert_locations(
        &response,
        vec![text_location(&file, 0, start, end, 2, 2)],
        1,
    );
    let updated = fs::read_to_string(file).unwrap();
    let parsed: Value = serde_json::from_str(&updated).unwrap();
    assert_eq!(parsed["nested"]["enabled"], true);
    assert_eq!(parsed["enabled"], false);
    assert!(updated.starts_with(&source[..start]));
    assert!(updated.ends_with(&source[end..]));
}

#[test]
fn line_insert_and_delete_receipts_describe_targets_not_eol_adjustments() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("notes.txt");
    for source in ["a\rb", "a\r\nb", "a\nb"] {
        fs::write(&file, source).unwrap();
        let anchor = format!("2:{}", compute_line_hash("b"));
        let inserted = success(run_identedit(&[
            "patch",
            file.to_str().unwrap(),
            "--at",
            &anchor,
            "--insert-after",
            "c",
        ]));
        assert_locations(
            &inserted,
            vec![text_location(&file, 0, source.len(), source.len(), 2, 2)],
            1,
        );

        fs::write(&file, source).unwrap();
        let deleted = success(run_identedit(&[
            "patch",
            file.to_str().unwrap(),
            "--at",
            &anchor,
            "--delete",
        ]));
        assert_locations(
            &deleted,
            vec![text_location(
                &file,
                0,
                source.len() - 1,
                source.len(),
                2,
                2,
            )],
            1,
        );
        assert_eq!(fs::read_to_string(&file).unwrap(), "a");
    }
}

#[test]
fn repaired_merge_receipt_includes_the_absorbed_line() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("notes.txt");
    let source = "intro\nleft &&\nright\ntail\n";
    fs::write(&file, source).unwrap();
    let anchor = format!("1:{}", compute_line_hash("left &&"));
    let args = [
        "patch",
        file.to_str().unwrap(),
        "--at",
        &anchor,
        "--replace",
        "left &&right",
        "--auto-repair",
    ];
    let preview = success(run_identedit(&[&args[..], &["--dry-run"]].concat()));
    assert_locations(&preview, vec![text_location(&file, 0, 6, 20, 2, 3)], 1);
    assert_eq!(fs::read_to_string(&file).unwrap(), source);
    let applied = success(run_identedit(&args));
    assert_eq!(preview["locations"], applied["locations"]);
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        "intro\nleft &&right\ntail\n"
    );
}

fn edit_plan(file: &Path, operations: Vec<Value>) -> Value {
    success(run_identedit_with_stdin(
        &["edit", "--json"],
        &json!({
            "command":"edit", "file":file, "operations":operations
        })
        .to_string(),
    ))
}

fn line_operation(number: usize, text: &str, replacement: &str) -> Value {
    json!({"target":{"type":"line","anchor":format!("{number}:{}",compute_line_hash(text))},
        "op":{"type":"replace","new_text":replacement}})
}

#[test]
fn apply_receipt_preserves_operation_indices_and_pre_edit_positions() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("notes.txt");
    fs::write(&file, "alpha\nbeta\ngamma\n").unwrap();
    let plan = edit_plan(
        &file,
        vec![
            line_operation(3, "gamma", "G"),
            line_operation(1, "alpha", "A\nextra"),
        ],
    );
    let preview = success(run_identedit_with_stdin(
        &["apply", "--dry-run", "--verbose"],
        &plan.to_string(),
    ));
    let applied = success(run_identedit_with_stdin(&["apply"], &plan.to_string()));
    assert_eq!(preview["locations"], applied["locations"]);
    assert_locations(
        &applied,
        vec![
            text_location(&file, 0, 11, 17, 3, 3),
            text_location(&file, 1, 0, 6, 1, 1),
        ],
        2,
    );
    assert_eq!(fs::read_to_string(file).unwrap(), "A\nextra\nbeta\nG\n");
}

#[test]
fn apply_repair_reports_remapped_plan_coordinates() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("notes.txt");
    fs::write(&file, "alpha\nbeta\n").unwrap();
    let plan = edit_plan(&file, vec![line_operation(2, "beta", "B")]);
    fs::write(&file, "intro\nalpha\nbeta\n").unwrap();
    let applied = success(run_identedit_with_stdin(
        &["apply", "--repair"],
        &plan.to_string(),
    ));
    assert_locations(&applied, vec![text_location(&file, 0, 12, 17, 3, 3)], 1);
    assert_eq!(fs::read_to_string(file).unwrap(), "intro\nalpha\nB\n");
}

#[test]
fn stale_input_has_no_success_receipt_and_does_not_write() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("notes.txt");
    fs::write(&file, "alpha\n").unwrap();
    let plan = edit_plan(&file, vec![line_operation(1, "alpha", "A")]);
    fs::write(&file, "changed\n").unwrap();
    let output = run_identedit_with_stdin(&["apply"], &plan.to_string());
    assert!(!output.status.success());
    let error: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(error.get("error").is_some());
    assert!(error.get("locations").is_none());
    assert!(output.stderr.is_empty());
    assert_eq!(fs::read_to_string(file).unwrap(), "changed\n");
}

#[test]
fn no_op_line_patch_reports_a_checked_target_without_claiming_a_change() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("notes.txt");
    fs::write(&file, "same\n").unwrap();
    let anchor = format!("1:{}", compute_line_hash("same"));
    let response = success(run_identedit(&[
        "patch",
        file.to_str().unwrap(),
        "--at",
        &anchor,
        "--replace",
        "same",
    ]));
    assert_eq!(response["changed"], false);
    assert_locations(&response, vec![text_location(&file, 0, 0, 5, 1, 1)], 1);
    assert_eq!(fs::read_to_string(file).unwrap(), "same\n");
}

#[test]
fn file_move_reports_paths_instead_of_a_fabricated_span() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("old.txt");
    let destination = dir.path().join("new.txt");
    fs::write(&file, "original").unwrap();
    let plan = json!({"files":[{"file":file,"operations":[{
        "target":{"type":"file", "expected_file_hash":common::hash_text("original")},
        "op":{"type":"move", "to":destination},
        "preview":{"move":{"from":file,"to":destination}}
    }]}],"transaction":{"mode":"all_or_nothing"}});
    let preview = success(run_identedit_with_stdin(
        &["apply", "--dry-run"],
        &plan.to_string(),
    ));
    let canonical_source = fs::canonicalize(&file).unwrap();
    assert_locations(
        &preview,
        vec![json!({"kind":"file_move","source":canonical_source,
        "destination":destination,"operation_index":0})],
        1,
    );
    assert!(file.exists());
    assert!(!destination.exists());
    let applied = success(run_identedit_with_stdin(&["apply"], &plan.to_string()));
    assert_eq!(preview["locations"], applied["locations"]);
    assert!(!file.exists());
    assert_eq!(fs::read_to_string(destination).unwrap(), "original");
}

fn move_plan(file: &Path, destination: &Path, text: &str) -> Value {
    json!({"file":file,"operations":[{
        "target":{"type":"file","expected_file_hash":common::hash_text(text)},
        "op":{"type":"move","to":destination},
        "preview":{"move":{"from":file,"to":destination}}
    }]})
}

fn apply_plan_in_directory(directory: &Path, files: Vec<Value>, dry_run: bool) -> Value {
    let plan_path = directory.join("plan.json");
    fs::write(
        &plan_path,
        json!({"files":files,"transaction":{"mode":"all_or_nothing"}}).to_string(),
    )
    .unwrap();
    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_identedit"));
    command.current_dir(directory).arg("apply").arg(&plan_path);
    if dry_run {
        command.arg("--dry-run");
    }
    success(command.output().unwrap())
}

fn reported_missing_destination(
    response: &Value,
    directory: &Path,
    name: &str,
) -> std::path::PathBuf {
    let destination = Path::new(
        response["locations"]["entries"][0]["destination"]
            .as_str()
            .unwrap(),
    );
    assert!(destination.is_absolute());
    assert!(!destination.components().any(|part| matches!(
        part,
        std::path::Component::CurDir | std::path::Component::ParentDir
    )));
    assert_eq!(destination.file_name().unwrap(), name);
    assert_eq!(
        fs::canonicalize(destination.parent().unwrap()).unwrap(),
        fs::canonicalize(directory).unwrap()
    );
    destination.to_path_buf()
}

#[test]
fn file_move_receipt_reports_normalized_missing_destinations() {
    for relative in [true, false] {
        let dir = tempdir().unwrap();
        let source = dir.path().join("old.txt");
        fs::write(&source, "original").unwrap();
        fs::create_dir(dir.path().join("nested")).unwrap();
        let submitted = if relative {
            Path::new("./nested/../new.txt").to_path_buf()
        } else {
            dir.path().join("nested/../new.txt")
        };
        let files = vec![move_plan(&source, &submitted, "original")];
        let preview = apply_plan_in_directory(dir.path(), files.clone(), true);
        let expected = reported_missing_destination(&preview, dir.path(), "new.txt");
        if !relative {
            assert_eq!(expected, dir.path().join("new.txt"));
        }
        let entry = json!({"kind":"file_move","source":fs::canonicalize(&source).unwrap(),
            "destination":expected,"operation_index":0});
        assert_locations(&preview, vec![entry.clone()], 1);
        assert!(source.exists());
        assert!(!expected.exists());

        let applied = apply_plan_in_directory(dir.path(), files, false);
        assert_locations(&applied, vec![entry], 1);
        assert!(!source.exists());
        assert_eq!(fs::read_to_string(expected).unwrap(), "original");
    }
}

#[test]
fn file_move_receipt_canonicalizes_existing_chain_destinations() {
    let dir = tempdir().unwrap();
    let first = dir.path().join("a.txt");
    let second = dir.path().join("b.txt");
    fs::write(&first, "first").unwrap();
    fs::write(&second, "second").unwrap();
    fs::create_dir(dir.path().join("nested")).unwrap();
    let canonical = fs::canonicalize(dir.path()).unwrap();
    let files = vec![
        move_plan(&first, Path::new("./nested/../b.txt"), "first"),
        move_plan(&second, Path::new("./nested/../c.txt"), "second"),
    ];
    let preview = apply_plan_in_directory(dir.path(), files.clone(), true);
    let destination = reported_missing_destination(&preview, dir.path(), "c.txt");
    let entries = vec![
        json!({"kind":"file_move","source":canonical.join("b.txt"),"destination":destination,"operation_index":0}),
        json!({"kind":"file_move","source":canonical.join("a.txt"),"destination":canonical.join("b.txt"),"operation_index":0}),
    ];
    assert_locations(&preview, entries.clone(), 2);
    assert_eq!(fs::read_to_string(&first).unwrap(), "first");
    assert_eq!(fs::read_to_string(&second).unwrap(), "second");

    let applied = apply_plan_in_directory(dir.path(), files, false);
    assert_locations(&applied, entries, 2);
    assert!(!first.exists());
    assert_eq!(fs::read_to_string(&second).unwrap(), "first");
    assert_eq!(
        fs::read_to_string(canonical.join("c.txt")).unwrap(),
        "second"
    );
}

#[test]
fn same_file_move_reports_source_and_destination_under_one_operation() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("module.py");
    let source = "def first():\n    pass\n\ndef last():\n    pass\n";
    fs::write(&file, source).unwrap();
    let first = common::select_first_handle(&file, "function_definition", Some("first"));
    let last = common::select_first_handle(&file, "function_definition", Some("last"));
    let node = |handle: &Value| {
        json!({"type":"node","identity":handle["identity"],
        "kind":handle["kind"],"expected_old_hash":common::hash_text(handle["text"].as_str().unwrap())})
    };
    let plan = edit_plan(
        &file,
        vec![json!({
            "target":node(&last),
            "op":{"type":"move_before","destination":node(&first)}
        })],
    );
    let applied = success(run_identedit_with_stdin(&["apply"], &plan.to_string()));
    let start = source.find("def last").unwrap();
    assert_locations(
        &applied,
        vec![
            text_location(&file, 0, start, source.len() - 1, 4, 5),
            text_location(&file, 0, 0, 0, 1, 1),
        ],
        2,
    );
    assert_eq!(applied["summary"]["operations_applied"], 1);
    assert_eq!(
        fs::read_to_string(file).unwrap(),
        format!("{}{}\n", &source[start..source.len() - 1], &source[..start])
    );
}

#[test]
fn per_file_cap_counts_omitted_locations_and_preserves_operation_indices() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("notes.txt");
    let source = (0..20).map(|i| format!("{i:02}\n")).collect::<String>();
    fs::write(&file, &source).unwrap();
    let ops = (0..20)
        .rev()
        .map(|i| line_operation(i + 1, &format!("{i:02}"), "changed"))
        .collect();
    let plan = edit_plan(&file, ops);
    let applied = success(run_identedit_with_stdin(&["apply"], &plan.to_string()));
    let entries = (0..16)
        .map(|index| {
            let line = 20 - index;
            text_location(&file, index, (line - 1) * 3, line * 3, line, line)
        })
        .collect();
    assert_locations(&applied, entries, 20);
    assert_eq!(fs::read_to_string(file).unwrap(), "changed\n".repeat(20));
}

#[test]
fn bom_and_unicode_offsets_are_bytes_and_line_numbers_count_all_eol_forms() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("notes.txt");
    let source = "\u{feff}\u{03bb}\r\n\u{1f600}\rtail\n";
    fs::write(&file, source).unwrap();
    let anchor = format!("2:{}", compute_line_hash("\u{1f600}"));
    let response = success(run_identedit(&[
        "patch",
        file.to_str().unwrap(),
        "--at",
        &anchor,
        "--replace",
        "x",
    ]));
    assert_locations(&response, vec![text_location(&file, 0, 7, 12, 2, 2)], 1);
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        "\u{feff}\u{03bb}\r\nx\rtail\n"
    );

    fs::write(&file, source).unwrap();
    let response = success(run_identedit(&[
        "patch",
        file.to_str().unwrap(),
        "--at",
        "file-start",
        "--insert",
        "head",
    ]));
    assert_locations(&response, vec![text_location(&file, 0, 3, 3, 1, 1)], 1);
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        "\u{feff}head\u{03bb}\r\n\u{1f600}\rtail\n"
    );
}

#[test]
fn rollback_error_does_not_publish_success_locations() {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let dir = tempdir().unwrap();
    let files: Vec<_> = ["a.txt", "b.txt"]
        .into_iter()
        .map(|name| dir.path().join(name))
        .collect();
    let mut changesets = Vec::new();
    for file in &files {
        fs::write(file, "original\n").unwrap();
        let plan = edit_plan(file, vec![line_operation(1, "original", "changed")]);
        changesets.push(plan["files"][0].clone());
    }
    let plan = json!({"files":changesets,"transaction":{"mode":"all_or_nothing"}});
    let mut process = Command::new(env!("CARGO_BIN_EXE_identedit"))
        .args(["apply", "--inject-failure-after-writes", "1"])
        .env("IDENTEDIT_EXPERIMENTAL", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    process
        .stdin
        .take()
        .unwrap()
        .write_all(plan.to_string().as_bytes())
        .unwrap();
    let output = process.wait_with_output().unwrap();
    assert!(!output.status.success());
    let error: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        error["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Injected apply failure")
    );
    assert!(error.get("locations").is_none());
    for file in files {
        assert_eq!(fs::read_to_string(file).unwrap(), "original\n");
    }
}
