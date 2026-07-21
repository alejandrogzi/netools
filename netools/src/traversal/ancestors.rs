// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the GNU General Public License v3.0.

//! Ancestor navigation.

use crate::io::reader::{NetStore, NodeRef};
use crate::model::ids::opt_node;

/// Iterator from a record up to (and excluding) the root, yielding each
/// ancestor nearest-first.
#[derive(Clone)]
pub struct AncestorIter<'a> {
    store: &'a NetStore,
    next: u32,
}

impl<'a> AncestorIter<'a> {
    #[inline]
    pub(crate) fn new(store: &'a NetStore, first: u32) -> AncestorIter<'a> {
        AncestorIter { store, next: first }
    }
}

impl<'a> Iterator for AncestorIter<'a> {
    type Item = NodeRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let id = opt_node(self.next)?;
        self.next = self.store.nodes[id.index()].parent;
        Some(NodeRef {
            store: self.store,
            id,
        })
    }
}

impl<'a> NodeRef<'a> {
    /// Iterate this record's ancestors, nearest first, up to the root.
    #[inline]
    pub fn ancestors(&self) -> AncestorIter<'a> {
        AncestorIter::new(self.store, self.node().parent)
    }

    /// The root record at the top of this record's path.
    pub fn root(&self) -> NodeRef<'a> {
        let mut current = *self;
        while let Some(parent) = current.parent() {
            current = parent;
        }
        current
    }

    /// The nearest ancestor of the given kind, if any.
    pub fn enclosing(&self, kind: crate::model::NodeKind) -> Option<NodeRef<'a>> {
        self.ancestors().find(|n| n.kind() == kind)
    }

    /// The nearest enclosing gap record, if any.
    #[inline]
    pub fn enclosing_gap(&self) -> Option<NodeRef<'a>> {
        self.enclosing(crate::model::NodeKind::Gap)
    }

    /// The nearest enclosing fill record, if any.
    #[inline]
    pub fn enclosing_fill(&self) -> Option<NodeRef<'a>> {
        self.enclosing(crate::model::NodeKind::Fill)
    }

    /// The nearest enclosing fill record that carries a chain id, if any.
    pub fn enclosing_fill_with_chain_id(&self) -> Option<NodeRef<'a>> {
        self.ancestors()
            .find(|n| n.kind() == crate::model::NodeKind::Fill && n.chain_id().is_some())
    }
}
