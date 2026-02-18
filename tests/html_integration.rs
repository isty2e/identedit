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
            "expected handle with name '{name}' for {kind}"
        );
    }
}

#[test]
fn select_covers_html_kinds_and_provider() {
    let html_file = fixture_path("example.html");

    assert_select_kind_and_optional_name(&html_file, "start_tag", None);
    assert_select_kind_and_optional_name(&html_file, "element", None);
}

#[test]
fn select_supports_htm_extension_alias() {
    let file_path = copy_fixture_to_temp("example.html", ".htm");
    let output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "start_tag",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "select failed for .htm alias: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn transform_replace_and_apply_support_html_start_tag() {
    let file_path = copy_fixture_to_temp("example.html", ".html");
    let select_output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "start_tag",
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
                .is_some_and(|text| text.starts_with("<section"))
        })
        .and_then(|handle| handle["identity"].as_str())
        .expect("section start_tag identity should be present");

    let replacement = "<section id=\"main\" class=\"updated\">";
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
    assert!(modified.contains("class=\"updated\""));
}

#[test]
fn select_transform_apply_pipeline_supports_html_title_rewrite() {
    let file_path = copy_fixture_to_temp("example.html", ".html");
    let select_output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "start_tag",
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
                .is_some_and(|text| text.starts_with("<title"))
        })
        .and_then(|handle| handle["identity"].as_str())
        .expect("title start_tag identity should be present");

    let replacement = "<title data-suite=\"identedit\">";
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

    let apply_response: Value =
        serde_json::from_slice(&apply_output.stdout).expect("stdout should be valid JSON");
    assert_eq!(apply_response["summary"]["files_modified"], 1);
    assert_eq!(apply_response["summary"]["operations_applied"], 1);

    let modified = fs::read_to_string(&file_path).expect("file should be readable");
    assert!(modified.contains("<title data-suite=\"identedit\">"));
}

#[test]
fn select_reports_parse_failure_for_syntax_invalid_html() {
    let file_path = write_temp_source(".html", "<html><body><");
    let output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "element",
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        !output.status.success(),
        "select should fail for syntax-invalid html"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "parse_failure");
    let message = response["error"]["message"]
        .as_str()
        .expect("error.message should be a string");
    assert!(message.contains("tree-sitter-html"));
    assert!(message.contains("Syntax errors detected in HTML source"));
}

#[test]
fn select_covers_realistic_html_fixture_landmarks() {
    let html_file = fixture_path("realistic_pico.html");
    let output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "start_tag",
        html_file.to_str().expect("path should be utf-8"),
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
        handles.iter().any(|handle| handle["text"] == "<nav>"),
        "realistic fixture should include a nav start tag"
    );
    assert!(
        handles.iter().any(|handle| handle["text"] == "<table>"),
        "realistic fixture should include a table start tag"
    );
    assert!(
        handles.iter().any(|handle| handle["text"] == "<form>"),
        "realistic fixture should include a form start tag"
    );
}

#[test]
fn transform_replace_and_apply_support_realistic_html_main_tag_rewrite() {
    let file_path = copy_fixture_to_temp("realistic_pico.html", ".html");
    let select_output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "start_tag",
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
                .is_some_and(|text| text.starts_with("<main"))
        })
        .and_then(|handle| handle["identity"].as_str())
        .expect("main start_tag identity should be present");

    let replacement = "<main class=\"container layout-grid\">";
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
    assert!(modified.contains("<main class=\"container layout-grid\">"));
}

#[test]
fn select_covers_complex_html_fixture_special_tags() {
    let html_file = fixture_path("complex_bootstrap_dashboard.html");
    let output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "start_tag",
        html_file.to_str().expect("path should be utf-8"),
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
                .is_some_and(|text| text.starts_with("<template "))
        }),
        "complex fixture should include a template start tag"
    );
    assert!(
        handles.iter().any(|handle| handle["text"]
            .as_str()
            .is_some_and(|text| text == "<dialog id=\"release-dialog\">")),
        "complex fixture should include dialog start tag"
    );
    assert!(
        handles.iter().any(|handle| handle["text"]
            .as_str()
            .is_some_and(|text| text == "<details>")),
        "complex fixture should include details start tag"
    );
}

#[test]
fn transform_replace_and_apply_support_complex_html_main_rewrite() {
    let file_path = copy_fixture_to_temp("complex_bootstrap_dashboard.html", ".html");
    let select_output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "start_tag",
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
                .is_some_and(|text| text.starts_with("<main id=\"dashboard-main\""))
        })
        .and_then(|handle| handle["identity"].as_str())
        .expect("dashboard main start_tag identity should be present");

    let replacement = "<main id=\"dashboard-main\" class=\"container-fluid py-3 has-grid\">";
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
    assert!(modified.contains("has-grid"));
}

#[test]
fn transform_reports_ambiguous_target_for_duplicate_html_element_identity() {
    let file_path = copy_fixture_to_temp("complex_bootstrap_dashboard.html", ".html");
    let select_output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "element",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        select_output.status.success(),
        "select failed: {}",
        String::from_utf8_lossy(&select_output.stderr)
    );

    let select_response: Value =
        serde_json::from_slice(&select_output.stdout).expect("stdout should be valid JSON");
    let li_identity = select_response["handles"]
        .as_array()
        .expect("handles should be an array")
        .iter()
        .find(|handle| {
            handle["text"].as_str().is_some_and(|text| {
                text == "<li class=\"nav-item\"><a href=\"#overview\">Overview</a></li>"
            })
        })
        .and_then(|handle| handle["identity"].as_str())
        .expect("expected duplicate li identity in fixture");

    let transform_output = run_identedit(&[
        "transform",
        "--identity",
        li_identity,
        "--replace",
        "<li class=\"nav-item\"><a href=\"#overview\">Overview (Updated)</a></li>",
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
fn select_covers_webapp_html_fixture_script_style_template_nodes() {
    let html_file = fixture_path("complex_vite_like_webapp.html");
    let output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "start_tag",
        html_file.to_str().expect("path should be utf-8"),
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
        handles.iter().any(|handle| handle["text"] == "<style>"),
        "fixture should include style start tag"
    );
    assert!(
        handles.iter().any(|handle| {
            handle["text"]
                .as_str()
                .is_some_and(|text| text.starts_with("<script type=\"module\""))
        }),
        "fixture should include module script start tag"
    );
    assert!(
        handles
            .iter()
            .any(|handle| handle["text"] == "<template id=\"task-item-template\">"),
        "fixture should include template start tag"
    );
}

#[test]
fn transform_replace_and_apply_support_webapp_html_app_main_rewrite() {
    let file_path = copy_fixture_to_temp("complex_vite_like_webapp.html", ".html");
    let select_output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "start_tag",
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
                .is_some_and(|text| text.starts_with("<main id=\"app-main\""))
        })
        .and_then(|handle| handle["identity"].as_str())
        .expect("app main start_tag identity should be present");

    let replacement = "<main id=\"app-main\" data-e2e=\"edge-hunt\" class=\"app-shell\">";
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
    assert!(modified.contains("data-e2e=\"edge-hunt\""));
}

#[test]
fn transform_reports_ambiguous_target_for_duplicate_webapp_html_list_item_identity() {
    let file_path = copy_fixture_to_temp("complex_vite_like_webapp.html", ".html");
    let select_output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "element",
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
                text == "<li class=\"queue-item\"><button type=\"button\">Retry</button></li>"
            })
        })
        .and_then(|handle| handle["identity"].as_str())
        .expect("expected duplicate list item identity in fixture");

    let transform_output = run_identedit(&[
        "transform",
        "--identity",
        duplicate_identity,
        "--replace",
        "<li class=\"queue-item\"><button type=\"button\">Retry now</button></li>",
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
fn transform_reports_ambiguous_target_for_stress_html_duplicate_identity_set() {
    let file_path = copy_fixture_to_temp("stress_html_duplicate_sections.html", ".html");
    let select_output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "element",
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
                text == "<li class=\"dup-item\"><a href=\"#retry\">Retry</a></li>"
            })
        })
        .and_then(|handle| handle["identity"].as_str())
        .expect("expected duplicate stress identity in fixture");

    let transform_output = run_identedit(&[
        "transform",
        "--identity",
        duplicate_identity,
        "--replace",
        "<li class=\"dup-item\"><a href=\"#retry\">Retry now</a></li>",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !transform_output.status.success(),
        "transform should fail for high-duplication ambiguous identity"
    );

    let response: Value =
        serde_json::from_slice(&transform_output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "ambiguous_target");
}

#[test]
fn transform_replace_and_apply_support_minified_html_fixture_main_rewrite() {
    let file_path = copy_fixture_to_temp("minified_dashboard.html", ".html");
    let select_output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "start_tag",
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
                .is_some_and(|text| text.starts_with("<main id=\"mini-main\""))
        })
        .and_then(|handle| handle["identity"].as_str())
        .expect("minified main start_tag identity should be present");

    let replacement = "<main id=\"mini-main\" class=\"grid compact\" data-round=\"r1\">";
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
    assert!(modified.contains("data-round=\"r1\""));
}

#[test]
fn transform_json_span_hint_disambiguates_stress_html_duplicate_identity() {
    let file_path = copy_fixture_to_temp("stress_html_duplicate_sections.html", ".html");
    let select_output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "element",
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
        .filter(|handle| {
            handle["text"].as_str().is_some_and(|text| {
                text == "<li class=\"dup-item\"><a href=\"#retry\">Retry</a></li>"
            })
        })
        .collect::<Vec<_>>();
    assert!(
        duplicate_handles.len() >= 2,
        "fixture must contain multiple duplicate identities for span_hint disambiguation"
    );

    let target = duplicate_handles[0];
    let span = &target["span"];
    let start = span["start"].as_u64().expect("span start");
    let end = span["end"].as_u64().expect("span end");
    let identity = target["identity"]
        .as_str()
        .expect("identity should be present");
    let expected_old_hash = target["expected_old_hash"]
        .as_str()
        .expect("expected_old_hash should be present");
    let old_text = target["text"].as_str().expect("text should be string");
    let replacement = "<li class=\"dup-item\"><a href=\"#retry\">Retry now</a></li>";

    let request = json!({
        "command": "transform",
        "file": file_path.to_str().expect("path should be utf-8"),
        "operations": [
            {
                "target": {
                    "identity": identity,
                    "kind": "element",
                    "expected_old_hash": expected_old_hash,
                    "span_hint": {"start": start, "end": end}
                },
                "op": {"type": "replace", "new_text": replacement}
            }
        ]
    });

    let transform_output = run_identedit_with_stdin(&["transform", "--json"], &request.to_string());
    assert!(
        transform_output.status.success(),
        "transform --json should disambiguate duplicate identity with span_hint: {}",
        String::from_utf8_lossy(&transform_output.stderr)
    );

    let transform_response: Value =
        serde_json::from_slice(&transform_output.stdout).expect("stdout should be valid JSON");
    let preview = &transform_response["files"][0]["operations"][0]["preview"];
    assert!(
        preview.get("old_text").is_none(),
        "compact preview should omit old_text by default"
    );
    assert_eq!(
        preview["old_hash"],
        identedit::changeset::hash_text(old_text),
        "compact preview old_hash should match selected duplicate node text"
    );
    assert_eq!(
        preview["old_len"],
        old_text.len(),
        "compact preview old_len should match selected duplicate node length"
    );
    assert_eq!(preview["matched_span"]["start"], start);
    assert_eq!(preview["matched_span"]["end"], end);

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
        modified.matches("Retry now").count(),
        1,
        "span_hint disambiguation should edit exactly one duplicate node"
    );
}

#[test]
fn select_handles_large_minified_single_line_html_without_regression() {
    let mut source = String::from(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><title>huge</title></head><body><main id=\"mini-main\">",
    );
    for index in 0..300 {
        source.push_str(&format!(
            "<section class=\"card\"><h2>item-{index}</h2><p>ok</p></section>"
        ));
    }
    source.push_str("</main></body></html>");

    let file_path = write_temp_source(".html", &source);
    let output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "start_tag",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "select should support large minified html: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    let handles = response["handles"]
        .as_array()
        .expect("handles should be an array");
    assert!(
        handles.len() >= 600,
        "expected many tags from single-line html stress fixture"
    );
}

#[test]
fn transform_json_duplicate_html_identity_with_missed_span_hint_returns_ambiguous_target() {
    let file_path = copy_fixture_to_temp("stress_html_duplicate_sections.html", ".html");
    let select_output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "element",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        select_output.status.success(),
        "select failed: {}",
        String::from_utf8_lossy(&select_output.stderr)
    );

    let select_response: Value =
        serde_json::from_slice(&select_output.stdout).expect("stdout should be valid JSON");
    let duplicate_handle = select_response["handles"]
        .as_array()
        .expect("handles should be an array")
        .iter()
        .find(|handle| {
            handle["text"].as_str().is_some_and(|text| {
                text == "<li class=\"dup-item\"><a href=\"#retry\">Retry</a></li>"
            })
        })
        .expect("expected duplicate handle in stress fixture");

    let identity = duplicate_handle["identity"]
        .as_str()
        .expect("identity should be present");
    let expected_old_hash = duplicate_handle["expected_old_hash"]
        .as_str()
        .expect("expected_old_hash should be present");
    let request = json!({
        "command": "transform",
        "file": file_path.to_str().expect("path should be utf-8"),
        "operations": [
            {
                "target": {
                    "identity": identity,
                    "kind": "element",
                    "expected_old_hash": expected_old_hash,
                    "span_hint": {"start": 1, "end": 2}
                },
                "op": {"type": "replace", "new_text": "<li class=\"dup-item\"><a href=\"#retry\">Retry alt</a></li>"}
            }
        ]
    });

    let transform_output = run_identedit_with_stdin(&["transform", "--json"], &request.to_string());
    assert!(
        !transform_output.status.success(),
        "transform --json should fail with ambiguous_target on stale span_hint for duplicate identity"
    );

    let response: Value =
        serde_json::from_slice(&transform_output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "ambiguous_target");
}

#[test]
fn select_response_contains_expected_old_hash_for_vite_like_html_fixture() {
    let html_file = fixture_path("complex_vite_like_webapp.html");
    let output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "start_tag",
        html_file.to_str().expect("path should be utf-8"),
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
        "fixture should produce start_tag handles"
    );
    assert!(
        handles.iter().all(|handle| {
            handle["expected_old_hash"]
                .as_str()
                .is_some_and(|hash| !hash.is_empty())
        }),
        "every selected handle should provide non-empty expected_old_hash"
    );
}

#[test]
fn transform_json_span_hint_can_target_second_duplicate_html_node() {
    let file_path = copy_fixture_to_temp("stress_html_duplicate_sections.html", ".html");
    let select_output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "element",
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
        .filter(|handle| {
            handle["text"].as_str().is_some_and(|text| {
                text == "<li class=\"dup-item\"><a href=\"#retry\">Retry</a></li>"
            })
        })
        .collect::<Vec<_>>();
    assert!(
        duplicate_handles.len() >= 2,
        "fixture must contain multiple duplicate identities"
    );

    let target = duplicate_handles[1];
    let span = &target["span"];
    let start = span["start"].as_u64().expect("span start");
    let end = span["end"].as_u64().expect("span end");
    let request = json!({
        "command": "transform",
        "file": file_path.to_str().expect("path should be utf-8"),
        "operations": [
            {
                "target": {
                    "identity": target["identity"],
                    "kind": "element",
                    "expected_old_hash": target["expected_old_hash"],
                    "span_hint": {"start": start, "end": end}
                },
                "op": {"type": "replace", "new_text": "<li class=\"dup-item\"><a href=\"#retry\">Retry second</a></li>"}
            }
        ]
    });

    let transform_output = run_identedit_with_stdin(&["transform", "--json"], &request.to_string());
    assert!(
        transform_output.status.success(),
        "transform --json should disambiguate second duplicate with span_hint: {}",
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
        modified.matches("Retry second").count(),
        1,
        "span_hint should allow editing a specific duplicate occurrence"
    );
}

#[test]
fn select_json_mode_multi_file_html_scan_returns_flat_handles() {
    let file_a = copy_fixture_to_temp("complex_vite_like_webapp.html", ".html");
    let file_b = copy_fixture_to_temp("minified_dashboard.html", ".html");
    let request = json!({
        "command": "select",
        "selector": {"kind": "start_tag"},
        "files": [
            file_a.to_str().expect("path should be utf-8"),
            file_b.to_str().expect("path should be utf-8")
        ]
    });

    let output = run_identedit_with_stdin(&["select", "--verbose", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "select --json should succeed for multi-file html scan: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["summary"]["files_scanned"], 2);
    let handles = response["handles"]
        .as_array()
        .expect("handles should be an array");
    assert!(
        handles
            .iter()
            .any(|handle| handle["file"] == file_a.to_str().expect("utf-8 path")),
        "flat handle list should include entries from first file"
    );
    assert!(
        handles
            .iter()
            .any(|handle| handle["file"] == file_b.to_str().expect("utf-8 path")),
        "flat handle list should include entries from second file"
    );
}

#[test]
fn transform_json_stale_hash_on_minified_html_returns_precondition_failed() {
    let file_path = copy_fixture_to_temp("minified_dashboard.html", ".html");
    let select_output = run_identedit(&[
        "select",
        "--verbose",
        "--kind",
        "start_tag",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        select_output.status.success(),
        "select failed: {}",
        String::from_utf8_lossy(&select_output.stderr)
    );

    let select_response: Value =
        serde_json::from_slice(&select_output.stdout).expect("stdout should be valid JSON");
    let handle = select_response["handles"]
        .as_array()
        .expect("handles should be an array")
        .iter()
        .find(|entry| {
            entry["text"]
                .as_str()
                .is_some_and(|text| text.starts_with("<main id=\"mini-main\""))
        })
        .expect("minified main start_tag handle should exist");
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
                "op": {"type": "replace", "new_text": "<main id=\"mini-main\" class=\"stale\">"}
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
fn select_json_mode_rejects_duplicate_html_file_entries() {
    let file_path = copy_fixture_to_temp("complex_vite_like_webapp.html", ".html");
    let request = json!({
        "command": "select",
        "selector": {"kind": "start_tag"},
        "files": [
            file_path.to_str().expect("path should be utf-8"),
            file_path.to_str().expect("path should be utf-8")
        ]
    });

    let output = run_identedit_with_stdin(&["select", "--verbose", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "select --json should reject duplicate file entries"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}
