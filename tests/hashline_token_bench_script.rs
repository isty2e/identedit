use std::fs;
use std::process::Command;

use serde_json::Value;
use tempfile::TempDir;

fn python_supports_tiktoken() -> bool {
    let output = Command::new("python3")
        .args(["-c", "import tiktoken"])
        .output();
    match output {
        Ok(result) => result.status.success(),
        Err(_) => false,
    }
}

#[test]
fn hashline_token_bench_script_emits_reproducible_report_shape() {
    if !python_supports_tiktoken() {
        eprintln!("skipping hashline token bench script test: python3+tiktoken unavailable");
        return;
    }

    let temp_dir = TempDir::new().expect("temp dir should be created");
    let output_path = temp_dir.path().join("report.json");
    let binary_path = env!("CARGO_BIN_EXE_identedit");

    let output = Command::new("python3")
        .args([
            "scripts/hashline_token_bench.py",
            "--binary",
            binary_path,
            "--fixtures",
            "tests/fixtures/hashline_bench/cases.json",
            "--model",
            "gpt-4o-mini",
            "--output",
            output_path.to_str().expect("output path should be utf-8"),
        ])
        .output()
        .expect("script process should run");

    assert!(
        output.status.success(),
        "benchmark script failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let report_text = fs::read_to_string(&output_path).expect("report should be readable");
    let report: Value = serde_json::from_str(&report_text).expect("report should be valid JSON");

    assert_eq!(report["tool"], "identedit-hashline-token-bench");
    assert_eq!(report["model"], "gpt-4o-mini");
    assert_eq!(report["summary"]["case_count"], 3);

    let cases = report["cases"]
        .as_array()
        .expect("'cases' must be an array");
    assert_eq!(cases.len(), 3);

    for case in cases {
        let commands = case["commands"]
            .as_array()
            .expect("each case should include command metrics");
        assert_eq!(commands.len(), 5);

        for command in commands {
            assert_eq!(
                command["exit_code"], 0,
                "benchmark command should succeed: {command}"
            );
            let argv = command["argv"]
                .as_array()
                .expect("command argv should be an array");
            let argv_joined = argv
                .iter()
                .map(|value| value.as_str().expect("argv entries should be strings"))
                .collect::<Vec<_>>()
                .join(" ");
            assert!(
                !argv_joined.contains("/var/folders/"),
                "report must not leak temp absolute paths: {argv_joined}"
            );
        }
    }
}
