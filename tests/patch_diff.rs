mod common;

use std::fs;
use std::path::Path;
use std::process::Command;

fn patch(root: &Path, args: &[&str], preview: bool) -> String {
    let mut command = Command::new(env!("CARGO_BIN_EXE_identedit"));
    command.current_dir(root).arg("patch").args(args);
    if preview {
        command.args(["--dry-run", "--diff", "--color", "never"]);
    }
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn line_preview_is_compact_and_preserves_surrounding_context() {
    let root = tempfile::tempdir().unwrap();
    let source = (1..=100).map(|n| format!("line {n}\n")).collect::<String>();
    fs::write(root.path().join("sample.txt"), &source).unwrap();
    let anchor = format!("50:{}", common::compute_line_hash("line 50"));
    let diff = patch(
        root.path(),
        &["sample.txt", "--at", &anchor, "--replace", "changed"],
        true,
    );
    assert!(diff.contains("@@ -47,7 +47,7 @@"), "{diff}");
    assert!(
        diff.contains(" line 49\n-line 50\n+changed\n line 51\n"),
        "{diff}"
    );
    assert!(diff.lines().count() < 15, "{diff}");
    assert_eq!(
        fs::read_to_string(root.path().join("sample.txt")).unwrap(),
        source
    );
}

#[test]
fn node_preview_exposes_the_actual_indentation_not_just_the_node_span() {
    let root = tempfile::tempdir().unwrap();
    let source = "class Processor:\n    def run(self):\n        return 1\n";
    fs::write(root.path().join("sample.py"), source).unwrap();
    let args = [
        "sample.py",
        "--symbol",
        "Processor.run",
        "--replace",
        "    def run(self):\n        return 2",
    ];
    let diff = patch(root.path(), &args, true);
    assert!(diff.contains("-    def run(self):\n"), "{diff}");
    assert!(diff.contains("+        def run(self):\n"), "{diff}");
    patch(root.path(), &args, false);
    assert_eq!(
        fs::read_to_string(root.path().join("sample.py")).unwrap(),
        "class Processor:\n        def run(self):\n        return 2\n"
    );
}

#[test]
fn preview_marks_missing_final_newlines_and_keeps_cr_bytes() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("sample.txt"), "a\r\nb").unwrap();
    let anchor = format!("2:{}", common::compute_line_hash("b"));
    let diff = patch(
        root.path(),
        &["sample.txt", "--at", &anchor, "--replace", "B"],
        true,
    );
    assert!(diff.contains(" a\r\n"), "{diff:?}");
    assert!(
        diff.contains("-b\n\\ No newline at end of file\n+B\n\\ No newline at end of file\n"),
        "{diff:?}"
    );
}

#[test]
fn unchanged_line_preview_is_empty() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("sample.txt"), "same\n").unwrap();
    let anchor = format!("1:{}", common::compute_line_hash("same"));
    assert!(
        patch(
            root.path(),
            &["sample.txt", "--at", &anchor, "--replace", "same"],
            true
        )
        .is_empty()
    );
}

#[test]
fn config_preview_includes_actual_json_create_missing_reserialization() {
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("sample.json");
    let source = "{\"zeta\": 1, \"alpha\": {}}\n";
    fs::write(&file, source).unwrap();
    let args = [
        "sample.json",
        "--config-path",
        "alpha.enabled",
        "--set-value",
        "true",
        "--create-missing",
    ];
    let diff = patch(root.path(), &args, true);
    assert!(diff.contains("-{\"zeta\": 1, \"alpha\": {}}\n"), "{diff}");
    assert_eq!(fs::read_to_string(&file).unwrap(), source);
    patch(root.path(), &args, false);
    let updated = fs::read_to_string(&file).unwrap();
    for line in updated.split_inclusive('\n') {
        assert!(diff.contains(&format!("+{line}")), "{diff}");
    }
}

#[test]
fn repaired_line_preview_uses_the_current_full_file_snapshot() {
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("sample.txt");
    let before = "added\nfirst\ntarget\nlast\n";
    fs::write(&file, before).unwrap();
    let anchor = format!("2:{}", common::compute_line_hash("target"));
    let args = [
        "sample.txt",
        "--at",
        &anchor,
        "--replace",
        "changed",
        "--auto-repair",
    ];
    let diff = patch(root.path(), &args, true);
    assert!(diff.contains("@@ -1,4 +1,4 @@"), "{diff}");
    assert!(
        diff.contains(" added\n first\n-target\n+changed\n last\n"),
        "{diff}"
    );
    assert_eq!(fs::read_to_string(&file).unwrap(), before);
    patch(root.path(), &args, false);
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        "added\nfirst\nchanged\nlast\n"
    );
}

#[test]
fn large_node_preview_keeps_distant_changes_separate() {
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("sample.py");
    let source = large_function();
    fs::write(&file, &source).unwrap();
    let replacement = source
        .replace("first = 1", "first = 10")
        .replace("last = 2", "last = 20");
    let diff = patch(
        root.path(),
        &[
            "sample.py",
            "--symbol",
            "sample",
            "--replace",
            replacement.trim_end_matches('\n'),
        ],
        true,
    );
    assert_eq!(diff.matches("--- ").count(), 1);
    assert_eq!(diff.matches("@@ -").count(), 2);
    assert!(diff.lines().count() < 25, "{diff}");
    assert!(diff.contains("-    first = 1\n+    first = 10\n"));
    assert!(diff.contains("-    last = 2\n+    last = 20\n"));
    assert_eq!(fs::read_to_string(&file).unwrap(), source);
}

fn large_function() -> String {
    let middle: String = (0..1_000).map(|n| format!("    # context {n}\n")).collect();
    format!("def sample():\n    first = 1\n{middle}    last = 2\n    return first + last\n")
}

// Explicitly run in macOS/Linux CI. Ordinary unit/CLI tests need no patch tool.
#[test]
#[ignore = "requires an external patch executable; CI runs this explicitly"]
fn external_patch_replays_cli_previews_to_the_actual_apply_bytes() {
    let tool = std::env::var_os("IDENTEDIT_TEST_PATCH").unwrap_or_else(|| "patch".into());
    let version = Command::new(&tool)
        .arg("--version")
        .output()
        .expect("patch must be installed");
    assert!(version.status.success());
    let is_gnu = String::from_utf8_lossy(&version.stdout).contains("GNU patch");
    for before in [
        "",
        "a",
        "a\n",
        "a\r",
        "a\r\n",
        "a\rb\nc\r\n",
        "\u{feff}é\n終",
    ] {
        for insert in ["x", "x\n", "\r\n", "x\ry\n終"] {
            for target in ["file-start", "file-end"] {
                assert_external_round_trip(
                    &tool,
                    is_gnu,
                    "sample file.txt",
                    before,
                    &["--at", target, "--insert", insert],
                );
            }
        }
    }
    assert_external_round_trip(
        &tool,
        is_gnu,
        "sample.py",
        "class Processor:\n    def run(self):\n        return 1\n",
        &[
            "--symbol",
            "Processor.run",
            "--replace",
            "    def run(self):\n        return 2",
        ],
    );
    assert_external_round_trip(
        &tool,
        is_gnu,
        "sample.json",
        "{\"zeta\": 1, \"alpha\": {}}\n",
        &[
            "--config-path",
            "alpha.enabled",
            "--set-value",
            "true",
            "--create-missing",
        ],
    );
    for before in ["old\n", "old\r\n", "a\rold\nc", "old"] {
        let anchor = format!(
            "1:{}",
            common::compute_line_hash(before.split(['\n', '\r']).next().unwrap())
        );
        assert_external_round_trip(
            &tool,
            is_gnu,
            "sample.txt",
            before,
            &["--at", &anchor, "--replace", "new"],
        );
        assert_external_round_trip(
            &tool,
            is_gnu,
            "sample.txt",
            before,
            &["--at", &anchor, "--delete"],
        );
    }
    assert_external_round_trip(
        &tool,
        is_gnu,
        "sample.yaml",
        "# config\nservice:\n  retries: 1 # keep\n",
        &["--config-path", "service.retries", "--set-value", "3"],
    );
    assert_external_round_trip(
        &tool,
        is_gnu,
        "sample.toml",
        "# config\n[service]\nretries = 1 # keep\n",
        &["--config-path", "service.retries", "--set-value", "3"],
    );
    assert_external_round_trip(
        &tool,
        is_gnu,
        "sample.txt",
        "added\nfirst\ntarget\nlast\n",
        &[
            "--at",
            &format!("2:{}", common::compute_line_hash("target")),
            "--replace",
            "changed",
            "--auto-repair",
        ],
    );
    let source = large_function();
    let replacement = source
        .replace("first = 1", "first = 10")
        .replace("last = 2", "last = 20");
    assert_external_round_trip(
        &tool,
        is_gnu,
        "sample.py",
        &source,
        &[
            "--symbol",
            "sample",
            "--replace",
            replacement.trim_end_matches('\n'),
        ],
    );
}

fn assert_external_round_trip(
    tool: &std::ffi::OsStr,
    is_gnu: bool,
    name: &str,
    before: &str,
    options: &[&str],
) {
    use std::io::Write;
    use std::process::Stdio;

    let root = tempfile::tempdir().unwrap();
    let file = root.path().join(name);
    fs::write(&file, before).unwrap();
    let args: Vec<_> = std::iter::once(name)
        .chain(options.iter().copied())
        .collect();
    let diff = patch(root.path(), &args, true);
    assert_eq!(fs::read_to_string(&file).unwrap(), before);
    patch(root.path(), &args, false);
    let expected = fs::read(&file).unwrap();
    for binary in [false, true].into_iter().filter(|binary| !binary || is_gnu) {
        fs::write(&file, before).unwrap();
        let mut command = Command::new(tool);
        command.current_dir(root.path()).args(["-p0", "--batch"]);
        if binary {
            command.arg("--binary");
        }
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(diff.as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{before:?}, {options:?}, binary={binary}\n{diff:?}\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            fs::read(&file).unwrap(),
            expected,
            "{before:?}, {options:?}, binary={binary}, {diff:?}"
        );
    }
}
