// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the GNU General Public License v3.0.

//! End-to-end parsing tests over the sequential reader.

use netools::{ChainId, KnownAttr, NetType, NodeKind, ParseMode, Reader, Strand};

const BASIC: &[u8] = include_bytes!("fixtures/basic.net");

fn basic_reader() -> Reader {
    Reader::from_owned_bytes(BASIC.to_vec()).expect("basic.net parses")
}

#[test]
fn counts_sections_and_records() {
    let reader = basic_reader();
    assert_eq!(reader.len(), 2);
    assert_eq!(reader.node_count(), 6);

    let nets: Vec<_> = reader.nets().collect();
    assert_eq!(nets[0].reference_name_bytes(), b"chr1");
    assert_eq!(nets[0].reference_size(), 248_956_422);
    assert_eq!(nets[0].len(), 5);
    assert_eq!(nets[0].root_count(), 2);

    assert_eq!(nets[1].reference_name_bytes(), b"chr2");
    assert_eq!(nets[1].reference_size(), 242_193_529);
    assert_eq!(nets[1].len(), 1);
    assert_eq!(nets[1].root_count(), 1);
}

#[test]
fn preorder_records_have_expected_shape() {
    let reader = basic_reader();
    let net = reader.net(0).unwrap();
    let nodes: Vec<_> = net.preorder().collect();
    assert_eq!(nodes.len(), 5);

    // Node 0: root fill.
    assert_eq!(nodes[0].kind(), NodeKind::Fill);
    assert_eq!(nodes[0].depth(), 0);
    assert_eq!(nodes[0].reference_start(), 10_000);
    assert_eq!(nodes[0].reference_end(), 60_000);
    assert_eq!(nodes[0].query_name_bytes(), b"chr2");
    assert_eq!(nodes[0].query_strand(), Strand::Forward);
    assert_eq!(nodes[0].query_start(), 30_000);
    assert_eq!(nodes[0].query_end(), 78_000);
    assert_eq!(nodes[0].chain_id(), Some(1 as ChainId));
    assert_eq!(nodes[0].attributes().alignment_score(), Some(300_000.0));
    assert_eq!(nodes[0].attributes().aligned_bases(), Some(45_000));
    assert_eq!(nodes[0].attributes().net_type(), Some(NetType::Top));

    // Node 1: gap at depth 1.
    assert_eq!(nodes[1].kind(), NodeKind::Gap);
    assert_eq!(nodes[1].depth(), 1);
    assert_eq!(nodes[1].reference_range().start, 20_000);
    assert_eq!(nodes[1].reference_range().end, 25_000);
    assert!(nodes[1].chain_id().is_none());

    // Node 2: nested fill at depth 2 on the minus strand.
    assert_eq!(nodes[2].kind(), NodeKind::Fill);
    assert_eq!(nodes[2].depth(), 2);
    assert_eq!(nodes[2].query_strand(), Strand::Reverse);
    assert_eq!(nodes[2].query_name_bytes(), b"chr3");
    assert_eq!(nodes[2].attributes().net_type(), Some(NetType::Syn));

    // Node 3: sibling gap at depth 1.
    assert_eq!(nodes[3].kind(), NodeKind::Gap);
    assert_eq!(nodes[3].depth(), 1);

    // Node 4: second root fill with an unknown `foo bar` attribute preserved.
    assert_eq!(nodes[4].kind(), NodeKind::Fill);
    assert_eq!(nodes[4].depth(), 0);
    assert_eq!(nodes[4].attributes().net_type(), Some(NetType::NonSyn));
    let extras: Vec<_> = nodes[4].attributes().extras().collect();
    assert_eq!(extras, vec![(b"foo".as_slice(), b"bar".as_slice())]);
    assert!(nodes[4].attributes().has(KnownAttr::ChainId));
}

#[test]
fn empty_input_is_an_error() {
    assert!(Reader::from_owned_bytes(Vec::new()).is_err());
    assert!(Reader::from_owned_bytes(b"   \n\n".to_vec()).is_err());
}

#[test]
fn missing_header_is_an_error() {
    let err = Reader::from_owned_bytes(b" fill 0 10 chr2 + 0 10 id 1\n".to_vec()).unwrap_err();
    assert_eq!(err.kind(), netools::NetErrorKind::MissingNetHeader);
}

#[test]
fn crlf_and_missing_final_newline() {
    let text = "net chrA 100\r\n fill 0 10 chrB + 0 10 id 1";
    let reader = Reader::from_owned_bytes(text.as_bytes().to_vec()).unwrap();
    assert_eq!(reader.len(), 1);
    let net = reader.net(0).unwrap();
    assert_eq!(net.reference_name_bytes(), b"chrA");
    assert_eq!(net.len(), 1);
    let first_id = net.preorder().next().unwrap().id();
    assert!(net.node(first_id).is_some());
}

#[test]
fn strict_rejects_missing_fill_id() {
    let text = "net chrA 100\n fill 0 10 chrB + 0 10\n";
    let permissive = Reader::options()
        .parse_mode(ParseMode::Compatible)
        .from_owned_bytes(text.as_bytes().to_vec());
    assert!(permissive.is_ok());

    let strict = Reader::options()
        .parse_mode(ParseMode::Strict)
        .from_owned_bytes(text.as_bytes().to_vec());
    assert!(strict.is_err());
}

#[test]
fn invalid_strand_and_overflow() {
    let bad_strand = "net chrA 100\n fill 0 10 chrB ? 0 10 id 1\n";
    assert_eq!(
        Reader::from_owned_bytes(bad_strand.as_bytes().to_vec())
            .unwrap_err()
            .kind(),
        netools::NetErrorKind::InvalidStrand
    );

    let overflow = "net chrA 100\n fill 4294967295 1 chrB + 0 10 id 1\n";
    assert_eq!(
        Reader::from_owned_bytes(overflow.as_bytes().to_vec())
            .unwrap_err()
            .kind(),
        netools::NetErrorKind::CoordinateOverflow
    );
}
