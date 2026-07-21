// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the GNU General Public License v3.0.

//! Library-level filter and sort tests. Requires the `write` feature.
#![cfg(feature = "write")]

use netools::{NetPredicate, NodeKind, Reader, TreeFilterPolicy};

const BASIC: &[u8] = include_bytes!("fixtures/basic.net");

fn reader() -> Reader {
    Reader::from_owned_bytes(BASIC.to_vec()).unwrap()
}

fn kinds(kind: NodeKind) -> NetPredicate {
    NetPredicate {
        kinds: Some([kind].into_iter().collect()),
        ..NetPredicate::new()
    }
}

#[test]
fn prune_removes_failing_subtrees() {
    let r = reader();
    let net = r.net(0).unwrap();
    let filtered = net.filter(&kinds(NodeKind::Fill), TreeFilterPolicy::Prune);
    let view = filtered.as_ref();
    // Only the two root fills survive; the gaps and the nested fill are pruned.
    let kinds: Vec<_> = view.preorder().map(|n| (n.depth(), n.kind())).collect();
    assert_eq!(kinds, vec![(0, NodeKind::Fill), (0, NodeKind::Fill)]);
}

#[test]
fn retain_ancestors_keeps_paths() {
    let r = reader();
    let net = r.net(0).unwrap();
    let pred = NetPredicate {
        chain_ids: Some([2u64].into_iter().collect()),
        ..NetPredicate::new()
    };
    let filtered = net.filter(&pred, TreeFilterPolicy::RetainAncestors);
    let view = filtered.as_ref();
    // The nested fill (chain 2) plus the gap and root fill on its path.
    let ids: Vec<_> = view.preorder().map(|n| (n.depth(), n.kind())).collect();
    assert_eq!(
        ids,
        vec![(0, NodeKind::Fill), (1, NodeKind::Gap), (2, NodeKind::Fill)]
    );
}

#[test]
fn promote_reparents_to_nearest_kept_ancestor() {
    let r = reader();
    let net = r.net(0).unwrap();
    let filtered = net.filter(&kinds(NodeKind::Fill), TreeFilterPolicy::Promote);
    let view = filtered.as_ref();
    // Fills only; the nested fill is reparented under the first root fill.
    let shape: Vec<_> = view.preorder().map(|n| (n.depth(), n.kind())).collect();
    assert_eq!(
        shape,
        vec![
            (0, NodeKind::Fill), // root fill 1
            (1, NodeKind::Fill), // nested fill 2, promoted under fill 1
            (0, NodeKind::Fill), // root fill 3
        ]
    );
    // The promoted structure re-parses to the same tree.
    let text = filtered.to_net_string();
    let reparsed = Reader::from_owned_bytes(text.into_bytes()).unwrap();
    assert_eq!(reparsed.net(0).unwrap().max_depth(), 1);
}

#[test]
fn sort_records_orders_siblings() {
    // Two root fills given out of reference order; sort_records reorders them.
    let text = "\
net chrA 100000
 fill 5000 100 q + 0 100 id 2
 fill 1000 100 q + 0 100 id 1
";
    let r = Reader::from_owned_bytes(text.as_bytes().to_vec()).unwrap();
    let sorted = r.net(0).unwrap().sort_records();
    let starts: Vec<_> = sorted
        .as_ref()
        .preorder()
        .map(|n| n.reference_start())
        .collect();
    assert_eq!(starts, vec![1000, 5000]);
}

#[test]
fn filter_is_semantically_stable_under_reparse() {
    let r = reader();
    let net = r.net(0).unwrap();
    let filtered = net.filter(&kinds(NodeKind::Fill), TreeFilterPolicy::Prune);
    let text = filtered.to_net_string();
    let reparsed = Reader::from_owned_bytes(text.into_bytes()).unwrap();
    assert_eq!(
        reparsed.net(0).unwrap().to_net_string(),
        filtered.to_net_string()
    );
}
