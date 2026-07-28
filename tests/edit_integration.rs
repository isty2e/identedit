#[cfg(unix)]
use std::ffi::OsString;
use std::fs;
use std::io::Write;
#[cfg(unix)]
use std::os::unix::ffi::OsStringExt;
use std::path::Path;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tempfile::{Builder, tempdir};

mod common;

fn fixture_path(name: &str) -> PathBuf {
    common::fixture_path(name)
}

fn copy_fixture_to_temp_python(name: &str) -> PathBuf {
    common::copy_fixture_to_temp_python(name)
}

fn copy_fixture_to_temp_json(name: &str) -> PathBuf {
    common::copy_fixture_to_temp_json(name)
}

fn run_identedit(args: &[&str]) -> Output {
    common::run_identedit(args)
}

#[cfg(unix)]
fn run_shell_script(script: &str, root: &Path, identity: Option<&str>) -> Output {
    common::run_shell_script(script, root, identity)
}

fn run_identedit_with_stdin(args: &[&str], input: &str) -> Output {
    common::run_identedit_with_stdin(args, input)
}

fn select_first_handle(file: &Path, kind: &str, name: Option<&str>) -> Value {
    common::select_first_handle(file, kind, name)
}

fn create_large_python_file(function_count: usize) -> PathBuf {
    common::create_large_python_file(function_count)
}

fn assert_compact_preview_old_state(preview: &Value, expected_old_text: &str) {
    assert!(
        preview.get("old_text").is_none(),
        "compact preview should omit old_text by default"
    );
    assert_eq!(
        preview["old_hash"],
        crate::common::hash_text(expected_old_text),
        "compact preview should include old_hash"
    );
    assert_eq!(
        preview["old_len"],
        expected_old_text.len(),
        "compact preview should include old_len"
    );
}

#[path = "edit_integration/boundary_conflicts.rs"]
mod boundary_conflicts;
#[path = "edit_integration/flag_mode_and_resolution.rs"]
mod flag_mode_and_resolution;
#[path = "edit_integration/preview_generation.rs"]
mod preview_generation;
#[path = "edit_integration/request_validation.rs"]
mod request_validation;
#[path = "edit_integration/resolution_and_filesystem.rs"]
mod resolution_and_filesystem;
#[path = "edit_integration/structural_moves.rs"]
mod structural_moves;
