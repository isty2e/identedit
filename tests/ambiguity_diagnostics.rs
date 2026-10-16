use std::fs;
use std::process::Output;

use serde_json::{Value, json};
use tempfile::tempdir;

mod common;

const LEGACY_SUGGESTION: &str =
    "Use a more specific selector or re-run 'identedit read' to disambiguate";
const BOUNDED_SUGGESTION: &str = "Use a more specific selector, or run 'identedit read --json <file>' without --symbol or filters to list all nodes. Add --verbose for bodies; --name matches provider names, not qualified paths. Copy guards and span_hint into patch --json or edit --json.";

fn successful_json(output: &Output) -> Value {
    assert!(
        output.status.success(),
        "{} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn method_source(count: usize) -> String {
    (0..count)
        .map(|index| format!("class C{index:04}:\n    def target(self):\n        return 0\n\n"))
        .collect()
}

#[test]
fn contextual_ambiguity_preserves_full_counts_and_bounded_order_across_commands() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("methods.py");
    let file_arg = file.to_str().unwrap();

    for count in [2, 16, 17, 1000] {
        let source = method_source(count);
        fs::write(&file, &source).unwrap();
        let inventory = successful_json(&common::run_identedit(&[
            "read",
            file_arg,
            "--json",
            "--kind",
            "function_definition",
        ]));
        let handles = inventory["handles"].as_array().unwrap();
        assert_eq!(handles.len(), count);
        let identity = handles[0]["identity"].as_str().unwrap();
        assert!(handles.iter().all(|handle| handle["identity"] == identity));

        let expected_candidates = handles
            .iter()
            .take(16)
            .enumerate()
            .map(|(index, handle)| {
                json!({
                    "identity": handle["identity"],
                    "expected_old_hash": handle["expected_old_hash"],
                    "kind": "function_definition",
                    "name": "target",
                    "qualified_name": format!("C{index:04}.target"),
                    "span": handle["span"],
                    "line": index * 4 + 2,
                    "preview": "def target(self):",
                })
            })
            .collect::<Vec<_>>();

        for command in ["read", "edit", "patch"] {
            for selector in [
                vec!["--symbol", "target"],
                vec!["--kind", "function_definition", "--name", "target"],
                vec!["--at", identity],
            ] {
                if command == "read" && selector[0] != "--symbol" {
                    continue;
                }

                let description = match selector[0] {
                    "--symbol" => "selector 'symbol='target''".to_string(),
                    "--kind" => "selector 'kind='function_definition', name='target''".to_string(),
                    "--at" => format!("identity '{identity}'"),
                    _ => unreachable!(),
                };
                let mut expected = json!({
                    "error": {
                        "type": "ambiguous_target",
                        "message": format!(
                            "Multiple targets matched {description} in file '{file_arg}' ({count} candidates)"
                        ),
                        "suggestion": if count > 16 { BOUNDED_SUGGESTION } else { LEGACY_SUGGESTION },
                        "candidates": expected_candidates,
                    },
                });
                if count > 16 {
                    expected["error"]["candidate_summary"] =
                        json!({"total": count, "omitted": count - 16});
                }

                let mut args = vec![command, file_arg];
                args.extend(selector);
                if command == "read" {
                    args.push("--json");
                } else {
                    args.push("--delete");
                }
                if command == "patch" {
                    args.push("--dry-run");
                }
                let output = common::run_identedit(&args);
                assert_eq!(output.status.code(), Some(1), "{args:?}");
                assert!(output.stderr.is_empty(), "{args:?}");
                let actual: Value = serde_json::from_slice(&output.stdout).unwrap();
                assert_eq!(actual, expected, "{args:?}");
                assert_eq!(fs::read_to_string(&file).unwrap(), source);

                if count == 17 {
                    args.push("--verbose");
                    let verbose = common::run_identedit(&args);
                    assert_eq!(verbose.status.code(), output.status.code());
                    assert_eq!(verbose.stdout, output.stdout);
                    assert_eq!(verbose.stderr, output.stderr);
                    assert_eq!(fs::read_to_string(&file).unwrap(), source);
                }
            }
        }
    }
}

#[test]
fn omitted_repeated_identity_can_be_selected_with_a_fresh_patch_span() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("duplicate.py");
    let file_arg = file.to_str().unwrap();
    let source = "class Box:\n    def target(self):\n        return 1\n\n".repeat(17);
    fs::write(&file, &source).unwrap();

    let ambiguous = common::run_identedit(&["read", file_arg, "--symbol", "Box.target", "--json"]);
    assert_eq!(ambiguous.status.code(), Some(1));
    let ambiguous: Value = serde_json::from_slice(&ambiguous.stdout).unwrap();
    assert_eq!(
        ambiguous["error"]["candidate_summary"],
        json!({"total": 17, "omitted": 1})
    );

    let inventory = successful_json(&common::run_identedit(&[
        "read",
        file_arg,
        "--json",
        "--verbose",
    ]));
    let methods = inventory["handles"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|handle| handle["kind"] == "function_definition")
        .collect::<Vec<_>>();
    assert_eq!(methods.len(), 17);
    let selected = methods[16];
    assert!(
        methods
            .iter()
            .all(|handle| handle["identity"] == selected["identity"])
    );
    assert_eq!(selected["text"], "def target(self):\n        return 1");

    let replacement = "def target(self):\n        return 2";
    let mut request = json!({
        "command": "patch",
        "file": file,
        "target": {
            "type": "node",
            "identity": selected["identity"],
            "kind": selected["kind"],
            "expected_old_hash": selected["expected_old_hash"],
        },
        "op": {"type": "replace", "new_text": replacement},
    });
    let unresolved = common::run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert_eq!(unresolved.status.code(), Some(1));
    let unresolved: Value = serde_json::from_slice(&unresolved.stdout).unwrap();
    assert_eq!(unresolved["error"]["type"], "ambiguous_target");
    assert!(unresolved["error"].get("candidates").is_none());
    assert!(unresolved["error"].get("candidate_summary").is_none());
    assert_eq!(fs::read_to_string(&file).unwrap(), source);

    let edit_request = json!({
        "command": "edit",
        "file": file,
        "operations": [{
            "target": {
                "type": "node",
                "identity": selected["identity"],
                "kind": selected["kind"],
                "expected_old_hash": selected["expected_old_hash"],
                "span_hint": selected["span"],
            },
            "op": request["op"],
        }],
    });
    let mut plan = successful_json(&common::run_identedit_with_stdin(
        &["edit", "--json"],
        &edit_request.to_string(),
    ));
    plan["files"][0]["operations"][0]["target"]
        .as_object_mut()
        .unwrap()
        .remove("span_hint");
    let unresolved_apply = common::run_identedit_with_stdin(&["apply"], &plan.to_string());
    assert_eq!(unresolved_apply.status.code(), Some(1));
    let unresolved_apply: Value = serde_json::from_slice(&unresolved_apply.stdout).unwrap();
    assert_eq!(unresolved_apply["error"]["type"], "ambiguous_target");
    assert!(unresolved_apply["error"].get("candidates").is_none());
    assert!(unresolved_apply["error"].get("candidate_summary").is_none());
    assert_eq!(fs::read_to_string(&file).unwrap(), source);

    request["target"]["span_hint"] = selected["span"].clone();
    successful_json(&common::run_identedit_with_stdin(
        &["patch", "--json"],
        &request.to_string(),
    ));
    let start = selected["span"]["start"].as_u64().unwrap() as usize;
    let end = selected["span"]["end"].as_u64().unwrap() as usize;
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        format!("{}{replacement}{}", &source[..start], &source[end..]),
    );
}

#[test]
fn bounded_candidates_preserve_long_opaque_names_and_distinct_kinds() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("names.json");
    let file_arg = file.to_str().unwrap();
    let name = format!("literal.{}", "한".repeat(130));
    let members = (0..9)
        .map(|index| format!("\"{name}\":{index}"))
        .collect::<Vec<_>>()
        .join(",");
    let source = format!("{{{members}}}\n");
    fs::write(&file, &source).unwrap();

    let output = common::run_identedit(&["read", file_arg, "--symbol", &name, "--json"]);
    assert_eq!(output.status.code(), Some(1));
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        response["error"]["candidate_summary"],
        json!({"total": 18, "omitted": 2})
    );
    let candidates = response["error"]["candidates"].as_array().unwrap();
    assert_eq!(candidates.len(), 16);
    for (index, candidate) in candidates.iter().enumerate() {
        assert_eq!(candidate["name"], name);
        assert_eq!(candidate["qualified_name"], name);
        assert_eq!(
            candidate["kind"],
            if index % 2 == 0 { "key" } else { "number" }
        );
    }
    assert_eq!(fs::read_to_string(&file).unwrap(), source);
}

#[test]
fn successful_discovery_and_a_unique_late_symbol_are_not_capped() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("late.py");
    let file_arg = file.to_str().unwrap();
    let source = method_source(17);
    fs::write(&file, &source).unwrap();

    let inventory = successful_json(&common::run_identedit(&[
        "read",
        file_arg,
        "--json",
        "--kind",
        "function_definition",
        "--name",
        "target",
    ]));
    assert_eq!(inventory["summary"]["matches"], 17);
    assert_eq!(inventory["handles"].as_array().unwrap().len(), 17);

    let selected = successful_json(&common::run_identedit(&[
        "read",
        file_arg,
        "--json",
        "--symbol",
        "C0016.target",
    ]));
    assert_eq!(selected["summary"]["matches"], 1);
    assert_eq!(selected["handles"][0], inventory["handles"][16]);
    assert_eq!(fs::read_to_string(&file).unwrap(), source);
}
