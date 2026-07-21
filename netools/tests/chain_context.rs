// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the GNU General Public License v3.0.

//! chainCleaner-oriented context and index tests.

use netools::{NetRange, Reader};

const BASIC: &[u8] = include_bytes!("fixtures/basic.net");

fn reader() -> Reader {
    Reader::from_owned_bytes(BASIC.to_vec()).unwrap()
}

#[test]
fn nested_fill_gap_contexts() {
    let r = reader();
    let net = r.net(0).unwrap();
    let contexts: Vec<_> = net.nested_fill_gap_contexts().collect();
    assert_eq!(contexts.len(), 1);
    let c = contexts[0];
    assert_eq!(c.fill_chain_id, 2);
    assert_eq!(c.parent_chain_id, 1);
    assert_eq!(c.fill_range, NetRange::new(21_000, 24_000));
    assert_eq!(c.parent_gap_range, NetRange::new(20_000, 25_000));
    assert_eq!(c.depth, 2);
}

#[test]
fn uninterrupted_spans_exclude_gaps_with_fills() {
    let r = reader();
    let net = r.net(0).unwrap();
    let root = net.roots().next().unwrap(); // fill 10000..60000, id 1
    let spans = root.uninterrupted_reference_spans();
    let ranges: Vec<_> = spans.iter().map(|s| s.reference_range).collect();
    // The first gap (20000..25000) has a nested fill and is excluded; the second
    // gap (30000..32000) has no fills and is not a cut.
    assert_eq!(
        ranges,
        vec![NetRange::new(10_000, 20_000), NetRange::new(25_000, 60_000)]
    );
    assert!(spans.iter().all(|s| s.chain_id == 1));
}

#[test]
fn chain_occurrences_and_used_ids() {
    let r = reader();
    let occ1: Vec<_> = r.chain_occurrences(1).collect();
    assert_eq!(occ1.len(), 1);
    assert_eq!(occ1[0].context.chain_id, 1);

    let occ2: Vec<_> = r.chain_occurrences(2).collect();
    assert_eq!(occ2.len(), 1);
    assert_eq!(occ2[0].fill.depth(), 2);

    assert_eq!(r.used_chain_ids(), vec![1, 2, 3, 5]);
    assert_eq!(r.net(0).unwrap().used_chain_ids(), vec![1, 2, 3]);
}

#[test]
fn alignment_span_index_queries() {
    let r = reader();
    let net = r.net(0).unwrap();
    let index = net.alignment_span_index();

    let hits: Vec<_> = index
        .overlapping(NetRange::new(22_000, 23_000))
        .map(|s| s.chain_id)
        .collect();
    assert_eq!(hits, vec![2]);

    // A higher-ranking chain (lower id here) overlapping the candidate region.
    assert!(index.any_overlap_where(NetRange::new(15_000, 16_000), |s| s.chain_id == 1));
    assert!(!index.any_overlap_where(NetRange::new(70_000, 80_000), |_| true));
}

#[test]
fn enclosing_helpers() {
    let r = reader();
    let net = r.net(0).unwrap();
    let nested = net.preorder().nth(2).unwrap(); // fill id 2 at depth 2
    assert_eq!(nested.enclosing_gap().unwrap().id().get(), 1);
    assert_eq!(nested.enclosing_fill().unwrap().id().get(), 0);
    assert_eq!(
        nested.enclosing_fill_with_chain_id().unwrap().chain_id(),
        Some(1)
    );
}

#[cfg(feature = "index")]
mod indexes {
    use super::*;
    use netools::{NetIndex, NetRange};

    #[test]
    fn section_index_scans_headers() {
        let index = NetIndex::from_bytes(BASIC.to_vec()).unwrap();
        assert_eq!(index.len(), 2);
        assert_eq!(index.get(b"chr1").unwrap().reference_size, 248_956_422);
        assert_eq!(index.get(b"chr2").unwrap().reference_size, 242_193_529);
        assert!(index.get(b"chrX").is_none());
        let (start, end) = index.net_bytes(index.get(b"chr2").unwrap().net_id).unwrap();
        assert!(end > start);
    }

    #[test]
    fn chain_id_index() {
        let r = reader();
        let index = r.chain_id_index();
        assert_eq!(index.occurrences(2).len(), 1);
        assert_eq!(index.occurrences(5).len(), 1);
        assert_eq!(index.occurrences(5)[0].net_id.get(), 1);
        assert!(!index.contains(999));
    }

    #[test]
    fn reference_interval_index() {
        let r = reader();
        let net = r.net(0).unwrap();
        let index = net.reference_interval_index();
        let overlapping: Vec<_> = index
            .overlapping(NetRange::new(22_000, 22_500))
            .map(|i| i.node_id.get())
            .collect();
        assert_eq!(overlapping, vec![0, 1, 2]);

        let containing: Vec<_> = index
            .containing(NetRange::new(22_000, 22_500))
            .map(|i| i.node_id.get())
            .collect();
        assert_eq!(containing, vec![0, 1, 2]);
    }
}
