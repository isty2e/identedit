use super::*;

#[test]
fn patch_kind_name_replace_applies_change_without_read_step() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let replacement = "def process_data(value):\n    return value * 11";

    let output = run_identedit(&[
        "patch",
        "--kind",
        "function_definition",
        "--name",
        "process_*",
        "--replace",
        replacement,
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "kind/name patch replace failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["summary"]["files_modified"], 1);
    assert_eq!(response["summary"]["operations_applied"], 1);

    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    assert!(
        modified.contains("return value * 11"),
        "replacement text should be written through kind/name targeting"
    );
}

#[test]
fn patch_scoped_regex_accepts_stdin_text_replacement() {
    let file_path = create_scoped_regex_fixture();
    let handle = select_named_function_handle(&file_path, "process_*");
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");

    let output = run_identedit_with_stdin(
        &[
            "patch",
            "--identity",
            identity,
            "--scoped-regex",
            "value",
            "--scoped-replacement",
            "--stdin-text",
            file_path.to_str().expect("path should be utf-8"),
        ],
        "payload",
    );

    assert!(
        output.status.success(),
        "scoped regex with stdin replacement failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    assert!(modified.contains("def process_data(payload):"));
    assert!(modified.contains("return payload + 1"));
    assert!(modified.contains("def helper(value):"));
}

#[test]
fn patch_scoped_regex_text_file_dry_run_does_not_modify_file() {
    let file_path = create_scoped_regex_fixture();
    let before = fs::read_to_string(&file_path).expect("fixture should be readable");
    let handle = select_named_function_handle(&file_path, "process_*");
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");
    let payload_path = create_temp_text_file("payload");

    let output = run_identedit(&[
        "patch",
        "--identity",
        identity,
        "--scoped-regex",
        "value",
        "--scoped-replacement",
        "--text-file",
        payload_path.to_str().expect("payload path should be utf-8"),
        "--dry-run",
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "scoped regex with text-file dry-run failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["regex_replacements"], 2);

    let after = fs::read_to_string(&file_path).expect("file should remain readable");
    assert_eq!(after, before);
}

#[test]
fn patch_scoped_regex_text_file_path_with_spaces_applies() {
    let file_path = create_scoped_regex_fixture();
    let handle = select_named_function_handle(&file_path, "process_*");
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");
    let payload_dir = Builder::new()
        .prefix("identedit scoped payload ")
        .tempdir()
        .expect("temp dir should be created");
    let payload_path = payload_dir.path().join("replacement text.txt");
    fs::write(&payload_path, "payload").expect("payload file should be written");

    let output = run_identedit(&[
        "patch",
        "--identity",
        identity,
        "--scoped-regex",
        "value",
        "--scoped-replacement",
        "--text-file",
        payload_path.to_str().expect("payload path should be utf-8"),
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "scoped regex text-file path with spaces should work: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    assert!(modified.contains("def process_data(payload):"));
    assert!(modified.contains("return payload + 1"));
}

#[test]
fn patch_symbol_replaces_unique_symbol_without_kind_name_flags() {
    let file_path = copy_fixture_to_temp_python("example.py");

    let output = run_identedit(&[
        "patch",
        "--symbol",
        "helper",
        "--replace",
        "def helper():\n    return \"patched\"",
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "symbol patch should infer the unique helper node: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    assert!(modified.contains("return \"patched\""));
    assert!(modified.contains("def process_data(value):"));
}

#[test]
fn patch_symbol_qualified_name_targets_nested_method() {
    let source = "def process_data(value):\n    return value + 1\n\n\nclass Processor:\n    def process_data(self, value):\n        return value + 2\n\n\nclass Other:\n    def process_data(self, value):\n        return value + 3\n";
    let file_path = create_temp_python_source(source);

    let output = run_identedit(&[
        "patch",
        "--symbol",
        "Processor.process_data",
        "--replace",
        "def process_data(self, value):\n        return value * 7",
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "qualified symbol patch should target exactly one method: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    assert!(modified.contains(
        "class Processor:\n    def process_data(self, value):\n        return value * 7"
    ));
    assert!(modified.contains("def process_data(value):\n    return value + 1"));
    assert!(
        modified
            .contains("class Other:\n    def process_data(self, value):\n        return value + 3")
    );
}

#[test]
fn patch_symbol_unqualified_duplicate_reports_ambiguous_target() {
    let source = "def process_data(value):\n    return value + 1\n\n\nclass Processor:\n    def process_data(self, value):\n        return value + 2\n";
    let file_path = create_temp_python_source(source);

    let output = run_identedit(&[
        "patch",
        "--symbol",
        "process_data",
        "--replace",
        "def process_data(value):\n    return 0",
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        !output.status.success(),
        "unqualified duplicate symbol should fail instead of guessing"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "ambiguous_target");
    let message = response["error"]["message"]
        .as_str()
        .expect("error message should be present");
    assert!(message.contains("symbol='process_data'"));
    let candidates = response["error"]["candidates"]
        .as_array()
        .expect("ambiguous symbol response should include candidate contexts");
    assert_eq!(candidates.len(), 2);
    assert_eq!(candidates[0]["name"], "process_data");
    assert_eq!(candidates[0]["qualified_name"], "process_data");
    assert_eq!(candidates[0]["line"], 1);
    assert_eq!(candidates[0]["preview"], "def process_data(value):");
    assert_eq!(candidates[1]["name"], "process_data");
    assert_eq!(candidates[1]["qualified_name"], "Processor.process_data");
    assert_eq!(candidates[1]["line"], 6);
    assert_eq!(candidates[1]["preview"], "def process_data(self, value):");
    assert!(candidates.iter().all(|candidate| {
        candidate["identity"]
            .as_str()
            .is_some_and(|value| value.len() == 16)
    }));
    assert_eq!(
        fs::read_to_string(&file_path).expect("file should be readable"),
        source,
        "ambiguous symbol patch must not mutate the source"
    );
}

#[test]
fn patch_symbol_duplicate_qualified_name_reports_ambiguous_target() {
    let source = "class Processor:\n    def process_data(self, value):\n        return value + 2\n\n\nclass Processor:\n    def process_data(self, value):\n        return value + 3\n";
    let file_path = create_temp_python_source(source);

    let output = run_identedit(&[
        "patch",
        "--symbol",
        "Processor.process_data",
        "--replace",
        "def process_data(self, value):\n        return 0",
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        !output.status.success(),
        "duplicate qualified symbol should fail instead of choosing the first match"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "ambiguous_target");
    let message = response["error"]["message"]
        .as_str()
        .expect("error message should be present");
    assert!(message.contains("symbol='Processor.process_data'"));
    let candidates = response["error"]["candidates"]
        .as_array()
        .expect("ambiguous qualified symbol response should include candidate contexts");
    assert_eq!(candidates.len(), 2);
    assert!(
        candidates
            .iter()
            .all(|candidate| candidate["qualified_name"] == "Processor.process_data")
    );
    assert_eq!(candidates[0]["line"], 2);
    assert_eq!(candidates[1]["line"], 7);
    assert_eq!(
        fs::read_to_string(&file_path).expect("file should be readable"),
        source,
        "ambiguous qualified symbol patch must not mutate the source"
    );
}

#[test]
fn patch_symbol_missing_symbol_reports_target_missing_without_mutation() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let before = fs::read_to_string(&file_path).expect("file should be readable");

    let output = run_identedit(&[
        "patch",
        "--symbol",
        "Processor.process_data",
        "--replace",
        "def process_data(self, value):\n        return 0",
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        !output.status.success(),
        "missing symbol should return target_missing"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "target_missing");
    let message = response["error"]["message"]
        .as_str()
        .expect("error message should be present");
    assert!(message.contains("symbol='Processor.process_data'"));
    assert_eq!(
        fs::read_to_string(&file_path).expect("file should be readable"),
        before,
        "missing symbol patch must not mutate the source"
    );
}

#[test]
fn patch_symbol_rejects_mixed_kind_name_selector() {
    let file_path = copy_fixture_to_temp_python("example.py");

    let output = run_identedit(&[
        "patch",
        "--symbol",
        "helper",
        "--kind",
        "function_definition",
        "--name",
        "helper",
        "--replace",
        "def helper():\n    return \"patched\"",
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        !output.status.success(),
        "symbol selector should not mix with kind/name selector"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let message = response["error"]["message"]
        .as_str()
        .expect("error message should be present");
    assert!(message.contains("--symbol") && message.contains("--kind"));
}

#[test]
fn patch_symbol_rejects_empty_symbol() {
    let file_path = copy_fixture_to_temp_python("example.py");

    let output = run_identedit(&[
        "patch",
        "--symbol",
        "   ",
        "--replace",
        "def helper():\n    return \"patched\"",
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        !output.status.success(),
        "empty symbol selector should fail"
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let message = response["error"]["message"]
        .as_str()
        .expect("error message should be present");
    assert!(message.contains("--symbol") && message.contains("empty"));
}

#[test]
fn patch_kind_name_replace_dry_run_previews_without_writing() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let before = fs::read_to_string(&file_path).expect("file should be readable");
    let replacement = "def process_data(value):\n    return value * 12";

    let output = run_identedit(&[
        "patch",
        "--kind",
        "function_definition",
        "--name",
        "process_*",
        "--replace",
        replacement,
        "--dry-run",
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "kind/name patch dry-run failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["transaction"]["status"], "dry_run");
    assert_eq!(response["summary"]["operations_applied"], 1);

    let after = fs::read_to_string(&file_path).expect("file should be readable");
    assert_eq!(before, after, "dry-run must not modify the source file");
}

#[test]
fn patch_kind_name_replace_dry_run_diff_outputs_unified_diff_without_writing() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let before = fs::read_to_string(&file_path).expect("file should be readable");

    let output = run_identedit(&[
        "patch",
        "--kind",
        "function_definition",
        "--name",
        "process_*",
        "--replace",
        "def process_data(value):\n    return value * 13",
        "--dry-run",
        "--diff",
        "--color",
        "never",
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "kind/name patch dry-run diff failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let diff = String::from_utf8(output.stdout).expect("stdout should be utf-8");
    assert!(diff.contains("--- "));
    assert!(diff.contains("+++ "));
    assert!(diff.contains("@@ -2,2 +2,1 @@"));
    assert!(!diff.contains("-def process_data(value):"));
    assert!(!diff.contains("+def process_data(value):"));
    assert!(diff.contains("-    result = value + 1"));
    assert!(diff.contains("+    return value * 13"));
    assert!(
        serde_json::from_str::<Value>(&diff).is_err(),
        "diff output must not be JSON"
    );
    assert_eq!(
        fs::read_to_string(&file_path).expect("file should be readable"),
        before,
        "dry-run diff must not modify the source file"
    );
}

#[test]
fn patch_kind_name_replace_dry_run_diff_omits_unchanged_suffix_context() {
    let source = "def sample():\n    before()\n    old()\n    after()\n";
    let mut temp_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp python file should be created");
    temp_file
        .write_all(source.as_bytes())
        .expect("temp python file write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let output = run_identedit(&[
        "patch",
        "--symbol",
        "sample",
        "--replace",
        "def sample():\n    before()\n    new()\n    after()",
        "--dry-run",
        "--diff",
        "--color",
        "never",
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "minimal diff should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let diff = String::from_utf8(output.stdout).expect("stdout should be utf-8");
    assert!(diff.contains("@@ -3,1 +3,1 @@"));
    assert!(diff.contains("-    old()"));
    assert!(diff.contains("+    new()"));
    assert!(!diff.contains("before()"));
    assert!(!diff.contains("after()"));
}

#[test]
fn patch_kind_name_replace_dry_run_diff_splits_separated_changes_into_hunks() {
    let source = "\
def sample(value):
    keep_a()
    old_a(value)
    keep_b()
    old_b(value)
    keep_c()
";
    let mut temp_file = Builder::new()
        .suffix(".py")
        .tempfile()
        .expect("temp python file should be created");
    temp_file
        .write_all(source.as_bytes())
        .expect("temp python file write should succeed");
    let file_path = temp_file.keep().expect("temp file should persist").1;

    let output = run_identedit(&[
        "patch",
        "--symbol",
        "sample",
        "--replace",
        "def sample(value):\n    keep_a()\n    new_a(value)\n    keep_b()\n    new_b(value)\n    keep_c()",
        "--dry-run",
        "--diff",
        "--color",
        "never",
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "multi-hunk minimal diff should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let diff = String::from_utf8(output.stdout).expect("stdout should be utf-8");
    assert!(diff.contains("@@ -3,1 +3,1 @@"));
    assert!(diff.contains("@@ -5,1 +5,1 @@"));
    assert!(diff.contains("-    old_a(value)"));
    assert!(diff.contains("+    new_a(value)"));
    assert!(diff.contains("-    old_b(value)"));
    assert!(diff.contains("+    new_b(value)"));
    assert!(!diff.contains("keep_a"));
    assert!(!diff.contains("keep_b"));
    assert!(!diff.contains("keep_c"));
}

#[test]
fn patch_kind_name_replace_dry_run_diff_splits_separated_deletions_into_hunks() {
    let source = "\
def sample():
    keep_a()
    drop_a()
    keep_b()
    drop_b()
    keep_c()
";
    let file_path = create_temp_python_source(source);

    let output = run_identedit(&[
        "patch",
        "--symbol",
        "sample",
        "--replace",
        "def sample():\n    keep_a()\n    keep_b()\n    keep_c()",
        "--dry-run",
        "--diff",
        "--color",
        "never",
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "multi-hunk deletion diff should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let diff = String::from_utf8(output.stdout).expect("stdout should be utf-8");
    assert!(diff.contains("@@ -3,1 +3,0 @@"));
    assert!(diff.contains("@@ -5,1 +4,0 @@"));
    assert!(diff.contains("-    drop_a()"));
    assert!(diff.contains("-    drop_b()"));
    assert!(!diff.contains("keep_a"));
    assert!(!diff.contains("keep_b"));
    assert!(!diff.contains("keep_c"));
}

#[test]
fn patch_kind_name_replace_dry_run_diff_splits_separated_insertions_into_hunks() {
    let source = "\
def sample():
    keep_a()
    keep_b()
    keep_c()
";
    let file_path = create_temp_python_source(source);

    let output = run_identedit(&[
        "patch",
        "--symbol",
        "sample",
        "--replace",
        "def sample():\n    keep_a()\n    add_a()\n    keep_b()\n    add_b()\n    keep_c()",
        "--dry-run",
        "--diff",
        "--color",
        "never",
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "multi-hunk insertion diff should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let diff = String::from_utf8(output.stdout).expect("stdout should be utf-8");
    assert!(diff.contains("@@ -3,0 +3,1 @@"));
    assert!(diff.contains("@@ -4,0 +5,1 @@"));
    assert!(diff.contains("+    add_a()"));
    assert!(diff.contains("+    add_b()"));
    assert!(!diff.contains("keep_a"));
    assert!(!diff.contains("keep_b"));
    assert!(!diff.contains("keep_c"));
}

#[test]
fn patch_kind_name_replace_dry_run_diff_is_empty_for_noop_replacement() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let output = run_identedit(&[
        "patch",
        "--symbol",
        "process_data",
        "--replace",
        "def process_data(value):\n    result = value + 1\n    return result",
        "--dry-run",
        "--diff",
        "--color",
        "never",
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "no-op minimal diff should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let diff = String::from_utf8(output.stdout).expect("stdout should be utf-8");
    assert_eq!(diff.trim(), "");
}

#[test]
fn patch_kind_name_replace_dry_run_diff_can_force_color() {
    let file_path = copy_fixture_to_temp_python("example.py");

    let output = run_identedit(&[
        "patch",
        "--kind",
        "function_definition",
        "--name",
        "process_*",
        "--replace",
        "def process_data(value):\n    return value * 17",
        "--dry-run",
        "--diff",
        "--color",
        "always",
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        output.status.success(),
        "colored dry-run diff failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let diff = String::from_utf8(output.stdout).expect("stdout should be utf-8");
    assert!(diff.contains("\u{1b}[31m-"));
    assert!(diff.contains("\u{1b}[32m+"));
    assert!(diff.contains("\u{1b}[36m@@"));
}

#[test]
fn patch_kind_name_requires_both_flags() {
    let file_path = copy_fixture_to_temp_python("example.py");

    let output = run_identedit(&[
        "patch",
        "--kind",
        "function_definition",
        "--replace",
        "def process_data(value):\n    return value * 2",
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        !output.status.success(),
        "patch should reject selector mode when --name is missing"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let message = response["error"]["message"]
        .as_str()
        .expect("error message should be present");
    assert!(
        message.contains("--kind") && message.contains("--name") && message.contains("Example"),
        "selector mode error should mention both required flags and show the direct fix"
    );
}

#[test]
fn patch_kind_name_reports_target_missing_for_unmatched_symbol() {
    let file_path = copy_fixture_to_temp_python("example.py");

    let output = run_identedit(&[
        "patch",
        "--kind",
        "function_definition",
        "--name",
        "does_not_exist",
        "--replace",
        "def does_not_exist():\n    return 0",
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        !output.status.success(),
        "patch should fail when kind/name selector matches no symbol"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "target_missing");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| {
                message.contains("function_definition") && message.contains("does_not_exist")
            }),
        "target-missing message should describe the selector"
    );
}

#[test]
fn patch_kind_name_reports_ambiguous_target_for_duplicate_symbol() {
    let file_path = copy_fixture_to_temp_python("ambiguous.py");

    let output = run_identedit(&[
        "patch",
        "--kind",
        "function_definition",
        "--name",
        "duplicate",
        "--replace",
        "def duplicate():\n    return 2",
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        !output.status.success(),
        "patch should fail when kind/name selector matches multiple symbols"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "ambiguous_target");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| {
                message.contains("function_definition") && message.contains("duplicate")
            }),
        "ambiguous-target message should describe the selector"
    );
    let candidates = response["error"]["candidates"]
        .as_array()
        .expect("ambiguous kind/name response should include candidate contexts");
    assert_eq!(candidates.len(), 2);
    assert_eq!(candidates[0]["kind"], "function_definition");
    assert_eq!(candidates[0]["name"], "duplicate");
    assert_eq!(candidates[0]["qualified_name"], "duplicate");
    assert_eq!(candidates[0]["line"], 1);
    assert_eq!(candidates[0]["preview"], "def duplicate():");
    assert_eq!(candidates[1]["line"], 5);
}

#[test]
fn patch_kind_name_rejects_mixed_with_identity_target() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_function_handle(&file_path, "process_*");
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");

    let output = run_identedit(&[
        "patch",
        "--identity",
        identity,
        "--kind",
        "function_definition",
        "--name",
        "process_*",
        "--replace",
        "def process_data(value):\n    return value * 13",
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        !output.status.success(),
        "patch should reject mixing selector targeting with identity targeting"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    let message = response["error"]["message"]
        .as_str()
        .expect("error message should be present");
    assert!(
        message.contains("Choose exactly one target selector")
            && message.contains("--identity")
            && message.contains("--kind"),
        "mixed target error should explain the valid selector families"
    );
}

#[test]
fn patch_kind_name_rejects_mixed_with_at_target() {
    let file_path = copy_fixture_to_temp_python("example.py");

    let output = run_identedit(&[
        "patch",
        "--at",
        "file-end",
        "--kind",
        "function_definition",
        "--name",
        "process_*",
        "--replace",
        "def process_data(value):\n    return value * 13",
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        !output.status.success(),
        "patch should reject mixing selector targeting with --at"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}

#[test]
fn patch_kind_name_scoped_regex_rewrites_only_selected_symbol() {
    let file_path = create_scoped_regex_fixture();

    let output = run_identedit(&[
        "patch",
        "--kind",
        "function_definition",
        "--name",
        "process_*",
        "--scoped-regex",
        "value",
        "--scoped-replacement",
        "item",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "selector scoped regex failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["summary"]["operations_applied"], 1);
    assert_eq!(response["regex_replacements"], 2);

    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    assert!(modified.contains("def process_data(item):"));
    assert!(modified.contains("return item + 1"));
    assert!(
        modified.contains("def helper(value):\n    return value + 2"),
        "selector scoped regex must not rewrite outside selected target span"
    );
}

#[test]
fn patch_kind_name_scoped_regex_dry_run_does_not_modify_file() {
    let file_path = create_scoped_regex_fixture();
    let before = fs::read_to_string(&file_path).expect("file should be readable");

    let output = run_identedit(&[
        "patch",
        "--kind",
        "function_definition",
        "--name",
        "process_*",
        "--scoped-regex",
        "value",
        "--scoped-replacement",
        "item",
        "--dry-run",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "selector scoped regex dry-run failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["transaction"]["status"], "dry_run");
    assert_eq!(response["summary"]["operations_applied"], 1);
    assert_eq!(
        fs::read_to_string(&file_path).expect("file should be readable"),
        before,
        "dry-run must not modify source text"
    );
}

#[test]
fn patch_kind_name_invalid_glob_reports_invalid_selector() {
    let file_path = copy_fixture_to_temp_python("example.py");

    let output = run_identedit(&[
        "patch",
        "--kind",
        "function_definition",
        "--name",
        "[",
        "--replace",
        "def process_data(value):\n    return value * 2",
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        !output.status.success(),
        "patch should reject invalid selector glob"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_selector");
}

#[test]
fn patch_kind_name_empty_kind_reports_invalid_request() {
    let file_path = copy_fixture_to_temp_python("example.py");

    let output = run_identedit(&[
        "patch",
        "--kind",
        "",
        "--name",
        "process_*",
        "--replace",
        "def process_data(value):\n    return value * 2",
        file_path.to_str().expect("path should be utf-8"),
    ]);

    assert!(
        !output.status.success(),
        "empty selector kind should be rejected"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
}

#[test]
fn patch_scoped_regex_flag_mode_rewrites_only_inside_target_span() {
    let file_path = create_scoped_regex_fixture();
    let handle = select_named_function_handle(&file_path, "process_*");
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");

    let output = run_identedit(&[
        "patch",
        "--identity",
        identity,
        "--scoped-regex",
        "value",
        "--scoped-replacement",
        "item",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        output.status.success(),
        "patch scoped regex failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["summary"]["operations_applied"], 1);
    assert_eq!(response["regex_replacements"], 2);

    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    assert!(modified.contains("def process_data(item):"));
    assert!(modified.contains("return item + 1"));
    assert!(
        modified.contains("def helper(value):\n    return value + 2"),
        "scoped regex must not rewrite outside selected target span"
    );
}

#[test]
fn patch_scoped_regex_flag_mode_rejects_zero_matches() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_function_handle(&file_path, "process_*");
    let identity = handle["identity"]
        .as_str()
        .expect("identity should be present");

    let output = run_identedit(&[
        "patch",
        "--identity",
        identity,
        "--scoped-regex",
        "does_not_exist",
        "--scoped-replacement",
        "x",
        file_path.to_str().expect("path should be utf-8"),
    ]);
    assert!(
        !output.status.success(),
        "patch scoped regex should fail when pattern has zero matches"
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("matched 0 occurrences")),
        "expected deterministic zero-match diagnostic"
    );
}

#[test]
fn patch_json_node_target_scoped_regex_applies_change_and_reports_count() {
    let file_path = create_scoped_regex_fixture();
    let handle = select_named_function_handle(&file_path, "process_*");
    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "node",
            "identity": handle["identity"],
            "kind": handle["kind"],
            "span_hint": handle["span"],
            "expected_old_hash": identedit::changeset::hash_text(
                handle["text"].as_str().expect("text should be string")
            )
        },
        "op": {
            "type": "scoped_regex",
            "pattern": "value",
            "replacement": "item"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        output.status.success(),
        "patch --json scoped regex failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["summary"]["operations_applied"], 1);
    assert_eq!(response["regex_replacements"], 2);

    let modified = fs::read_to_string(&file_path).expect("modified file should be readable");
    assert!(modified.contains("def process_data(item):"));
    assert!(modified.contains("return item + 1"));
    assert!(
        modified.contains("def helper(value):\n    return value + 2"),
        "scoped regex must not rewrite outside selected target span"
    );
}

#[test]
fn patch_json_node_target_scoped_regex_rejects_invalid_pattern() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_function_handle(&file_path, "process_*");
    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "node",
            "identity": handle["identity"],
            "kind": handle["kind"],
            "span_hint": handle["span"],
            "expected_old_hash": identedit::changeset::hash_text(
                handle["text"].as_str().expect("text should be string")
            )
        },
        "op": {
            "type": "scoped_regex",
            "pattern": "(unterminated",
            "replacement": "x"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "invalid scoped regex pattern must be rejected"
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("Invalid scoped regex pattern")),
        "expected deterministic invalid-pattern diagnostic"
    );
}

#[test]
fn patch_json_node_target_scoped_regex_rejects_zero_matches() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_function_handle(&file_path, "process_*");
    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "node",
            "identity": handle["identity"],
            "kind": handle["kind"],
            "span_hint": handle["span"],
            "expected_old_hash": identedit::changeset::hash_text(
                handle["text"].as_str().expect("text should be string")
            )
        },
        "op": {
            "type": "scoped_regex",
            "pattern": "does_not_exist",
            "replacement": "x"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "scoped regex should fail when pattern has zero matches"
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["error"]["type"], "invalid_request");
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("matched 0 occurrences")),
        "expected deterministic zero-match diagnostic"
    );
}

#[test]
fn patch_json_node_target_scoped_regex_preserves_stale_precondition_behavior() {
    let file_path = copy_fixture_to_temp_python("example.py");
    let handle = select_named_function_handle(&file_path, "process_*");
    fs::write(
        &file_path,
        "def process_data(value):\n    return value + 100\n\n\ndef helper():\n    return \"helper\"\n",
    )
    .expect("fixture mutation should succeed");

    let request = json!({
        "command": "patch",
        "file": file_path.to_string_lossy().to_string(),
        "target": {
            "type": "node",
            "identity": handle["identity"],
            "kind": handle["kind"],
            "span_hint": handle["span"],
            "expected_old_hash": identedit::changeset::hash_text(
                handle["text"].as_str().expect("text should be string")
            )
        },
        "op": {
            "type": "scoped_regex",
            "pattern": "value",
            "replacement": "item"
        }
    });

    let output = run_identedit_with_stdin(&["patch", "--json"], &request.to_string());
    assert!(
        !output.status.success(),
        "scoped regex should preserve stale precondition behavior"
    );

    let response: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    let error_type = response["error"]["type"]
        .as_str()
        .expect("error type should be present");
    assert!(
        matches!(error_type, "precondition_failed" | "target_missing"),
        "expected stale target diagnostic, got: {error_type}"
    );
}
