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

fn write_temp_bytes(suffix: &str, bytes: &[u8]) -> PathBuf {
    let mut temp_file = Builder::new()
        .suffix(suffix)
        .tempfile()
        .expect("temp source file should be created");
    temp_file
        .write_all(bytes)
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

fn assert_select_kind_and_optional_name(file: &Path, kind: &str, expected_name: Option<&str>) {
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

    if let Some(name) = expected_name {
        assert!(
            handles.iter().any(|handle| handle["name"] == name),
            "expected handle with name '{name}' for kind {kind}"
        );
    }
}

#[test]
fn select_covers_scss_kinds_and_provider() {
    let scss_file = fixture_path("example.scss");

    assert_select_kind_and_optional_name(&scss_file, "stylesheet", None);
    assert_select_kind_and_optional_name(&scss_file, "rule_set", None);
    assert_select_kind_and_optional_name(&scss_file, "mixin_statement", Some("card"));
    assert_select_kind_and_optional_name(&scss_file, "function_statement", Some("doubled"));
    assert_select_kind_and_optional_name(&scss_file, "include_statement", None);
}

#[test]
fn select_supports_case_insensitive_scss_extension() {
    let file_path = copy_fixture_to_temp("example.scss", ".SCSS");
    assert_select_kind_and_optional_name(&file_path, "mixin_statement", Some("card"));
}

#[test]
fn select_supports_utf8_bom_prefixed_scss_files() {
    let fixture = fs::read(fixture_path("example.scss")).expect("fixture should be readable");
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(&fixture);
    let file_path = write_temp_bytes(".scss", &bytes);

    assert_select_kind_and_optional_name(&file_path, "mixin_statement", Some("card"));
}

#[test]
fn select_reports_parse_failure_for_syntax_invalid_scss() {
    let file_path = write_temp_source(
        ".scss",
        "$primary: #0a84ff;\n@mixin broken($value) {\n  color: $value;\n",
    );
    let output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "mixin_statement",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "syntax-invalid scss should fail under the scss provider"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "parse_failure");
    let message = response["error"]["message"]
        .as_str()
        .expect("error.message should be a string");
    assert!(message.contains("tree-sitter-scss"));
    assert!(message.contains("Syntax errors detected in SCSS source"));
}

#[test]
fn transform_replace_and_apply_support_scss_mixin_statement() {
    let file_path = copy_fixture_to_temp("example.scss", ".scss");
    let select_output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "mixin_statement",
        "--name",
        "card",
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
        .expect("identity should be present");

    let replacement = "@mixin card($padding) {\n  padding: $padding;\n  border-radius: 12px;\n  background-color: darken($primary, 10%);\n}";
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
    assert!(modified.contains("border-radius: 12px;"));
    assert!(modified.contains("darken($primary, 10%)"));
}

#[test]
fn transform_reports_ambiguous_target_for_duplicate_scss_mixin_identity() {
    let source = "@mixin configure($value) {\n  color: $value;\n}\n\n@mixin configure($value) {\n  color: $value;\n}\n";
    let file_path = write_temp_source(".scss", source);
    let select_output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "mixin_statement",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        select_output.status.success(),
        "select failed: {}",
        String::from_utf8_lossy(&select_output.stderr)
    );

    let select_response: Value =
        serde_json::from_slice(&select_output.stdout).expect("stdout should be valid JSON");
    let handles = select_response["handles"]
        .as_array()
        .expect("handles should be an array");
    let duplicate_identity = handles
        .iter()
        .map(|handle| {
            handle["identity"]
                .as_str()
                .expect("identity should be string")
        })
        .find(|identity| {
            handles
                .iter()
                .filter(|h| h["identity"] == *identity)
                .count()
                >= 2
        })
        .expect("fixture should include duplicate mixin identity");

    let output = run_identedit(&[
        "transform",
        "--identity",
        duplicate_identity,
        "--replace",
        "@mixin configure($value) {\n  color: lighten($value, 10%);\n}",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "transform should fail for ambiguous duplicate SCSS mixin identity"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "ambiguous_target");
}

#[test]
fn transform_json_span_hint_disambiguates_duplicate_scss_mixin_identity() {
    let source = "@mixin configure($value) {\n  color: $value;\n}\n\n@mixin configure($value) {\n  color: $value;\n}\n";
    let file_path = write_temp_source(".scss", source);
    let select_output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "mixin_statement",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        select_output.status.success(),
        "select failed: {}",
        String::from_utf8_lossy(&select_output.stderr)
    );

    let select_response: Value =
        serde_json::from_slice(&select_output.stdout).expect("stdout should be valid JSON");
    let duplicate_handles = select_response["handles"]
        .as_array()
        .expect("handles should be an array")
        .iter()
        .filter(|handle| handle["name"] == "configure")
        .collect::<Vec<_>>();
    assert!(
        duplicate_handles.len() >= 2,
        "fixture should include at least two configure mixins"
    );

    let target = duplicate_handles[1];
    let span = &target["span"];
    let request = json!({
        "command": "transform",
        "file": file_path.to_string_lossy(),
        "operations": [{
            "target": {
                "type": "node",
                "identity": target["identity"],
                "kind": target["kind"],
                "expected_old_hash": target["expected_old_hash"],
                "span_hint": {"start": span["start"], "end": span["end"]}
            },
            "op": {
                "type": "replace",
                "new_text": "@mixin configure($value) {\n  color: lighten($value, 10%);\n}"
            }
        }]
    });
    let request_body = serde_json::to_string(&request).expect("request should serialize");

    let transform_output = run_identedit_with_stdin(&["transform", "--json"], &request_body);
    assert!(
        transform_output.status.success(),
        "transform --json should disambiguate duplicate SCSS mixin identity: {}",
        String::from_utf8_lossy(&transform_output.stderr)
    );

    let transform_json =
        std::str::from_utf8(&transform_output.stdout).expect("transform output should be utf-8");
    let apply_output = run_identedit_with_stdin(&["apply"], transform_json);
    assert!(
        apply_output.status.success(),
        "apply failed after SCSS span_hint disambiguation: {}",
        String::from_utf8_lossy(&apply_output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("file should be readable");
    assert_eq!(modified.matches("lighten($value, 10%)").count(), 1);
    assert_eq!(modified.matches("color: $value;").count(), 1);
}

#[test]
fn transform_json_duplicate_scss_identity_with_missed_span_hint_returns_ambiguous_target() {
    let source = "@mixin configure($value) {\n  color: $value;\n}\n\n@mixin configure($value) {\n  color: $value;\n}\n";
    let file_path = write_temp_source(".scss", source);
    let select_output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "mixin_statement",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        select_output.status.success(),
        "select failed: {}",
        String::from_utf8_lossy(&select_output.stderr)
    );

    let select_response: Value =
        serde_json::from_slice(&select_output.stdout).expect("stdout should be valid JSON");
    let target = select_response["handles"]
        .as_array()
        .expect("handles should be an array")
        .iter()
        .find(|handle| handle["name"] == "configure")
        .expect("configure handle should be present");

    let request = json!({
        "command": "transform",
        "file": file_path.to_string_lossy(),
        "operations": [{
            "target": {
                "type": "node",
                "identity": target["identity"],
                "kind": target["kind"],
                "expected_old_hash": target["expected_old_hash"],
                "span_hint": {"start": 1, "end": 2}
            },
            "op": {
                "type": "replace",
                "new_text": "@mixin configure($value) {\n  color: lighten($value, 10%);\n}"
            }
        }]
    });
    let request_body = serde_json::to_string(&request).expect("request should serialize");

    let output = run_identedit_with_stdin(&["transform", "--json"], &request_body);
    assert!(
        !output.status.success(),
        "transform --json should fail when span_hint misses duplicate SCSS mixins"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "ambiguous_target");
}

#[test]
fn apply_reports_precondition_failed_after_scss_source_mutation() {
    let file_path = copy_fixture_to_temp("example.scss", ".scss");
    let select_output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "mixin_statement",
        "--name",
        "card",
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
        .expect("identity should be present");

    let transform_output = run_identedit(&[
        "transform",
        "--identity",
        identity,
        "--replace",
        "@mixin card($padding) {\n  padding: $padding;\n  border-radius: 12px;\n  background-color: $primary;\n}",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        transform_output.status.success(),
        "transform failed: {}",
        String::from_utf8_lossy(&transform_output.stderr)
    );

    let original = fs::read_to_string(&file_path).expect("file should be readable");
    let mutated = original.replace("border-radius: 8px;", "border-radius: 9px;");
    fs::write(&file_path, mutated).expect("mutated source write should succeed");

    let transform_json =
        std::str::from_utf8(&transform_output.stdout).expect("transform output should be utf-8");
    let apply_output = run_identedit_with_stdin(&["apply"], transform_json);
    assert!(
        !apply_output.status.success(),
        "apply should fail when SCSS source changes after transform"
    );

    let response: Value =
        serde_json::from_slice(&apply_output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "precondition_failed");
}

#[test]
fn select_ignores_scss_construct_tokens_inside_block_comments() {
    let source =
        "/*\n@mixin fake($x) {\n  color: $x;\n}\n*/\n\n@mixin real($x) {\n  color: $x;\n}\n";
    let file_path = write_temp_source(".scss", source);
    let output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "mixin_statement",
        file_path.to_str().expect("path should be utf-8"),
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
    let mixin_names = handles
        .iter()
        .filter_map(|handle| handle["name"].as_str())
        .collect::<Vec<_>>();

    assert!(mixin_names.contains(&"real"));
    assert!(!mixin_names.contains(&"fake"));
}

#[test]
fn select_reports_parse_failure_for_unterminated_block_comment_in_scss() {
    let file_path = write_temp_source(".scss", "/* broken comment\n$primary: #0a84ff;\n");
    let output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "stylesheet",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "unterminated comment should fail under the scss provider"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "parse_failure");
}

#[test]
fn transform_replace_and_apply_preserve_crlf_scss_source_segments() {
    let source = "@mixin card($padding) {\r\n  padding: $padding;\r\n  border-radius: 8px;\r\n}\r\n\r\n.dashboard {\r\n  @include card(12px);\r\n}\r\n";
    let file_path = write_temp_source(".scss", source);
    let select_output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "mixin_statement",
        "--name",
        "card",
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
        .expect("identity should be present");

    let transform_output = run_identedit(&[
        "transform",
        "--identity",
        identity,
        "--replace",
        "@mixin card($padding) {\n  padding: $padding;\n  border-radius: 10px;\n}",
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
    assert!(
        modified.starts_with("@mixin card($padding) {\n"),
        "replacement text should be applied as requested"
    );
    assert!(
        modified.contains("\r\n.dashboard {\r\n"),
        "untouched segments should preserve original CRLF line endings"
    );
}

#[test]
fn select_reports_parse_failure_for_nul_in_scss_source() {
    let file_path = write_temp_bytes(".scss", b"$primary: #0a84ff;\n\0@mixin card($x){color:$x;}");
    let output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "mixin_statement",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "nul byte in scss source should fail under the scss provider"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "parse_failure");
}

#[test]
fn transform_reports_ambiguous_target_for_duplicate_scss_function_identity() {
    let source = "@function tone($value) {\n  @return $value * 2;\n}\n\n@function tone($value) {\n  @return $value * 2;\n}\n";
    let file_path = write_temp_source(".scss", source);
    let select_output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "function_statement",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        select_output.status.success(),
        "select failed: {}",
        String::from_utf8_lossy(&select_output.stderr)
    );

    let select_response: Value =
        serde_json::from_slice(&select_output.stdout).expect("stdout should be valid JSON");
    let handles = select_response["handles"]
        .as_array()
        .expect("handles should be an array");
    let duplicate_identity = handles
        .iter()
        .map(|handle| {
            handle["identity"]
                .as_str()
                .expect("identity should be string")
        })
        .find(|identity| {
            handles
                .iter()
                .filter(|h| h["identity"] == *identity)
                .count()
                >= 2
        })
        .expect("fixture should include duplicate function identity");

    let output = run_identedit(&[
        "transform",
        "--identity",
        duplicate_identity,
        "--replace",
        "@function tone($value) {\n  @return $value * 3;\n}",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "transform should fail for ambiguous duplicate SCSS function identity"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "ambiguous_target");
}

#[test]
fn select_finds_nested_rule_sets_inside_media_queries() {
    let file_path = write_temp_source(
        ".scss",
        "@media screen and (min-width: 768px) {\n  .dashboard {\n    .card {\n      color: red;\n    }\n  }\n}\n",
    );
    let output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "rule_set",
        file_path.to_str().expect("path should be utf-8"),
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
        handles.len() >= 2,
        "expected nested rule sets to produce multiple rule_set handles"
    );
    assert!(
        handles
            .iter()
            .filter_map(|handle| handle["text"].as_str())
            .any(|text| text.contains(".card")),
        "expected at least one nested rule_set for .card"
    );
}
