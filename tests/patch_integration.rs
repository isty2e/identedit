use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::Output;

use serde_json::{Value, json};
use tempfile::Builder;

mod common;

fn run_identedit(args: &[&str]) -> Output {
    common::run_identedit(args)
}

fn run_identedit_with_stdin(args: &[&str], input: &str) -> Output {
    common::run_identedit_with_stdin(args, input)
}

fn copy_fixture_to_temp_python(name: &str) -> std::path::PathBuf {
    common::copy_fixture_to_temp_python(name)
}

fn copy_fixture_to_temp_json(name: &str) -> std::path::PathBuf {
    common::copy_fixture_to_temp_json(name)
}

fn copy_fixture_to_temp_with_suffix(name: &str, suffix: &str) -> std::path::PathBuf {
    let source = common::fixture_path(name);
    let content = fs::read_to_string(&source).expect("fixture should be readable");
    let mut temp_file = Builder::new()
        .suffix(suffix)
        .tempfile()
        .expect("temp file should be created");
    temp_file
        .write_all(content.as_bytes())
        .expect("temp fixture write should succeed");
    temp_file.keep().expect("temp file should persist").1
}

fn select_named_function_handle(file: &Path, pattern: &str) -> Value {
    common::select_first_handle(file, "function_definition", Some(pattern))
}

fn create_scoped_regex_fixture() -> std::path::PathBuf {
    let mut temp_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp python file should be created");
    temp_file
        .write_all(
            b"def process_data(value):\n    return value + 1\n\n\ndef helper(value):\n    return value + 2\n",
        )
        .expect("fixture write should succeed");
    temp_file.keep().expect("temp file should persist").1
}

fn create_temp_python_source(content: &str) -> std::path::PathBuf {
    let mut temp_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp python file should be created");
    temp_file
        .write_all(content.as_bytes())
        .expect("temp python fixture write should succeed");
    temp_file.keep().expect("temp file should persist").1
}

fn create_temp_text_file(content: &str) -> std::path::PathBuf {
    let mut temp_file = Builder::new()
        .suffix(".txt")
        .tempfile()
        .expect("temp text file should be created");
    temp_file
        .write_all(content.as_bytes())
        .expect("temp text file write should succeed");
    temp_file.keep().expect("temp text file should persist").1
}

fn create_temp_yaml_source(content: &str) -> std::path::PathBuf {
    let mut temp_file = Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("temp yaml file should be created");
    temp_file
        .write_all(content.as_bytes())
        .expect("temp yaml file write should succeed");
    temp_file.keep().expect("temp yaml file should persist").1
}

fn patch_yaml_config_path(
    file_path: &Path,
    raw_path: &str,
    new_text: &str,
    create_missing: bool,
) -> Output {
    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "config_path",
            "path": raw_path
        },
        "op": {
            "type": "set",
            "new_text": new_text,
            "create_missing": create_missing
        }
    });

    run_identedit_with_stdin(&["patch", "--json"], &request.to_string())
}

fn create_temp_binary_file(bytes: &[u8]) -> std::path::PathBuf {
    let mut temp_file = Builder::new()
        .suffix(".bin")
        .tempfile()
        .expect("temp binary file should be created");
    temp_file
        .write_all(bytes)
        .expect("temp binary file write should succeed");
    temp_file.keep().expect("temp binary file should persist").1
}

fn line_ref(source: &str, line: usize) -> String {
    let line_text = source
        .lines()
        .nth(line - 1)
        .expect("line should exist for anchor");
    let hash = identedit::hashline::compute_line_hash(line_text);
    format!("{line}:{hash}")
}

#[path = "patch_integration/config_path_append_delete.rs"]
mod config_path_append_delete;
#[path = "patch_integration/config_path_core.rs"]
mod config_path_core;
#[path = "patch_integration/config_path_json.rs"]
mod config_path_json;
#[path = "patch_integration/config_path_toml_basic.rs"]
mod config_path_toml_basic;
#[path = "patch_integration/config_path_toml_comments.rs"]
mod config_path_toml_comments;
#[path = "patch_integration/config_path_toml_conflicts.rs"]
mod config_path_toml_conflicts;
#[path = "patch_integration/config_path_toml_quoted_keys.rs"]
mod config_path_toml_quoted_keys;
#[path = "patch_integration/config_path_toml_value_fragments.rs"]
mod config_path_toml_value_fragments;
#[path = "patch_integration/config_path_toml_whitespace.rs"]
mod config_path_toml_whitespace;
#[path = "patch_integration/config_path_yaml_block_scalars.rs"]
mod config_path_yaml_block_scalars;
#[path = "patch_integration/config_path_yaml_comment_block_scalars.rs"]
mod config_path_yaml_comment_block_scalars;
#[path = "patch_integration/config_path_yaml_comment_key_quoting.rs"]
mod config_path_yaml_comment_key_quoting;
#[path = "patch_integration/config_path_yaml_comment_rejections.rs"]
mod config_path_yaml_comment_rejections;
#[path = "patch_integration/config_path_yaml_comment_structure.rs"]
mod config_path_yaml_comment_structure;
#[path = "patch_integration/config_path_yaml_create.rs"]
mod config_path_yaml_create;
#[path = "patch_integration/config_path_yaml_existing_core.rs"]
mod config_path_yaml_existing_core;
#[path = "patch_integration/config_path_yaml_existing_delete.rs"]
mod config_path_yaml_existing_delete;
#[path = "patch_integration/config_path_yaml_existing_scalar_safety.rs"]
mod config_path_yaml_existing_scalar_safety;
#[path = "patch_integration/config_path_yaml_flow.rs"]
mod config_path_yaml_flow;
#[path = "patch_integration/config_path_yaml_multidoc.rs"]
mod config_path_yaml_multidoc;
#[path = "patch_integration/config_path_yaml_no_comment_create.rs"]
mod config_path_yaml_no_comment_create;
#[path = "patch_integration/config_path_yaml_text_file.rs"]
mod config_path_yaml_text_file;
#[path = "patch_integration/file_targets.rs"]
mod file_targets;
#[path = "patch_integration/flag_validation.rs"]
mod flag_validation;
#[path = "patch_integration/json_mode.rs"]
mod json_mode;
#[path = "patch_integration/line_targets.rs"]
mod line_targets;
#[path = "patch_integration/node_targets.rs"]
mod node_targets;
#[path = "patch_integration/symbol_targets.rs"]
mod symbol_targets;
#[path = "patch_integration/text_sources.rs"]
mod text_sources;
