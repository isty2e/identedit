use std::path::Path;
use std::process::Output;

use serde_json::{Value, json};
use tempfile::TempDir;

mod common;

fn successful_json(output: Output) -> Value {
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn config_plan(file: &Path, path: &str, operation: &[&str]) -> Value {
    let mut arguments = vec!["edit", file.to_str().unwrap(), "--config-path", path];
    arguments.extend_from_slice(operation);
    successful_json(common::run_identedit(&arguments))
}

fn apply(plan: &Value, options: &[&str]) -> Output {
    let mut arguments = vec!["apply"];
    arguments.extend_from_slice(options);
    common::run_identedit_with_stdin(&arguments, &plan.to_string())
}

fn assert_precondition_failure(output: Output) {
    assert!(!output.status.success());
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        response["error"]["type"], "precondition_failed",
        "{response}"
    );
}

fn empty_plan(file: &Path, hash: Option<&str>) -> Value {
    let mut entry = json!({"file": file, "operations": []});
    if let Some(hash) = hash {
        entry["expected_file_hash"] = json!(hash);
    }
    json!({"files": [entry]})
}

fn merge(directory: &Path, plans: &[Value]) -> Output {
    let files = plans
        .iter()
        .enumerate()
        .map(|(index, plan)| {
            let file = directory.join(format!("plan-{index}.json"));
            std::fs::write(&file, plan.to_string()).unwrap();
            file
        })
        .collect::<Vec<_>>();
    let mut arguments = vec!["merge"];
    arguments.extend(files.iter().map(|file| file.to_str().unwrap()));
    common::run_identedit(&arguments)
}

#[test]
fn config_producers_persist_the_original_full_file_hash_for_every_verb() {
    let directory = TempDir::new().unwrap();
    for (extension, source) in [
        ("json", "{\"value\":1,\"items\":[1],\"outside\":0}\r\n"),
        ("toml", "value = 1\r\nitems = [1]\r\noutside = 0\r\n"),
        ("yaml", "value: 1\r\nitems: [1]\r\noutside: 0\r\n"),
    ] {
        let file = directory.path().join(format!("settings.{extension}"));
        std::fs::write(&file, source).unwrap();
        for (path, operation) in [
            ("value", vec!["--set-value", "2"]),
            ("items", vec!["--append-value", "2"]),
            ("value", vec!["--delete"]),
            ("new_key", vec!["--set-value", "2", "--create-missing"]),
        ] {
            let plan = config_plan(&file, path, &operation);
            assert_eq!(
                plan["files"][0]["expected_file_hash"],
                common::hash_text(source)
            );
            successful_json(apply(&plan, &["--dry-run"]));
            assert_eq!(std::fs::read(&file).unwrap(), source.as_bytes());
        }
    }
}

#[test]
fn outside_container_changes_reject_config_plans_even_with_repair_or_noop() {
    let directory = TempDir::new().unwrap();
    for (extension, original, changed) in [
        (
            "json",
            "{\"inside\":{\"value\":1},\"outside\":0}\n",
            "{\"inside\":{\"value\":1},\"outside\":9}\n",
        ),
        (
            "toml",
            "[inside]\nvalue = 1\n[outside]\nvalue = 0\n",
            "[inside]\nvalue = 1\n[outside]\nvalue = 9\n",
        ),
        (
            "yaml",
            "inside:\n  value: 1\noutside: 0\n",
            "inside:\n  value: 1\noutside: 9\n",
        ),
    ] {
        let file = directory.path().join(format!("settings.{extension}"));
        for replacement in ["1", "2"] {
            std::fs::write(&file, original).unwrap();
            let plan = config_plan(&file, "inside.value", &["--set-value", replacement]);
            std::fs::write(&file, changed).unwrap();
            for options in [
                vec![],
                vec!["--dry-run"],
                vec!["--repair"],
                vec!["--repair", "--dry-run"],
            ] {
                assert_precondition_failure(apply(&plan, &options));
                assert_eq!(std::fs::read_to_string(&file).unwrap(), changed);
            }
        }
    }
}

#[test]
fn yaml_alias_context_cannot_change_after_config_compilation() {
    let directory = TempDir::new().unwrap();
    let file = directory.path().join("settings.yaml");
    let original = "defaults: &base\n  timeout: 1\nother: 0\n";
    std::fs::write(&file, original).unwrap();
    let mut plan = config_plan(&file, "defaults.timeout", &["--set-value", "2"]);
    let changed = format!("{original}consumer: *base\n");
    std::fs::write(&file, &changed).unwrap();

    assert_precondition_failure(apply(&plan, &["--dry-run"]));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), changed);

    // A saved legacy plan keeps its original container-only precondition.
    plan["files"][0]
        .as_object_mut()
        .unwrap()
        .remove("expected_file_hash");
    successful_json(apply(&plan, &["--dry-run"]));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), changed);
}

#[test]
fn file_hash_failures_name_the_file_and_require_recompilation() {
    let directory = TempDir::new().unwrap();
    let file = directory.path().join("settings.json");
    let destination = directory.path().join("moved.json");
    let source = "{\"inside\":{\"value\":1},\"outside\":0}\n";
    let changed = source.replace("outside\":0", "outside\":9");
    std::fs::write(&file, source).unwrap();
    let file = std::fs::canonicalize(file).unwrap();
    let config = config_plan(&file, "inside.value", &["--set-value", "2"]);
    let boundary = successful_json(common::run_identedit(&[
        "edit",
        file.to_str().unwrap(),
        "--at",
        "file-end",
        "--insert",
        "\n",
    ]));
    let expected_hash = common::hash_text(source);
    let actual_hash = common::hash_text(&changed);
    let movement = json!({"files":[{
        "file":file,
        "operations":[{
            "target":{"type":"file","expected_file_hash":expected_hash},
            "op":{"type":"move","to":destination},
            "preview":{"move":{"from":file,"to":destination}}
        }]
    }]});
    std::fs::write(&file, &changed).unwrap();
    let patch = json!({
        "command":"patch", "file":file,
        "target":{"type":"config_path","path":"inside.value","expected_file_hash":expected_hash},
        "op":{"type":"set","new_text":"2"}
    });

    for output in [
        apply(&config, &["--dry-run"]),
        apply(&boundary, &["--dry-run"]),
        apply(&movement, &["--dry-run"]),
        common::run_identedit_with_stdin(&["patch", "--json"], &patch.to_string()),
    ] {
        assert!(!output.status.success());
        let response: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(response["error"]["type"], "precondition_failed");
        assert_eq!(
            response["error"]["message"],
            format!(
                "File '{}' does not match the expected source hash. Expected hash '{}', got '{}'",
                file.display(),
                expected_hash,
                actual_hash,
            )
        );
        assert_eq!(
            response["error"]["suggestion"],
            "Re-read the file and recompile the plan (rerun edit/patch); repair cannot refresh this file condition"
        );
        assert_eq!(std::fs::read_to_string(&file).unwrap(), changed);
        assert!(!destination.exists());
    }
}

#[test]
fn legacy_node_hash_failure_keeps_node_handle_recovery() {
    let directory = TempDir::new().unwrap();
    let file = directory.path().join("settings.json");
    let source = "{\"inside\":{\"value\":1},\"outside\":0}\n";
    std::fs::write(&file, source).unwrap();
    let mut plan = config_plan(&file, "inside.value", &["--set-value", "2"]);
    let expected = plan["files"][0]["operations"][0]["target"]["expected_old_hash"]
        .as_str()
        .unwrap()
        .to_owned();
    plan["files"][0]
        .as_object_mut()
        .unwrap()
        .remove("expected_file_hash");
    let changed = source.replace("value\":1", "value\":9");
    std::fs::write(&file, &changed).unwrap();

    let output = apply(&plan, &["--dry-run"]);
    assert!(!output.status.success());
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["error"]["type"], "precondition_failed");
    assert_eq!(
        response["error"]["message"],
        format!(
            "Target node has changed since selection. Expected hash '{}', got '{}'",
            expected,
            common::hash_text("{\"value\":9}"),
        )
    );
    assert_eq!(
        response["error"]["suggestion"],
        "Re-run 'identedit read' to get updated handles"
    );
    assert_eq!(std::fs::read_to_string(file).unwrap(), changed);
}

#[test]
fn config_boundary_creation_keeps_a_hash_of_bom_and_whitespace_bytes() {
    let directory = TempDir::new().unwrap();
    let source = "\u{feff} \r\n";
    for extension in ["json", "toml", "yaml"] {
        let file = directory.path().join(format!("empty.{extension}"));
        std::fs::write(&file, source).unwrap();
        let plan = config_plan(&file, "value", &["--set-value", "2", "--create-missing"]);
        assert_eq!(
            plan["files"][0]["expected_file_hash"],
            common::hash_text(source)
        );
        successful_json(apply(&plan, &["--dry-run"]));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), source);
        successful_json(apply(&plan, &[]));
        assert!(std::fs::read_to_string(&file).unwrap().starts_with(source));
    }
}

#[test]
fn patch_config_json_cannot_replace_a_stale_supplied_file_hash() {
    let directory = TempDir::new().unwrap();
    let file = directory.path().join("settings.json");
    let observed = "{\"inside\":{\"value\":1},\"outside\":0}\n";
    let changed = observed.replace("outside\":0", "outside\":9");
    std::fs::write(&file, &changed).unwrap();
    let request = json!({
        "command":"patch", "file":file,
        "target":{"type":"config_path","path":"inside.value","expected_file_hash":common::hash_text(observed)},
        "op":{"type":"set","new_text":"2"}
    });
    assert_precondition_failure(common::run_identedit_with_stdin(
        &["patch", "--json"],
        &request.to_string(),
    ));
    assert_eq!(std::fs::read_to_string(file).unwrap(), changed);
}

#[test]
fn merge_conjoins_file_guards_and_retains_guarded_empty_entries() {
    let directory = TempDir::new().unwrap();
    let file = directory.path().join("settings.json");
    let source = "{\"value\":1}\n";
    std::fs::write(&file, source).unwrap();
    let hash = common::hash_text(source);
    for guards in [
        [None, Some(hash.as_str())],
        [Some(hash.as_str()), None],
        [Some(hash.as_str()), Some(hash.as_str())],
    ] {
        let plans = guards.map(|guard| empty_plan(&file, guard));
        let merged = successful_json(merge(directory.path(), &plans));
        assert_eq!(merged["files"][0]["expected_file_hash"], hash);
        assert!(
            merged["files"][0]["operations"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        successful_json(apply(&merged, &["--dry-run"]));

        std::fs::write(&file, "{\"value\":2}\n").unwrap();
        assert_precondition_failure(apply(&merged, &[]));
        std::fs::write(&file, source).unwrap();
    }

    let output = merge(
        directory.path(),
        &[
            empty_plan(&file, Some(&hash)),
            empty_plan(&file, Some(&common::hash_text("different"))),
        ],
    );
    assert!(!output.status.success());
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["error"]["type"], "invalid_request");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), source);
}

#[test]
fn merge_rejects_conflicting_move_and_file_conditions() {
    let directory = TempDir::new().unwrap();
    let file = directory.path().join("source.json");
    let destination = directory.path().join("moved.json");
    let source = "{\"value\":1}\n";
    std::fs::write(&file, source).unwrap();
    let hash = common::hash_text(source);
    let conflicting = common::hash_text("different source");
    let movement = json!({"files":[{
        "file":file,
        "operations":[{
            "target":{"type":"file","expected_file_hash":hash},
            "op":{"type":"move","to":destination},
            "preview":{"move":{"from":file,"to":destination}}
        }]
    }]});
    let mut combined = movement.clone();
    combined["files"][0]["expected_file_hash"] = json!(conflicting);

    for plans in [
        vec![empty_plan(&file, Some(&conflicting)), movement.clone()],
        vec![movement.clone(), empty_plan(&file, Some(&conflicting))],
        vec![combined],
    ] {
        let output = merge(directory.path(), &plans);
        assert!(
            !output.status.success(),
            "merge must reject contradictory guards"
        );
        let response: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(response["error"]["type"], "invalid_request");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), source);
        assert!(!destination.exists());
    }

    for expected in [None, Some(hash.as_str())] {
        for plans in [
            vec![empty_plan(&file, expected), movement.clone()],
            vec![movement.clone(), empty_plan(&file, expected)],
        ] {
            let merged = successful_json(merge(directory.path(), &plans));
            assert_eq!(
                merged["files"][0]["operations"],
                movement["files"][0]["operations"]
            );
            assert_eq!(merged["files"][0]["expected_file_hash"], json!(expected));
            successful_json(apply(&merged, &["--dry-run"]));
            assert_eq!(std::fs::read_to_string(&file).unwrap(), source);
            assert!(!destination.exists());
        }
    }

    let mut stale_movement = movement;
    stale_movement["files"][0]["operations"][0]["target"]["expected_file_hash"] =
        json!(conflicting);
    let stale_merged = successful_json(merge(
        directory.path(),
        &[empty_plan(&file, Some(&conflicting)), stale_movement],
    ));
    assert_eq!(stale_merged["files"][0]["expected_file_hash"], conflicting);
    assert_precondition_failure(apply(&stale_merged, &["--dry-run"]));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), source);
    assert!(!destination.exists());
}

#[test]
fn merge_checks_file_conditions_against_file_boundary_targets() {
    let directory = TempDir::new().unwrap();
    let file = directory.path().join("settings.json");
    let source = "{\"value\":1}\n";
    std::fs::write(&file, source).unwrap();
    let hash = common::hash_text(source);
    let conflicting = common::hash_text("different source");

    for boundary in ["file-start", "file-end"] {
        let boundary_plan = successful_json(common::run_identedit(&[
            "edit",
            file.to_str().unwrap(),
            "--at",
            boundary,
            "--insert",
            "\n",
        ]));
        for plans in [
            vec![empty_plan(&file, Some(&conflicting)), boundary_plan.clone()],
            vec![boundary_plan.clone(), empty_plan(&file, Some(&conflicting))],
        ] {
            let output = merge(directory.path(), &plans);
            assert!(
                !output.status.success(),
                "merge must reject contradictory guards"
            );
            let response: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(response["error"]["type"], "invalid_request");
            assert_eq!(std::fs::read_to_string(&file).unwrap(), source);
        }

        let merged = successful_json(merge(
            directory.path(),
            &[empty_plan(&file, Some(&hash)), boundary_plan],
        ));
        assert_eq!(merged["files"][0]["expected_file_hash"], hash);
        successful_json(apply(&merged, &["--dry-run"]));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), source);
    }
}

#[test]
fn guarded_empty_entries_check_arbitrary_bytes_without_parsing_or_writing() {
    use std::fs::FileTimes;
    use std::time::{Duration, UNIX_EPOCH};

    let directory = TempDir::new().unwrap();
    for (name, source) in [
        ("valid.json", b"{\"value\":1}\n".as_slice()),
        ("invalid.py", b"def incomplete(:\n".as_slice()),
        ("binary.bin", b"\xff\0raw\r\n".as_slice()),
    ] {
        let file = directory.path().join(name);
        std::fs::write(&file, source).unwrap();
        std::fs::File::options()
            .write(true)
            .open(&file)
            .unwrap()
            .set_times(FileTimes::new().set_modified(UNIX_EPOCH + Duration::from_secs(1_000_000)))
            .unwrap();
        let before = std::fs::metadata(&file).unwrap();
        let plan = empty_plan(&file, Some(&common::hash_bytes(source)));

        for options in [vec!["--dry-run"], vec![], vec!["--repair"]] {
            let response = successful_json(apply(&plan, &options));
            assert_eq!(response["summary"]["files_modified"], 0);
            assert_eq!(response["summary"]["operations_applied"], 0);
            assert_eq!(std::fs::read(&file).unwrap(), source);
            assert_eq!(
                std::fs::metadata(&file).unwrap().modified().unwrap(),
                before.modified().unwrap()
            );
        }

        std::fs::write(&file, b"changed").unwrap();
        assert_precondition_failure(apply(&plan, &[]));
        assert_eq!(std::fs::read(file).unwrap(), b"changed");
    }
}

#[cfg(unix)]
#[test]
fn guarded_empty_entries_preserve_hardlink_identity() {
    use std::os::unix::fs::MetadataExt;

    let directory = TempDir::new().unwrap();
    let file = directory.path().join("settings.json");
    let alias = directory.path().join("alias.json");
    let source = b"{\"value\":1}\n";
    std::fs::write(&file, source).unwrap();
    std::fs::hard_link(&file, &alias).unwrap();
    let before = std::fs::metadata(&file).unwrap();
    let plan = empty_plan(&file, Some(&common::hash_bytes(source)));

    successful_json(apply(&plan, &[]));
    assert_eq!(std::fs::metadata(&file).unwrap().ino(), before.ino());
    assert_eq!(std::fs::metadata(&file).unwrap().nlink(), 2);
    std::fs::write(&alias, b"still linked").unwrap();
    assert_eq!(std::fs::read(file).unwrap(), b"still linked");
}

#[test]
fn file_guards_reject_changed_bytes_before_decoding_or_parsing() {
    let directory = TempDir::new().unwrap();
    let file = directory.path().join("settings.json");
    let source = "{\"inside\":{\"value\":1}}\n";
    std::fs::write(&file, source).unwrap();
    let plan = config_plan(&file, "inside.value", &["--set-value", "2"]);
    let anchor = format!("1:{}", common::compute_line_hash(source.trim_end()));
    let mut line_plan = successful_json(common::run_identedit(&[
        "edit",
        file.to_str().unwrap(),
        "--at",
        &anchor,
        "--replace",
        "{}",
    ]));
    line_plan["files"][0]["expected_file_hash"] = json!(common::hash_text(source));
    let patch = json!({
        "command":"patch", "file":file,
        "target":{
            "type":"config_path", "path":"inside.value",
            "expected_file_hash":common::hash_text(source)
        },
        "op":{"type":"set","new_text":"2"}
    });

    for changed in [b"{\"inside\":".as_slice(), b"\xff\0changed".as_slice()] {
        std::fs::write(&file, changed).unwrap();
        for plan in [&plan, &line_plan] {
            for options in [vec![], vec!["--dry-run"], vec!["--repair"]] {
                assert_precondition_failure(apply(plan, &options));
                assert_eq!(std::fs::read(&file).unwrap(), changed);
            }
        }
        assert_precondition_failure(common::run_identedit_with_stdin(
            &["patch", "--json"],
            &patch.to_string(),
        ));
        assert_eq!(std::fs::read(&file).unwrap(), changed);
    }
}

#[test]
fn line_repair_cannot_refresh_a_file_precondition() {
    let directory = TempDir::new().unwrap();
    let file = directory.path().join("settings.txt");
    let original = "first\nsecond\n";
    std::fs::write(&file, original).unwrap();
    let anchor = format!("2:{}", common::compute_line_hash("second"));
    let mut plan = successful_json(common::run_identedit(&[
        "edit",
        file.to_str().unwrap(),
        "--at",
        &anchor,
        "--replace",
        "updated",
    ]));
    assert!(plan["files"][0].get("expected_file_hash").is_none());
    plan["files"][0]["expected_file_hash"] = json!(common::hash_text(original));
    let changed = "prefix\nfirst\nsecond\n";
    std::fs::write(&file, changed).unwrap();

    assert_precondition_failure(apply(&plan, &["--repair"]));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), changed);

    plan["files"][0]
        .as_object_mut()
        .unwrap()
        .remove("expected_file_hash");
    successful_json(apply(&plan, &["--repair"]));
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "prefix\nfirst\nupdated\n"
    );
}

#[test]
fn file_moves_must_satisfy_both_file_and_target_preconditions() {
    let directory = TempDir::new().unwrap();
    let file = directory.path().join("source.bin");
    let destination = directory.path().join("destination.bin");
    let source = b"\xff\0raw\r\n";
    std::fs::write(&file, source).unwrap();
    let hash = common::hash_bytes(source);
    let mut plan = json!({"files":[{
        "file": file,
        "expected_file_hash": common::hash_text("stale"),
        "operations":[{
            "target":{"type":"file","expected_file_hash":hash},
            "op":{"type":"move","to":destination},
            "preview":{"move":{"from":file,"to":destination}}
        }]
    }]});

    for options in [vec![], vec!["--dry-run"], vec!["--repair"]] {
        let output = apply(&plan, &options);
        assert!(!output.status.success());
        let response: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(response["error"]["type"], "invalid_request");
        assert_eq!(std::fs::read(&file).unwrap(), source);
        assert!(!destination.exists());
    }

    plan["files"][0]["expected_file_hash"] = json!(hash);
    successful_json(apply(&plan, &["--dry-run"]));
    successful_json(apply(&plan, &[]));
    assert!(!file.exists());
    assert_eq!(std::fs::read(destination).unwrap(), source);
}

#[test]
fn a_stale_file_condition_aborts_the_entire_transaction_before_writes() {
    let directory = TempDir::new().unwrap();
    let first = directory.path().join("a.json");
    let second = directory.path().join("b.yaml");
    let first_source = "{\"value\":1}\n";
    let second_source = "inside:\n  value: 1\noutside: 0\n";
    std::fs::write(&first, first_source).unwrap();
    std::fs::write(&second, second_source).unwrap();
    let mut plan = config_plan(&first, "value", &["--set-value", "2"]);
    let second_plan = config_plan(&second, "inside.value", &["--set-value", "2"]);
    plan["files"]
        .as_array_mut()
        .unwrap()
        .push(second_plan["files"][0].clone());
    let changed = second_source.replace("outside: 0", "outside: 9");
    std::fs::write(&second, &changed).unwrap();

    assert_precondition_failure(apply(&plan, &[]));
    assert_eq!(std::fs::read_to_string(&first).unwrap(), first_source);
    assert_eq!(std::fs::read_to_string(&second).unwrap(), changed);
}

#[test]
fn rollback_uses_post_write_guards_not_the_compilers_source_condition() {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let directory = TempDir::new().unwrap();
    let mut entries = Vec::new();
    let source = "{\"value\":1}\n";
    for name in ["a.json", "b.json"] {
        let file = directory.path().join(name);
        std::fs::write(&file, source).unwrap();
        let plan = config_plan(&file, "value", &["--set-value", "2"]);
        entries.push(plan["files"][0].clone());
    }
    let plan = json!({"files": entries});
    let mut child = Command::new(env!("CARGO_BIN_EXE_identedit"))
        .args(["apply", "--inject-failure-after-writes", "1"])
        .env("IDENTEDIT_EXPERIMENTAL", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(plan.to_string().as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(!output.status.success());
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["error"]["type"], "invalid_request");
    for name in ["a.json", "b.json"] {
        assert_eq!(
            std::fs::read_to_string(directory.path().join(name)).unwrap(),
            source
        );
    }
}
