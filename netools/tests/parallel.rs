// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the GNU General Public License v3.0.

//! Sequential/parallel parity. Requires the `parallel` and `write` features.
#![cfg(all(feature = "parallel", feature = "write"))]

use netools::{Reader, Writer};

const BASIC: &[u8] = include_bytes!("fixtures/basic.net");

fn serialize(reader: &Reader) -> Vec<u8> {
    let mut writer = Writer::new(Vec::new());
    for net in reader.nets() {
        writer.write_net(net).unwrap();
    }
    writer.into_inner()
}

fn parse_seq(bytes: &[u8]) -> Reader {
    Reader::options()
        .parallel(false)
        .from_owned_bytes(bytes.to_vec())
        .unwrap()
}

fn parse_par(bytes: &[u8]) -> Reader {
    Reader::options()
        .parallel(true)
        .from_owned_bytes(bytes.to_vec())
        .unwrap()
}

/// A many-section document to exercise real fan-out.
fn many_sections(n: usize) -> Vec<u8> {
    let mut s = String::new();
    for i in 0..n {
        s.push_str(&format!("net chr{i} {}\n", 1_000_000 + i));
        s.push_str(&format!(
            " fill 0 {size} q{i} + 0 {size} id {id} score {score} ali {ali} type top\n",
            size = 1000 + i,
            id = i + 1,
            score = 100 * (i + 1),
            ali = 900 + i,
        ));
        s.push_str(&format!(
            "  gap {gs} 100 q{i} + {gs} 100\n   fill {gf} 50 r{i} - 0 50 id {id2} score 5 type syn\n",
            gs = 100 + i,
            gf = 110 + i,
            id2 = 1000 + i,
        ));
    }
    s.into_bytes()
}

fn assert_parity(bytes: &[u8]) {
    let seq = parse_seq(bytes);
    let par = parse_par(bytes);
    assert_eq!(seq.len(), par.len());
    assert_eq!(seq.node_count(), par.node_count());
    assert_eq!(
        serialize(&seq),
        serialize(&par),
        "sequential and parallel canonical output differ"
    );
}

#[test]
fn basic_fixture_parity() {
    assert_parity(BASIC);
}

#[test]
fn many_sections_parity() {
    assert_parity(&many_sections(64));
}

#[test]
fn parity_across_thread_counts() {
    let bytes = many_sections(40);
    let seq_out = serialize(&parse_seq(&bytes));

    for threads in [1usize, 2, 4, 16] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .unwrap();
        let out = pool.install(|| serialize(&parse_par(&bytes)));
        assert_eq!(out, seq_out, "mismatch with {threads} threads");
    }
}

#[test]
fn parallel_error_is_deterministic() {
    // The stray strand error is in the second section; both parsers must report
    // the same error kind.
    let bytes =
        "net chrA 1000\n fill 0 10 q + 0 10 id 1\nnet chrB 1000\n fill 0 10 q ? 0 10 id 1\n";
    let seq = Reader::options()
        .parallel(false)
        .from_owned_bytes(bytes.as_bytes().to_vec())
        .unwrap_err();
    let par = Reader::options()
        .parallel(true)
        .from_owned_bytes(bytes.as_bytes().to_vec())
        .unwrap_err();
    assert_eq!(seq.kind(), par.kind());
    assert_eq!(seq.byte_offset, par.byte_offset);
}

#[test]
fn par_nets_visits_all_sections() {
    use rayon::prelude::*;
    let reader = parse_par(&many_sections(32));
    let total: usize = reader.par_nets().map(|net| net.len()).sum();
    let expected: usize = reader.nets().map(|net| net.len()).sum();
    assert_eq!(total, expected);
}
