// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the GNU General Public License v3.0.

//! Streaming and owned-section tests. Requires the `write` feature for
//! canonical-string comparisons.
#![cfg(feature = "write")]

use std::io::Cursor;

use netools::{NetEvent, Reader, StreamingReader, ValidationMode};

const BASIC: &[u8] = include_bytes!("fixtures/basic.net");

fn multi_section() -> Vec<u8> {
    let mut s = String::new();
    for i in 0..8 {
        s.push_str(&format!("net chr{i} {}\n", 500_000 + i));
        s.push_str(&format!(
            " fill 0 {sz} q{i} + 0 {sz} id {id} score {sc} type top\n  gap 10 5 q{i} + 10 5\n   fill 11 3 r{i} - 0 3 id {id2} score 1 type syn\n",
            sz = 1000 + i,
            id = i + 1,
            sc = 100 + i,
            id2 = 100 + i,
        ));
    }
    s.into_bytes()
}

#[test]
fn streaming_matches_full_reader() {
    for bytes in [BASIC.to_vec(), multi_section()] {
        let full = Reader::from_owned_bytes(bytes.clone()).unwrap();
        let mut stream = StreamingReader::new(Cursor::new(bytes));

        let mut i = 0;
        while let Some(owned) = stream.next_net().unwrap() {
            let full_net = full.net(i).unwrap();
            assert_eq!(owned.reference_name(), full_net.reference_name_bytes());
            assert_eq!(owned.reference_size(), full_net.reference_size());
            assert_eq!(owned.len(), full_net.len());
            assert_eq!(owned.to_net_string(), full_net.to_net_string());
            i += 1;
        }
        assert_eq!(i, full.len());
    }
}

#[test]
fn event_stream_structure() {
    let bytes = multi_section();
    let full = Reader::from_owned_bytes(bytes.clone()).unwrap();
    let mut stream = StreamingReader::new(Cursor::new(bytes));

    let mut starts = 0;
    let mut records = 0;
    let mut ends = 0;
    let mut open = false;
    while let Some(event) = stream.next_event().unwrap() {
        match event {
            NetEvent::NetStart { .. } => {
                assert!(!open, "nested NetStart");
                open = true;
                starts += 1;
            }
            NetEvent::Record { .. } => {
                assert!(open, "record outside a section");
                records += 1;
            }
            NetEvent::NetEnd => {
                assert!(open, "NetEnd without NetStart");
                open = false;
                ends += 1;
            }
        }
    }
    assert!(!open);
    assert_eq!(starts, full.len());
    assert_eq!(ends, full.len());
    assert_eq!(records, full.node_count());
}

#[test]
fn streaming_reports_stray_record_before_header() {
    let bytes = b" fill 0 10 q + 0 10 id 1\nnet chrA 100\n".to_vec();
    let mut stream = StreamingReader::new(Cursor::new(bytes));
    assert!(stream.next_net().is_err());
}

#[test]
fn to_owned_and_compact_preserve_output() {
    let full = Reader::from_owned_bytes(BASIC.to_vec()).unwrap();
    let net = full.net(0).unwrap();

    let mut owned = net.to_owned();
    let before = owned.to_net_string();
    assert_eq!(before, net.to_net_string());

    owned.compact();
    let after = owned.to_net_string();
    assert_eq!(before, after);
    assert!(!owned.validate(ValidationMode::Strict).has_errors());
}

#[test]
fn clone_subtree_rebases_depth() {
    let full = Reader::from_owned_bytes(BASIC.to_vec()).unwrap();
    let net = full.net(0).unwrap();
    // The first gap (preorder index 1) contains a nested fill (index 2).
    let gap = net.preorder().nth(1).unwrap();
    assert_eq!(gap.kind(), netools::NodeKind::Gap);
    let subtree = gap.clone_subtree();
    assert_eq!(subtree.len(), 2);
    let view = subtree.as_ref();
    let roots: Vec<_> = view.roots().collect();
    assert_eq!(roots.len(), 1);
    assert_eq!(roots[0].depth(), 0);
    assert_eq!(roots[0].kind(), netools::NodeKind::Gap);
    assert_eq!(view.max_depth(), 1);
}
