use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{Value, json};
use tempfile::tempdir;

fn run_identedit(arguments: &[&str], kanna_home: &Path) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_identedit"));
    command.args(arguments);
    command.env("IDENTEDIT_HOME", kanna_home);
    command.output().expect("failed to run identedit binary")
}

fn command_available(name: &str) -> bool {
    Command::new(name).arg("--version").output().is_ok()
}

fn locate_tree_sitter_json_source() -> PathBuf {
    let cargo_home = env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env::var_os("HOME").expect("HOME must be set")).join(".cargo")
        });
    let registry_src = cargo_home.join("registry").join("src");

    let mut source_dir = None;
    for hash_dir in
        fs::read_dir(&registry_src).expect("cargo registry source directory should exist")
    {
        let hash_dir = hash_dir.expect("hash directory entry should be readable");
        if !hash_dir
            .file_type()
            .expect("hash directory metadata should be readable")
            .is_dir()
        {
            continue;
        }

        for crate_dir in
            fs::read_dir(hash_dir.path()).expect("crate directories should be readable")
        {
            let crate_dir = crate_dir.expect("crate directory entry should be readable");
            let file_name = crate_dir.file_name();
            let file_name = file_name.to_string_lossy();
            if file_name.starts_with("tree-sitter-json-") {
                source_dir = Some(crate_dir.path());
                break;
            }
        }

        if source_dir.is_some() {
            break;
        }
    }

    source_dir.expect("tree-sitter-json crate source must exist in cargo registry")
}

fn prepare_local_grammar_repo(repo_dir: &Path) {
    let source_dir = locate_tree_sitter_json_source();
    let source_src_dir = source_dir.join("src");
    let destination_src_dir = repo_dir.join("src");
    let destination_tree_sitter_dir = destination_src_dir.join("tree_sitter");

    fs::create_dir_all(&destination_tree_sitter_dir)
        .expect("destination src directory should exist");
    fs::copy(
        source_src_dir.join("parser.c"),
        destination_src_dir.join("parser.c"),
    )
    .expect("parser.c should be copied");
    for header in ["parser.h", "array.h", "alloc.h"] {
        fs::copy(
            source_src_dir.join("tree_sitter").join(header),
            destination_tree_sitter_dir.join(header),
        )
        .expect("tree-sitter header should be copied");
    }
}

fn run_git(arguments: &[&str], cwd: &Path) {
    let output = Command::new("git")
        .args(arguments)
        .current_dir(cwd)
        .output()
        .expect("git command should start");
    assert!(
        output.status.success(),
        "git command failed: {}\n{}",
        arguments.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn initialize_git_repository(path: &Path) {
    run_git(&["init"], path);
    run_git(&["add", "."], path);
    let output = Command::new("git")
        .args([
            "-c",
            "user.name=identedit-tests",
            "-c",
            "user.email=identedit-tests@example.com",
            "commit",
            "-m",
            "fixture",
        ])
        .current_dir(path)
        .output()
        .expect("git commit should start");
    assert!(
        output.status.success(),
        "git commit failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(target_os = "macos")]
fn shared_library_extension() -> &'static str {
    "dylib"
}

#[cfg(not(target_os = "macos"))]
fn shared_library_extension() -> &'static str {
    "so"
}

#[test]
fn grammar_install_convention_language_without_ext_returns_invalid_request() {
    let kanna_home = tempdir().expect("identedit home tempdir should be created");

    let output = run_identedit(&["grammar", "install", "unknownlang"], kanna_home.path());
    assert!(
        !output.status.success(),
        "install should fail without --ext for convention fallback language"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("error output should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");

    let message = response["error"]["message"]
        .as_str()
        .expect("error message should be string");
    assert!(
        message.contains("--ext is required"),
        "message should explain missing --ext, got: {message}"
    );
    assert!(
        message.contains("unknownlang"),
        "message should mention rejected language, got: {message}"
    );
}

#[test]
fn grammar_install_invalid_repo_returns_grammar_install_failed_with_guidance() {
    let kanna_home = tempdir().expect("identedit home tempdir should be created");
    let missing_repo = kanna_home.path().join("missing-repo");

    let output = run_identedit(
        &[
            "grammar",
            "install",
            "jsonlocal",
            "--repo",
            missing_repo.to_str().expect("path should be utf-8"),
            "--ext",
            "jlocal",
        ],
        kanna_home.path(),
    );
    assert!(
        !output.status.success(),
        "install should fail for missing repository path"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("error output should be valid JSON");
    assert_eq!(response["error"]["type"], "grammar_install_failed");

    let message = response["error"]["message"]
        .as_str()
        .expect("error message should be string");
    assert!(
        message.contains("failed to install grammar 'jsonlocal'"),
        "message should mention failing language, got: {message}"
    );
    assert!(
        message.contains("Convention fallback failed. Retry with --repo and --symbol"),
        "message should include convention guidance, got: {message}"
    );
}

#[cfg(unix)]
#[test]
fn grammar_install_with_local_repo_enables_runtime_selection() {
    if !command_available("git") || !command_available("cc") {
        return;
    }

    let grammar_repo_dir = tempdir().expect("grammar repo tempdir should be created");
    prepare_local_grammar_repo(grammar_repo_dir.path());
    initialize_git_repository(grammar_repo_dir.path());

    let kanna_home = tempdir().expect("identedit home tempdir should be created");

    let install_output = run_identedit(
        &[
            "grammar",
            "install",
            "jsonlocal",
            "--repo",
            grammar_repo_dir
                .path()
                .to_str()
                .expect("path should be utf-8"),
            "--symbol",
            "tree_sitter_json",
            "--ext",
            "jlocal",
        ],
        kanna_home.path(),
    );
    assert!(
        install_output.status.success(),
        "grammar install failed: {}",
        String::from_utf8_lossy(&install_output.stderr)
    );

    let install_response: Value = serde_json::from_slice(&install_output.stdout)
        .expect("install output should be valid JSON");
    assert_eq!(install_response["installed"]["lang"], "jsonlocal");
    assert_eq!(install_response["installed"]["symbol"], "tree_sitter_json");
    assert_eq!(install_response["installed"]["extensions"][0], "jlocal");

    let workspace = tempdir().expect("workspace tempdir should be created");
    let target_file = workspace.path().join("fixture.jlocal");
    fs::write(
        &target_file,
        "{\n  \"config\": {\n    \"enabled\": true\n  }\n}\n",
    )
    .expect("fixture file should be written");

    let select_output = run_identedit(
        &[
            "read",
            "--json",
            "--kind",
            "object",
            target_file.to_str().expect("path should be utf-8"),
        ],
        kanna_home.path(),
    );
    assert!(
        select_output.status.success(),
        "select after install failed: {}",
        String::from_utf8_lossy(&select_output.stderr)
    );

    let select_response: Value =
        serde_json::from_slice(&select_output.stdout).expect("select output should be valid JSON");
    assert_eq!(select_response["summary"]["files_scanned"], 1);
    assert!(
        select_response["summary"]["matches"]
            .as_u64()
            .is_some_and(|count| count > 0),
        "dynamic grammar provider should emit at least one object match"
    );
}

#[cfg(unix)]
#[test]
fn grammar_install_symbol_mismatch_does_not_create_manifest_or_library() {
    if !command_available("git") || !command_available("cc") {
        return;
    }

    let grammar_repo_dir = tempdir().expect("grammar repo tempdir should be created");
    prepare_local_grammar_repo(grammar_repo_dir.path());
    initialize_git_repository(grammar_repo_dir.path());

    let kanna_home = tempdir().expect("identedit home tempdir should be created");

    let install_output = run_identedit(
        &[
            "grammar",
            "install",
            "jsonbad",
            "--repo",
            grammar_repo_dir
                .path()
                .to_str()
                .expect("path should be utf-8"),
            "--symbol",
            "tree_sitter_json_typo",
            "--ext",
            "jbad",
        ],
        kanna_home.path(),
    );
    assert!(
        !install_output.status.success(),
        "install should fail when symbol does not exist"
    );

    let response: Value =
        serde_json::from_slice(&install_output.stdout).expect("error output should be valid JSON");
    assert_eq!(response["error"]["type"], "grammar_install_failed");

    let message = response["error"]["message"]
        .as_str()
        .expect("error message should be string");
    assert!(
        message.contains("tree_sitter_json_typo"),
        "message should mention missing symbol candidate, got: {message}"
    );

    let manifest_path = kanna_home.path().join("grammars").join("manifest.json");
    assert!(
        !manifest_path.exists(),
        "manifest must not be created after failed install"
    );

    let library_path = kanna_home
        .path()
        .join("grammars")
        .join(format!("jsonbad.{}", shared_library_extension()));
    assert!(
        !library_path.exists(),
        "library file must not remain after failed install"
    );
}

#[test]
fn corrupted_manifest_is_ignored_and_bundled_provider_still_works() {
    let kanna_home = tempdir().expect("identedit home tempdir should be created");
    let grammars_dir = kanna_home.path().join("grammars");
    fs::create_dir_all(&grammars_dir).expect("grammars directory should be created");
    fs::write(grammars_dir.join("manifest.json"), "{not valid json")
        .expect("manifest fixture should be written");

    let workspace = tempdir().expect("workspace tempdir should be created");
    let target_file = workspace.path().join("fixture.py");
    fs::write(
        &target_file,
        "def process_data(value):\n    return value + 1\n",
    )
    .expect("python fixture should be written");

    let select_output = run_identedit(
        &[
            "read",
            "--json",
            "--kind",
            "function_definition",
            target_file.to_str().expect("path should be utf-8"),
        ],
        kanna_home.path(),
    );
    assert!(
        select_output.status.success(),
        "bundled provider should still work with malformed manifest: {}",
        String::from_utf8_lossy(&select_output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&select_output.stdout).expect("select output should be valid JSON");
    assert_eq!(response["summary"]["matches"], 1);
}

#[cfg(unix)]
#[test]
fn dynamic_provider_wins_over_json_provider_on_extension_collision() {
    if !command_available("git") || !command_available("cc") {
        return;
    }

    let grammar_repo_dir = tempdir().expect("grammar repo tempdir should be created");
    prepare_local_grammar_repo(grammar_repo_dir.path());
    initialize_git_repository(grammar_repo_dir.path());

    let kanna_home = tempdir().expect("identedit home tempdir should be created");

    let install_output = run_identedit(
        &[
            "grammar",
            "install",
            "jsonshadow",
            "--repo",
            grammar_repo_dir
                .path()
                .to_str()
                .expect("path should be utf-8"),
            "--symbol",
            "tree_sitter_json",
            "--ext",
            "json",
        ],
        kanna_home.path(),
    );
    assert!(
        install_output.status.success(),
        "grammar install failed: {}",
        String::from_utf8_lossy(&install_output.stderr)
    );

    let workspace = tempdir().expect("workspace tempdir should be created");
    let target_file = workspace.path().join("fixture.json");
    fs::write(&target_file, "{ \"k\": {\"enabled\": true} }\n")
        .expect("json fixture should be written");

    let select_output = run_identedit(
        &[
            "read",
            "--json",
            "--kind",
            "object",
            target_file.to_str().expect("path should be utf-8"),
        ],
        kanna_home.path(),
    );
    assert!(
        select_output.status.success(),
        "select should succeed with extension collision: {}",
        String::from_utf8_lossy(&select_output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&select_output.stdout).expect("select output should be valid JSON");
    assert!(
        response["summary"]["matches"]
            .as_u64()
            .is_some_and(|count| count > 0),
        "dynamic provider should emit object matches"
    );
}

#[test]
fn missing_dynamic_library_entry_in_manifest_is_ignored_at_runtime() {
    let kanna_home = tempdir().expect("identedit home tempdir should be created");
    let grammars_dir = kanna_home.path().join("grammars");
    fs::create_dir_all(&grammars_dir).expect("grammars directory should be created");

    let missing_library_path = grammars_dir.join(format!("ghost.{}", shared_library_extension()));
    let manifest = json!({
        "grammars": [
            {
                "lang": "ghost",
                "repo": "https://example.invalid/tree-sitter-ghost.git",
                "symbol": "tree_sitter_ghost",
                "extensions": ["ghost"],
                "library_path": missing_library_path
            }
        ]
    });
    fs::write(
        grammars_dir.join("manifest.json"),
        serde_json::to_string_pretty(&manifest).expect("manifest serialization should succeed"),
    )
    .expect("manifest should be written");

    let workspace = tempdir().expect("workspace tempdir should be created");
    let target_file = workspace.path().join("fixture.ghost");
    fs::write(
        &target_file,
        "function process_data(value) {\n  return value + 1;\n}\n",
    )
    .expect("fallback fixture should be written");

    let select_output = run_identedit(
        &[
            "read",
            "--json",
            "--kind",
            "function_definition",
            target_file.to_str().expect("path should be utf-8"),
        ],
        kanna_home.path(),
    );
    assert!(
        select_output.status.success(),
        "select should succeed even with missing dynamic library entry: {}",
        String::from_utf8_lossy(&select_output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&select_output.stdout).expect("select output should be valid JSON");
    assert_eq!(response["summary"]["matches"], 1);
}
