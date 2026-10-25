mod common;

use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

use serde_json::{Value, json};

const SOURCE: &str = "def first():\n    return 1\n\ndef second():\n    return 2\n";

fn run(root: &Path, args: &[&str], input: Option<&str>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_identedit"));
    command.current_dir(root).args(args).stdin(Stdio::piped());
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn().unwrap();
    if let Some(input) = input {
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
    } else {
        drop(child.stdin.take());
    }
    child.wait_with_output().unwrap()
}

fn success(output: Output) -> String {
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn plan(root: &Path, files: Value) -> Value {
    let request = json!({"command":"edit", "files":files});
    serde_json::from_str(&success(run(
        root,
        &["edit", "--json"],
        Some(&request.to_string()),
    )))
    .unwrap()
}

fn node_file(name: &str, replacement: &str) -> Value {
    json!({"file":name,"operations":[{
        "target":{"type":"symbol","name":"first"},
        "op":{"type":"replace","new_text":replacement}
    }]})
}

fn node_plan(root: &Path) -> Value {
    plan(
        root,
        json!([node_file("sample.py", "def first():\n    return 9")]),
    )
}

fn response(root: &Path, args: &[&str], input: &Value) -> Value {
    let input = (!input.is_null()).then(|| input.to_string());
    serde_json::from_str(&success(run(root, args, input.as_deref()))).unwrap()
}

#[test]
fn saved_plan_diff_inputs_match_the_committed_diff_without_writing() {
    let root = tempfile::tempdir().unwrap();
    for name in ["a.py", "z.py"] {
        fs::write(root.path().join(name), SOURCE).unwrap();
    }
    let p = plan(
        root.path(),
        json!([
            node_file("z.py", "def first():\n    return 8"),
            node_file("a.py", "def first():\n    return 9")
        ]),
    );
    fs::write(root.path().join("plan.json"), p.to_string()).unwrap();
    let raw = p.to_string();
    let wrapped = json!({"command":"apply","changeset":p}).to_string();
    let expected = success(run(
        root.path(),
        &[
            "apply",
            "--dry-run",
            "--diff",
            "--color",
            "never",
            "plan.json",
        ],
        None,
    ));
    for (args, input) in [
        (vec!["apply", "--dry-run", "--diff"], raw.as_str()),
        (
            vec!["apply", "--json", "--dry-run", "--diff", "--verbose"],
            wrapped.as_str(),
        ),
    ] {
        assert_eq!(success(run(root.path(), &args, Some(input))), expected);
    }
    assert!(expected.find("--- a.py").unwrap() < expected.find("--- z.py").unwrap());
    for name in ["a.py", "z.py"] {
        assert_eq!(fs::read_to_string(root.path().join(name)).unwrap(), SOURCE);
    }
    let committed = response(root.path(), &["apply"], &p);
    assert_eq!(committed["diff"]["text"], expected);
    assert_eq!(committed["diff"]["file_moves"], json!([]));
    assert!(
        fs::read_to_string(root.path().join("a.py"))
            .unwrap()
            .contains("return 9")
    );
    assert!(
        fs::read_to_string(root.path().join("z.py"))
            .unwrap()
            .contains("return 8")
    );
}

#[test]
fn preview_and_commit_omit_diff_only_when_requested_or_not_committed() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("sample.py"), SOURCE).unwrap();
    let p = node_plan(root.path());
    let preview = response(root.path(), &["apply", "--dry-run"], &p);
    assert!(preview.get("diff").is_none());
    let committed = response(root.path(), &["apply", "--no-diff", "--verbose"], &p);
    assert!(committed.get("diff").is_none());
    assert_eq!(committed["summary"]["operations_applied"], 1);
    assert!(committed["applied"].is_array());
    assert!(
        fs::read_to_string(root.path().join("sample.py"))
            .unwrap()
            .contains("return 9")
    );
}

#[test]
fn patch_flags_show_actual_changes_not_incidental_context_as_additions() {
    let root = tempfile::tempdir().unwrap();
    let before = "def first():\n    # external\n    return 1\n";
    fs::write(root.path().join("sample.py"), before).unwrap();
    let result = response(
        root.path(),
        &[
            "patch",
            "sample.py",
            "--symbol",
            "first",
            "--scoped-regex",
            "return 1",
            "--scoped-replacement",
            "return 9",
        ],
        &Value::Null,
    );
    let text = result["diff"]["text"].as_str().unwrap();
    assert!(
        text.contains("     # external\n-    return 1\n+    return 9\n"),
        "{text}"
    );
    assert_eq!(
        fs::read_to_string(root.path().join("sample.py")).unwrap(),
        before.replace("return 1", "return 9")
    );
}

#[test]
fn line_patch_diff_preserves_bytes_and_noop_is_empty() {
    let root = tempfile::tempdir().unwrap();
    for before in ["old\r\n", "old", "old\n"] {
        for no_diff in [false, true] {
            fs::write(root.path().join("sample.txt"), before).unwrap();
            let anchor = format!("1:{}", common::compute_line_hash("old"));
            let mut args = vec!["patch", "sample.txt", "--at", &anchor, "--replace", "new"];
            if no_diff {
                args.push("--no-diff");
            }
            let result = response(root.path(), &args, &Value::Null);
            if no_diff {
                assert!(result.get("diff").is_none());
            } else {
                let diff = result["diff"]["text"].as_str().unwrap();
                if before.ends_with("\r\n") {
                    assert!(diff.contains("-old\r\n+new\r\n"));
                }
                if before == "old" {
                    assert!(diff.contains("\\ No newline at end of file"));
                }
            }
            assert_eq!(
                fs::read_to_string(root.path().join("sample.txt")).unwrap(),
                before.replace("old", "new")
            );
        }
    }
    fs::write(root.path().join("sample.txt"), "same\n").unwrap();
    let anchor = format!("1:{}", common::compute_line_hash("same"));
    let result = response(
        root.path(),
        &["patch", "sample.txt", "--at", &anchor, "--replace", "same"],
        &Value::Null,
    );
    assert_eq!(result["changed"], false);
    assert_eq!(result["diff"], json!({"text":"","file_moves":[]}));
}

#[test]
fn patch_json_ingresses_share_default_and_opt_out_contracts() {
    let root = tempfile::tempdir().unwrap();
    for mode in ["node", "line", "config", "boundary"] {
        for (option_no_diff, cli_no_diff) in [(false, false), (true, false), (false, true)] {
            fs::write(root.path().join("sample.py"), SOURCE).unwrap();
            fs::write(root.path().join("sample.txt"), "old\n").unwrap();
            fs::write(root.path().join("config.toml"), "value = 1\n").unwrap();
            let (file, target, op) = match mode {
                "node" => {
                    let read: Value = serde_json::from_str(&success(run(
                        root.path(),
                        &[
                            "read",
                            "sample.py",
                            "--symbol",
                            "first",
                            "--json",
                            "--verbose",
                        ],
                        None,
                    )))
                    .unwrap();
                    let h = &read["handles"][0];
                    (
                        "sample.py",
                        json!({"type":"node","identity":h["identity"],"kind":h["kind"],"span_hint":h["span"],"expected_old_hash":common::hash_text(h["text"].as_str().unwrap())}),
                        json!({"type":"scoped_regex","pattern":"return 1","replacement":"return 9"}),
                    )
                }
                "line" => (
                    "sample.txt",
                    json!({"type":"line","anchor":format!("1:{}", common::compute_line_hash("old"))}),
                    json!({"type":"replace","new_text":"new"}),
                ),
                "config" => (
                    "config.toml",
                    json!({"type":"config_path","path":"value"}),
                    json!({"type":"set","new_text":"9"}),
                ),
                _ => (
                    "sample.txt",
                    json!({"type":"file_start","expected_file_hash":common::hash_text("old\n")}),
                    json!({"type":"insert","new_text":"prefix\n"}),
                ),
            };
            let request = json!({"command":"patch","file":file,"target":target,"op":op,"options":{"no_diff":option_no_diff}});
            let args = if cli_no_diff {
                vec!["patch", "--json", "--no-diff"]
            } else {
                vec!["patch", "--json"]
            };
            let result = response(root.path(), &args, &request);
            assert_eq!(
                result.get("diff").is_none(),
                option_no_diff || cli_no_diff,
                "{mode}: {result}"
            );
            assert_ne!(
                fs::read_to_string(root.path().join(file)).unwrap(),
                if file == "sample.py" {
                    SOURCE
                } else if file == "sample.txt" {
                    "old\n"
                } else {
                    "value = 1\n"
                }
            );
        }
    }
}

#[test]
fn output_option_errors_do_not_write_or_wait_for_stdin() {
    let root = tempfile::tempdir().unwrap();
    for args in [
        vec!["apply", "--diff"],
        vec!["apply", "--color", "always"],
        vec!["apply", "--dry-run", "--diff", "--no-diff"],
        vec!["patch", "--dry-run", "--diff", "--no-diff"],
    ] {
        let output = run(root.path(), &args, None);
        assert_eq!(output.status.code(), Some(1));
        let error: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(error["error"]["type"], "invalid_request");
        assert!(output.stderr.is_empty());
    }
}

fn move_entry(name: &str, destination: &str, bytes: &[u8]) -> Value {
    json!({"file":name,"operations":[{
        "target":{"type":"file","expected_file_hash":common::hash_bytes(bytes)},
        "op":{"type":"move","to":destination},
        "preview":{"move":{"from":name,"to":destination}}
    }]})
}

#[test]
fn mixed_rename_plans_reject_text_preview_but_commit_complete_change_information() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("sample.py"), SOURCE).unwrap();
    let binary = [0xff, 0x80];
    fs::write(root.path().join("move.bin"), binary).unwrap();
    let mut p = node_plan(root.path());
    p["files"]
        .as_array_mut()
        .unwrap()
        .push(move_entry("move.bin", "moved.bin", &binary));
    let error = run(
        root.path(),
        &["apply", "--dry-run", "--diff"],
        Some(&p.to_string()),
    );
    assert_eq!(error.status.code(), Some(1));
    assert_eq!(
        serde_json::from_slice::<Value>(&error.stdout).unwrap()["error"]["type"],
        "invalid_request"
    );
    assert_eq!(fs::read(root.path().join("move.bin")).unwrap(), binary);
    assert!(!root.path().join("moved.bin").exists());
    assert_eq!(
        fs::read_to_string(root.path().join("sample.py")).unwrap(),
        SOURCE
    );
    let committed = response(root.path(), &["apply"], &p);
    assert!(
        committed["diff"]["text"]
            .as_str()
            .unwrap()
            .contains("+    return 9")
    );
    assert_eq!(
        committed["diff"]["file_moves"],
        json!([{
            "source":fs::canonicalize(root.path()).unwrap().join("move.bin").to_str().unwrap(),
            "destination":fs::canonicalize(root.path().join("moved.bin")).unwrap().to_str().unwrap()
        }])
    );
    assert_eq!(fs::read(root.path().join("moved.bin")).unwrap(), binary);
    assert!(!root.path().join("move.bin").exists());
}

#[test]
fn change_output_is_not_limited_by_locations_cap() {
    let root = tempfile::tempdir().unwrap();
    let mut requests = Vec::new();
    let mut moves = Vec::new();
    for n in 0..17 {
        let file = format!("sample{n:02}.py");
        fs::write(root.path().join(&file), SOURCE).unwrap();
        requests.push(node_file(&file, "def first():\n    return 9"));
        let source = format!("source{n:02}.bin");
        fs::write(root.path().join(&source), [0xff]).unwrap();
        moves.push(move_entry(&source, &format!("dest{n:02}.bin"), &[0xff]));
    }
    let p = plan(root.path(), json!(requests));
    let preview = success(run(
        root.path(),
        &["apply", "--dry-run", "--diff"],
        Some(&p.to_string()),
    ));
    assert_eq!(preview.matches("--- sample").count(), 17);
    let moved = response(root.path(), &["apply"], &json!({"files":moves}));
    assert_eq!(moved["locations"]["entries"].as_array().unwrap().len(), 16);
    assert_eq!(moved["diff"]["file_moves"].as_array().unwrap().len(), 17);
    assert_eq!(moved["diff"]["text"], "");
}

#[test]
fn unchanged_apply_preview_is_empty_and_color_does_not_change_bytes() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("sample.py"), SOURCE).unwrap();
    let p = node_plan(root.path());
    let plain = success(run(
        root.path(),
        &["apply", "--dry-run", "--diff", "--color", "never"],
        Some(&p.to_string()),
    ));
    let colored = success(run(
        root.path(),
        &["apply", "--dry-run", "--diff", "--color", "always"],
        Some(&p.to_string()),
    ));
    let ansi = regex::Regex::new("\x1b\\[[0-9;]*m").unwrap();
    assert_eq!(ansi.replace_all(&colored, ""), plain);
    let unchanged = plan(
        root.path(),
        json!([node_file("sample.py", "def first():\n    return 1")]),
    );
    assert!(
        success(run(
            root.path(),
            &["apply", "--dry-run", "--diff"],
            Some(&unchanged.to_string())
        ))
        .is_empty()
    );
    let result = response(root.path(), &["apply"], &unchanged);
    assert_eq!(result["diff"], json!({"text":"","file_moves":[]}));
}

fn copied_target(root: &Path, file: &str, symbol: &str) -> Value {
    let read: Value = serde_json::from_str(&success(run(
        root,
        &["read", file, "--symbol", symbol, "--json", "--verbose"],
        None,
    )))
    .unwrap();
    let h = &read["handles"][0];
    json!({"identity":h["identity"],"kind":h["kind"],"span_hint":h["span"],"expected_old_hash":common::hash_text(h["text"].as_str().unwrap())})
}

#[test]
fn structural_move_previews_and_commits_cover_every_affected_file() {
    let root = tempfile::tempdir().unwrap();
    for cross_file in [false, true] {
        fs::write(root.path().join("source.py"), SOURCE).unwrap();
        fs::write(
            root.path().join("destination.py"),
            "def anchor():\n    return 3\n",
        )
        .unwrap();
        let destination_file = if cross_file {
            "destination.py"
        } else {
            "source.py"
        };
        let destination_symbol = if cross_file { "anchor" } else { "first" };
        let source_symbol = if cross_file { "first" } else { "second" };
        let mut op = json!({"type":if cross_file { "move_to_before" } else { "move_before" },"destination":copied_target(root.path(), destination_file, destination_symbol),"suffix":"\n\n"});
        if cross_file {
            op["destination_file"] = json!(destination_file);
        }
        let p = plan(
            root.path(),
            json!([{"file":"source.py","operations":[{"target":copied_target(root.path(), "source.py", source_symbol),"op":op}]}]),
        );
        let diff = success(run(
            root.path(),
            &["apply", "--dry-run", "--diff"],
            Some(&p.to_string()),
        ));
        assert_eq!(
            fs::read_to_string(root.path().join("source.py")).unwrap(),
            SOURCE
        );
        let result = response(root.path(), &["apply"], &p);
        assert_eq!(result["diff"]["text"], diff);
        assert_eq!(diff.matches("--- ").count(), if cross_file { 2 } else { 1 });
        if cross_file {
            assert!(
                !fs::read_to_string(root.path().join("source.py"))
                    .unwrap()
                    .contains("def first")
            );
            assert!(
                fs::read_to_string(root.path().join(destination_file))
                    .unwrap()
                    .contains("def first")
            );
        } else {
            let text = fs::read_to_string(root.path().join("source.py")).unwrap();
            assert_eq!(text.matches("def first").count(), 1);
            assert!(text.find("def second").unwrap() < text.find("def first").unwrap());
        }
    }
}

#[test]
fn a_commit_failure_rolls_back_without_publishing_success_diff() {
    let root = tempfile::tempdir().unwrap();
    for name in ["a.py", "z.py"] {
        fs::write(root.path().join(name), SOURCE).unwrap();
    }
    let p = plan(
        root.path(),
        json!([
            node_file("a.py", "def first():\n    return 9"),
            node_file("z.py", "def first():\n    return 8")
        ]),
    );
    fs::write(root.path().join("plan.json"), p.to_string()).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_identedit"))
        .current_dir(root.path())
        .env("IDENTEDIT_EXPERIMENTAL", "1")
        .args(["apply", "--inject-failure-after-writes", "1", "plan.json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let error: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(error["error"]["type"], "invalid_request");
    assert!(error.get("diff").is_none());
    for name in ["a.py", "z.py"] {
        assert_eq!(fs::read_to_string(root.path().join(name)).unwrap(), SOURCE);
    }
}

#[test]
fn guard_only_binary_entries_are_checked_without_text_conversion() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("sample.py"), SOURCE).unwrap();
    let bytes = [0xff];
    fs::write(root.path().join("guard.bin"), bytes).unwrap();
    let mut p = node_plan(root.path());
    p["files"].as_array_mut().unwrap().push(
        json!({"file":"guard.bin","operations":[],"expected_file_hash":common::hash_bytes(&bytes)}),
    );
    assert!(
        success(run(
            root.path(),
            &["apply", "--dry-run", "--diff"],
            Some(&p.to_string())
        ))
        .contains("+    return 9")
    );
    fs::write(root.path().join("guard.bin"), [0xfe]).unwrap();
    for args in [vec!["apply", "--dry-run", "--diff"], vec!["apply"]] {
        let output = run(root.path(), &args, Some(&p.to_string()));
        assert_eq!(output.status.code(), Some(1));
        let error: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(error["error"]["type"], "precondition_failed");
        assert!(error.get("diff").is_none());
        assert_eq!(
            fs::read_to_string(root.path().join("sample.py")).unwrap(),
            SOURCE
        );
    }
}

#[test]
fn a_later_invalid_result_never_leaks_a_partial_diff() {
    let root = tempfile::tempdir().unwrap();
    for file in ["a.py", "z.py"] {
        fs::write(root.path().join(file), SOURCE).unwrap();
    }
    let p = plan(
        root.path(),
        json!([
            node_file("a.py", "def first():\n    return 9"),
            node_file("z.py", "def first(:\n")
        ]),
    );
    for args in [vec!["apply", "--dry-run", "--diff"], vec!["apply"]] {
        let output = run(root.path(), &args, Some(&p.to_string()));
        assert_eq!(output.status.code(), Some(1));
        let error: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(error["error"]["type"], "parse_failure");
        assert!(error.get("diff").is_none());
        for file in ["a.py", "z.py"] {
            assert_eq!(fs::read_to_string(root.path().join(file)).unwrap(), SOURCE);
        }
    }
}

#[test]
fn repair_preview_uses_remapped_text_but_never_refreshes_config_conditions() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("sample.txt"), "first\ntarget\nlast\n").unwrap();
    let anchor = format!("2:{}", common::compute_line_hash("target"));
    let p = plan(
        root.path(),
        json!([{"file":"sample.txt","operations":[{"target":{"type":"line","anchor":anchor},"op":{"type":"replace","new_text":"changed"}}]}]),
    );
    fs::write(
        root.path().join("sample.txt"),
        "added\nfirst\ntarget\nlast\n",
    )
    .unwrap();
    let diff = success(run(
        root.path(),
        &["apply", "--repair", "--dry-run", "--diff"],
        Some(&p.to_string()),
    ));
    assert!(
        diff.contains(" added\n first\n-target\n+changed\n last\n"),
        "{diff}"
    );
    assert_eq!(
        fs::read_to_string(root.path().join("sample.txt")).unwrap(),
        "added\nfirst\ntarget\nlast\n"
    );
    fs::write(root.path().join("config.toml"), "value = 1\n").unwrap();
    let p = plan(
        root.path(),
        json!([{"file":"config.toml","operations":[{"target":{"type":"config_path","path":"value"},"op":{"type":"set","new_text":"9"}}]}]),
    );
    fs::write(root.path().join("config.toml"), "value = 1\n# external\n").unwrap();
    let output = run(
        root.path(),
        &["apply", "--repair", "--dry-run", "--diff"],
        Some(&p.to_string()),
    );
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap()["error"]["type"],
        "precondition_failed"
    );
}
