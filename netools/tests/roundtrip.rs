// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the GNU General Public License v3.0.

//! Parse -> write -> parse round-trip equivalence. Requires the `write` feature.
#![cfg(feature = "write")]

use netools::{NetRef, Reader, Writer};

const BASIC: &[u8] = include_bytes!("fixtures/basic.net");

fn serialize(reader: &Reader) -> Vec<u8> {
    let mut writer = Writer::new(Vec::new());
    for net in reader.nets() {
        writer.write_net(net).unwrap();
    }
    writer.into_inner()
}

/// Compare two sections for full semantic equivalence.
fn assert_nets_equal(a: NetRef<'_>, b: NetRef<'_>) {
    assert_eq!(a.reference_name_bytes(), b.reference_name_bytes());
    assert_eq!(a.reference_size(), b.reference_size());
    assert_eq!(a.len(), b.len());

    let an: Vec<_> = a.preorder().collect();
    let bn: Vec<_> = b.preorder().collect();
    assert_eq!(an.len(), bn.len());

    for (x, y) in an.iter().zip(bn.iter()) {
        assert_eq!(x.kind(), y.kind());
        assert_eq!(x.depth(), y.depth());
        assert_eq!(x.reference_range(), y.reference_range());
        assert_eq!(x.query_range(), y.query_range());
        assert_eq!(x.query_strand(), y.query_strand());
        assert_eq!(x.query_name_bytes(), y.query_name_bytes());

        let xa = x.attributes();
        let ya = y.attributes();
        assert_eq!(xa.mask(), ya.mask());
        assert_eq!(xa.chain_id(), ya.chain_id());
        assert_eq!(xa.alignment_score(), ya.alignment_score());
        assert_eq!(xa.aligned_bases(), ya.aligned_bases());
        assert_eq!(xa.net_type(), ya.net_type());
        let xe: Vec<_> = xa.extras().collect();
        let ye: Vec<_> = ya.extras().collect();
        assert_eq!(xe, ye);
    }
}

fn roundtrip_bytes(input: &[u8]) {
    let first = Reader::from_owned_bytes(input.to_vec()).unwrap();
    let written = serialize(&first);
    let second = Reader::from_owned_bytes(written.clone()).unwrap();

    assert_eq!(first.len(), second.len());
    for (a, b) in first.nets().zip(second.nets()) {
        assert_nets_equal(a, b);
    }

    // Writing again must be byte-identical (writer output is a fixed point).
    let third = serialize(&second);
    assert_eq!(written, third);
}

#[test]
fn basic_fixture_roundtrips() {
    roundtrip_bytes(BASIC);
}

#[test]
fn varied_attributes_roundtrip() {
    let input = "\
net chr1 1000000
 fill 0 500000 chrQ - 100 480000 id 10 score 12345 ali 470000 qFar 3 qOver 0 qDup 12 type syn tN 5 qN 7 tR 100 qR 200 custom xyz another 42
  gap 1000 2000 chrQ - 1500 1900 tN 3 qN 0
   fill 1200 500 chrR + 0 500 id 11 score 5 type inv
";
    roundtrip_bytes(input.as_bytes());
}

#[test]
fn negative_counts_and_signs_roundtrip() {
    let input = "net chrA 100000\n fill 0 100 chrB + 0 100 id 1 qFar -5 tN 0\n";
    roundtrip_bytes(input.as_bytes());
}

#[test]
fn permissive_noncanonical_indent_normalizes() {
    // Non-canonical indentation (jumps) parsed permissively then written
    // canonically must survive a re-parse with the same structure.
    let input = "net chrA 100000\n   fill 0 100 chrB + 0 100 id 1\n      gap 10 10 chrB + 10 10\n";
    let first = Reader::options()
        .parse_mode(netools::ParseMode::Permissive)
        .from_owned_bytes(input.as_bytes().to_vec())
        .unwrap();
    let written = serialize(&first);
    let second = Reader::from_owned_bytes(written).unwrap();
    assert_nets_equal(first.net(0).unwrap(), second.net(0).unwrap());
    assert_eq!(second.net(0).unwrap().max_depth(), 1);
}
