// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the GNU General Public License v3.0.

//! CLI integration tests. Requires the `cli` feature (the binary target).
#![cfg(feature = "cli")]

use std::path::PathBuf;
use std::process::Command;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_netools")
}

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/basic.net")
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
    let (_out, ok) = run(&["merge", path, path]);
    assert!(!ok, "duplicate references should fail by default");

    let (out, ok) = run(&["merge", "--duplicates", "keep-all", path, path]);
    assert!(ok);
    assert_eq!(out.matches("net chr").count(), 4);
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
