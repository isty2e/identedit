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
    command.args(arguments);
    command.output().expect("failed to run identedit binary")
}

fn run_identedit_with_stdin(arguments: &[&str], input: &str) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_identedit"));
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

fn assert_select_kind_contains_text(file: &Path, kind: &str, snippet: &str) {
    let output = run_identedit(&[
        "read",
        "--json",
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
    assert!(
        handles.iter().any(|handle| {
            handle["text"]
                .as_str()
                .is_some_and(|text| text.contains(snippet))
        }),
        "expected at least one {kind} handle containing '{snippet}'"
    );
}

#[test]
fn select_covers_xml_kinds_and_provider() {
    let xml_file = fixture_path("example.xml");

    assert_select_kind_contains_text(&xml_file, "document", "<catalog>");
    assert_select_kind_contains_text(&xml_file, "element", "<name>Identedit</name>");
    assert_select_kind_contains_text(&xml_file, "Attribute", "id=\"a\"");
    assert_select_kind_contains_text(&xml_file, "Comment", "standard item");
    assert_select_kind_contains_text(&xml_file, "CDSect", "Agent <safe> content");
    assert_select_kind_contains_text(&xml_file, "StyleSheetPI", "xml-stylesheet");
}

#[test]
fn select_supports_case_insensitive_xml_extension() {
    let file_path = copy_fixture_to_temp("example.xml", ".XML");
    assert_select_kind_contains_text(&file_path, "element", "<name>Identedit</name>");
}

#[test]
fn select_supports_utf8_bom_prefixed_xml_files() {
    let fixture = fs::read(fixture_path("example.xml")).expect("fixture should be readable");
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(&fixture);
    let file_path = write_temp_bytes(".xml", &bytes);

    assert_select_kind_contains_text(&file_path, "element", "<name>Identedit</name>");
}

#[test]
fn select_reports_parse_failure_for_syntax_invalid_xml() {
    let file_path = write_temp_source(
        ".xml",
        "<?xml version=\"1.0\"?><root><item><name>broken</name></root>",
    );
    let output = run_identedit(&[
        "read",
        "--json",
        "--verbose",
        "--kind",
        "element",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "syntax-invalid xml should fail under the xml provider"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "parse_failure");
    let message = response["error"]["message"]
        .as_str()
        .expect("error.message should be a string");
    assert!(message.contains("tree-sitter-xml"));
    assert!(message.contains("Syntax errors detected in XML source"));
}

#[test]
fn transform_replace_and_apply_support_xml_element() {
    let file_path = copy_fixture_to_temp("example.xml", ".xml");
    let select_output = run_identedit(&[
        "read",
        "--json",
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
    let identity = select_response["handles"]
        .as_array()
        .expect("handles should be an array")
        .iter()
        .find(|handle| {
            handle["text"].as_str().is_some_and(|text| {
                text.contains("<description><![CDATA[Agent <safe> content]]></description>")
            })
        })
        .and_then(|handle| handle["identity"].as_str())
        .expect("description element identity should be present");

    let replacement = "<description><![CDATA[Agent <safe> updated]]></description>";
    let transform_output = run_identedit(&[
        "edit",
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
    assert!(modified.contains("<![CDATA[Agent <safe> updated]]>"));
}

#[test]
fn transform_reports_ambiguous_target_for_duplicate_xml_element_identity() {
    let source = "<root><entry>same</entry><entry>same</entry></root>";
    let file_path = write_temp_source(".xml", source);
    let select_output = run_identedit(&[
        "read",
        "--json",
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
                .filter(|handle| handle["identity"] == *identity)
                .count()
                >= 2
        })
        .expect("fixture should include duplicate element identity");

    let output = run_identedit(&[
        "edit",
        "--identity",
        duplicate_identity,
        "--replace",
        "<entry>updated</entry>",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "transform should fail for ambiguous duplicate XML element identity"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "ambiguous_target");
}

#[test]
fn transform_json_span_hint_disambiguates_duplicate_xml_element_identity() {
    let source = "<root><entry>same</entry><entry>same</entry></root>";
    let file_path = write_temp_source(".xml", source);
    let select_output = run_identedit(&[
        "read",
        "--json",
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
                .filter(|handle| handle["identity"].as_str() == Some(*identity))
                .count()
                >= 2
        })
        .expect("fixture should include duplicate element identity");
    let duplicate_handles = handles
        .iter()
        .filter(|handle| handle["identity"].as_str() == Some(duplicate_identity))
        .collect::<Vec<_>>();
    assert!(
        duplicate_handles.len() >= 2,
        "fixture should include duplicate element handles"
    );

    let target = duplicate_handles[1];
    let span = &target["span"];
    let request = json!({
        "command": "edit",
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
                "new_text": "<entry>updated</entry>"
            }
        }]
    });
    let request_body = serde_json::to_string(&request).expect("request should serialize");

    let transform_output = run_identedit_with_stdin(&["edit", "--json"], &request_body);
    assert!(
        transform_output.status.success(),
        "transform --json should disambiguate duplicate XML element identity: {}",
        String::from_utf8_lossy(&transform_output.stderr)
    );

    let transform_json =
        std::str::from_utf8(&transform_output.stdout).expect("transform output should be utf-8");
    let apply_output = run_identedit_with_stdin(&["apply"], transform_json);
    assert!(
        apply_output.status.success(),
        "apply failed after XML span_hint disambiguation: {}",
        String::from_utf8_lossy(&apply_output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("file should be readable");
    assert_eq!(modified.matches("<entry>updated</entry>").count(), 1);
    assert_eq!(modified.matches("<entry>same</entry>").count(), 1);
}

#[test]
fn transform_json_duplicate_xml_identity_with_missed_span_hint_returns_ambiguous_target() {
    let source = "<root><entry>same</entry><entry>same</entry></root>";
    let file_path = write_temp_source(".xml", source);
    let select_output = run_identedit(&[
        "read",
        "--json",
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
                .filter(|handle| handle["identity"].as_str() == Some(*identity))
                .count()
                >= 2
        })
        .expect("fixture should include duplicate element identity");
    let target = handles
        .iter()
        .find(|handle| handle["identity"].as_str() == Some(duplicate_identity))
        .expect("duplicate element handle should be present");

    let request = json!({
        "command": "edit",
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
                "new_text": "<entry>updated</entry>"
            }
        }]
    });
    let request_body = serde_json::to_string(&request).expect("request should serialize");

    let output = run_identedit_with_stdin(&["edit", "--json"], &request_body);
    assert!(
        !output.status.success(),
        "transform --json should fail when span_hint misses duplicate XML targets"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "ambiguous_target");
}

#[test]
fn apply_reports_stale_target_error_after_xml_source_mutation() {
    let file_path = copy_fixture_to_temp("example.xml", ".xml");
    let select_output = run_identedit(&[
        "read",
        "--json",
        "--verbose",
        "--kind",
        "StyleSheetPI",
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
        .expect("stylesheet PI identity should be present");

    let replacement = "<?xml-stylesheet type=\"text/xsl\" href=\"theme.xsl\"?>";
    let transform_output = run_identedit(&[
        "edit",
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

    let original = fs::read_to_string(&file_path).expect("file should be readable");
    let mutated = original.replace("xml-stylesheet", "xml-theme");
    fs::write(&file_path, mutated).expect("mutated source write should succeed");

    let transform_json =
        std::str::from_utf8(&transform_output.stdout).expect("transform output should be utf-8");
    let apply_output = run_identedit_with_stdin(&["apply"], transform_json);
    assert!(
        !apply_output.status.success(),
        "apply should fail when XML source changes after transform"
    );

    let response: Value =
        serde_json::from_slice(&apply_output.stdout).expect("stdout should be valid JSON");
    let error_type = response["error"]["type"]
        .as_str()
        .expect("error.type should be a string");
    assert!(
        error_type == "precondition_failed" || error_type == "target_missing",
        "expected stale XML apply to fail with precondition_failed or target_missing, got {error_type}"
    );
}

#[test]
fn select_ignores_empty_tag_tokens_inside_comments_and_cdata() {
    let source = "<root><!-- <ghost/> --><![CDATA[<ghost/>]]><real/></root>";
    let file_path = write_temp_source(".xml", source);
    let output = run_identedit(&[
        "read",
        "--json",
        "--verbose",
        "--kind",
        "EmptyElemTag",
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

    assert_eq!(
        handles.len(),
        1,
        "only the real self-closing tag should be parsed as EmptyElemTag"
    );
    assert_eq!(handles[0]["text"], "<real/>");
}

#[test]
fn transform_replace_and_apply_preserve_crlf_xml_source_segments() {
    let source = "<?xml version=\"1.0\"?>\r\n<root>\r\n  <value>old</value>\r\n</root>\r\n";
    let file_path = write_temp_source(".xml", source);
    let select_output = run_identedit(&[
        "read",
        "--json",
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
    let identity = select_response["handles"]
        .as_array()
        .expect("handles should be an array")
        .iter()
        .find(|handle| {
            handle["text"]
                .as_str()
                .is_some_and(|text| text.contains("<value>old</value>"))
        })
        .and_then(|handle| handle["identity"].as_str())
        .expect("value element identity should be present");

    let replacement = "<value>new</value>";
    let transform_output = run_identedit(&[
        "edit",
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
    assert!(modified.contains("\r\n"));
    assert!(modified.contains("<value>new</value>"));
}
