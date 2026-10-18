use super::*;

fn config(path: &str, op: Value) -> Value {
    json!({"target":{"type":"config_path","path":path},"op":op})
}

fn set(path: &str, value: &str) -> Value {
    config(path, json!({"type":"set","new_text":value}))
}

fn create(path: &str, value: &str) -> Value {
    config(
        path,
        json!({"type":"set","new_text":value,"create_missing":true}),
    )
}

fn successful(output: Output) -> Value {
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn compile(request: &Value) -> Value {
    successful(run_identedit_with_stdin(
        &["edit", "--json"],
        &request.to_string(),
    ))
}

fn failure(output: Output, kind: &str, pointers: &[&str]) -> Value {
    assert!(!output.status.success());
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["error"]["type"], kind, "{response}");
    assert_eq!(
        response["error"]["request_pointers"],
        json!(pointers),
        "{response}"
    );
    response
}

fn reject(request: &Value, kind: &str, pointers: &[&str]) -> Value {
    failure(
        run_identedit_with_stdin(&["edit", "--json"], &request.to_string()),
        kind,
        pointers,
    )
}

fn apply(plan: &Value, options: &[&str]) -> Output {
    let mut args = vec!["apply"];
    args.extend_from_slice(options);
    run_identedit_with_stdin(&args, &plan.to_string())
}

fn replay(file: &Path, before: &str, after: &str, plan: &Value) {
    assert_eq!(fs::read(file).unwrap(), before.as_bytes());
    assert_eq!(
        plan["files"][0]["expected_file_hash"],
        crate::common::hash_text(before)
    );
    let preview = successful(apply(plan, &["--dry-run"]));
    assert_eq!(preview["transaction"]["status"], "dry_run");
    assert_eq!(fs::read(file).unwrap(), before.as_bytes());
    successful(apply(plan, &[]));
    assert_eq!(fs::read(file).unwrap(), after.as_bytes());
}

fn guarded(handle: &Value) -> Value {
    json!({"type":"node","identity":handle["identity"],"kind":handle["kind"],
        "span_hint":handle["span"],"expected_old_hash":handle["expected_old_hash"]})
}

#[test]
fn sibling_settings_compose_without_changing_other_bytes_in_either_envelope() {
    let directory = tempdir().unwrap();
    for (extension, before, after) in [
        (
            "json",
            "\u{feff}{\"service\": {\"retries\": 1, \"timeout\": 2}, \"note\": \"λ\"}\r\n",
            "\u{feff}{\"service\": {\"retries\": 3, \"timeout\": 4}, \"note\": \"λ\"}\r\n",
        ),
        (
            "toml",
            "\u{feff}# λ\r\n[service]\r\nretries = 1 # first\r\ntimeout = 2\r\n",
            "\u{feff}# λ\r\n[service]\r\nretries = 3 # first\r\ntimeout = 4\r\n",
        ),
        (
            "yaml",
            "\u{feff}# λ\r\nservice:\r\n  retries: 1 # first\r\n  timeout: 2\r\n",
            "\u{feff}# λ\r\nservice:\r\n  retries: 3 # first\r\n  timeout: 4\r\n",
        ),
    ] {
        let file = directory.path().join(format!("settings.{extension}"));
        for batch in [false, true] {
            fs::write(&file, before).unwrap();
            let operations = json!([set("service.retries", "3"), set("service.timeout", "4")]);
            let request = if batch {
                json!({"command":"edit","files":[{"file":file,"operations":operations}]})
            } else {
                json!({"command":"edit","file":file,"operations":operations})
            };
            let plan = compile(&request);
            assert_eq!(plan["files"][0]["operations"].as_array().unwrap().len(), 1);
            replay(&file, before, after, &plan);
        }
    }
}

#[test]
fn nested_guard_scopes_keep_the_outer_contributors_actual_guard_in_both_orders() {
    let directory = tempdir().unwrap();
    for (extension, before, after, outer_text) in [
        (
            "json",
            "{\"group\":{\"inner\":{\"x\":1},\"y\":2},\"tail\":0}",
            "{\"group\":{\"inner\":{\"x\":10},\"y\":20},\"tail\":0}",
            "{\"inner\":{\"x\":1},\"y\":2}",
        ),
        (
            "toml",
            "group = { inner = { x = 1 }, y = 2 }\ntail = 0\n",
            "group = { inner = { x = 10 }, y = 20 }\ntail = 0\n",
            "{ inner = { x = 1 }, y = 2 }",
        ),
        (
            "yaml",
            "group:\n  inner:\n    x: 1\n  y: 2\ntail: 0\n",
            "group:\n  inner:\n    x: 10\n  y: 20\ntail: 0\n",
            "inner:\n    x: 1\n  y: 2\n",
        ),
    ] {
        let file = directory.path().join(format!("nested.{extension}"));
        for reverse in [false, true] {
            fs::write(&file, before).unwrap();
            let mut operations = vec![set("group.inner.x", "10"), set("group.y", "20")];
            if reverse {
                operations.reverse();
            }
            let plan = compile(&json!({"command":"edit","file":file,"operations":operations}));
            let compiled = plan["files"][0]["operations"].as_array().unwrap();
            assert_eq!(compiled.len(), 1);
            let outer = successful(run_identedit(&[
                "edit",
                file.to_str().unwrap(),
                "--config-path",
                "group.y",
                "--set-value",
                "20",
            ]));
            assert_eq!(
                compiled[0]["target"],
                outer["files"][0]["operations"][0]["target"]
            );
            let span = &compiled[0]["target"]["span_hint"];
            let guarded_text = &before
                [span["start"].as_u64().unwrap() as usize..span["end"].as_u64().unwrap() as usize];
            assert_eq!(guarded_text.trim_end(), outer_text.trim_end());
            replay(&file, before, after, &plan);
        }
    }
}

#[test]
fn disjoint_containers_remain_separate_guarded_operations() {
    let directory = tempdir().unwrap();
    let file = directory.path().join("settings.json");
    let before = "{\"a\":{\"x\":1},\"b\":{\"x\":2}}";
    fs::write(&file, before).unwrap();
    let plan = compile(&json!({"command":"edit","file":file,
        "operations":[set("a.x","10"),set("b.x","20")]}));
    assert_eq!(plan["files"][0]["operations"].as_array().unwrap().len(), 2);
    replay(&file, before, "{\"a\":{\"x\":10},\"b\":{\"x\":20}}", &plan);
}

#[test]
fn equal_span_toml_document_and_table_do_not_change_the_selected_guard_kind() {
    let directory = tempdir().unwrap();
    let file = directory.path().join("settings.toml");
    let before = "[service]\na = 1\nb = 2";
    fs::write(&file, before).unwrap();
    let read = successful(run_identedit(&[
        "read",
        file.to_str().unwrap(),
        "--json",
        "--verbose",
    ]));
    let handles = read["handles"].as_array().unwrap();
    let table = handles.iter().find(|h| h["kind"] == "table").unwrap();
    let document = handles.iter().find(|h| h["kind"] == "document").unwrap();
    assert_eq!(table["span"], document["span"]);
    let plan = compile(&json!({"command":"edit","file":file,
        "operations":[set("service.a","10"),set("service.b","20")]}));
    assert_eq!(plan["files"][0]["operations"][0]["target"], guarded(table));
    replay(&file, before, "[service]\na = 10\nb = 20", &plan);
}

#[test]
fn deletion_does_not_shift_the_indices_of_later_requests() {
    let directory = tempdir().unwrap();
    for (extension, before, after) in [
        (
            "json",
            "{\"items\":[10, 20, 30, 40]}",
            "{\"items\":[20, 300, 40]}",
        ),
        (
            "toml",
            "items = [10, 20, 30, 40]\n",
            "items = [20, 300, 40]\n",
        ),
        (
            "yaml",
            "items: [10, 20, 30, 40]\n",
            "items: [20, 300, 40]\n",
        ),
    ] {
        let file = directory.path().join(format!("settings.{extension}"));
        fs::write(&file, before).unwrap();
        let plan = compile(&json!({"command":"edit","file":file,"operations":[
            config("items[0]",json!({"type":"delete"})),set("items[2]","300")]}));
        replay(&file, before, after, &plan);
    }
}

#[test]
fn block_sequence_edits_keep_original_indices_and_disjoint_line_delimiters() {
    let directory = tempdir().unwrap();
    let file = directory.path().join("settings.yaml");
    let before = "items:\n  - 10\n  - 20\n  - 30\n  - 40\n";
    fs::write(&file, before).unwrap();
    let plan = compile(&json!({"command":"edit","file":file,"operations":[
        config("items[1]",json!({"type":"delete"})),set("items[3]","400")]}));
    replay(&file, before, "items:\n  - 10\n  - 30\n  - 400\n", &plan);
}

#[test]
fn config_paths_are_not_structural_move_destinations() {
    let directory = tempdir().unwrap();
    let file = directory.path().join("settings.json");
    let destination_file = directory.path().join("other.json");
    let before = "{\"inside\":{\"a\":1},\"outside\":0}";
    fs::write(&file, before).unwrap();
    fs::write(&destination_file, before).unwrap();
    let handle = select_first_handle(&file, "number", Some("outside"));
    let config_target = json!({"type":"config_path","path":"inside.a"});
    for operation in [
        json!({"target":guarded(&handle),"op":{"type":"move_before","destination":config_target}}),
        json!({"target":guarded(&handle),"op":{"type":"move_to_after","destination_file":destination_file,"destination":config_target}}),
        json!({"target":config_target,"op":{"type":"move_before","destination":guarded(&handle)}}),
    ] {
        reject(
            &json!({"command":"edit","file":file,"operations":[operation]}),
            "invalid_request",
            &["/operations/0"],
        );
        assert_eq!(fs::read(&file).unwrap(), before.as_bytes());
        assert_eq!(fs::read(&destination_file).unwrap(), before.as_bytes());
    }
}

#[test]
fn append_combines_with_a_disjoint_original_element_write() {
    let directory = tempdir().unwrap();
    for (extension, before, after) in [
        ("json", "{\"items\":[1, 2]}", "{\"items\":[10, 2, 3]}"),
        ("toml", "items = [1, 2]\n", "items = [10, 2, 3]\n"),
        ("yaml", "items: [1, 2]\n", "items: [10, 2, 3]\n"),
        (
            "yml",
            "items:\n  - 1\n  - 2\n",
            "items:\n  - 10\n  - 2\n  - 3",
        ),
    ] {
        let file = directory.path().join(format!("settings.{extension}"));
        fs::write(&file, before).unwrap();
        let plan = compile(&json!({"command":"edit","file":file,"operations":[
            config("items",json!({"type":"append","new_text":"3"})),set("items[0]","10")]}));
        replay(&file, before, after, &plan);
    }
}

#[test]
fn conflicting_writes_keep_noops_delimiters_points_and_opaque_creates() {
    let directory = tempdir().unwrap();
    let file = directory.path().join("settings.json");
    for (before, operations) in [
        ("{\"a\":1}", vec![set("a", "1"), set("[\"a\"]", "1")]),
        (
            "{\"a\":{\"x\":1}}",
            vec![set("a", "{\"x\":1}"), set("a.x", "2")],
        ),
        (
            "{\"items\":[1, 2]}",
            vec![
                config("items", json!({"type":"append","new_text":"3"})),
                config("items", json!({"type":"append","new_text":"4"})),
            ],
        ),
        (
            "{\"items\":[1, 2]}",
            vec![
                config("items[0]", json!({"type":"delete"})),
                config("items[1]", json!({"type":"delete"})),
            ],
        ),
        (
            "{\"items\":[1, 2]}",
            vec![
                set("items[1]", "20"),
                config("items", json!({"type":"append","new_text":"3"})),
            ],
        ),
        ("{\"a\":1}", vec![create("new", "2"), set("a", "3")]),
        ("{\"a\":1}", vec![create("new", "2"), create("other", "3")]),
        ("", vec![create("new", "2"), create("other", "3")]),
    ] {
        fs::write(&file, before).unwrap();
        reject(
            &json!({"command":"edit","file":file,"operations":operations}),
            "invalid_request",
            &["/operations/0", "/operations/1"],
        );
        assert_eq!(fs::read(&file).unwrap(), before.as_bytes());
    }
}

#[test]
fn config_conflict_messages_keep_original_path_spellings() {
    let directory = tempdir().unwrap();
    for (extension, before) in [
        ("json", "{\"a\":1,\"b\":2}"),
        ("toml", "a = 1\nb = 2\n"),
        ("yaml", "a: 1\nb: 2\n"),
    ] {
        let file = directory.path().join(format!("settings.{extension}"));
        fs::write(&file, before).unwrap();
        for paths in [["a", "[\"a\"]"], ["[\"a\"]", "a"]] {
            let response = reject(
                &json!({"command":"edit","file":file,"operations":[
                    set(paths[0],"10"),set("b","20"),set(paths[1],"30")]}),
                "invalid_request",
                &["/operations/0", "/operations/2"],
            );
            let message = response["error"]["message"].as_str().unwrap();
            for path in paths {
                assert!(message.contains(&format!("'{path}'")), "{response}");
            }
            assert_eq!(fs::read(&file).unwrap(), before.as_bytes());
        }
    }
}

#[test]
fn create_missing_on_existing_paths_uses_precise_existing_value_writes() {
    let directory = tempdir().unwrap();
    for (extension, before, after) in [
        ("json", "{\"a\":1,\"b\":2}", "{\"a\":10,\"b\":20}"),
        ("toml", "a = 1\nb = 2\n", "a = 10\nb = 20\n"),
        ("yaml", "a: 1\nb: 2\n", "a: 10\nb: 20\n"),
    ] {
        let file = directory.path().join(format!("settings.{extension}"));
        fs::write(&file, before).unwrap();
        let plan = compile(&json!({"command":"edit","file":file,
            "operations":[create("a","10"),create("b","20")]}));
        replay(&file, before, after, &plan);
    }
}

#[test]
fn separate_yaml_documents_can_compose_their_opaque_root_creates() {
    let directory = tempdir().unwrap();
    let file = directory.path().join("settings.yaml");
    let before = "a: 1\n---\nb: 2\n";
    fs::write(&file, before).unwrap();
    let mut first = create("new", "10");
    first["target"]["document_index"] = json!(0);
    let mut second = create("other", "20");
    second["target"]["document_index"] = json!(1);
    let plan = compile(&json!({"command":"edit","file":file,"operations":[first,second]}));
    assert_eq!(plan["files"][0]["operations"].as_array().unwrap().len(), 2);
    replay(
        &file,
        before,
        "a: 1\nnew: 10\n---\nb: 2\nother: 20\n",
        &plan,
    );
}

#[test]
fn a_created_path_is_not_visible_to_later_requests() {
    let directory = tempdir().unwrap();
    let file = directory.path().join("settings.json");
    let before = "{\"a\":1}";
    fs::write(&file, before).unwrap();
    reject(
        &json!({"command":"edit","file":file,"operations":[
        create("new","2"),set("new","3")]}),
        "invalid_request",
        &["/operations/1"],
    );
    assert_eq!(fs::read(&file).unwrap(), before.as_bytes());
}

#[test]
fn repeated_file_entries_compose_and_preserve_original_error_pointers() {
    let directory = tempdir().unwrap();
    let file = directory.path().join("settings.json");
    let before = "{\"a\":1,\"b\":2}";
    fs::write(&file, before).unwrap();
    let plan = compile(&json!({"command":"edit","files":[
        {"file":file,"operations":[set("a","10")]},
        {"file":file,"operations":[set("b","20")]}]}));
    assert_eq!(plan["files"].as_array().unwrap().len(), 1);
    assert_eq!(plan["files"][0]["operations"].as_array().unwrap().len(), 1);
    let response = reject(
        &json!({"command":"edit","files":[
        {"file":file,"operations":[]},
        {"file":file,"operations":[set("a","10")]},
        {"file":file,"operations":[set("[\"a\"]","20")]}]}),
        "invalid_request",
        &["/files/1/operations/0", "/files/2/operations/0"],
    );
    let message = response["error"]["message"].as_str().unwrap();
    for path in ["a", "[\"a\"]"] {
        assert!(message.contains(&format!("'{path}'")), "{response}");
    }
    replay(&file, before, "{\"a\":10,\"b\":20}", &plan);
}

#[test]
fn config_conflict_pointers_are_sorted_numerically_not_lexically() {
    let directory = tempdir().unwrap();
    let file = directory.path().join("settings.json");
    let source = json!(
        (0..10)
            .map(|i| (format!("k{i}"), json!(i)))
            .collect::<serde_json::Map<_, _>>()
    )
    .to_string();
    fs::write(&file, &source).unwrap();
    let mut operations = (0..10)
        .map(|i| set(&format!("k{i}"), "99"))
        .collect::<Vec<_>>();
    operations.push(set("k2", "99"));
    reject(
        &json!({"command":"edit","file":file,"operations":operations}),
        "invalid_request",
        &["/operations/2", "/operations/10"],
    );
    assert_eq!(fs::read_to_string(&file).unwrap(), source);
}

#[test]
fn raw_conflicts_name_the_outer_guard_contributor_not_the_first_inner_request() {
    let directory = tempdir().unwrap();
    let file = directory.path().join("settings.json");
    let before = "{\"group\":{\"inner\":{\"x\":1},\"middle\":2,\"y\":3},\"tail\":0}";
    fs::write(&file, before).unwrap();
    let handle = select_first_handle(&file, "number", Some("middle"));
    reject(
        &json!({"command":"edit","file":file,"operations":[
        set("group.inner.x","10"),
        {"target":guarded(&handle),"op":{"type":"replace","new_text":"20"}},
        set("group.y","30")]}),
        "invalid_request",
        &["/operations/1", "/operations/2"],
    );
    assert_eq!(fs::read(&file).unwrap(), before.as_bytes());
}

#[test]
fn config_and_disjoint_raw_node_edits_keep_fixed_guards_and_full_file_condition() {
    let directory = tempdir().unwrap();
    let file = directory.path().join("settings.json");
    let before = "{\"inside\":{\"a\":1,\"b\":2},\"outside\":0}";
    fs::write(&file, before).unwrap();
    let handle = select_first_handle(&file, "number", Some("outside"));
    let plan = compile(&json!({"command":"edit","file":file,"operations":[
        set("inside.a","10"),
        {"target":guarded(&handle),"op":{"type":"replace","new_text":"9"}},
        set("inside.b","20")]}));
    let operations = plan["files"][0]["operations"].as_array().unwrap();
    assert_eq!(operations.len(), 2);
    assert_eq!(operations[1]["target"], guarded(&handle));
    replay(
        &file,
        before,
        "{\"inside\":{\"a\":10,\"b\":20},\"outside\":9}",
        &plan,
    );
}

#[test]
fn co_resident_raw_lines_keep_their_existing_syntax_only_boundary() {
    let directory = tempdir().unwrap();
    let file = directory.path().join("settings.yaml");
    let before = "defaults: &base\n  a: 1\n  b: 2\nconsumer: none\n";
    fs::write(&file, before).unwrap();
    let anchor = format!("4:{}", crate::common::compute_line_hash("consumer: none"));
    let plan = compile(&json!({"command":"edit","file":file,"operations":[
        set("defaults.a","10"),
        {"target":{"type":"line","anchor":anchor},"op":{"type":"replace","new_text":"consumer: *base"}},
        set("defaults.b","20")]}));
    assert_eq!(plan["files"][0]["operations"].as_array().unwrap().len(), 2);
    replay(
        &file,
        before,
        "defaults: &base\n  a: 10\n  b: 20\nconsumer: *base\n",
        &plan,
    );
}

#[test]
fn config_normalization_never_refreshes_a_co_resident_copied_node_guard() {
    let directory = tempdir().unwrap();
    let file = directory.path().join("settings.json");
    let original = "{\"inside\":{\"a\":1,\"b\":2},\"outside\":0}";
    fs::write(&file, original).unwrap();
    let handle = select_first_handle(&file, "number", Some("outside"));
    let changed = "{\"inside\":{\"a\":1,\"b\":2},\"outside\":9}";
    fs::write(&file, changed).unwrap();
    reject(
        &json!({"command":"edit","file":file,"operations":[
        set("inside.a","10"),
        {"target":guarded(&handle),"op":{"type":"replace","new_text":"20"}},
        set("inside.b","20")]}),
        "precondition_failed",
        &["/operations/1"],
    );
    assert_eq!(fs::read(&file).unwrap(), changed.as_bytes());
}

#[test]
fn inferred_destination_config_errors_use_the_explicit_config_file_context() {
    let directory = tempdir().unwrap();
    let source = directory.path().join("source.rs");
    let destination = directory.path().join("destination.rs");
    let source_before = "fn moved() {}\nfn stay() {}\n";
    let destination_before = "fn target() {}\n";
    fs::write(&source, source_before).unwrap();
    fs::write(&destination, destination_before).unwrap();
    let moved = select_first_handle(&source, "function_item", Some("moved"));
    let target = select_first_handle(&destination, "function_item", Some("target"));
    reject(
        &json!({"command":"edit","files":[
            {"file":source,"operations":[{"target":guarded(&moved),"op":{
                "type":"move_to_before","destination_file":destination,"destination":guarded(&target)}}]},
            {"file":destination,"operations":[set("unsupported","1")]}
        ]}),
        "invalid_request",
        &["/files/1"],
    );
    assert_eq!(fs::read(&source).unwrap(), source_before.as_bytes());
    assert_eq!(
        fs::read(&destination).unwrap(),
        destination_before.as_bytes()
    );
}

#[test]
fn duplicate_target_discriminators_cannot_fall_back_to_guarded_nodes() {
    let directory = tempdir().unwrap();
    let source = directory.path().join("source.json");
    let destination = directory.path().join("destination.json");
    let before = "{\"value\":1,\"other\":2}";
    fs::write(&source, before).unwrap();
    fs::write(&destination, before).unwrap();
    let first = guarded(&select_first_handle(&source, "number", Some("value")));
    let second = guarded(&select_first_handle(&source, "number", Some("other")));
    let other_file = guarded(&select_first_handle(&destination, "number", Some("other")));

    for first_type in ["config_path", "symbol", "selector"] {
        for escaped_key in [false, true] {
            let key = if escaped_key {
                r#""ty\u0070e""#
            } else {
                r#""type""#
            };
            let duplicate = |node: &Value| {
                let wire = node.to_string();
                format!(
                    "{{{key}:{}, {}}}",
                    json!(first_type),
                    &wire[1..wire.len() - 1]
                )
            };
            let mut operations = vec![format!(
                "{{\"target\":{},\"op\":{{\"type\":\"replace\",\"new_text\":\"10\"}}}}",
                duplicate(&first)
            )];
            for kind in [
                "move_before",
                "move_after",
                "move_to_before",
                "move_to_after",
            ] {
                let (target, extra) = if kind.starts_with("move_to_") {
                    (
                        &other_file,
                        format!(",\"destination_file\":{}", json!(destination)),
                    )
                } else {
                    (&second, String::new())
                };
                operations.push(format!(
                    "{{\"target\":{first},\"op\":{{\"type\":\"{kind}\",\"destination\":{}{extra}}}}}",
                    duplicate(target)
                ));
            }
            for operation in operations {
                let input = format!(
                    "{{\"command\":\"edit\",\"file\":{},\"operations\":[{operation}]}}",
                    json!(source)
                );
                failure(
                    run_identedit_with_stdin(&["edit", "--json"], &input),
                    "invalid_request",
                    &["/operations/0"],
                );
                assert_eq!(fs::read(&source).unwrap(), before.as_bytes());
                assert_eq!(fs::read(&destination).unwrap(), before.as_bytes());
            }
        }
    }
}

#[test]
fn malformed_config_forms_fail_without_writing_any_batch_file() {
    let directory = tempdir().unwrap();
    let first = directory.path().join("first.json");
    let second = directory.path().join("second.json");
    let before = "{\"a\":1,\"b\":2}";
    for operation in [
        config("a", json!({"type":"replace","new_text":"2"})),
        config(
            "a",
            json!({"type":"set","new_text":"2","create_missing":null}),
        ),
        config(
            "a",
            json!({"type":"append","new_text":"2","create_missing":false}),
        ),
        config("a", json!({"type":"delete","create_missing":false})),
        config("a", json!({"type":"delete","unknown":true})),
        set("missing", "2"),
        set("a[9]", "2"),
        set("a", "["),
        json!({"target":{"type":"config_path","path":"a","document_index":0},"op":{"type":"set","new_text":"2"}}),
    ] {
        fs::write(&first, before).unwrap();
        fs::write(&second, before).unwrap();
        reject(
            &json!({"command":"edit","files":[
            {"file":first,"operations":[set("a","10"),set("b","20")]},
            {"file":second,"operations":[operation]}]}),
            "invalid_request",
            &["/files/1/operations/0"],
        );
        assert_eq!(fs::read(&first).unwrap(), before.as_bytes());
        assert_eq!(fs::read(&second).unwrap(), before.as_bytes());
    }
}

#[test]
fn new_config_targets_reject_unknown_duplicate_and_mixed_guard_fields() {
    let directory = tempdir().unwrap();
    let file = directory.path().join("settings.json");
    let before = "{\"a\":1}";
    fs::write(&file, before).unwrap();
    for field in [
        "identity",
        "kind",
        "span_hint",
        "expected_old_hash",
        "unknown",
    ] {
        let mut operation = set("a", "2");
        operation["target"][field] = Value::Null;
        reject(
            &json!({"command":"edit","file":file,"operations":[operation]}),
            "invalid_request",
            &["/operations/0"],
        );
        let mut operation = set("a", "2");
        operation[field] = Value::Null;
        reject(
            &json!({"command":"edit","file":file,"operations":[operation]}),
            "invalid_request",
            &["/operations/0"],
        );
    }
    for operation in [
        r#"{"target":{"type":"config_path","path":"a","path":"b"},"op":{"type":"delete"}}"#,
        r#"{"target":{"type":"config_path","type":"config_path","path":"a"},"op":{"type":"delete"}}"#,
        r#"{"target":{"type":"config_path","path":"a"},"op":{"type":"set","new_text":"2","new_text":"3"}}"#,
        r#"{"target":{"type":"config_path","path":"a"},"op":{"type":"delete","type":"delete"}}"#,
    ] {
        let input = format!(
            "{{\"command\":\"edit\",\"file\":{},\"operations\":[{}]}}",
            json!(file),
            operation
        );
        failure(
            run_identedit_with_stdin(&["edit", "--json"], &input),
            "invalid_request",
            &["/operations/0"],
        );
    }
    assert_eq!(fs::read(&file).unwrap(), before.as_bytes());
}

#[test]
fn supplied_file_hash_is_checked_before_decoding_and_never_refreshed() {
    let directory = tempdir().unwrap();
    let file = directory.path().join("settings.json");
    let original = "{\"a\":1}";
    let mut operation = set("a", "2");
    operation["target"]["expected_file_hash"] = json!(crate::common::hash_text(original));
    for changed in [b"{\"a\":3}".as_slice(), b"\xff".as_slice()] {
        fs::write(&file, changed).unwrap();
        reject(
            &json!({"command":"edit","file":file,"operations":[operation]}),
            "precondition_failed",
            &["/operations/0"],
        );
        assert_eq!(fs::read(&file).unwrap(), changed);
    }
}

#[test]
fn batch_config_conditions_survive_apply_repair_and_abort_other_file_writes() {
    let directory = tempdir().unwrap();
    let first = directory.path().join("first.json");
    let second = directory.path().join("second.json");
    let before = "{\"inside\":{\"a\":1,\"b\":2},\"outside\":0}";
    let changed = "{\"inside\":{\"a\":1,\"b\":2},\"outside\":9}";
    for noop in [false, true] {
        fs::write(&first, before).unwrap();
        fs::write(&second, before).unwrap();
        let plan = compile(&json!({"command":"edit","files":[
            {"file":first,"operations":[set("inside.a","10")]},
            {"file":second,"operations":[set("inside.a",if noop {"1"} else {"10"}),set("inside.b",if noop {"2"} else {"20"})]}]}));
        fs::write(&second, changed).unwrap();
        for options in [
            vec![],
            vec!["--dry-run"],
            vec!["--repair"],
            vec!["--repair", "--dry-run"],
        ] {
            let output = apply(&plan, &options);
            assert!(!output.status.success());
            let response: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(
                response["error"]["type"], "precondition_failed",
                "{response}"
            );
            assert_eq!(fs::read(&first).unwrap(), before.as_bytes());
            assert_eq!(fs::read(&second).unwrap(), changed.as_bytes());
        }
    }
}

#[test]
fn singleton_config_json_and_existing_flag_plans_are_equivalent() {
    let directory = tempdir().unwrap();
    for (extension, before) in [
        ("json", "{\"value\":1,\"items\":[1, 2]}\n"),
        ("toml", "value = 1\nitems = [1, 2]\n"),
        ("yaml", "value: 1\nitems: [1, 2]\n"),
    ] {
        let file = directory.path().join(format!("settings.{extension}"));
        fs::write(&file, before).unwrap();
        for (operation, path, flags) in [
            (set("value", "2"), "value", vec!["--set-value", "2"]),
            (
                config("items", json!({"type":"append","new_text":"3"})),
                "items",
                vec!["--append-value", "3"],
            ),
            (
                config("value", json!({"type":"delete"})),
                "value",
                vec!["--delete"],
            ),
            (
                create("new", "2"),
                "new",
                vec!["--set-value", "2", "--create-missing"],
            ),
        ] {
            let actual = compile(&json!({"command":"edit","file":file,"operations":[operation]}));
            let mut args = vec!["edit", file.to_str().unwrap(), "--config-path", path];
            args.extend(flags);
            let expected = successful(run_identedit(&args));
            assert_eq!(actual, expected);
            successful(apply(&actual, &["--dry-run"]));
            assert_eq!(fs::read(&file).unwrap(), before.as_bytes());
        }
    }
}

#[test]
fn empty_and_comment_only_creation_keeps_legacy_boundary_plans() {
    let directory = tempdir().unwrap();
    for (extension, before) in [
        ("json", ""),
        ("json", "\u{feff} \r\n"),
        ("toml", ""),
        ("toml", "\u{feff}\r\n"),
        ("yaml", ""),
        ("yaml", "\u{feff}\r\n"),
        ("yaml", "# keep\r\n"),
    ] {
        let file = directory.path().join(format!("settings.{extension}"));
        fs::write(&file, before).unwrap();
        let actual =
            compile(&json!({"command":"edit","file":file,"operations":[create("new","2")]}));
        let expected = successful(run_identedit(&[
            "edit",
            file.to_str().unwrap(),
            "--config-path",
            "new",
            "--set-value",
            "2",
            "--create-missing",
        ]));
        assert_eq!(actual, expected);
        successful(apply(&actual, &["--dry-run"]));
        assert_eq!(fs::read(&file).unwrap(), before.as_bytes());
    }
}
