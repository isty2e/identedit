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

#[path = "transform_integration/scenario_01_flags_and_paths.rs"]
mod scenario_01_flags_and_paths;
#[path = "transform_integration/scenario_02_json_validation.rs"]
mod scenario_02_json_validation;
#[path = "transform_integration/scenario_03_preview_and_file_targets.rs"]
mod scenario_03_preview_and_file_targets;
#[path = "transform_integration/scenario_04_boundary_conflicts.rs"]
mod scenario_04_boundary_conflicts;
#[path = "transform_integration/scenario_05_fallback_and_misc.rs"]
mod scenario_05_fallback_and_misc;
