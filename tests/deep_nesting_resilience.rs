use std::io::Write;
use std::path::Path;
use std::process::{Command, Output};

use serde_json::Value;
use tempfile::Builder;

fn run_select(kind: &str, file: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_identedit"))
        .env("IDENTEDIT_ALLOW_LEGACY", "1")
        .args([
            "select",
            "--kind",
            kind,
            file.to_str().expect("path should be utf-8"),
        ])
        .output()
        .expect("failed to run identedit binary")
}

fn parse_structured_output(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).expect("stdout should always be structured JSON")
}

#[test]
fn deeply_nested_json_select_never_panics() {
    for depth in [
        16usize, 64, 128, 256, 384, 768, 1024, 1536, 2048, 3072, 4096,
    ] {
        let mut temp_file = Builder::new()
            .suffix(".json")
            .tempfile()
            .expect("temp json file should be created");
        let source = format!("{}0{}", "[".repeat(depth), "]".repeat(depth));
        temp_file
            .write_all(source.as_bytes())
            .expect("nested json fixture write should succeed");

        let output = run_select("array", temp_file.path());
        let response = parse_structured_output(&output);
        if output.status.success() {
            assert_eq!(
                response["summary"]["files_scanned"], 1,
                "select should report one scanned file at depth {depth}"
            );
        } else {
            let error_type = response["error"]["type"]
                .as_str()
                .expect("error.type should be a string");
            assert!(
                matches!(error_type, "parse_failure" | "invalid_request"),
                "unexpected error type for nested JSON depth {depth}: {error_type}"
            );
        }
    }
}

#[test]
fn deeply_nested_python_select_never_panics() {
    for depth in [
        16usize, 64, 128, 256, 384, 768, 1024, 1536, 2048, 3072, 4096,
    ] {
        let mut temp_file = Builder::new()
            .suffix(".py")
            .tempfile()
            .expect("temp python file should be created");
        let source = format!("value = {}1{}\n", "(".repeat(depth), ")".repeat(depth));
        temp_file
            .write_all(source.as_bytes())
            .expect("nested python fixture write should succeed");

        let output = run_select("module", temp_file.path());
        let response = parse_structured_output(&output);
        if output.status.success() {
            assert_eq!(
                response["summary"]["files_scanned"], 1,
                "select should report one scanned file at depth {depth}"
            );
        } else {
            let error_type = response["error"]["type"]
                .as_str()
                .expect("error.type should be a string");
            assert!(
                matches!(error_type, "parse_failure" | "invalid_request"),
                "unexpected error type for nested Python depth {depth}: {error_type}"
            );
        }
    }
}
