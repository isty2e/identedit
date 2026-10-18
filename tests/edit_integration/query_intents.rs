use super::*;

fn guarded(handle: &Value) -> Value {
    json!({
        "type": "node",
        "identity": handle["identity"],
        "kind": handle["kind"],
        "span_hint": handle["span"],
        "expected_old_hash": handle["expected_old_hash"]
    })
}

fn compile(request: &Value, verbose: bool) -> Value {
    let arguments = if verbose {
        vec!["edit", "--json", "--verbose"]
    } else {
        vec!["edit", "--json"]
    };
    let output = run_identedit_with_stdin(&arguments, &request.to_string());
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn reject(request: &Value, expected_type: &str, pointers: &[&str]) -> Value {
    let output = run_identedit_with_stdin(&["edit", "--json"], &request.to_string());
    assert!(!output.status.success());
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["error"]["type"], expected_type, "{response}");
    assert_eq!(response["error"]["request_pointers"], json!(pointers));
    response
}

#[test]
fn queries_and_regex_produce_equivalent_guarded_plans_in_both_envelopes() {
    let root = tempdir().unwrap();
    let file = root.path().join("example.py");
    let before = "\u{feff}# λ\r\nclass Service:\r\n    def run(self, timeout):\r\n        # timeout\r\n        return timeout + 1\r\n\r\ndef load_config():\r\n    return 2\r\n";
    fs::write(&file, before).unwrap();

    for (name, query, op) in [
        (
            "run",
            json!({"type":"symbol","name":"Service.run"}),
            json!({"type":"scoped_regex","pattern":"\\b(timeout)\\b","replacement":"${1}_ms"}),
        ),
        (
            "load_config",
            json!({"type":"selector","kind":"function_definition","name":"load_*"}),
            json!({"type":"replace","new_text":"def load_config():\r\n    return 3"}),
        ),
    ] {
        let handle = select_first_handle(&file, "function_definition", Some(name));
        for verbose in [false, true] {
            for batch in [false, true] {
                let request = |target: Value| {
                    let entry = json!({"file":file,"operations":[{"target":target,"op":op}]});
                    if batch {
                        json!({"command":"edit","files":[entry]})
                    } else {
                        json!({"command":"edit","file":file,"operations":entry["operations"]})
                    }
                };
                let expected = compile(&request(guarded(&handle)), verbose);
                let actual = compile(&request(query.clone()), verbose);
                assert_eq!(actual, expected);
                assert_eq!(fs::read_to_string(&file).unwrap(), before);
                let mut apply = actual;
                apply["transaction"] = json!({"mode":"all_or_nothing"});
                let output = run_identedit_with_stdin(&["apply", "--dry-run"], &apply.to_string());
                assert!(
                    output.status.success(),
                    "{}",
                    String::from_utf8_lossy(&output.stdout)
                );
                assert_eq!(fs::read_to_string(&file).unwrap(), before);
            }
        }
    }
}

#[test]
fn ambiguity_keeps_full_counts_bounded_details_and_original_input_location() {
    let root = tempdir().unwrap();
    let file = root.path().join("duplicates.py");
    let before = "def target():\n    pass\n\n".repeat(20);
    fs::write(&file, &before).unwrap();
    let request = json!({"command":"edit","files":[
        {"file":file,"operations":[]},
        {"file":file,"operations":[{"target":{"type":"symbol","name":"target"},"op":{"type":"delete"}}]}
    ]});
    let response = reject(&request, "ambiguous_target", &["/files/1/operations/0"]);
    assert_eq!(
        response["error"]["candidates"].as_array().unwrap().len(),
        16
    );
    assert_eq!(
        response["error"]["candidate_summary"],
        json!({"total":20,"omitted":4})
    );
    assert_eq!(fs::read_to_string(&file).unwrap(), before);
}

#[test]
fn invalid_queries_and_recipes_fail_with_typed_errors_without_writes() {
    let root = tempdir().unwrap();
    let file = root.path().join("example.py");
    let before = "def target():\n    return 1\n";
    fs::write(&file, before).unwrap();
    for (target, op, error) in [
        (
            json!({"type":"symbol","name":"missing"}),
            json!({"type":"delete"}),
            "target_missing",
        ),
        (
            json!({"type":"selector","kind":"function_definition","name":"["}),
            json!({"type":"delete"}),
            "invalid_selector",
        ),
        (
            json!({"type":"symbol","name":"target"}),
            json!({"type":"scoped_regex","pattern":"[","replacement":"x"}),
            "invalid_request",
        ),
        (
            json!({"type":"symbol","name":"target"}),
            json!({"type":"scoped_regex","pattern":"absent","replacement":"x"}),
            "invalid_request",
        ),
        (
            json!({"type":"symbol","name":"target","extra":true}),
            json!({"type":"delete"}),
            "invalid_request",
        ),
    ] {
        reject(
            &json!({"command":"edit","file":file,"operations":[{"target":target,"op":op}]}),
            error,
            &["/operations/0"],
        );
        assert_eq!(fs::read_to_string(&file).unwrap(), before);
    }

    let invalid_source = "def broken(:\n";
    fs::write(&file, invalid_source).unwrap();
    let response = reject(
        &json!({"command":"edit","file":file,"operations":[{
            "target":{"type":"symbol","name":"broken"},"op":{"type":"delete"}
        }]}),
        "parse_failure",
        &["/operations/0"],
    );
    assert_eq!(response["error"]["stage"], "source");
    assert_eq!(fs::read_to_string(&file).unwrap(), invalid_source);
}

#[test]
fn query_guard_mixing_including_null_is_rejected_without_changing_legacy_nulls() {
    let root = tempdir().unwrap();
    let file = root.path().join("example.py");
    let before = "def target():\n    return 1\n";
    fs::write(&file, before).unwrap();
    for key in [
        "identity",
        "kind",
        "span_hint",
        "expected_old_hash",
        "expected_file_hash",
    ] {
        let mut target = json!({"type":"symbol","name":"target"});
        target[key] = Value::Null;
        reject(
            &json!({"command":"edit","file":file,"operations":[{
                "target":target,"op":{"type":"delete"}
            }]}),
            "invalid_request",
            &["/operations/0"],
        );

        let mut operation =
            json!({"target":{"type":"symbol","name":"target"},"op":{"type":"delete"}});
        operation[key] = Value::Null;
        reject(
            &json!({"command":"edit","file":file,"operations":[operation]}),
            "invalid_request",
            &["/operations/0"],
        );
    }

    let handle = select_first_handle(&file, "function_definition", Some("target"));
    let legacy = json!({"command":"edit","file":file,"operations":[{
        "target":guarded(&handle),"identity":null,"kind":null,"span_hint":null,"expected_old_hash":null,
        "op":{"type":"replace","new_text":handle["text"]}
    }]});
    compile(&legacy, false);
    assert_eq!(fs::read_to_string(&file).unwrap(), before);
}

#[test]
fn guarded_regex_never_refreshes_a_stale_node_or_handle_reference() {
    let root = tempdir().unwrap();
    let file = root.path().join("example.py");
    fs::write(&file, "def target():\n    return 1\n").unwrap();
    let handle = select_first_handle(&file, "function_definition", Some("target"));
    let before = "def target():\n    return 2\n";
    fs::write(&file, before).unwrap();
    let mut entry = guarded(&handle);
    entry.as_object_mut().unwrap().remove("type");

    for target in [guarded(&handle), json!({"type":"handle_ref","ref":"old"})] {
        reject(
            &json!({"command":"edit","file":file,"handle_table":{"old":entry},"operations":[{
                "target":target,"op":{"type":"scoped_regex","pattern":"return","replacement":"yield"}
            }]}),
            "precondition_failed",
            &["/operations/0"],
        );
        assert_eq!(fs::read_to_string(&file).unwrap(), before);
    }
}

#[test]
fn apply_uses_compiled_guards_not_the_original_qualified_query() {
    let root = tempdir().unwrap();
    let file = root.path().join("example.py");
    let before = "class Service:\n    def run(self):\n        return 1\n";
    fs::write(&file, before).unwrap();
    let changeset = compile(
        &json!({"command":"edit","file":file,"operations":[{
            "target":{"type":"symbol","name":"Service.run"},
            "op":{"type":"scoped_regex","pattern":"return 1","replacement":"return 2"}
        }]}),
        false,
    );
    fs::write(&file, before.replace("Service", "Renamed")).unwrap();
    let output = run_identedit_with_stdin(&["apply"], &changeset.to_string());
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        before
            .replace("Service", "Renamed")
            .replace("return 1", "return 2")
    );
}

#[test]
fn conflict_pointers_use_numeric_original_indexes_and_repeated_file_entries() {
    let root = tempdir().unwrap();
    let file = root.path().join("example.py");
    let before = (0..11)
        .map(|i| format!("def f{i}():\n    return {i}\n\n"))
        .collect::<String>();
    fs::write(&file, &before).unwrap();
    let operations = (0..11).map(|i| {
        if i == 10 { json!({"target":{"type":"symbol","name":"f2"},"op":{"type":"insert_before","new_text":"# before\n"}}) }
        else { json!({"target":{"type":"symbol","name":format!("f{i}")},"op":{"type":"replace","new_text":format!("def f{i}():\n    return {i}")}}) }
    }).collect::<Vec<_>>();
    reject(
        &json!({"command":"edit","file":file,"operations":operations}),
        "invalid_request",
        &["/operations/2", "/operations/10"],
    );

    reject(
        &json!({"command":"edit","files":[
            {"file":file,"operations":[{"target":{"type":"symbol","name":"f2"},"op":{"type":"replace","new_text":"def f2():\n    return 20"}}]},
            {"file":file,"operations":[]},
            {"file":file,"operations":[{"target":{"type":"selector","kind":"function_definition","name":"f2"},"op":{"type":"insert_before","new_text":"# before\n"}}]}
        ]}),
        "invalid_request",
        &["/files/0/operations/0", "/files/2/operations/0"],
    );
    assert_eq!(fs::read_to_string(&file).unwrap(), before);
}

#[test]
fn cross_file_move_expansion_retains_its_origin_for_destination_conflicts() {
    let root = tempdir().unwrap();
    let source = root.path().join("source.py");
    let destination = root.path().join("destination.py");
    let source_before = "def source():\n    return 1\n";
    let destination_before = "def destination():\n    return 2\n";
    fs::write(&source, source_before).unwrap();
    fs::write(&destination, destination_before).unwrap();
    reject(
        &json!({"command":"edit","files":[
            {"file":source,"operations":[{
                "target":{"type":"symbol","name":"source"},
                "op":{"type":"move_to_after","destination_file":destination,
                    "destination":{"type":"selector","kind":"function_definition","name":"destination"},"prefix":"\n\n"}
            }]},
            {"file":destination,"operations":[]},
            {"file":destination,"operations":[{
                "target":{"type":"symbol","name":"destination"},
                "op":{"type":"replace","new_text":"def destination():\n    return 3"}
            }]}
        ]}),
        "invalid_request",
        &["/files/0/operations/0", "/files/2/operations/0"],
    );
    assert_eq!(fs::read_to_string(&source).unwrap(), source_before);
    assert_eq!(
        fs::read_to_string(&destination).unwrap(),
        destination_before
    );
}

#[test]
fn every_structural_move_accepts_query_endpoints_without_writing_during_planning() {
    let root = tempdir().unwrap();
    let source = root.path().join("source.py");
    let destination = root.path().join("destination.py");
    let source_before = "def first():\n    return 1\n\ndef second():\n    return 2\n";
    let destination_before = "def destination():\n    return 3\n";
    for verb in [
        "move_before",
        "move_after",
        "move_to_before",
        "move_to_after",
    ] {
        fs::write(&source, source_before).unwrap();
        fs::write(&destination, destination_before).unwrap();
        let cross_file = verb.starts_with("move_to_");
        let name = if cross_file { "destination" } else { "second" };
        let mut op = json!({"type":verb,"destination":{"type":"selector","kind":"function_definition","name":name},"prefix":"\n\n","suffix":"\n\n"});
        if cross_file {
            op["destination_file"] = json!(destination);
        }
        let plan = compile(
            &json!({"command":"edit","file":source,"operations":[{
                "target":{"type":"symbol","name":"first"},"op":op
            }]}),
            false,
        );
        assert_eq!(fs::read_to_string(&source).unwrap(), source_before);
        assert_eq!(
            fs::read_to_string(&destination).unwrap(),
            destination_before
        );
        let output = run_identedit_with_stdin(&["apply"], &plan.to_string());
        assert!(
            output.status.success(),
            "{verb}: {}",
            String::from_utf8_lossy(&output.stdout)
        );
        let resulting_file = if cross_file { &destination } else { &source };
        let result = fs::read_to_string(resulting_file).unwrap();
        assert_eq!(result.matches("def first():").count(), 1);
        assert!(result.contains("    return 1"));
        if cross_file {
            assert!(
                !fs::read_to_string(&source)
                    .unwrap()
                    .contains("def first():")
            );
        }
    }
}

#[test]
fn regex_noop_and_zero_width_match_are_preserved_but_non_node_recipes_are_rejected() {
    let root = tempdir().unwrap();
    let file = root.path().join("example.py");
    let before = "def target():\n    return 1\n";
    fs::write(&file, before).unwrap();
    for (pattern, replacement) in [("return", "return"), ("^", "# before\n")] {
        let plan = compile(
            &json!({"command":"edit","file":file,"operations":[{
                "target":{"type":"symbol","name":"target"},
                "op":{"type":"scoped_regex","pattern":pattern,"replacement":replacement}
            }]}),
            false,
        );
        let output = run_identedit_with_stdin(&["apply", "--dry-run"], &plan.to_string());
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        assert_eq!(plan["files"][0]["operations"].as_array().unwrap().len(), 1);
        assert_eq!(fs::read_to_string(&file).unwrap(), before);
    }

    reject(
        &json!({"command":"edit","file":file,"operations":[{
            "target":{"type":"file_end","expected_file_hash":crate::common::hash_text(before)},
            "op":{"type":"scoped_regex","pattern":"x","replacement":"y"}
        }]}),
        "invalid_request",
        &["/operations/0"],
    );

    let patch = json!({"command":"patch","file":file,"target":{"type":"symbol","name":"target"},"op":{"type":"delete"}});
    let output = run_identedit_with_stdin(&["patch", "--json"], &patch.to_string());
    assert!(!output.status.success());
    assert_eq!(fs::read_to_string(&file).unwrap(), before);
}

#[test]
fn same_anchor_and_nested_conflicts_report_the_actual_witness_and_self_move_deduplicates() {
    let root = tempdir().unwrap();
    let file = root.path().join("example.py");
    let before = "class Service:\n    def run(self):\n        return 1\n";
    fs::write(&file, before).unwrap();
    let target = json!({"type":"symbol","name":"Service.run"});

    reject(
        &json!({"command":"edit","file":file,"operations":[
            {"target":target,"op":{"type":"replace","new_text":"def run(self):\n        return 2"}},
            {"target":target,"op":{"type":"replace","new_text":"def run(self):\n        return 3"}},
            {"target":target,"op":{"type":"insert_after","new_text":"\n"}}
        ]}),
        "invalid_request",
        &["/operations/0", "/operations/2"],
    );

    reject(
        &json!({"command":"edit","file":file,"operations":[
            {"target":target,"op":{"type":"delete"}},
            {"target":{"type":"symbol","name":"Service"},"op":{"type":"delete"}}
        ]}),
        "invalid_request",
        &["/operations/0", "/operations/1"],
    );

    reject(
        &json!({"command":"edit","file":file,"operations":[
            {"target":target,"op":{"type":"move_before","destination":target}}
        ]}),
        "invalid_request",
        &["/operations/0"],
    );
    assert_eq!(fs::read_to_string(&file).unwrap(), before);
}

#[test]
fn repeated_file_entries_keep_local_ref_tables_and_can_mix_queries_with_guards() {
    let root = tempdir().unwrap();
    let file = root.path().join("example.py");
    let before =
        "def first():\n    return 1\n\ndef second():\n    return 2\n\ndef third():\n    return 3\n";
    fs::write(&file, before).unwrap();
    let entry = |name: &str, replacement: &str| {
        let mut handle = guarded(&select_first_handle(
            &file,
            "function_definition",
            Some(name),
        ));
        handle.as_object_mut().unwrap().remove("type");
        json!({"file":file,"handle_table":{"same":handle},"operations":[{
            "target":{"type":"handle_ref","ref":"same"},
            "op":{"type":"scoped_regex","pattern":"return [0-9]","replacement":replacement}
        }]})
    };
    let plan = compile(
        &json!({"command":"edit","files":[
            entry("first", "return 10"),
            {"file":file,"operations":[{"target":{"type":"symbol","name":"third"},"op":{"type":"scoped_regex","pattern":"return 3","replacement":"return 30"}}]},
            entry("second", "return 20")
        ]}),
        false,
    );
    assert_eq!(plan["files"].as_array().unwrap().len(), 1);
    assert_eq!(fs::read_to_string(&file).unwrap(), before);
    let output = run_identedit_with_stdin(&["apply"], &plan.to_string());
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        before
            .replace("return 1", "return 10")
            .replace("return 2", "return 20")
            .replace("return 3", "return 30")
    );
}

#[test]
fn a_later_query_cannot_target_an_earlier_planned_rename() {
    let root = tempdir().unwrap();
    let file = root.path().join("example.py");
    let before = "def old():\n    return 1\n";
    fs::write(&file, before).unwrap();
    reject(
        &json!({"command":"edit","file":file,"operations":[
            {"target":{"type":"symbol","name":"old"},"op":{"type":"replace","new_text":"def new():\n    return 1"}},
            {"target":{"type":"symbol","name":"new"},"op":{"type":"delete"}}
        ]}),
        "target_missing",
        &["/operations/1"],
    );
    assert_eq!(fs::read_to_string(&file).unwrap(), before);
}

#[test]
fn per_operation_decoding_preserves_duplicate_field_rejection() {
    let root = tempdir().unwrap();
    let file = root.path().join("example.py");
    let before = "def target():\n    return 1\n";
    fs::write(&file, before).unwrap();
    let target = guarded(&select_first_handle(
        &file,
        "function_definition",
        Some("target"),
    ));
    let deletion = json!({"type":"delete"});
    let operations = [
        format!("{{\"target\":{target},\"op\":{deletion},\"op\":{deletion}}}"),
        format!(
            "{{\"target\":{target},\"op\":{{\"type\":\"replace\",\"new_text\":\"first\",\"new_text\":\"second\"}}}}"
        ),
    ];
    for operation in operations {
        let request = format!(
            "{{\"command\":\"edit\",\"file\":{},\"operations\":[{operation}]}}",
            json!(file)
        );
        let output = run_identedit_with_stdin(&["edit", "--json"], &request);
        assert!(
            !output.status.success(),
            "duplicate fields must not be silently discarded"
        );
        let response: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(response["error"]["type"], "invalid_request");
        assert_eq!(
            response["error"]["request_pointers"],
            json!(["/operations/0"])
        );
        assert_eq!(fs::read_to_string(&file).unwrap(), before);
    }
}
