#![allow(dead_code)]

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use serde_json::Value;
use tempfile::Builder;

pub fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

pub fn copy_fixture_to_temp_python(name: &str) -> PathBuf {
    let source = fixture_path(name);
    let content = fs::read_to_string(&source).expect("fixture should be readable");
    let mut temp_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp python file should be created");
    temp_file
        .write_all(content.as_bytes())
        .expect("temp fixture write should succeed");
    temp_file.keep().expect("temp file should persist").1
}

pub fn copy_fixture_to_temp_json(name: &str) -> PathBuf {
    let source = fixture_path(name);
    let content = fs::read_to_string(&source).expect("fixture should be readable");
    let mut temp_file = Builder::new()
        .suffix(".json")
        .tempfile()
        .expect("temp json file should be created");
    temp_file
        .write_all(content.as_bytes())
        .expect("temp fixture write should succeed");
    temp_file.keep().expect("temp file should persist").1
}

pub fn run_identedit(args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_identedit"));
    command.env("IDENTEDIT_ALLOW_LEGACY", "1");
    command.args(args);
    command.output().expect("failed to run identedit binary")
}

pub fn run_identedit_with_stdin(args: &[&str], input: &str) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_identedit"));
    command.env("IDENTEDIT_ALLOW_LEGACY", "1");
    command.args(args);
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

#[cfg(unix)]
pub fn run_shell_script(script: &str, root: &Path, identity: Option<&str>) -> Output {
    let mut command = Command::new("sh");
    command.arg("-c").arg(script);
    command.env("IDENTEDIT_BIN", env!("CARGO_BIN_EXE_identedit"));
    command.env("IDENTEDIT_ALLOW_LEGACY", "1");
    command.env("IDENTEDIT_ROOT", root);
    if let Some(value) = identity {
        command.env("IDENTEDIT_IDENTITY", value);
    }
    command.output().expect("failed to run shell command")
}

pub fn select_first_handle(file: &Path, kind: &str, name: Option<&str>) -> Value {
    let mut args = vec!["select", "--verbose", "--kind", kind];
    if let Some(pattern) = name {
        args.push("--name");
        args.push(pattern);
    }
    args.push(file.to_str().expect("path should be utf-8"));

    let output = run_identedit(&args);
    assert!(
        output.status.success(),
        "select failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    response["handles"][0].clone()
}

pub fn create_large_python_file(function_count: usize) -> PathBuf {
    let mut content = String::new();
    for index in 0..function_count {
        content.push_str(&format!(
            "def function_{index:04}(value):\n    return value + {index}\n\n\n"
        ));
    }

    let mut temp_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp python file should be created");
    temp_file
        .write_all(content.as_bytes())
        .expect("large fixture write should succeed");
    temp_file.keep().expect("temp file should persist").1
}
