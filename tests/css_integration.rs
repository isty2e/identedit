use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use serde_json::{Value, json};
use tempfile::Builder;

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn copy_fixture_to_temp(name: &str, suffix: &str) -> PathBuf {
    let source = fixture_path(name);
    let content = fs::read_to_string(&source).expect("fixture should be readable");
    let mut temp_file = Builder::new()
        .suffix(suffix)
        .tempfile()
        .expect("temp source file should be created");
    temp_file
        .write_all(content.as_bytes())
        .expect("temp fixture write should succeed");
    temp_file.keep().expect("temp file should persist").1
}

fn write_temp_source(suffix: &str, source: &str) -> PathBuf {
    let mut temp_file = Builder::new()
        .suffix(suffix)
        .tempfile()
        .expect("temp source file should be created");
    temp_file
        .write_all(source.as_bytes())
        .expect("temp source write should succeed");
    temp_file.keep().expect("temp file should persist").1
}

fn run_identedit(arguments: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_identedit"));
    command.env("IDENTEDIT_ALLOW_LEGACY", "1");
    command.args(arguments);
    command.output().expect("failed to run identedit binary")
}

fn run_identedit_with_stdin(arguments: &[&str], input: &str) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_identedit"));
    command.env("IDENTEDIT_ALLOW_LEGACY", "1");
    command.args(arguments);
    command.stdin(Stdio::piped());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());

    let mut child = command.spawn().expect("failed to spawn identedit binary");
    let stdin = child.stdin.as_mut().expect("stdin should be available");
    stdin
        .write_all(input.as_bytes())
        .expect("stdin write should succeed");

    child
        .wait_with_output()
        .expect("failed to read process output")
}

fn assert_select_kind(file: &Path, kind: &str) {
    let output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        kind,
        file.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "select failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    let handles = response["handles"]
        .as_array()
        .expect("handles should be an array");
    assert!(
        !handles.is_empty(),
        "expected at least one handle for {kind}"
    );
}

#[test]
fn select_covers_css_kinds_and_provider() {
    let css_file = fixture_path("example.css");

    assert_select_kind(&css_file, "stylesheet");
    assert_select_kind(&css_file, "rule_set");
}

#[test]
fn select_supports_case_insensitive_css_extension() {
    let file_path = copy_fixture_to_temp("example.css", ".CSS");
    let output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "rule_set",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "select failed for .CSS extension: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn transform_replace_and_apply_support_css_stylesheet() {
    let file_path = copy_fixture_to_temp("example.css", ".css");
    let select_output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "stylesheet",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        select_output.status.success(),
        "select failed: {}",
        String::from_utf8_lossy(&select_output.stderr)
    );

    let select_response: Value =
        serde_json::from_slice(&select_output.stdout).expect("stdout should be valid JSON");
    let identity = select_response["handles"][0]["identity"]
        .as_str()
        .expect("stylesheet identity should be present");

    let replacement = "body {\n  margin: 0;\n  color: blue;\n}\n";
    let transform_output = run_identedit(&[
        "transform",
        "--identity",
        identity,
        "--replace",
        replacement,
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        transform_output.status.success(),
        "transform failed: {}",
        String::from_utf8_lossy(&transform_output.stderr)
    );

    let transform_response: Value =
        serde_json::from_slice(&transform_output.stdout).expect("stdout should be valid JSON");
    assert_eq!(
        transform_response["files"][0]["operations"][0]["op"]["type"],
        "replace"
    );
    assert_eq!(
        transform_response["files"][0]["operations"][0]["op"]["new_text"],
        replacement
    );

    let transform_json =
        std::str::from_utf8(&transform_output.stdout).expect("transform output should be utf-8");
    let apply_output = run_identedit_with_stdin(&["apply"], transform_json);
    assert!(
        apply_output.status.success(),
        "apply failed: {}",
        String::from_utf8_lossy(&apply_output.stderr)
    );

    let apply_response: Value =
        serde_json::from_slice(&apply_output.stdout).expect("stdout should be valid JSON");
    assert_eq!(apply_response["summary"]["files_modified"], 1);
    assert_eq!(apply_response["summary"]["operations_applied"], 1);

    let modified = fs::read_to_string(&file_path).expect("file should be readable");
    assert!(modified.contains("color: blue;"));
}

#[test]
fn select_reports_parse_failure_for_syntax_invalid_css() {
    let file_path = write_temp_source(".css", "body { color: red;\n");
    let output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "stylesheet",
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        !output.status.success(),
        "select should fail for syntax-invalid css"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "parse_failure");
    let message = response["error"]["message"]
        .as_str()
        .expect("error.message should be a string");
    assert!(message.contains("tree-sitter-css"));
    assert!(message.contains("Syntax errors detected in CSS source"));
}

#[test]
fn select_covers_realistic_css_fixture_with_media_rule() {
    let css_file = fixture_path("realistic_normalize.css");
    let output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "rule_set",
        css_file.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "select failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    let handles = response["handles"]
        .as_array()
        .expect("handles should be an array");
    assert!(
        handles.iter().any(|handle| {
            handle["text"]
                .as_str()
                .is_some_and(|text| text.contains(".dashboard"))
        }),
        "realistic fixture should include nested .dashboard rule_set"
    );
}

#[test]
fn transform_replace_and_apply_support_realistic_css_body_rule_rewrite() {
    let file_path = copy_fixture_to_temp("realistic_normalize.css", ".css");
    let select_output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "rule_set",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        select_output.status.success(),
        "select failed: {}",
        String::from_utf8_lossy(&select_output.stderr)
    );

    let select_response: Value =
        serde_json::from_slice(&select_output.stdout).expect("stdout should be valid JSON");
    let identity = select_response["handles"]
        .as_array()
        .expect("handles should be an array")
        .iter()
        .find(|handle| {
            handle["text"]
                .as_str()
                .is_some_and(|text| text.starts_with("body {"))
        })
        .and_then(|handle| handle["identity"].as_str())
        .expect("body rule_set identity should be present");

    let replacement = "body {\n  margin: 0;\n  font-family: system-ui, -apple-system, sans-serif;\n  color: #1f2937;\n}";
    let transform_output = run_identedit(&[
        "transform",
        "--identity",
        identity,
        "--replace",
        replacement,
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        transform_output.status.success(),
        "transform failed: {}",
        String::from_utf8_lossy(&transform_output.stderr)
    );

    let transform_json =
        std::str::from_utf8(&transform_output.stdout).expect("transform output should be utf-8");
    let apply_output = run_identedit_with_stdin(&["apply"], transform_json);
    assert!(
        apply_output.status.success(),
        "apply failed: {}",
        String::from_utf8_lossy(&apply_output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("file should be readable");
    assert!(modified.contains("color: #1f2937;"));
}

#[test]
fn select_covers_complex_css_fixture_nested_features() {
    let css_file = fixture_path("complex_openprops.css");
    let output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "rule_set",
        css_file.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "select failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    let handles = response["handles"]
        .as_array()
        .expect("handles should be an array");
    assert!(
        handles.iter().any(|handle| {
            handle["text"]
                .as_str()
                .is_some_and(|text| text.contains(":where(.card)"))
        }),
        "complex fixture should include :where selector rule"
    );
    assert!(
        handles.iter().any(|handle| {
            handle["text"]
                .as_str()
                .is_some_and(|text| text.starts_with(".chip {"))
        }),
        "complex fixture should include duplicated .chip rule"
    );
}

#[test]
fn transform_replace_and_apply_support_complex_css_root_rule_rewrite() {
    let file_path = copy_fixture_to_temp("complex_openprops.css", ".css");
    let select_output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "rule_set",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        select_output.status.success(),
        "select failed: {}",
        String::from_utf8_lossy(&select_output.stderr)
    );

    let select_response: Value =
        serde_json::from_slice(&select_output.stdout).expect("stdout should be valid JSON");
    let identity = select_response["handles"]
        .as_array()
        .expect("handles should be an array")
        .iter()
        .find(|handle| {
            handle["text"]
                .as_str()
                .is_some_and(|text| text.starts_with(":root {"))
        })
        .and_then(|handle| handle["identity"].as_str())
        .expect(":root rule_set identity should be present");

    let replacement = ":root {\n  --surface: hsl(220 20% 98%);\n  --text: hsl(220 40% 18%);\n  --brand: hsl(252 84% 59%);\n  --radius: 0.75rem;\n  --space: clamp(0.75rem, 2vw, 1.25rem);\n  --shadow-sm: 0 1px 2px hsl(220 15% 20% / 0.15);\n}";
    let transform_output = run_identedit(&[
        "transform",
        "--identity",
        identity,
        "--replace",
        replacement,
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        transform_output.status.success(),
        "transform failed: {}",
        String::from_utf8_lossy(&transform_output.stderr)
    );

    let transform_json =
        std::str::from_utf8(&transform_output.stdout).expect("transform output should be utf-8");
    let apply_output = run_identedit_with_stdin(&["apply"], transform_json);
    assert!(
        apply_output.status.success(),
        "apply failed: {}",
        String::from_utf8_lossy(&apply_output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("file should be readable");
    assert!(modified.contains("--shadow-sm: 0 1px 2px"));
}

#[test]
fn transform_reports_ambiguous_target_for_duplicate_css_rule_identity() {
    let file_path = copy_fixture_to_temp("complex_openprops.css", ".css");
    let select_output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "rule_set",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        select_output.status.success(),
        "select failed: {}",
        String::from_utf8_lossy(&select_output.stderr)
    );

    let select_response: Value =
        serde_json::from_slice(&select_output.stdout).expect("stdout should be valid JSON");
    let chip_identity = select_response["handles"]
        .as_array()
        .expect("handles should be an array")
        .iter()
        .find(|handle| {
            handle["text"]
                .as_str()
                .is_some_and(|text| text.starts_with(".chip {"))
        })
        .and_then(|handle| handle["identity"].as_str())
        .expect("expected duplicate .chip identity in fixture");

    let transform_output = run_identedit(&[
        "transform",
        "--identity",
        chip_identity,
        "--replace",
        ".chip { display: inline-flex; gap: 0.5rem; }",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !transform_output.status.success(),
        "transform should fail for ambiguous duplicate css identity"
    );

    let response: Value =
        serde_json::from_slice(&transform_output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "ambiguous_target");
}

#[test]
fn select_covers_bootstrap_escape_css_fixture_tokens() {
    let css_file = fixture_path("complex_bootstrap_escape.css");
    let output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "rule_set",
        css_file.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "select failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    let handles = response["handles"]
        .as_array()
        .expect("handles should be an array");

    assert!(
        handles.iter().any(|handle| {
            handle["text"]
                .as_str()
                .is_some_and(|text| text.starts_with(".form-select {"))
        }),
        "fixture should include bootstrap-like form-select data-uri rule"
    );
    assert!(
        handles.iter().any(|handle| {
            handle["text"]
                .as_str()
                .is_some_and(|text| text.starts_with(".btn\\:primary:hover {"))
        }),
        "fixture should include escaped pseudo-class selector"
    );
}

#[test]
fn transform_replace_and_apply_support_bootstrap_escape_css_form_select_rewrite() {
    let file_path = copy_fixture_to_temp("complex_bootstrap_escape.css", ".css");
    let select_output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "rule_set",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        select_output.status.success(),
        "select failed: {}",
        String::from_utf8_lossy(&select_output.stderr)
    );

    let select_response: Value =
        serde_json::from_slice(&select_output.stdout).expect("stdout should be valid JSON");
    let identity = select_response["handles"]
        .as_array()
        .expect("handles should be an array")
        .iter()
        .find(|handle| {
            handle["text"]
                .as_str()
                .is_some_and(|text| text.starts_with(".form-select {"))
        })
        .and_then(|handle| handle["identity"].as_str())
        .expect(".form-select rule_set identity should be present");

    let replacement = ".form-select {\n  display: block;\n  width: 100%;\n  border: 1px solid var(--identedit-border);\n  background-image: none;\n  background-color: var(--identedit-surface);\n}";
    let transform_output = run_identedit(&[
        "transform",
        "--identity",
        identity,
        "--replace",
        replacement,
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        transform_output.status.success(),
        "transform failed: {}",
        String::from_utf8_lossy(&transform_output.stderr)
    );

    let transform_json =
        std::str::from_utf8(&transform_output.stdout).expect("transform output should be utf-8");
    let apply_output = run_identedit_with_stdin(&["apply"], transform_json);
    assert!(
        apply_output.status.success(),
        "apply failed: {}",
        String::from_utf8_lossy(&apply_output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("file should be readable");
    assert!(modified.contains("background-image: none;"));
    assert!(modified.contains("--identedit-surface"));
}

#[test]
fn transform_reports_ambiguous_target_for_duplicate_escaped_selector_identity() {
    let file_path = copy_fixture_to_temp("complex_bootstrap_escape.css", ".css");
    let select_output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "rule_set",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        select_output.status.success(),
        "select failed: {}",
        String::from_utf8_lossy(&select_output.stderr)
    );

    let select_response: Value =
        serde_json::from_slice(&select_output.stdout).expect("stdout should be valid JSON");
    let duplicate_identity = select_response["handles"]
        .as_array()
        .expect("handles should be an array")
        .iter()
        .find(|handle| {
            handle["text"]
                .as_str()
                .is_some_and(|text| text.starts_with(".btn\\:primary:hover {"))
        })
        .and_then(|handle| handle["identity"].as_str())
        .expect("expected duplicate escaped selector identity in fixture");

    let transform_output = run_identedit(&[
        "transform",
        "--identity",
        duplicate_identity,
        "--replace",
        ".btn\\:primary:hover {\n  color: #fff;\n  background-color: #0a58ca;\n}",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !transform_output.status.success(),
        "transform should fail for ambiguous duplicate identity"
    );

    let response: Value =
        serde_json::from_slice(&transform_output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "ambiguous_target");
}

#[test]
fn transform_reports_ambiguous_target_for_duplicate_css_identity_across_media_blocks() {
    let file_path = copy_fixture_to_temp("stress_css_duplicate_media.css", ".css");
    let select_output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "rule_set",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        select_output.status.success(),
        "select failed: {}",
        String::from_utf8_lossy(&select_output.stderr)
    );

    let select_response: Value =
        serde_json::from_slice(&select_output.stdout).expect("stdout should be valid JSON");
    let duplicate_identity = select_response["handles"]
        .as_array()
        .expect("handles should be an array")
        .iter()
        .find(|handle| {
            handle["text"].as_str().is_some_and(|text| {
                text.starts_with(".alert-pill {") && text.contains("\n    display: inline-flex;")
            })
        })
        .and_then(|handle| handle["identity"].as_str())
        .expect("expected duplicate .alert-pill identity in fixture");

    let transform_output = run_identedit(&[
        "transform",
        "--identity",
        duplicate_identity,
        "--replace",
        ".alert-pill {\n  display: inline-flex;\n  gap: 0.5rem;\n  color: #0f172a;\n}",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !transform_output.status.success(),
        "transform should fail for cross-media duplicate identity"
    );

    let response: Value =
        serde_json::from_slice(&transform_output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "ambiguous_target");
}

#[test]
fn transform_replace_and_apply_support_minified_css_fixture_rule_rewrite() {
    let file_path = copy_fixture_to_temp("minified_bundle.css", ".css");
    let select_output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "rule_set",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        select_output.status.success(),
        "select failed: {}",
        String::from_utf8_lossy(&select_output.stderr)
    );

    let select_response: Value =
        serde_json::from_slice(&select_output.stdout).expect("stdout should be valid JSON");
    let identity = select_response["handles"]
        .as_array()
        .expect("handles should be an array")
        .iter()
        .find(|handle| {
            handle["text"]
                .as_str()
                .is_some_and(|text| text.starts_with(".mini-card{"))
        })
        .and_then(|handle| handle["identity"].as_str())
        .expect(".mini-card identity should be present in minified fixture");

    let replacement = ".mini-card {\n  padding: 1rem;\n  border: 1px solid #94a3b8;\n  background: var(--surface);\n  color: var(--ink);\n  box-shadow: 0 1px 2px rgb(15 23 42 / 0.16);\n}";
    let transform_output = run_identedit(&[
        "transform",
        "--identity",
        identity,
        "--replace",
        replacement,
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        transform_output.status.success(),
        "transform failed: {}",
        String::from_utf8_lossy(&transform_output.stderr)
    );

    let transform_json =
        std::str::from_utf8(&transform_output.stdout).expect("transform output should be utf-8");
    let apply_output = run_identedit_with_stdin(&["apply"], transform_json);
    assert!(
        apply_output.status.success(),
        "apply failed: {}",
        String::from_utf8_lossy(&apply_output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("file should be readable");
    assert!(modified.contains("box-shadow: 0 1px 2px"));
}

#[test]
fn transform_replace_and_apply_supports_bom_cr_only_css_source() {
    let source = "\u{FEFF}.token{color:red}\r.panel{padding:1rem}\r";
    let file_path = write_temp_source(".css", source);
    let select_output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "rule_set",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        select_output.status.success(),
        "select should support BOM + CR-only css source: {}",
        String::from_utf8_lossy(&select_output.stderr)
    );

    let select_response: Value =
        serde_json::from_slice(&select_output.stdout).expect("stdout should be valid JSON");
    let identity = select_response["handles"]
        .as_array()
        .expect("handles should be an array")
        .iter()
        .find(|handle| {
            handle["text"]
                .as_str()
                .is_some_and(|text| text.starts_with(".token{color:red}"))
        })
        .and_then(|handle| handle["identity"].as_str())
        .expect(".token identity should be present");

    let replacement = ".token{color:#0f172a}";
    let transform_output = run_identedit(&[
        "transform",
        "--identity",
        identity,
        "--replace",
        replacement,
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        transform_output.status.success(),
        "transform should support BOM + CR-only css source: {}",
        String::from_utf8_lossy(&transform_output.stderr)
    );

    let transform_json =
        std::str::from_utf8(&transform_output.stdout).expect("transform output should be utf-8");
    let apply_output = run_identedit_with_stdin(&["apply"], transform_json);
    assert!(
        apply_output.status.success(),
        "apply should support BOM + CR-only css source: {}",
        String::from_utf8_lossy(&apply_output.stderr)
    );

    let modified_bytes = fs::read(&file_path).expect("file bytes should be readable");
    assert!(
        modified_bytes.starts_with(&[0xEF, 0xBB, 0xBF]),
        "UTF-8 BOM bytes must be preserved after css replace"
    );
    let modified = String::from_utf8(modified_bytes).expect("modified css should stay utf-8");
    assert!(
        modified.contains('\r'),
        "CR-only delimiters should be preserved"
    );
    assert!(modified.contains(".token{color:#0f172a}"));
}

#[test]
fn transform_json_file_start_insert_supports_bom_cr_only_css_source() {
    let source = "\u{FEFF}.token{color:red}\r";
    let file_path = write_temp_source(".css", source);
    let select_output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "rule_set",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        select_output.status.success(),
        "select should succeed for BOM + CR-only css source: {}",
        String::from_utf8_lossy(&select_output.stderr)
    );

    let select_response: Value =
        serde_json::from_slice(&select_output.stdout).expect("stdout should be valid JSON");
    let expected_file_hash = select_response["file_preconditions"][0]["expected_file_hash"]
        .as_str()
        .expect("expected_file_hash should be present");
    let request = json!({
        "command": "transform",
        "file": file_path.to_str().expect("path should be utf-8"),
        "operations": [
            {
                "target": {
                    "type": "file_start",
                    "expected_file_hash": expected_file_hash
                },
                "op": {"type": "insert", "new_text": "/* lead */\r"}
            }
        ]
    });

    let transform_output = run_identedit_with_stdin(&["transform", "--json"], &request.to_string());
    assert!(
        transform_output.status.success(),
        "transform --json file_start insert should support BOM + CR-only css: {}",
        String::from_utf8_lossy(&transform_output.stderr)
    );

    let transform_response: Value =
        serde_json::from_slice(&transform_output.stdout).expect("stdout should be valid JSON");
    assert_eq!(
        transform_response["files"][0]["operations"][0]["preview"]["matched_span"]["start"],
        3
    );
    assert_eq!(
        transform_response["files"][0]["operations"][0]["preview"]["matched_span"]["end"],
        3
    );

    let transform_json =
        std::str::from_utf8(&transform_output.stdout).expect("transform output should be utf-8");
    let apply_output = run_identedit_with_stdin(&["apply"], transform_json);
    assert!(
        apply_output.status.success(),
        "apply should support BOM + CR-only css file_start insert: {}",
        String::from_utf8_lossy(&apply_output.stderr)
    );

    let modified_bytes = fs::read(&file_path).expect("file bytes should be readable");
    assert!(
        modified_bytes.starts_with(&[0xEF, 0xBB, 0xBF]),
        "UTF-8 BOM bytes must remain at file start"
    );
    let modified = String::from_utf8(modified_bytes).expect("modified css should stay utf-8");
    assert!(
        modified.starts_with("\u{FEFF}/* lead */\r.token{color:red}"),
        "file_start insert should be placed after BOM on CR-only css source"
    );
}

#[test]
fn transform_json_file_end_insert_supports_bom_cr_only_css_source() {
    let source = "\u{FEFF}.token{color:red}\r.panel{padding:1rem}\r";
    let file_path = write_temp_source(".css", source);
    let select_output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "rule_set",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        select_output.status.success(),
        "select should succeed for BOM + CR-only css source: {}",
        String::from_utf8_lossy(&select_output.stderr)
    );

    let select_response: Value =
        serde_json::from_slice(&select_output.stdout).expect("stdout should be valid JSON");
    let expected_file_hash = select_response["file_preconditions"][0]["expected_file_hash"]
        .as_str()
        .expect("expected_file_hash should be present");
    let file_len = fs::read(&file_path)
        .expect("file bytes should be readable")
        .len();
    let request = json!({
        "command": "transform",
        "file": file_path.to_str().expect("path should be utf-8"),
        "operations": [
            {
                "target": {
                    "type": "file_end",
                    "expected_file_hash": expected_file_hash
                },
                "op": {"type": "insert", "new_text": "/* tail */\r"}
            }
        ]
    });

    let transform_output = run_identedit_with_stdin(&["transform", "--json"], &request.to_string());
    assert!(
        transform_output.status.success(),
        "transform --json file_end insert should support BOM + CR-only css: {}",
        String::from_utf8_lossy(&transform_output.stderr)
    );

    let transform_response: Value =
        serde_json::from_slice(&transform_output.stdout).expect("stdout should be valid JSON");
    assert_eq!(
        transform_response["files"][0]["operations"][0]["preview"]["matched_span"]["start"],
        file_len
    );
    assert_eq!(
        transform_response["files"][0]["operations"][0]["preview"]["matched_span"]["end"],
        file_len
    );

    let transform_json =
        std::str::from_utf8(&transform_output.stdout).expect("transform output should be utf-8");
    let apply_output = run_identedit_with_stdin(&["apply"], transform_json);
    assert!(
        apply_output.status.success(),
        "apply should support BOM + CR-only css file_end insert: {}",
        String::from_utf8_lossy(&apply_output.stderr)
    );

    let modified_bytes = fs::read(&file_path).expect("file bytes should be readable");
    assert!(
        modified_bytes.starts_with(&[0xEF, 0xBB, 0xBF]),
        "UTF-8 BOM bytes must remain at file start"
    );
    let modified = String::from_utf8(modified_bytes).expect("modified css should stay utf-8");
    assert!(modified.ends_with("/* tail */\r"));
}

#[test]
fn select_handles_large_minified_single_line_css_without_regression() {
    let mut source = String::from(":root{--ink:#111}");
    for index in 0..400 {
        source.push_str(&format!(
            ".card{index}{{padding:1rem;border:1px solid #cbd5e1;color:#0f172a}}"
        ));
    }
    let file_path = write_temp_source(".css", &source);
    let output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "rule_set",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "select should support large minified css: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    let handles = response["handles"]
        .as_array()
        .expect("handles should be an array");
    assert!(
        handles.len() >= 401,
        "expected many rule_set handles from single-line css stress fixture"
    );
}

#[test]
fn transform_json_span_hint_disambiguates_duplicate_css_identity_across_media() {
    let file_path = copy_fixture_to_temp("stress_css_duplicate_media.css", ".css");
    let select_output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "rule_set",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        select_output.status.success(),
        "select failed: {}",
        String::from_utf8_lossy(&select_output.stderr)
    );

    let select_response: Value =
        serde_json::from_slice(&select_output.stdout).expect("stdout should be valid JSON");
    let duplicates = select_response["handles"]
        .as_array()
        .expect("handles should be an array")
        .iter()
        .filter(|handle| {
            handle["text"].as_str().is_some_and(|text| {
                text.starts_with(".alert-pill {") && text.contains("\n    display: inline-flex;")
            })
        })
        .collect::<Vec<_>>();
    assert!(
        duplicates.len() >= 2,
        "fixture should include duplicate media-scoped .alert-pill identities"
    );

    let target = duplicates[1];
    let span = &target["span"];
    let request = json!({
        "command": "transform",
        "file": file_path.to_str().expect("path should be utf-8"),
        "operations": [
            {
                "target": {
                    "identity": target["identity"],
                    "kind": "rule_set",
                    "expected_old_hash": target["expected_old_hash"],
                    "span_hint": {"start": span["start"], "end": span["end"]}
                },
                "op": {"type": "replace", "new_text": ".alert-pill {\n    display: inline-flex;\n    gap: 0.25rem;\n    color: #1d4ed8;\n  }"}
            }
        ]
    });

    let transform_output = run_identedit_with_stdin(&["transform", "--json"], &request.to_string());
    assert!(
        transform_output.status.success(),
        "transform --json should disambiguate duplicate css identity: {}",
        String::from_utf8_lossy(&transform_output.stderr)
    );

    let transform_json =
        std::str::from_utf8(&transform_output.stdout).expect("transform output should be utf-8");
    let apply_output = run_identedit_with_stdin(&["apply"], transform_json);
    assert!(
        apply_output.status.success(),
        "apply failed: {}",
        String::from_utf8_lossy(&apply_output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("file should be readable");
    assert_eq!(
        modified.matches("color: #1d4ed8;").count(),
        1,
        "span_hint disambiguation should modify exactly one duplicate css node"
    );
}

#[test]
fn select_name_filter_returns_zero_for_nameless_css_rule_sets() {
    let css_file = fixture_path("minified_bundle.css");
    let output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "rule_set",
        "--name",
        "*",
        css_file.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "select should succeed even with name filter on nameless css rule_set: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["summary"]["matches"], 0);
    let handles = response["handles"]
        .as_array()
        .expect("handles should be an array");
    assert!(
        handles.is_empty(),
        "name filter should exclude nameless css rules"
    );
}

#[test]
fn transform_json_stale_hash_on_minified_css_returns_precondition_failed() {
    let file_path = copy_fixture_to_temp("minified_bundle.css", ".css");
    let select_output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "rule_set",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        select_output.status.success(),
        "select failed: {}",
        String::from_utf8_lossy(&select_output.stderr)
    );

    let select_response: Value =
        serde_json::from_slice(&select_output.stdout).expect("stdout should be valid JSON");
    let handle = select_response["handles"][0].clone();
    let span = &handle["span"];
    let request = json!({
        "command": "transform",
        "file": file_path.to_str().expect("path should be utf-8"),
        "operations": [
            {
                "target": {
                    "identity": handle["identity"],
                    "kind": handle["kind"],
                    "expected_old_hash": "deadbeef",
                    "span_hint": {"start": span["start"], "end": span["end"]}
                },
                "op": {"type": "replace", "new_text": ".mini-card{color:#111827}"}
            }
        ]
    });

    let output = run_identedit_with_stdin(&["transform", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "transform --json should fail for stale expected_old_hash"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "precondition_failed");
}

#[test]
fn select_bom_only_css_rule_set_returns_empty_result() {
    let file_path = write_temp_source(".css", "\u{FEFF}");
    let output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "rule_set",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "select should succeed on BOM-only css source: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["summary"]["matches"], 0);
    let handles = response["handles"]
        .as_array()
        .expect("handles should be an array");
    assert!(
        handles.is_empty(),
        "BOM-only css should not produce rule_set handles"
    );
}
