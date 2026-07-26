// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the GNU General Public License v3.0.

//! CLI integration tests. Requires the `cli` feature (the binary target).
#![cfg(feature = "cli")]

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_netools")
}

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/basic.net")
}

fn temp_dir(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("netools-{label}-{}-{nonce}", std::process::id()));
    std::fs::create_dir_all(&path).unwrap();
    path
}

fn run(args: &[&str]) -> (String, bool) {
    let output = Command::new(bin())
        .args(args)
        .output()
        .expect("run netools");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.success(),
    )
}

#[test]
fn stats_reports_counts() {
    let (out, ok) = run(&["stats", fixture().to_str().unwrap()]);
    assert!(ok);
    assert!(out.contains("reference nets       2"));
    assert!(out.contains("fills              4"));
    assert!(out.contains("gaps               2"));
}

#[test]
fn validate_succeeds_on_valid_file() {
    let (_out, ok) = run(&["validate", "--mode", "strict", fixture().to_str().unwrap()]);
    assert!(ok);
}

#[test]
fn filter_prunes_to_fills() {
    let (out, ok) = run(&[
        "filter",
        "--kind",
        "fill",
        "--min-score",
        "10000",
        fixture().to_str().unwrap(),
    ]);
    assert!(ok);
    // Two qualifying root fills remain; gaps are pruned.
    assert_eq!(out.matches("fill ").count(), 2);
    assert_eq!(out.matches("gap ").count(), 0);
}

#[test]
fn view_flat_has_one_row_per_record() {
    let (out, ok) = run(&["view", "--flat", fixture().to_str().unwrap()]);
    assert!(ok);
    assert_eq!(out.lines().count(), 6);
}

#[test]
fn sort_natural_orders_sections() {
    // Reverse-ordered sections should come out chr1 before chr2.
    let (out, ok) = run(&["sort", "--nets", "natural", fixture().to_str().unwrap()]);
    assert!(ok);
    let chr1 = out.find("net chr1").unwrap();
    let chr2 = out.find("net chr2").unwrap();
    assert!(chr1 < chr2);
}

#[test]
fn merge_errors_on_duplicate() {
    let f = fixture();
    let path = f.to_str().unwrap();
    let inputs = format!("{path},{path}");
    let (_out, ok) = run(&["merge", "--nets", &inputs]);
    assert!(!ok, "duplicate references should fail by default");

    let (out, ok) = run(&["merge", "--nets", &inputs, "--duplicates", "keep-all"]);
    assert!(ok);
    assert_eq!(out.matches("net chr").count(), 4);

    let root = temp_dir("merge-list");
    let list = root.join("nets.list");
    std::fs::write(&list, format!("{path}\n\n{path}\n")).unwrap();
    let (out, ok) = run(&[
        "merge",
        "--file",
        list.to_str().unwrap(),
        "--duplicates",
        "keep-all",
    ]);
    assert!(ok);
    assert_eq!(out.matches("net chr").count(), 4);

    let (_out, ok) = run(&["merge", path]);
    assert!(!ok, "positional merge inputs must be rejected");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn split_uses_net_flag_or_stdin_and_rejects_positionals() {
    use std::io::Write;

    let root = temp_dir("split-inputs");
    let from_file = root.join("from-file");
    let manifest = root.join("manifest.tsv");
    let fixture = fixture();
    let result = Command::new(bin())
        .args([
            "split",
            "--net",
            fixture.to_str().unwrap(),
            "--out-dir",
            from_file.to_str().unwrap(),
            "--manifest",
            manifest.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(from_file.join("chr1.net").is_file());
    assert!(from_file.join("chr2.net").is_file());
    assert!(manifest.is_file());

    let from_stdin = root.join("from-stdin");
    let mut child = Command::new(bin())
        .args(["split", "--out-dir", from_stdin.to_str().unwrap()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&std::fs::read(&fixture).unwrap())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(from_stdin.join("chr1.net").is_file());
    assert!(from_stdin.join("chr2.net").is_file());

    let positional = root.join("positional");
    let result = Command::new(bin())
        .args([
            "split",
            fixture.to_str().unwrap(),
            "--out-dir",
            positional.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!result.status.success(), "positional split input must fail");
    assert!(!positional.exists());

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn round_trips_through_stdin() {
    // `view` with no filters is an identity canonicalisation; feeding the fixture
    // via stdin should reproduce it.
    use std::io::Write;
    let mut child = Command::new(bin())
        .args(["view", "-"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let input = std::fs::read(fixture()).unwrap();
    child.stdin.take().unwrap().write_all(&input).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, input);
}

#[test]
fn net_ccr_preset_uses_chain_flag_or_stdin() {
    use std::io::Write;

    let dir = temp_dir("net-chain-inputs");
    let chain = dir.join("input.chain");
    let reference_sizes = dir.join("reference.sizes");
    let query_sizes = dir.join("query.sizes");
    let path_output = dir.join("path-reference.net");
    let stdin_output = dir.join("stdin-reference.net");
    std::fs::write(
        &chain,
        "chain 5000 chr1 1000 + 100 300 q1 1000 + 200 400 1\n200\n",
    )
    .unwrap();
    std::fs::write(&reference_sizes, "chr1\t1000\n").unwrap();
    std::fs::write(&query_sizes, "q1\t1000\n").unwrap();

    let result = Command::new(bin())
        .args([
            "net",
            "--chain",
            chain.to_str().unwrap(),
            "--reference-sizes",
            reference_sizes.to_str().unwrap(),
            "--query-sizes",
            query_sizes.to_str().unwrap(),
            "--reference-net",
            path_output.to_str().unwrap(),
            "--preset",
            "ccr",
        ])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let expected = concat!(
        "net chr1 1000\n",
        " fill 100 200 q1 + 200 200 id 1 score 5000 ali 200\n",
    );
    assert_eq!(std::fs::read_to_string(&path_output).unwrap(), expected);

    let mut child = Command::new(bin())
        .args([
            "net",
            "--reference-sizes",
            reference_sizes.to_str().unwrap(),
            "--query-sizes",
            query_sizes.to_str().unwrap(),
            "--reference-net",
            stdin_output.to_str().unwrap(),
            "--preset",
            "ccr",
        ])
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&std::fs::read(&chain).unwrap())
        .unwrap();
    let status = child.wait().unwrap();
    assert!(status.success());
    assert_eq!(std::fs::read_to_string(&stdin_output).unwrap(), expected);

    let result = Command::new(bin())
        .args([
            "net",
            chain.to_str().unwrap(),
            "--reference-sizes",
            reference_sizes.to_str().unwrap(),
            "--query-sizes",
            query_sizes.to_str().unwrap(),
            "--reference-net",
            path_output.to_str().unwrap(),
            "--preset",
            "ccr",
        ])
        .output()
        .unwrap();
    assert!(!result.status.success(), "positional chain input must fail");

    let result = Command::new(bin())
        .args(["chain-net", "--help"])
        .output()
        .unwrap();
    assert!(
        !result.status.success(),
        "the old chain-net subcommand must not remain as an alias"
    );

    std::fs::remove_dir_all(dir).unwrap();
}
