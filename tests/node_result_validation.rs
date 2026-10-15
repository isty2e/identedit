use std::fs;
use std::path::Path;
use std::process::Output;

use serde_json::{Value, json};
use tempfile::tempdir;

mod common;

const JAVASCRIPT: &str = "function target() { return 1; }\nconst stable = 2;\n";
const VALID_REPLACEMENT: &str = "function target() { return 9; }";
const INVALID_REPLACEMENT: &str = "function target() { return 9;";

fn node_target(file: &Path, kind: &str, name: Option<&str>) -> Value {
    let handle = common::select_first_handle(file, kind, name);
    json!({
        "type": "node",
        "identity": handle["identity"],
        "kind": handle["kind"],
        "expected_old_hash": handle["expected_old_hash"],
        "span_hint": handle["span"]
    })
}

fn replacement(target: Value, new_text: &str) -> Value {
    json!({"target": target, "op": {"type": "replace", "new_text": new_text}})
}

fn compile(files: Value) -> Value {
    let request = json!({"command": "edit", "files": files});
    let output = common::run_identedit_with_stdin(&["edit", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "plan compilation failed: {} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn apply(changeset: &Value, dry_run: bool) -> Output {
    let arguments = if dry_run {
        vec!["apply", "--dry-run"]
    } else {
        vec!["apply"]
    };
    common::run_identedit_with_stdin(&arguments, &changeset.to_string())
}

fn assert_rejected(output: &Output, file: &Path, before: &str) {
    assert!(!output.status.success(), "invalid result must be rejected");
    assert!(output.stderr.is_empty(), "runtime errors use stdout JSON");
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["error"]["type"], "parse_failure");
    let message = response["error"]["message"].as_str().unwrap();
    assert!(message.contains("proposed result"), "{message}");
    assert!(message.contains(file.to_str().unwrap()), "{message}");
    assert!(message.contains("before writing"), "{message}");
    assert!(response["error"]["suggestion"].is_string());
    assert_eq!(fs::read_to_string(file).unwrap(), before);
}

#[test]
fn node_patch_rejects_invalid_result_for_each_selector_and_preview_mode() {
    let root = tempdir().unwrap();
    let file = root.path().join("example.js");
    fs::write(&file, JAVASCRIPT).unwrap();
    let target = node_target(&file, "function_declaration", Some("target"));

    for selector in ["--symbol", "--at"] {
        let value = if selector == "--symbol" {
            "target"
        } else {
            target["identity"].as_str().unwrap()
        };
        for mode in [vec![], vec!["--dry-run"], vec!["--dry-run", "--diff"]] {
            fs::write(&file, JAVASCRIPT).unwrap();
            let mut arguments = vec![
                "patch",
                file.to_str().unwrap(),
                selector,
                value,
                "--replace",
                INVALID_REPLACEMENT,
            ];
            arguments.extend(mode);
            let output = common::run_identedit(&arguments);
            assert_rejected(&output, &file, JAVASCRIPT);
        }
    }
}

#[test]
fn edit_compiles_invalid_result_but_apply_rejects_it_with_or_without_json_wrapper() {
    let root = tempdir().unwrap();
    let file = root.path().join("example.js");
    fs::write(&file, JAVASCRIPT).unwrap();
    let target = node_target(&file, "function_declaration", Some("target"));
    let changeset = compile(json!([{
        "file": file, "operations": [replacement(target, INVALID_REPLACEMENT)]
    }]));
    assert_eq!(fs::read_to_string(&file).unwrap(), JAVASCRIPT);

    for dry_run in [true, false] {
        assert_rejected(&apply(&changeset, dry_run), &file, JAVASCRIPT);
        let repaired =
            common::run_identedit_with_stdin(&["apply", "--repair"], &changeset.to_string());
        assert_rejected(&repaired, &file, JAVASCRIPT);
        let request = json!({"command": "apply", "changeset": changeset});
        let mut arguments = vec!["apply", "--json"];
        if dry_run {
            arguments.push("--dry-run");
        }
        let output = common::run_identedit_with_stdin(&arguments, &request.to_string());
        assert_rejected(&output, &file, JAVASCRIPT);
    }
}

#[test]
fn patch_json_uses_the_same_result_validation() {
    let root = tempdir().unwrap();
    let file = root.path().join("example.js");
    fs::write(&file, JAVASCRIPT).unwrap();
    let target = node_target(&file, "function_declaration", Some("target"));
    let mut request = replacement(target, INVALID_REPLACEMENT);
    request["command"] = json!("patch");
    request["file"] = json!(file);

    for dry_run in [true, false] {
        let mut arguments = vec!["patch", "--json"];
        if dry_run {
            arguments.push("--dry-run");
        }
        let output = common::run_identedit_with_stdin(&arguments, &request.to_string());
        assert_rejected(&output, &file, JAVASCRIPT);
    }
}

#[test]
fn mixed_node_and_line_or_boundary_edits_validate_the_complete_final_file() {
    let root = tempdir().unwrap();
    let file = root.path().join("example.js");
    fs::write(&file, JAVASCRIPT).unwrap();
    let target = node_target(&file, "function_declaration", Some("target"));
    let bad_line = json!({
        "target": {"type": "line", "anchor": format!("2:{}", common::compute_line_hash("const stable = 2;"))},
        "op": {"type": "replace", "new_text": "const stable = ;"}
    });
    let bad_boundary = json!({
        "target": {"type": "file_end", "expected_file_hash": common::hash_text(JAVASCRIPT)},
        "op": {"type": "insert", "new_text": "\nfunction broken("}
    });

    for new_text in [VALID_REPLACEMENT, "function target() { return 1; }"] {
        for other in [&bad_line, &bad_boundary] {
            let changeset = compile(json!([{
                "file": file, "operations": [replacement(target.clone(), new_text), other]
            }]));
            for dry_run in [true, false] {
                assert_rejected(&apply(&changeset, dry_run), &file, JAVASCRIPT);
            }
        }
    }
}

#[test]
fn complementary_edits_are_checked_after_all_replacements_not_individually() {
    let root = tempdir().unwrap();
    let file = root.path().join("example.js");
    let before = "function target() { return 1; }\n";
    fs::write(&file, before).unwrap();
    let target = node_target(&file, "function_declaration", Some("target"));
    let changeset = compile(json!([{
        "file": file,
        "operations": [
            replacement(target, INVALID_REPLACEMENT),
            {"target": {"type": "file_end", "expected_file_hash": common::hash_text(before)},
             "op": {"type": "insert", "new_text": "}\n"}}
        ]
    }]));
    let preview = apply(&changeset, true);
    assert!(
        preview.status.success(),
        "{}",
        String::from_utf8_lossy(&preview.stdout)
    );
    assert_eq!(fs::read_to_string(&file).unwrap(), before);
    let output = apply(&changeset, false);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert_eq!(
        fs::read_to_string(file).unwrap(),
        "function target() { return 9;\n}\n"
    );
}

#[test]
fn invalid_result_aborts_other_file_edits_and_whole_file_moves_before_commit() {
    let root = tempdir().unwrap();
    let good = root.path().join("a.js");
    let bad = root.path().join("z.js");
    let source = root.path().join("source.txt");
    let destination = root.path().join("destination.txt");
    fs::write(&good, JAVASCRIPT).unwrap();
    fs::write(&bad, JAVASCRIPT).unwrap();
    fs::write(&source, "move me").unwrap();
    let good_target = node_target(&good, "function_declaration", Some("target"));
    let bad_target = node_target(&bad, "function_declaration", Some("target"));
    let mut changeset = compile(json!([
        {"file": good, "operations": [replacement(good_target, VALID_REPLACEMENT)]},
        {"file": bad, "operations": [replacement(bad_target, INVALID_REPLACEMENT)]}
    ]));
    changeset["files"].as_array_mut().unwrap().push(json!({
        "file": source, "operations": [{
            "target": {"type": "file", "expected_file_hash": common::hash_text("move me")},
            "op": {"type": "move", "to": destination},
            "preview": {"move": {"from": source, "to": destination}}
        }]}
    ));

    for dry_run in [true, false] {
        assert_rejected(&apply(&changeset, dry_run), &bad, JAVASCRIPT);
        assert_eq!(fs::read_to_string(&good).unwrap(), JAVASCRIPT);
        assert_eq!(fs::read_to_string(&source).unwrap(), "move me");
        assert!(!destination.exists());
    }
}

#[test]
fn line_only_and_file_boundary_only_edits_do_not_claim_syntax_validation() {
    let root = tempdir().unwrap();
    let file = root.path().join("example.js");
    let before = "not valid javascript ???\n";
    let line = json!({
        "target": {"type": "line", "anchor": format!("1:{}", common::compute_line_hash(before.trim_end()))},
        "op": {"type": "replace", "new_text": "still invalid ???"}
    });
    let boundary = json!({
        "target": {"type": "file_end", "expected_file_hash": common::hash_text(before)},
        "op": {"type": "insert", "new_text": "more invalid ???\n"}
    });

    for operation in [line, boundary] {
        fs::write(&file, before).unwrap();
        let changeset = compile(json!([{"file": file, "operations": [operation]}]));
        assert!(apply(&changeset, true).status.success());
        assert_eq!(fs::read_to_string(&file).unwrap(), before);
        assert!(apply(&changeset, false).status.success());
        assert_ne!(fs::read_to_string(&file).unwrap(), before);
    }
}

#[test]
fn direct_line_patch_does_not_enable_the_node_result_gate() {
    let root = tempdir().unwrap();
    let file = root.path().join("example.js");
    let before = "function target() { return 1; }\n";
    fs::write(&file, before).unwrap();
    let anchor = format!("1:{}", common::compute_line_hash(before.trim_end()));
    let arguments = [
        "patch",
        file.to_str().unwrap(),
        "--at",
        &anchor,
        "--replace",
        INVALID_REPLACEMENT,
    ];
    let mut preview_arguments = arguments.to_vec();
    preview_arguments.extend(["--dry-run", "--diff"]);
    let preview = common::run_identedit(&preview_arguments);
    assert!(preview.status.success());
    assert!(String::from_utf8_lossy(&preview.stdout).contains(INVALID_REPLACEMENT));
    assert_eq!(fs::read_to_string(&file).unwrap(), before);
    assert!(common::run_identedit(&arguments).status.success());
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        format!("{INVALID_REPLACEMENT}\n")
    );
}

#[test]
fn line_replacement_preserves_nul_bytes_without_a_syntax_claim() {
    let root = tempdir().unwrap();
    let file = root.path().join("example.py");
    let before = "value = 1\n";
    fs::write(&file, before).unwrap();
    let operation = json!({
        "target": {"type": "line", "anchor": format!("1:{}", common::compute_line_hash("value = 1"))},
        "op": {"type": "replace", "new_text": "value = 'A\u{0000}B'"}
    });
    let changeset = compile(json!([{"file": file, "operations": [operation]}]));
    assert!(apply(&changeset, false).status.success());
    assert_eq!(fs::read(&file).unwrap(), b"value = 'A\0B'\n");
}

#[test]
fn scoped_regex_results_share_the_node_result_gate() {
    let root = tempdir().unwrap();
    let file = root.path().join("example.js");
    fs::write(&file, JAVASCRIPT).unwrap();
    for dry_run in [true, false] {
        let mut arguments = vec![
            "patch",
            file.to_str().unwrap(),
            "--symbol",
            "target",
            "--scoped-regex",
            "}",
            "--scoped-replacement",
            "",
        ];
        if dry_run {
            arguments.push("--dry-run");
        }
        let output = common::run_identedit(&arguments);
        assert_rejected(&output, &file, JAVASCRIPT);
    }
}

#[test]
fn cross_file_node_move_cannot_bypass_result_validation_after_lowering() {
    let root = tempdir().unwrap();
    let source = root.path().join("a.py");
    let destination = root.path().join("z.py");
    let before_source = "def target():\n    return 1\n";
    let before_destination = "def stable():\n    return 2\n";
    fs::write(&source, before_source).unwrap();
    fs::write(&destination, before_destination).unwrap();
    let source_target = node_target(&source, "function_definition", Some("target"));
    let destination_target = node_target(&destination, "function_definition", Some("stable"));
    let operation = json!({
        "target": source_target,
        "op": {"type": "move_to_before", "destination_file": destination,
               "destination": destination_target}
    });
    let changeset = compile(json!([{"file": source, "operations": [operation]}]));
    for dry_run in [true, false] {
        assert_rejected(
            &apply(&changeset, dry_run),
            &destination,
            before_destination,
        );
        assert_eq!(fs::read_to_string(&source).unwrap(), before_source);
    }
}

#[test]
fn raw_python_in_file_moves_without_required_separators_are_rejected() {
    let root = tempdir().unwrap();
    let file = root.path().join("example.py");
    let before = "def first():\n    return 1\n\ndef second():\n    return 2\n";
    fs::write(&file, before).unwrap();
    let first = node_target(&file, "function_definition", Some("first"));
    let second = node_target(&file, "function_definition", Some("second"));
    for operation in [
        json!({"target": second.clone(), "op": {"type": "move_before", "destination": first.clone()}}),
        json!({"target": first, "op": {"type": "move_after", "destination": second}}),
    ] {
        let changeset = compile(json!([{"file": file, "operations": [operation]}]));
        for dry_run in [true, false] {
            assert_rejected(&apply(&changeset, dry_run), &file, before);
        }
    }
}

#[test]
fn fallback_node_edits_have_no_syntax_gate() {
    let root = tempdir().unwrap();
    let file = root.path().join("example.unknown");
    fs::write(&file, JAVASCRIPT).unwrap();
    let output = common::run_identedit(&[
        "patch",
        file.to_str().unwrap(),
        "--symbol",
        "target",
        "--replace",
        INVALID_REPLACEMENT,
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert_ne!(fs::read_to_string(file).unwrap(), JAVASCRIPT);
}

#[test]
fn invalid_node_insertions_deletions_and_in_file_moves_are_also_checked() {
    let root = tempdir().unwrap();
    let file = root.path().join("example.js");
    fs::write(&file, JAVASCRIPT).unwrap();
    let function = node_target(&file, "function_declaration", Some("target"));
    let parameters = node_target(&file, "formal_parameters", None);
    let end = json!({"type": "file_end", "expected_file_hash": common::hash_text(JAVASCRIPT)});
    let operations = [
        json!({"target": function.clone(), "op": {"type": "insert_before", "new_text": "function broken(\n"}}),
        json!({"target": function, "op": {"type": "insert_after", "new_text": "\nfunction broken("}}),
        json!({"target": parameters.clone(), "op": {"type": "delete"}}),
        json!({"target": parameters, "op": {"type": "move_after", "destination": end}}),
    ];

    for operation in operations {
        let changeset = compile(json!([{"file": file, "operations": [operation]}]));
        for dry_run in [true, false] {
            assert_rejected(&apply(&changeset, dry_run), &file, JAVASCRIPT);
        }
    }
}

#[test]
fn node_noop_and_valid_delete_preserve_normal_success() {
    let root = tempdir().unwrap();
    let file = root.path().join("example.js");
    fs::write(&file, JAVASCRIPT).unwrap();
    let target = node_target(&file, "function_declaration", Some("target"));
    let noop = replacement(target.clone(), "function target() { return 1; }");
    let delete = json!({"target": target, "op": {"type": "delete"}});

    for (operation, expected) in [(noop, JAVASCRIPT), (delete, "\nconst stable = 2;\n")] {
        let changeset = compile(json!([{"file": file, "operations": [operation]}]));
        assert!(apply(&changeset, true).status.success());
        assert_eq!(fs::read_to_string(&file).unwrap(), JAVASCRIPT);
        assert!(apply(&changeset, false).status.success());
        assert_eq!(fs::read_to_string(&file).unwrap(), expected);
    }
}

#[test]
fn header_validation_keeps_either_c_or_cpp_admission_including_dialect_changes() {
    let root = tempdir().unwrap();
    let file = root.path().join("example.h");
    let c_only = "int target(value)\nint value;\n{\n    return value;\n}\n";
    let cpp_only = "class Target { public: int value() { return 1; } };\n";

    for (before, after) in [(c_only, cpp_only), (cpp_only, c_only)] {
        fs::write(&file, before).unwrap();
        let target = node_target(&file, "translation_unit", None);
        let changeset = compile(json!([{
            "file": file, "operations": [replacement(target, after)]
        }]));
        assert!(apply(&changeset, true).status.success());
        assert_eq!(fs::read_to_string(&file).unwrap(), before);
        assert!(apply(&changeset, false).status.success());
        assert_eq!(
            fs::read_to_string(&file).unwrap().trim_end(),
            after.trim_end()
        );
    }

    let before = fs::read_to_string(&file).unwrap();
    let target = node_target(&file, "translation_unit", None);
    let changeset = compile(json!([{
        "file": file, "operations": [replacement(target, "int broken( {")]
    }]));
    for dry_run in [true, false] {
        assert_rejected(&apply(&changeset, dry_run), &file, &before);
    }
}

#[test]
fn json_node_replacement_is_checked_by_the_json_provider() {
    let root = tempdir().unwrap();
    let file = root.path().join("example.json");
    let before = "{\"values\": [1, 2], \"stable\": true}\n";
    fs::write(&file, before).unwrap();
    let target = node_target(&file, "array", Some("values"));
    let changeset = compile(json!([{
        "file": file, "operations": [replacement(target, "[1,")]
    }]));
    for dry_run in [true, false] {
        let output = apply(&changeset, dry_run);
        assert_rejected(&output, &file, before);
        let response: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert!(
            response["error"]["message"]
                .as_str()
                .unwrap()
                .contains("'json'")
        );
    }
}

#[test]
fn result_validation_preserves_utf8_bom_and_crlf_bytes() {
    let root = tempdir().unwrap();
    let file = root.path().join("example.js");
    let before = "\u{feff}// Unicode: 한글\r\nfunction target() { return 'λ'; }\r\n";
    fs::write(&file, before).unwrap();
    let target = node_target(&file, "function_declaration", Some("target"));
    let changeset = compile(json!([{
        "file": file, "operations": [replacement(target, VALID_REPLACEMENT)]
    }]));
    assert!(apply(&changeset, false).status.success());
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        before.replace("function target() { return 'λ'; }", VALID_REPLACEMENT)
    );
}

#[test]
fn original_parse_failure_is_not_mislabeled_as_a_rejected_result() {
    let root = tempdir().unwrap();
    let file = root.path().join("example.js");
    let before = "function target( {";
    fs::write(&file, before).unwrap();
    let output = common::run_identedit(&[
        "patch",
        file.to_str().unwrap(),
        "--symbol",
        "target",
        "--replace",
        VALID_REPLACEMENT,
    ]);
    assert!(!output.status.success());
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["error"]["type"], "parse_failure");
    assert!(
        !response["error"]["message"]
            .as_str()
            .unwrap()
            .contains("proposed result")
    );
    assert_eq!(fs::read_to_string(file).unwrap(), before);
}

#[test]
fn provider_acceptance_is_not_python_compilation_or_scope_safety() {
    let root = tempdir().unwrap();
    let file = root.path().join("example.py");
    let before = "class Example:\n    def first(self):\n        return 1\n\n    def target(self):\n        return 2\n\n    def after(self):\n        return 3\n";

    for payload in [
        "    def target(self):\n        return 9",
        "    def target(self): return 9",
    ] {
        fs::write(&file, before).unwrap();
        let output = common::run_identedit(&[
            "patch",
            file.to_str().unwrap(),
            "--symbol",
            "Example.target",
            "--replace",
            payload,
        ]);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            before.replacen("def target(self):\n        return 2", payload, 1)
        );
        let read = common::run_identedit(&[
            "read",
            file.to_str().unwrap(),
            "--symbol",
            "Example.first.target",
            "--json",
        ]);
        assert!(
            read.status.success(),
            "{}",
            String::from_utf8_lossy(&read.stdout)
        );
        let response: Value = serde_json::from_slice(&read.stdout).unwrap();
        assert_eq!(response["summary"]["matches"], 1);
    }
}
