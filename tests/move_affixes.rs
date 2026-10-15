use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;

use serde_json::{Value, json};
use tempfile::{TempDir, tempdir};

mod common;

fn successful_json(output: Output) -> Value {
    assert!(
        output.status.success(),
        "{} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn rejected_json(output: Output) -> Value {
    assert!(!output.status.success());
    serde_json::from_slice(&output.stdout).unwrap()
}

fn node_target(handle: &Value) -> Value {
    json!({
        "type": "node",
        "identity": handle["identity"],
        "kind": handle["kind"],
        "span_hint": handle["span"],
        "expected_old_hash": handle["expected_old_hash"],
    })
}

fn named_handle(file: &Path, name: &str) -> Value {
    let response = successful_json(common::run_identedit(&[
        "read",
        file.to_str().unwrap(),
        "--symbol",
        name,
        "--json",
        "--verbose",
    ]));
    assert_eq!(response["summary"]["matches"], 1);
    response["handles"][0].clone()
}

fn file_operation<'a>(plan: &'a Value, file: &Path) -> &'a Value {
    &plan["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["file"] == file.to_str().unwrap())
        .unwrap()["operations"][0]
}

struct RawMoveFixture {
    _directory: TempDir,
    source: PathBuf,
    destination: PathBuf,
    source_text: String,
    destination_text: String,
    source_handle: Value,
    destination_handle: Value,
    cross_file: bool,
    before: bool,
}

impl RawMoveFixture {
    fn new(cross_file: bool, before: bool, extension: &str) -> Self {
        let directory = tempdir().unwrap();
        let source = directory.path().join(format!("source.{extension}"));
        let destination = if cross_file {
            directory.path().join(format!("destination.{extension}"))
        } else {
            source.clone()
        };
        let (source_text, destination_text) = if extension == "py" {
            (
                "class Source:\n    def first(self):\n        return 1\n\n    def second(self):\n        return 2\n".to_string(),
                "class Destination:\n    def second(self):\n        return 2\n".to_string(),
            )
        } else {
            (
                "function first() { return 1; }\n\nfunction second() { return 2; }\n".to_string(),
                "function second() { return 2; }\n".to_string(),
            )
        };
        let destination_text = if cross_file {
            destination_text
        } else {
            source_text.clone()
        };
        fs::write(&source, &source_text).unwrap();
        fs::write(&destination, &destination_text).unwrap();
        let source_name = if before && !cross_file {
            "second"
        } else {
            "first"
        };
        let destination_name = if before && !cross_file {
            "first"
        } else {
            "second"
        };
        let source_handle = named_handle(&source, source_name);
        let destination_handle = named_handle(&destination, destination_name);

        Self {
            _directory: directory,
            source,
            destination,
            source_text,
            destination_text,
            source_handle,
            destination_handle,
            cross_file,
            before,
        }
    }

    fn request(&self, prefix: &str, suffix: &str) -> Value {
        let kind = match (self.cross_file, self.before) {
            (false, true) => "move_before",
            (false, false) => "move_after",
            (true, true) => "move_to_before",
            (true, false) => "move_to_after",
        };
        let mut operation = json!({
            "type": kind,
            "destination": node_target(&self.destination_handle),
            "prefix": prefix,
            "suffix": suffix,
        });
        if self.cross_file {
            operation["destination_file"] = json!(self.destination);
        }
        json!({
            "command": "edit",
            "file": self.source,
            "operations": [{ "target": node_target(&self.source_handle), "op": operation }],
        })
    }

    fn files(&self) -> (String, String) {
        (
            fs::read_to_string(&self.source).unwrap(),
            fs::read_to_string(&self.destination).unwrap(),
        )
    }

    fn compile(&self, request: &Value) -> Value {
        let before = self.files();
        let plan = successful_json(common::run_identedit_with_stdin(
            &["edit", "--json", "--verbose"],
            &request.to_string(),
        ));
        assert_eq!(self.files(), before, "edit must not write");
        let source = file_operation(&plan, &self.source);
        assert_eq!(source["target"], node_target(&self.source_handle));
        assert_eq!(
            source["preview"]["matched_span"],
            self.source_handle["span"]
        );
        assert_eq!(source["preview"]["old_text"], self.source_handle["text"]);
        assert_eq!(source["preview"]["new_text"], "");
        plan
    }

    fn expected_at(&self, offset: usize, prefix: &str, suffix: &str) -> (String, String) {
        let start = self.source_handle["span"]["start"].as_u64().unwrap() as usize;
        let end = self.source_handle["span"]["end"].as_u64().unwrap() as usize;
        let moved = self.source_handle["text"].as_str().unwrap();
        let payload = format!("{prefix}{moved}{suffix}");
        let mut source = self.source_text.clone();
        source.replace_range(start..end, "");
        if self.cross_file {
            let mut destination = self.destination_text.clone();
            destination.insert_str(offset, &payload);
            (source, destination)
        } else {
            let offset = if offset >= end {
                offset - (end - start)
            } else {
                offset
            };
            source.insert_str(offset, &payload);
            (source.clone(), source)
        }
    }

    fn expected(&self, prefix: &str, suffix: &str) -> (String, String) {
        let boundary = if self.before { "start" } else { "end" };
        self.expected_at(
            self.destination_handle["span"][boundary].as_u64().unwrap() as usize,
            prefix,
            suffix,
        )
    }

    fn apply_and_compare(&self, plan: &Value, expected: (String, String)) {
        let before = self.files();
        successful_json(common::run_identedit_with_stdin(
            &["apply", "--dry-run"],
            &plan.to_string(),
        ));
        assert_eq!(self.files(), before, "dry-run must not write");
        successful_json(common::run_identedit_with_stdin(
            &["apply"],
            &plan.to_string(),
        ));
        assert_eq!(self.files(), expected);
    }
}

#[test]
fn all_four_moves_insert_literal_affixes_exactly_once() {
    for cross_file in [false, true] {
        for before in [false, true] {
            let fixture = RawMoveFixture::new(cross_file, before, "js");
            let prefix = "\r\n// 앞\t prefix \r\n";
            let suffix = "\r\n// 뒤\t suffix \r\n";
            let plan = fixture.compile(&fixture.request(prefix, suffix));
            if cross_file {
                let destination = file_operation(&plan, &fixture.destination);
                assert_eq!(
                    destination["preview"]["new_text"],
                    format!(
                        "{prefix}{}{suffix}",
                        fixture.source_handle["text"].as_str().unwrap()
                    )
                );
                assert_eq!(
                    destination["op"]["new_text"],
                    destination["preview"]["new_text"]
                );
            } else {
                let operation = &file_operation(&plan, &fixture.source)["op"];
                assert_eq!(operation["prefix"], prefix);
                assert_eq!(operation["suffix"], suffix);
            }
            fixture.apply_and_compare(&plan, fixture.expected(prefix, suffix));
        }
    }
}

#[test]
fn python_callers_can_supply_the_next_destination_line_indentation() {
    for cross_file in [false, true] {
        for before in [false, true] {
            let fixture = RawMoveFixture::new(cross_file, before, "py");
            let (prefix, suffix) = if before {
                ("", "\n\n    ")
            } else {
                ("\n\n    ", "")
            };
            let mut request = fixture.request(prefix, suffix);
            if before && !cross_file {
                let mut source = node_target(&fixture.source_handle);
                let mut destination = node_target(&fixture.destination_handle);
                source.as_object_mut().unwrap().remove("type");
                destination.as_object_mut().unwrap().remove("type");
                request["handle_table"] = json!({"source":source,"destination":destination});
                request["operations"][0]["target"] = json!({"type":"handle_ref","ref":"source"});
                request["operations"][0]["op"]["destination"] =
                    json!({"type":"handle_ref","ref":"destination"});
            }
            let plan = fixture.compile(&request);
            fixture.apply_and_compare(&plan, fixture.expected(prefix, suffix));
            let name = fixture.source_handle["name"].as_str().unwrap();
            let container = if cross_file { "Destination" } else { "Source" };
            named_handle(&fixture.destination, &format!("{container}.{name}"));
        }
    }
}

#[test]
fn omitted_and_empty_affixes_preserve_the_complete_legacy_plan_and_effects() {
    for cross_file in [false, true] {
        for before in [false, true] {
            let fixture = RawMoveFixture::new(cross_file, before, "js");
            let empty = fixture.request("", "");
            let mut omitted = empty.clone();
            let operation = omitted["operations"][0]["op"].as_object_mut().unwrap();
            operation.remove("prefix");
            operation.remove("suffix");
            let legacy_plan = fixture.compile(&omitted);
            assert_eq!(fixture.compile(&empty), legacy_plan);
            if !cross_file {
                assert_eq!(
                    file_operation(&legacy_plan, &fixture.source)["op"],
                    json!({
                        "type": if before { "move_before" } else { "move_after" },
                        "destination": node_target(&fixture.destination_handle),
                    })
                );
            }
            fixture.apply_and_compare(&legacy_plan, fixture.expected("", ""));
        }
    }
}

#[test]
fn either_affix_can_be_omitted_independently() {
    for cross_file in [false, true] {
        for omitted in ["prefix", "suffix"] {
            let fixture = RawMoveFixture::new(cross_file, false, "js");
            let mut request = fixture.request("\n// prefix\n", "\n// suffix\n");
            request["operations"][0]["op"]
                .as_object_mut()
                .unwrap()
                .remove(omitted);
            let prefix = if omitted == "prefix" {
                ""
            } else {
                "\n// prefix\n"
            };
            let suffix = if omitted == "suffix" {
                ""
            } else {
                "\n// suffix\n"
            };
            let plan = fixture.compile(&request);
            fixture.apply_and_compare(&plan, fixture.expected(prefix, suffix));
        }
    }
}

#[test]
fn existing_file_and_line_destination_points_accept_affixes() {
    for kind in ["file_start", "file_end", "line"] {
        for before in [false, true] {
            let fixture = RawMoveFixture::new(false, true, "js");
            let prefix = "\n// prefix\n";
            let suffix = "\n// suffix\n";
            let mut request = fixture.request(prefix, suffix);
            request["operations"][0]["op"]["type"] =
                json!(if before { "move_before" } else { "move_after" });
            let (destination, offset) = match kind {
                "file_start" => (
                    json!({"type":kind,"expected_file_hash":common::hash_text(&fixture.source_text)}),
                    0,
                ),
                "file_end" => (
                    json!({"type":kind,"expected_file_hash":common::hash_text(&fixture.source_text)}),
                    fixture.source_text.len(),
                ),
                _ => {
                    let lines = successful_json(common::run_identedit(&[
                        "read",
                        fixture.source.to_str().unwrap(),
                        "--mode",
                        "line",
                        "--json",
                    ]));
                    let offset = if before {
                        0
                    } else {
                        fixture.source_text.find('\n').unwrap() + 1
                    };
                    (
                        json!({"type":"line","anchor":lines["handles"][0]["anchor"]}),
                        offset,
                    )
                }
            };
            request["operations"][0]["op"]["destination"] = destination;
            let plan = fixture.compile(&request);
            fixture.apply_and_compare(&plan, fixture.expected_at(offset, prefix, suffix));
        }
    }
}

#[test]
fn source_and_destination_hash_guards_are_not_weakened() {
    for cross_file in [false, true] {
        for corrupt_source in [false, true] {
            let fixture = RawMoveFixture::new(cross_file, false, "js");
            let mut plan = fixture.compile(&fixture.request("\n", "\n"));
            let target = if corrupt_source {
                &mut plan["files"][0]["operations"][0]["target"]
            } else if cross_file {
                &mut plan["files"][1]["operations"][0]["target"]
            } else {
                &mut plan["files"][0]["operations"][0]["op"]["destination"]
            };
            target["expected_old_hash"] = json!(common::hash_text("wrong old bytes"));
            let snapshots = fixture.files();
            for args in [vec!["apply", "--dry-run"], vec!["apply"]] {
                let rejected =
                    rejected_json(common::run_identedit_with_stdin(&args, &plan.to_string()));
                assert_eq!(rejected["error"]["type"], "precondition_failed");
                assert_eq!(fixture.files(), snapshots);
            }
        }
    }
}

#[test]
fn invalid_composed_results_reject_before_any_file_is_written() {
    for cross_file in [false, true] {
        let fixture = RawMoveFixture::new(cross_file, false, "js");
        let plan = fixture.compile(&fixture.request("\nfunction (\n", "\n"));
        let snapshots = fixture.files();
        for args in [vec!["apply", "--dry-run"], vec!["apply"]] {
            let response =
                rejected_json(common::run_identedit_with_stdin(&args, &plan.to_string()));
            assert_eq!(response["error"]["type"], "parse_failure");
            assert_eq!(response["error"]["stage"], "proposed_result");
            assert_eq!(fixture.files(), snapshots);
        }
    }
}

#[test]
fn raw_python_moves_still_do_not_infer_separators() {
    let fixture = RawMoveFixture::new(false, false, "py");
    let plan = fixture.compile(&fixture.request("", ""));
    let snapshots = fixture.files();
    let response = rejected_json(common::run_identedit_with_stdin(
        &["apply"],
        &plan.to_string(),
    ));
    assert_eq!(response["error"]["type"], "parse_failure");
    assert_eq!(response["error"]["stage"], "proposed_result");
    assert_eq!(fixture.files(), snapshots);
}

#[test]
fn wrapped_apply_preserves_the_composed_move() {
    let fixture = RawMoveFixture::new(false, false, "js");
    let plan = fixture.compile(&fixture.request("\n// prefix\n", "\n// suffix\n"));
    successful_json(common::run_identedit_with_stdin(
        &["apply", "--json"],
        &json!({"command":"apply","changeset":plan}).to_string(),
    ));
    assert_eq!(
        fixture.files(),
        fixture.expected("\n// prefix\n", "\n// suffix\n")
    );
}

#[test]
fn a_move_can_coexist_with_an_edit_inside_its_destination_node() {
    let fixture = RawMoveFixture::new(false, false, "js");
    let prefix = "\n// prefix\n";
    let suffix = "\n// suffix\n";
    let inventory = successful_json(common::run_identedit(&[
        "read",
        fixture.source.to_str().unwrap(),
        "--json",
        "--verbose",
    ]));
    let number = inventory["handles"]
        .as_array()
        .unwrap()
        .iter()
        .find(|handle| handle["kind"] == "number" && handle["text"] == "2")
        .unwrap();
    let mut request = fixture.request(prefix, suffix);
    request["operations"].as_array_mut().unwrap().push(json!({
        "target": node_target(number),
        "op": {"type":"replace","new_text":"20"},
    }));
    let plan = fixture.compile(&request);
    let (source, _) = fixture.expected(prefix, suffix);
    let expected = source.replace("return 2;", "return 20;");
    fixture.apply_and_compare(&plan, (expected.clone(), expected));
}

#[test]
fn affixes_do_not_expand_cross_file_destination_kinds() {
    let fixture = RawMoveFixture::new(true, false, "js");
    let mut request = fixture.request("\n", "\n");
    request["operations"][0]["op"]["destination"] = json!({
        "type":"file_end",
        "expected_file_hash": common::hash_text(&fixture.destination_text),
    });
    let response = rejected_json(common::run_identedit_with_stdin(
        &["edit", "--json"],
        &request.to_string(),
    ));
    assert_eq!(response["error"]["type"], "invalid_request");
    assert_eq!(
        fixture.files(),
        (
            fixture.source_text.clone(),
            fixture.destination_text.clone()
        )
    );
}

#[test]
fn malformed_and_unrelated_affixes_are_rejected_without_writes() {
    for cross_file in [false, true] {
        for field in ["prefix", "suffix"] {
            for invalid in [Value::Null, json!(7), json!(["text"])] {
                let fixture = RawMoveFixture::new(cross_file, false, "js");
                let mut request = fixture.request("\n", "\n");
                request["operations"][0]["op"][field] = invalid;
                let snapshots = fixture.files();
                let response = rejected_json(common::run_identedit_with_stdin(
                    &["edit", "--json"],
                    &request.to_string(),
                ));
                assert_eq!(response["error"]["type"], "invalid_request");
                assert_eq!(fixture.files(), snapshots);
            }
        }
    }
    let fixture = RawMoveFixture::new(false, false, "js");
    let mut request = fixture.request("\n", "\n");
    request["operations"][0]["op"] =
        json!({"type":"replace","new_text":"ignored","prefix":"unexpected"});
    let response = rejected_json(common::run_identedit_with_stdin(
        &["edit", "--json"],
        &request.to_string(),
    ));
    assert_eq!(response["error"]["type"], "invalid_request");
    assert_eq!(
        fixture.files(),
        (
            fixture.source_text.clone(),
            fixture.destination_text.clone()
        )
    );
}

#[test]
fn direct_apply_does_not_silently_discard_invalid_move_fields() {
    let fixture = RawMoveFixture::new(false, false, "js");
    let plan = fixture.compile(&fixture.request("\n", "\n"));
    for (field, value) in [
        ("prefix", Value::Null),
        ("suffix", json!(42)),
        ("prefx", json!("typo")),
    ] {
        let mut invalid = plan.clone();
        invalid["files"][0]["operations"][0]["op"][field] = value;
        let response = rejected_json(common::run_identedit_with_stdin(
            &["apply"],
            &invalid.to_string(),
        ));
        assert_eq!(response["error"]["type"], "invalid_request");
        assert_eq!(
            fixture.files(),
            (
                fixture.source_text.clone(),
                fixture.destination_text.clone()
            )
        );
    }
}

#[test]
fn affixes_do_not_relax_existing_overlap_rules() {
    let fixture = RawMoveFixture::new(false, false, "js");
    let mut request = fixture.request("\n", "\n");
    request["operations"].as_array_mut().unwrap().push(json!({
        "target":node_target(&fixture.destination_handle),
        "op":{"type":"insert_after","new_text":"\n"},
    }));
    let response = rejected_json(common::run_identedit_with_stdin(
        &["edit", "--json"],
        &request.to_string(),
    ));
    assert_eq!(response["error"]["type"], "invalid_request");
    assert_eq!(
        fixture.files(),
        (
            fixture.source_text.clone(),
            fixture.destination_text.clone()
        )
    );
}
