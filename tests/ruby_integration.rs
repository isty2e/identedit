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

fn assert_select_kind_and_optional_name(file: &Path, kind: &str, expected_name: Option<&str>) {
    let output = run_identedit(&[
        "read",
        "--json",
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
fn select_covers_ruby_kinds_and_provider() {
    let ruby_file = fixture_path("example.rb");

    assert_select_kind_and_optional_name(&ruby_file, "module", Some("ExampleApp"));
    assert_select_kind_and_optional_name(&ruby_file, "class", Some("ExampleService"));
    assert_select_kind_and_optional_name(&ruby_file, "method", Some("process_data"));
}

#[test]
fn select_supports_case_insensitive_ruby_extension() {
    let file_path = copy_fixture_to_temp("example.rb", ".RB");
    assert_select_kind_and_optional_name(&file_path, "method", Some("process_data"));
}

#[test]
fn select_supports_utf8_bom_prefixed_ruby_files() {
    let fixture = fs::read(fixture_path("example.rb")).expect("fixture should be readable");
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(&fixture);
    let file_path = write_temp_bytes(".rb", &bytes);

    assert_select_kind_and_optional_name(&file_path, "method", Some("process_data"));
}

#[test]
fn select_reports_parse_failure_for_syntax_invalid_ruby() {
    let file_path = write_temp_source(".rb", "class Broken\n  def run(value)\n    value + 1\n");
    let output = run_identedit(&[
        "read",
        "--json",
        "--kind",
        "class",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "syntax-invalid ruby should fail under the ruby provider"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "parse_failure");
    let message = response["error"]["message"]
        .as_str()
        .expect("error.message should be a string");
    assert!(message.contains("tree-sitter-ruby"));
    assert!(message.contains("Syntax errors detected in Ruby source"));
}

#[test]
fn transform_replace_and_apply_support_ruby_method() {
    let file_path = copy_fixture_to_temp("example.rb", ".rb");
    let select_output = run_identedit(&[
        "read",
        "--json",
        "--kind",
        "method",
        "--name",
        "process_data",
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

    let replacement = "def process_data(value)\n      value + VALUE + 2\n    end";
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
    assert!(modified.contains("value + VALUE + 2"));
}

#[test]
fn transform_reports_ambiguous_target_for_duplicate_ruby_method_identity() {
    let source = "class First\n  def configure(value)\n    value + 1\n  end\nend\n\nclass Second\n  def configure(value)\n    value + 1\n  end\nend\n";
    let file_path = write_temp_source(".rb", source);
    let select_output = run_identedit(&[
        "read",
        "--json",
        "--kind",
        "method",
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
        .expect("fixture should include duplicate method identity");

    let output = run_identedit(&[
        "edit",
        "--identity",
        duplicate_identity,
        "--replace",
        "def configure(value)\n    value + 2\n  end",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "transform should fail for ambiguous duplicate Ruby identity"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "ambiguous_target");
}

#[test]
fn transform_json_span_hint_disambiguates_duplicate_ruby_method_identity() {
    let source = "class First\n  def configure(value)\n    value + 1\n  end\nend\n\nclass Second\n  def configure(value)\n    value + 1\n  end\nend\n";
    let file_path = write_temp_source(".rb", source);
    let select_output = run_identedit(&[
        "read",
        "--json",
        "--kind",
        "method",
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
        "fixture should include at least two configure methods"
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
                "new_text": "def configure(value)\n    value + 2\n  end"
            }
        }]
    });
    let request_body = serde_json::to_string(&request).expect("request should serialize");

    let transform_output = run_identedit_with_stdin(&["edit", "--json"], &request_body);
    assert!(
        transform_output.status.success(),
        "transform --json should disambiguate duplicate Ruby method identity: {}",
        String::from_utf8_lossy(&transform_output.stderr)
    );

    let transform_json =
        std::str::from_utf8(&transform_output.stdout).expect("transform output should be utf-8");
    let apply_output = run_identedit_with_stdin(&["apply"], transform_json);
    assert!(
        apply_output.status.success(),
        "apply failed after Ruby span_hint disambiguation: {}",
        String::from_utf8_lossy(&apply_output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("file should be readable");
    assert_eq!(modified.matches("value + 2").count(), 1);
    assert_eq!(modified.matches("value + 1").count(), 1);
}

#[test]
fn transform_json_duplicate_ruby_identity_with_missed_span_hint_returns_ambiguous_target() {
    let source = "class First\n  def configure(value)\n    value + 1\n  end\nend\n\nclass Second\n  def configure(value)\n    value + 1\n  end\nend\n";
    let file_path = write_temp_source(".rb", source);
    let select_output = run_identedit(&[
        "read",
        "--json",
        "--kind",
        "method",
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
                "new_text": "def configure(value)\n    value + 2\n  end"
            }
        }]
    });
    let request_body = serde_json::to_string(&request).expect("request should serialize");

    let output = run_identedit_with_stdin(&["edit", "--json"], &request_body);
    assert!(
        !output.status.success(),
        "transform --json should fail when span_hint misses duplicate Ruby methods"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "ambiguous_target");
}

#[test]
fn apply_reports_precondition_failed_after_ruby_source_mutation() {
    let file_path = copy_fixture_to_temp("example.rb", ".rb");
    let select_output = run_identedit(&[
        "read",
        "--json",
        "--kind",
        "method",
        "--name",
        "process_data",
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
        "edit",
        "--identity",
        identity,
        "--replace",
        "def process_data(value)\n      value + VALUE + 5\n    end",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        transform_output.status.success(),
        "transform failed: {}",
        String::from_utf8_lossy(&transform_output.stderr)
    );

    let original = fs::read_to_string(&file_path).expect("file should be readable");
    let mutated = original.replace("value + VALUE", "value - VALUE");
    fs::write(&file_path, mutated).expect("mutated source write should succeed");

    let transform_json =
        std::str::from_utf8(&transform_output.stdout).expect("transform output should be utf-8");
    let apply_output = run_identedit_with_stdin(&["apply"], transform_json);
    assert!(
        !apply_output.status.success(),
        "apply should fail when Ruby source changes after transform"
    );

    let response: Value =
        serde_json::from_slice(&apply_output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "precondition_failed");
}

#[test]
fn select_ignores_method_like_tokens_inside_heredoc() {
    let source = "class Example\n  def real(value)\n    value + 1\n  end\n\n  DOC = <<~RUBY\n    def fake(value)\n      value + 2\n    end\n  RUBY\nend\n";
    let file_path = write_temp_source(".rb", source);
    let output = run_identedit(&[
        "read",
        "--json",
        "--kind",
        "method",
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
    let method_names = handles
        .iter()
        .filter_map(|handle| handle["name"].as_str())
        .collect::<Vec<_>>();

    assert!(
        method_names.contains(&"real"),
        "expected to find real method in parsed handles"
    );
    assert!(
        !method_names.contains(&"fake"),
        "heredoc content should not be parsed as a real method"
    );
}

#[test]
fn select_reports_parse_failure_for_unterminated_heredoc() {
    let file_path = write_temp_source(
        ".rb",
        "class Broken\n  DOC = <<~RUBY\n    this heredoc never terminates\n",
    );
    let output = run_identedit(&[
        "read",
        "--json",
        "--kind",
        "class",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "unterminated heredoc should fail under the ruby provider"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "parse_failure");
    let message = response["error"]["message"]
        .as_str()
        .expect("error.message should be a string");
    assert!(message.contains("tree-sitter-ruby"));
    assert!(message.contains("Syntax errors detected in Ruby source"));
}

#[test]
fn transform_replace_and_apply_preserve_crlf_ruby_source_segments() {
    let source = "class Example\r\n  def process_data(value)\r\n    value + 1\r\n  end\r\nend\r\n";
    let file_path = write_temp_source(".rb", source);
    let select_output = run_identedit(&[
        "read",
        "--json",
        "--kind",
        "method",
        "--name",
        "process_data",
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
        "edit",
        "--identity",
        identity,
        "--replace",
        "def process_data(value)\n    value + 2\n  end",
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
        modified.starts_with("class Example\r\n"),
        "expected untouched CRLF prefix to remain intact"
    );
    assert!(
        modified.contains("\r\nend\r\n"),
        "expected trailing CRLF segments outside replaced span to remain"
    );
}

#[test]
fn select_reports_parse_failure_for_nul_in_ruby_source() {
    let file_path = write_temp_bytes(
        ".rb",
        b"class Example\n\0def process_data(value)\n  value + 1\nend\n",
    );
    let output = run_identedit(&[
        "read",
        "--json",
        "--kind",
        "method",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "nul byte in ruby source should fail under the ruby provider"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "parse_failure");
}

#[test]
fn select_supports_singleton_method_kind() {
    let file_path = write_temp_source(
        ".rb",
        "class Builder\n  def self.build(value)\n    value + 1\n  end\nend\n",
    );
    let output = run_identedit(&[
        "read",
        "--json",
        "--kind",
        "singleton_method",
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
        !handles.is_empty(),
        "expected singleton_method handle for def self.build"
    );
}

#[test]
fn select_preserves_multiple_reopened_class_nodes() {
    let file_path = write_temp_source(
        ".rb",
        "class Service\n  def first\n    1\n  end\nend\n\nclass Service\n  def second\n    2\n  end\nend\n",
    );
    let output = run_identedit(&[
        "read",
        "--json",
        "--kind",
        "class",
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

    let service_classes = handles
        .iter()
        .filter(|handle| handle["name"] == "Service")
        .collect::<Vec<_>>();
    assert_eq!(
        service_classes.len(),
        2,
        "expected two distinct class nodes for reopened Service declarations"
    );
    assert_ne!(
        service_classes[0]["span"], service_classes[1]["span"],
        "reopened class declarations should keep distinct spans"
    );
}
