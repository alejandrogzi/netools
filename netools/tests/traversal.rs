// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the GNU General Public License v3.0.

//! Traversal API tests over the basic fixture.

use netools::{NodeKind, Reader, TraversalEvent};

const BASIC: &[u8] = include_bytes!("fixtures/basic.net");

fn reader() -> Reader {
    Reader::from_owned_bytes(BASIC.to_vec()).unwrap()
}

#[test]
fn children_parents_and_siblings() {
    let r = reader();
    let net = r.net(0).unwrap();
    let nodes: Vec<_> = net.preorder().collect();

    // node 0 has two gap children: node 1 and node 3.
    let children: Vec<_> = nodes[0].children().map(|n| n.id().get()).collect();
    assert_eq!(children, vec![1, 3]);

    assert_eq!(nodes[1].parent().unwrap().id().get(), 0);
    assert_eq!(nodes[1].next_sibling().unwrap().id().get(), 3);
    assert_eq!(nodes[3].previous_sibling().unwrap().id().get(), 1);
    assert!(nodes[3].next_sibling().is_none());
    assert!(nodes[0].parent().is_none());

    // node 2 is a nested fill under gap node 1.
    assert_eq!(nodes[2].parent().unwrap().id().get(), 1);
    let ancestors: Vec<_> = nodes[2].ancestors().map(|n| n.id().get()).collect();
    assert_eq!(ancestors, vec![1, 0]);
    assert_eq!(nodes[2].root().id().get(), 0);
}

#[test]
fn subtree_and_descendants() {
    let r = reader();
    let net = r.net(0).unwrap();
    let node0 = net.preorder().next().unwrap();

    let subtree: Vec<_> = node0.subtree().map(|n| n.id().get()).collect();
    assert_eq!(subtree, vec![0, 1, 2, 3]);
    let descendants: Vec<_> = node0.descendants().map(|n| n.id().get()).collect();
    assert_eq!(descendants, vec![1, 2, 3]);
    assert_eq!(node0.subtree_len(), 4);
}

#[test]
fn fills_gaps_depth_and_max_depth() {
    let r = reader();
    let net = r.net(0).unwrap();

    let fills: Vec<_> = net.fills().map(|n| n.id().get()).collect();
    assert_eq!(fills, vec![0, 2, 4]);
    let gaps: Vec<_> = net.gaps().map(|n| n.id().get()).collect();
    assert_eq!(gaps, vec![1, 3]);
    let d1: Vec<_> = net.nodes_at_depth(1).map(|n| n.id().get()).collect();
    assert_eq!(d1, vec![1, 3]);
    assert_eq!(net.max_depth(), 2);
}

#[test]
fn child_gaps_with_fills() {
    let r = reader();
    let net = r.net(0).unwrap();
    let root = net.roots().next().unwrap();
    let gaps_with_fills: Vec<_> = root.child_gaps_with_fills().map(|n| n.id().get()).collect();
    // Only the first gap (node 1) has a nested fill.
    assert_eq!(gaps_with_fills, vec![1]);
}

#[test]
fn event_traversal_is_balanced() {
    let r = reader();
    let net = r.net(0).unwrap();
    let mut enters = 0;
    let mut leaves = 0;
    let mut depth: i32 = 0;
    let mut max_depth = 0;
    for event in net.events() {
        match event {
            TraversalEvent::Enter(_) => {
                enters += 1;
                depth += 1;
                max_depth = max_depth.max(depth);
            }
            TraversalEvent::Leave(_) => {
                leaves += 1;
                depth -= 1;
            }
        }
        assert!(depth >= 0, "leave without matching enter");
    }
    assert_eq!(enters, 5);
    assert_eq!(leaves, 5);
    assert_eq!(depth, 0);
    assert_eq!(max_depth, 3); // depths 0,1,2 -> nesting of 3
}

#[test]
fn visitor_counts_kinds() {
    use std::ops::ControlFlow;
    struct Counter {
        fills: usize,
        gaps: usize,
    }
    impl netools::NetVisitor for Counter {
        fn enter(&mut self, node: netools::NodeRef<'_>) -> ControlFlow<()> {
            match node.kind() {
                NodeKind::Fill => self.fills += 1,
                NodeKind::Gap => self.gaps += 1,
            }
            ControlFlow::Continue(())
        }
    }
    let r = reader();
    let net = r.net(0).unwrap();
    let mut counter = Counter { fills: 0, gaps: 0 };
    let _ = net.walk(&mut counter);
    assert_eq!(counter.fills, 3);
    assert_eq!(counter.gaps, 2);
}
