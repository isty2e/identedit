mod common;

use std::fs;
use std::path::Path;
use std::process::Output;

use serde_json::{Value, json};

fn set(file: &Path, path: &str, value: &str, create: bool) -> Output {
    let mut args = vec![
        "patch",
        file.to_str().unwrap(),
        "--config-path",
        path,
        "--set-value",
        value,
    ];
    if create {
        args.push("--create-missing");
    }
    common::run_identedit(&args)
}

fn assert_creates(source: &str, path: &str, value: &str, expected: &str) {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("config.json");
    fs::write(&file, source).unwrap();
    let output = set(&file, path, value, true);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert_eq!(fs::read_to_string(&file).unwrap(), expected);
}

#[test]
fn preserves_original_number_spellings_and_unrelated_duplicate_members() {
    assert_creates(
        r#"{"e":1e3,"f":1.50,"dup":1,"dup":2,"obj":{}}"#,
        "obj.added",
        "true",
        r#"{"e":1e3,"f":1.50,"dup":1,"dup":2,"obj":{"added":true}}"#,
    );
}

#[test]
fn preserves_large_integer_and_unicode_escape_spellings() {
    assert_creates(
        "{\"n\":12345678901234567890123,\"na\\u006de\":\"\\u0061\",\"obj\":{}}\n",
        "obj.added",
        "false",
        "{\"n\":12345678901234567890123,\"na\\u006de\":\"\\u0061\",\"obj\":{\"added\":false}}\n",
    );
}

#[test]
fn preserves_raw_payload_including_duplicates_numbers_and_whitespace() {
    let payload =
        " \r\n{\"dup\":1,\"dup\":2,\"e\":1e3,\"f\":1.50,\"n\":12345678901234567890123}\t ";
    assert_creates(
        "{\"keep\":1}\n",
        "added",
        payload,
        &format!("{{\"keep\":1,\"added\":{payload}}}\n"),
    );
}

#[test]
fn inserts_at_deepest_object_without_reordering_or_reformatting_siblings() {
    assert_creates(
        "{\"zeta\":1, \"obj\":{\"z\":0, \"a\":2}, \"alpha\":0}\n",
        "obj.new.deep",
        "3",
        "{\"zeta\":1, \"obj\":{\"z\":0, \"a\":2, \"new\":{\"deep\":3}}, \"alpha\":0}\n",
    );
}

#[test]
fn follows_local_member_spacing_indentation_and_line_endings() {
    for (source, expected) in [
        ("{\"z\": 1}", "{\"z\": 1,\"a\": 2}"),
        ("{\"z\":1, \"b\":2}", "{\"z\":1, \"b\":2, \"a\":2}"),
        (
            "{\n    \"z\" : 1\n}\n",
            "{\n    \"z\" : 1,\n    \"a\" : 2\n}\n",
        ),
        (
            "{\r\n\t\"z\": 1\r\n}\r\n",
            "{\r\n\t\"z\": 1,\r\n\t\"a\": 2\r\n}\r\n",
        ),
        ("{\r  \"z\": 1\r}", "{\r  \"z\": 1,\r  \"a\": 2\r}"),
        (
            "{\n  \"z\":1,\r\n  \"b\":2\n}\r\n",
            "{\n  \"z\":1,\r\n  \"b\":2,\r\n  \"a\":2\n}\r\n",
        ),
    ] {
        assert_creates(source, "a", "2", expected);
    }
}

#[test]
fn empty_objects_and_sources_have_deterministic_insertion_defaults() {
    for (source, expected) in [
        ("{}", "{\"a\":{\"b\":1}}"),
        ("{  }", "{\"a\":{\"b\":1}  }"),
        ("{\r\n}", "{\r\n  \"a\": {\"b\":1}\r\n}"),
        ("", "{\"a\":{\"b\":1}}"),
        (" \r\n\t", " \r\n\t{\"a\":{\"b\":1}}"),
        ("\u{feff}\n", "\u{feff}\n{\"a\":{\"b\":1}}"),
    ] {
        assert_creates(source, "a.b", "1", expected);
    }
}

#[test]
fn preserves_outer_trivia_bom_and_escaped_path_keys() {
    assert_creates(
        "\u{feff} \r\n{\"na\\u006de\": {}}\t\r\n",
        r#"["name"]["quote\"key"]["雪"]"#,
        r#""\u0062""#,
        "\u{feff} \r\n{\"na\\u006de\": {\"quote\\\"key\":{\"雪\":\"\\u0062\"}}}\t\r\n",
    );
}

#[test]
fn traverses_existing_arrays_without_creating_or_growing_them() {
    assert_creates(
        "[{\"keep\":1}, {}]\n",
        "[1].new.leaf",
        "null",
        "[{\"keep\":1}, {\"new\":{\"leaf\":null}}]\n",
    );
    assert_creates(
        "{\"arr\":[{\"z\":1}]}\n",
        "arr[0].a",
        "2",
        "{\"arr\":[{\"z\":1,\"a\":2}]}\n",
    );
}

#[test]
fn rejects_ambiguous_keys_scalar_conflicts_and_missing_array_indices_without_writing() {
    for (source, path) in [
        (r#"{"obj":{},"o\u0062j":{}}"#, "obj.added"),
        (r#"{"obj":{"a":1},"obj":{"b":2}}"#, "obj.added"),
        (r#"{"obj":{"added":1,"added":2}}"#, "obj.added"),
        (r#"{"obj":1}"#, "obj.added"),
        (r#"{"arr":[]}"#, "arr[0].added"),
        ("{}", "new.arr[0]"),
        ("", "[0].added"),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("config.json");
        fs::write(&file, source).unwrap();
        let output = set(&file, path, "1", true);
        assert!(!output.status.success(), "{source} {path}");
        assert_eq!(fs::read_to_string(&file).unwrap(), source);
        let error: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(error["error"]["type"], "invalid_request");
    }
}

#[test]
fn single_value_validation_prevents_member_injection_in_existing_and_missing_paths() {
    for source in ["", "{}", r#"{"value":0}"#] {
        for payload in [
            "1,\"injected\":true",
            "{} {}",
            "true false",
            "/*comment*/1",
            "1//comment",
            "",
            "[1,]",
            "\"\\q\"",
        ] {
            let dir = tempfile::tempdir().unwrap();
            let file = dir.path().join("config.json");
            fs::write(&file, source).unwrap();
            for create in [false, true] {
                let output = set(&file, "value", payload, create);
                assert!(
                    !output.status.success(),
                    "{source:?} {payload:?} create={create}"
                );
                assert_eq!(fs::read_to_string(&file).unwrap(), source);
            }
        }
    }
}

#[test]
fn rejects_non_single_json_sources_in_missing_path_creation_without_writing() {
    for source in ["{} {}", "/*comment*/{}", "{}//comment", "\u{a0}"] {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("config.json");
        fs::write(&file, source).unwrap();
        let output = set(&file, "added", "1", true);
        assert!(!output.status.success(), "{source:?}");
        assert_eq!(fs::read_to_string(&file).unwrap(), source);
    }
}

#[test]
fn json_request_path_preserves_source_and_payload_without_shell_encoding() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("config.json");
    fs::write(&file, "{\"z\":1e3}\n").unwrap();
    let request = json!({
        "command": "patch", "file": file,
        "target": {"type": "config_path", "path": "added"},
        "op": {"type": "set", "new_text": "{\"x\":1,\"x\":2}", "create_missing": true}
    });
    let output = common::run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        "{\"z\":1e3,\"added\":{\"x\":1,\"x\":2}}\n"
    );
}

#[test]
fn repeated_create_flag_updates_existing_values_surgically() {
    let source = "{\"z\":1e3,\"obj\":{\"value\":0,\"keep\":1.50}}\n";
    assert_creates(
        source,
        "obj.value",
        " {\"dup\":1,\"dup\":2} ",
        "{\"z\":1e3,\"obj\":{\"value\": {\"dup\":1,\"dup\":2} ,\"keep\":1.50}}\n",
    );
}

#[test]
fn dry_run_text_file_payload_preserves_the_original_and_plan_is_reusable() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("config.json");
    let payload = dir.path().join("value.json");
    let source = "{\"z\":1e3,\"obj\":{}}\r\n";
    let value = "{\n  \"a\":1.50,\n  \"a\":2\n}\n";
    fs::write(&file, source).unwrap();
    fs::write(&payload, value).unwrap();
    let args = [
        file.to_str().unwrap(),
        "--config-path",
        "obj.new",
        "--set-value",
        "--text-file",
        payload.to_str().unwrap(),
        "--create-missing",
    ];
    let mut preview = vec!["patch", "--dry-run"];
    preview.extend(args);
    let output = common::run_identedit(&preview);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert_eq!(fs::read_to_string(&file).unwrap(), source);

    let mut edit = vec!["edit"];
    edit.extend(args);
    let plan = common::run_identedit(&edit);
    assert!(
        plan.status.success(),
        "{}",
        String::from_utf8_lossy(&plan.stdout)
    );
    assert_eq!(fs::read_to_string(&file).unwrap(), source);
    let output =
        common::run_identedit_with_stdin(&["apply"], std::str::from_utf8(&plan.stdout).unwrap());
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        format!("{{\"z\":1e3,\"obj\":{{\"new\":{value}}}}}\r\n")
    );
}

#[test]
fn create_missing_plan_rejects_unrelated_root_changes_before_apply() {
    for source in ["", " \r\n", "{\"obj\":{},\"keep\":1}"] {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("config.json");
        fs::write(&file, source).unwrap();
        let plan = common::run_identedit(&[
            "edit",
            file.to_str().unwrap(),
            "--config-path",
            "obj.new",
            "--set-value",
            "1",
            "--create-missing",
        ]);
        assert!(
            plan.status.success(),
            "{}",
            String::from_utf8_lossy(&plan.stdout)
        );
        let external = if source.contains("keep") {
            source.replace("keep\":1", "keep\":2")
        } else {
            "{\"external\":1}".to_string()
        };
        fs::write(&file, &external).unwrap();
        let output = common::run_identedit_with_stdin(
            &["apply"],
            std::str::from_utf8(&plan.stdout).unwrap(),
        );
        assert!(!output.status.success());
        assert_eq!(fs::read_to_string(&file).unwrap(), external);
    }
}

#[test]
fn exact_file_guard_fails_before_creation_without_mutating_external_content() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("config.json");
    let original = "{\"obj\":{}}\n";
    let external = "{\"obj\":{},\"external\":1}\n";
    fs::write(&file, external).unwrap();
    let request = json!({
        "command":"patch", "file":file,
        "target":{"type":"config_path", "path":"obj.new", "expected_file_hash":common::hash_text(original)},
        "op":{"type":"set", "new_text":"1", "create_missing":true}
    });
    let output = common::run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(!output.status.success());
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["error"]["type"], "precondition_failed");
    assert_eq!(fs::read_to_string(&file).unwrap(), external);
}

#[test]
fn new_member_does_not_copy_nested_value_indentation_as_its_separator() {
    assert_creates(
        "{\"obj\":{\"z\":[\n    1,\n    2\n  ]}}",
        "obj.a",
        "3",
        "{\"obj\":{\"z\":[\n    1,\n    2\n  ],\"a\":3}}",
    );
    assert_creates(
        "{\n  \"obj\": {\r\n  }\n}\n",
        "obj.a",
        "{\"raw\":\"a,b:c}\\\"\",\"raw\":0}",
        "{\n  \"obj\": {\r\n    \"a\": {\"raw\":\"a,b:c}\\\"\",\"raw\":0}\r\n  }\n}\n",
    );
}

#[test]
fn parser_limited_values_are_either_preserved_or_rejected_before_writing() {
    for source in ["", "{}"] {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("config.json");
        fs::write(&file, source).unwrap();
        // Do not require every valid JSON spelling to be supported by the bundled grammar.
        let output = set(&file, "new", "1e+3", true);
        let after = fs::read_to_string(&file).unwrap();
        if output.status.success() {
            assert_eq!(after, "{\"new\":1e+3}");
        } else {
            assert_eq!(after, source);
            let response: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(response["error"]["type"], "invalid_request");
        }
    }
}

#[test]
fn numeric_values_are_not_limited_by_f64_or_u64_materialization() {
    for value in [
        "-0",
        "-12e-999",
        "1e999",
        "0.00000000000000000000",
        "12345678901234567890123456789012345678901234567890",
    ] {
        // JSON ingress avoids interpreting a negative value as another shell flag.
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("config.json");
        let source = "{\"keep\":1e999}\n";
        fs::write(&file, source).unwrap();
        let request = json!({
            "command":"patch", "file":file,
            "target":{"type":"config_path", "path":"new"},
            "op":{"type":"set", "new_text":value, "create_missing":true}
        });
        let output = common::run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            format!("{{\"keep\":1e999,\"new\":{value}}}\n")
        );
    }
}

fn nested_json_value(depth: usize, layers: &[(&str, &str)]) -> String {
    let mut value = String::new();
    let mut closing = Vec::with_capacity(depth);
    for index in 0..depth {
        let (open, close) = layers[index % layers.len()];
        value.push_str(open);
        closing.push(close);
    }
    value.push('0');
    for close in closing.into_iter().rev() {
        value.push_str(close);
    }
    value
}

#[test]
fn deeply_nested_values_are_preserved_for_existing_and_missing_paths() {
    let shapes: &[&[(&str, &str)]] = &[
        &[("[", "]")],
        &[("{\"k\":", "}")],
        &[("[", "]"), ("{\"k\":", "}")],
    ];
    for depth in [127, 128, 129, 256, 512] {
        for shape in shapes {
            let value = nested_json_value(depth, shape);
            let source = "{\"value\":0,\"keep\":1}\n";
            for (path, create, expected) in [
                (
                    "value",
                    false,
                    format!("{{\"value\":{value},\"keep\":1}}\n"),
                ),
                ("value", true, format!("{{\"value\":{value},\"keep\":1}}\n")),
                (
                    "new",
                    true,
                    format!("{{\"value\":0,\"keep\":1,\"new\":{value}}}\n"),
                ),
            ] {
                let dir = tempfile::tempdir().unwrap();
                let file = dir.path().join("config.json");
                fs::write(&file, source).unwrap();
                let output = set(&file, path, &value, create);
                assert!(
                    output.status.success(),
                    "depth={depth}, path={path}, create={create}: {}",
                    String::from_utf8_lossy(&output.stdout)
                );
                assert_eq!(fs::read_to_string(&file).unwrap(), expected);
            }
        }
    }
}

#[test]
fn deep_source_documents_and_missing_key_chains_preserve_original_bytes() {
    for depth in [129, 256, 512] {
        let value = nested_json_value(depth, &[("[", "]"), ("{\"k\":", "}")]);
        let source = format!("{{\"deep\":{value},\"obj\":{{}}}}\n");
        assert_creates(
            &source,
            "obj.new",
            &value,
            &format!("{{\"deep\":{value},\"obj\":{{\"new\":{value}}}}}\n"),
        );
        assert_creates("", "new", &value, &format!("{{\"new\":{value}}}"));

        let path = vec!["k"; depth].join(".");
        let expected = format!("{}1{}", "{\"k\":".repeat(depth), "}".repeat(depth));
        assert_creates("{}", &path, "1", &expected);
    }
}

#[test]
fn deeply_nested_invalid_and_multiple_values_are_rejected_without_writing() {
    for depth in [129, 512] {
        let valid = nested_json_value(depth, &[("[", "]"), ("{\"k\":", "}")]);
        for value in [
            valid[..valid.len() - 1].to_string(),
            format!("{valid} false"),
            format!("{valid},\"injected\":true"),
            format!("{valid}/*comment*/"),
            format!("{}[0,]{}", "[".repeat(depth), "]".repeat(depth)),
        ] {
            for (source, path, create) in [
                ("{\"value\":0}", "value", false),
                ("{\"value\":0}", "value", true),
                ("{}", "new", true),
                ("", "new", true),
            ] {
                let dir = tempfile::tempdir().unwrap();
                let file = dir.path().join("config.json");
                fs::write(&file, source).unwrap();
                let output = set(&file, path, &value, create);
                assert!(
                    !output.status.success(),
                    "depth={depth}, path={path}, create={create}"
                );
                let response: Value = serde_json::from_slice(&output.stdout).unwrap();
                assert_eq!(response["error"]["type"], "invalid_request");
                assert_eq!(fs::read_to_string(&file).unwrap(), source);
            }
        }
    }
}
