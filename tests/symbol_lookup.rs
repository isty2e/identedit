//! Run the opt-in comparison with two release binaries:
//! `IDENTEDIT_SYMBOL_BASELINE=/path/to/baseline cargo test --locked --release
//! --test symbol_lookup compare_symbol_lookup_scaling -- --ignored --nocapture`.
//! The baseline must be built before the optimization, with the same lockfile
//! and toolchain. Keep this harness in both builds and build the baseline with
//! `cargo test --locked --release --test symbol_lookup --no-run` so the Cargo
//! build mode and dependency features match. Timings include process startup,
//! parsing and JSON output. Same-name qualification and ambiguity diagnostics
//! are measured separately, not promised linear scaling.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};
use std::time::{Duration, Instant};

use serde_json::Value;
use tempfile::tempdir;

mod common;

fn successful_json(output: &Output) -> Value {
    assert!(
        output.status.success(),
        "{} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn symbol_targets_preserve_nested_and_provider_names_across_commands() {
    let fixtures = [
        (
            "py",
            "class Outer:\n    class Inner:\n        def target(self):\n            return 1\n",
            " Outer.Inner.target ",
            "target",
        ),
        (
            "lua",
            "function api.v1.target()\n    return 1\nend\n",
            "api.v1.target",
            "api.v1.target",
        ),
        (
            "lua",
            "function api.v1:target()\n    local function leaf() return 1 end\nend\n",
            "api.v1:target.leaf",
            "leaf",
        ),
    ];
    let dir = tempdir().unwrap();

    for (extension, source, query, name) in fixtures {
        let file = dir.path().join(format!("fixture.{extension}"));
        fs::write(&file, source).unwrap();
        let file = file.to_str().unwrap();
        let read = successful_json(&common::run_identedit(&[
            "read",
            file,
            "--symbol",
            query,
            "--json",
            "--verbose",
        ]));
        assert_eq!(read["summary"]["matches"], 1);
        let handle = &read["handles"][0];
        assert_eq!(handle["name"], name);
        let identity = handle["identity"].as_str().unwrap();
        let replacement = handle["text"].as_str().unwrap();

        for command in ["edit", "patch"] {
            let mut args = vec![command, file, "--replace", replacement];
            if command == "patch" {
                args.push("--dry-run");
            }
            let mut by_identity = args.clone();
            by_identity.extend(["--at", identity]);
            args.extend(["--symbol", query]);

            let actual = successful_json(&common::run_identedit(&args));
            let expected = successful_json(&common::run_identedit(&by_identity));
            assert_eq!(actual, expected, "{command}: {query}");
            if command == "edit" {
                assert_eq!(
                    actual["files"][0]["operations"][0]["target"]["identity"],
                    identity
                );
            }
            assert_eq!(fs::read_to_string(file).unwrap(), source);
        }
    }
}

#[test]
fn symbol_errors_preserve_ordered_duplicate_and_overload_candidates_across_commands() {
    let fixtures: &[(&str, &str, &str, &[&str])] = &[
        (
            "py",
            "def target():\n    return 0\nclass Box:\n    def target(self):\n        return 1\nclass Box:\n    def target(self):\n        return 2\n",
            "target",
            &["target", "Box.target", "Box.target"],
        ),
        (
            "py",
            "class Box:\n    def target(self):\n        return 1\nclass Box:\n    def target(self):\n        return 2\n",
            "Box.target",
            &["Box.target", "Box.target"],
        ),
        (
            "cs",
            "class Box { int target(int x) { return x; } string target(string x) { return x; } }\n",
            "Box.target",
            &["Box.target", "Box.target"],
        ),
        (
            "json",
            "{\"outer.part\":{\"inner::target\":1}}\n",
            "outer.part.inner::target",
            &["outer.part.inner::target", "outer.part.inner::target"],
        ),
        (
            "json",
            "{\"literal.dot\":1}\n",
            "literal.dot",
            &["literal.dot", "literal.dot"],
        ),
        (
            "lua",
            "function api.v1.target() return 1 end\nfunction api.v1()\n    local function target() return 2 end\nend\n",
            "api.v1.target",
            &["api.v1.target", "api.v1.target"],
        ),
        (
            "lua",
            "function api.v1.target() return 1 end\n",
            "v1.target",
            &[],
        ),
        (
            "py",
            "class Box:\n    def target(self):\n        return 1\n",
            "Other.target",
            &[],
        ),
    ];
    let dir = tempdir().unwrap();

    for (extension, source, query, qualified_names) in fixtures {
        let file = dir.path().join(format!("fixture.{extension}"));
        fs::write(&file, source).unwrap();
        let file = file.to_str().unwrap();
        let mut expected_error = None;

        for command in ["read", "edit", "patch"] {
            let mut args = vec![command, file, "--symbol", query];
            if command == "read" {
                args.push("--json");
            } else {
                args.extend(["--replace", "unused"]);
            }
            let output = common::run_identedit(&args);
            assert!(!output.status.success(), "{command}: {query}");
            let response: Value = serde_json::from_slice(&output.stdout).unwrap();
            let error = &response["error"];
            if qualified_names.is_empty() {
                assert_eq!(error["type"], "target_missing");
            } else {
                assert_eq!(error["type"], "ambiguous_target");
                let candidates = error["candidates"].as_array().unwrap();
                let actual_names = candidates
                    .iter()
                    .map(|candidate| candidate["qualified_name"].as_str().unwrap())
                    .collect::<Vec<_>>();
                assert_eq!(&actual_names, qualified_names, "{command}: {query}");
                assert!(
                    candidates
                        .windows(2)
                        .all(|pair| pair[0]["span"]["start"].as_u64()
                            < pair[1]["span"]["start"].as_u64())
                );
            }
            if let Some(expected) = &expected_error {
                assert_eq!(error, expected, "{command}: {query}");
            } else {
                expected_error = Some(error.clone());
            }
            assert_eq!(fs::read_to_string(file).unwrap(), *source);
        }
    }
}

fn run_binary(binary: &Path, args: &[&str]) -> Output {
    Command::new(binary).args(args).output().unwrap()
}

fn assert_same_output(actual: &Output, expected: &Output) {
    assert_eq!(actual.status.code(), expected.status.code());
    assert!(actual.stdout == expected.stdout, "stdout differs");
    assert!(actual.stderr == expected.stderr, "stderr differs");
}

fn compare_case(
    baseline: &Path,
    candidate: &Path,
    file: &Path,
    query: Option<&str>,
    size: usize,
    label: &str,
) {
    let mut args = vec!["read", file.to_str().unwrap(), "--json"];
    if let Some(query) = query {
        args.extend(["--symbol", query]);
    }
    let expected = run_binary(baseline, &args);
    assert_same_output(&run_binary(candidate, &args), &expected);
    let response: Value = serde_json::from_slice(&expected.stdout).unwrap();
    let count = if expected.status.success() {
        response["summary"]["matches"].as_u64().unwrap()
    } else {
        response["error"]["candidates"]
            .as_array()
            .map_or(0, |items| items.len() as u64)
    };
    let mut samples = [Vec::new(), Vec::new()];

    for trial in 0..5 {
        for index in if trial % 2 == 0 { [0, 1] } else { [1, 0] } {
            let binary = [baseline, candidate][index];
            let start = Instant::now();
            let actual = run_binary(binary, &args);
            samples[index].push(start.elapsed());
            assert_same_output(&actual, &expected);
        }
    }

    let medians = samples.each_ref().map(|values| {
        let mut sorted = values.clone();
        sorted.sort();
        sorted[sorted.len() / 2]
    });
    println!(
        "case={label} declarations={size} matches_or_candidates={count} status={} baseline_ms={:.3} candidate_ms={:.3} baseline_samples_ms={:?} candidate_samples_ms={:?}",
        expected.status.code().unwrap(),
        medians[0].as_secs_f64() * 1000.0,
        medians[1].as_secs_f64() * 1000.0,
        samples[0]
            .iter()
            .map(Duration::as_millis)
            .collect::<Vec<_>>(),
        samples[1]
            .iter()
            .map(Duration::as_millis)
            .collect::<Vec<_>>()
    );
}

fn verify_inventory(baseline: &Path, candidate: &Path, file: &Path, function_count: usize) {
    let args = ["read", file.to_str().unwrap(), "--json"];
    let expected = run_binary(baseline, &args);
    let inventory = successful_json(&expected);
    assert_same_output(&run_binary(candidate, &args), &expected);
    let handles = inventory["handles"].as_array().unwrap();
    assert_eq!(
        handles
            .iter()
            .filter(|handle| handle["kind"] == "function_definition")
            .count(),
        function_count
    );
    println!(
        "inventory declarations={function_count} handles={}",
        handles.len()
    );
}

#[test]
#[ignore = "requires a pinned pre-optimization release binary in IDENTEDIT_SYMBOL_BASELINE"]
fn compare_symbol_lookup_scaling() {
    let baseline = std::env::var_os("IDENTEDIT_SYMBOL_BASELINE")
        .expect("set IDENTEDIT_SYMBOL_BASELINE to the pre-optimization release binary");
    let baseline = Path::new(&baseline);
    let candidate = Path::new(env!("CARGO_BIN_EXE_identedit"));
    for (label, binary) in [("baseline", baseline), ("candidate", candidate)] {
        println!(
            "{label}={} blake3={}",
            binary.display(),
            blake3::hash(&fs::read(binary).unwrap())
        );
    }
    println!(
        "platform={}-{} debug_assertions={} trials=5 warmups=1 order=alternating",
        std::env::consts::OS,
        std::env::consts::ARCH,
        cfg!(debug_assertions),
    );
    let dir = tempdir().unwrap();

    for size in [2500, 5000, 10000] {
        let file = dir.path().join("flat.py");
        let source = (0..size)
            .map(|index| format!("def f{index}():\n    return {index}\n\n"))
            .collect::<String>();
        fs::write(&file, &source).unwrap();
        verify_inventory(baseline, candidate, &file, size);
        compare_case(baseline, candidate, &file, None, size, "flat-inventory");
        compare_case(baseline, candidate, &file, Some("f0"), size, "flat-bare");
        compare_case(
            baseline,
            candidate,
            &file,
            Some("absent"),
            size,
            "flat-missing",
        );

        let file = dir.path().join("nested.py");
        let source = format!(
            "class Container:\n{}",
            source
                .lines()
                .map(|line| format!("    {line}\n"))
                .collect::<String>()
        );
        fs::write(&file, source).unwrap();
        verify_inventory(baseline, candidate, &file, size);
        compare_case(
            baseline,
            candidate,
            &file,
            Some("Container.f0"),
            size,
            "nested-qualified",
        );
        compare_case(
            baseline,
            candidate,
            &file,
            Some("Other.f0"),
            size,
            "nested-wrong-container",
        );
    }

    for size in [250, 500, 1000] {
        let file = dir.path().join("same-name.py");
        fs::write(&file, "def duplicate():\n    return 0\n\n".repeat(size)).unwrap();
        verify_inventory(baseline, candidate, &file, size);
        compare_case(
            baseline,
            candidate,
            &file,
            Some("duplicate"),
            size,
            "same-name-ambiguity",
        );

        let file = dir.path().join("same-name-qualified.py");
        let source = (0..size)
            .map(|index| format!("class C{index}:\n    def duplicate(self):\n        return 0\n\n"))
            .collect::<String>();
        fs::write(&file, source).unwrap();
        verify_inventory(baseline, candidate, &file, size);
        compare_case(
            baseline,
            candidate,
            &file,
            Some("C0.duplicate"),
            size,
            "same-name-qualified",
        );
    }
}
