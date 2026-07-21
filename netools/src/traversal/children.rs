// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the GNU General Public License v3.0.

//! Sibling, child, and root navigation.

use crate::io::reader::{NetRef, NetStore, NodeRef};
use crate::model::NodeId;
use crate::model::ids::{NO_NODE, opt_node};

/// Iterator over a sibling chain (roots of a section or children of a node).
#[derive(Clone)]
pub struct SiblingIter<'a> {
    store: &'a NetStore,
    next: u32,
}

impl<'a> SiblingIter<'a> {
    #[inline]
    pub(crate) fn new(store: &'a NetStore, first: u32) -> SiblingIter<'a> {
        SiblingIter { store, next: first }
    }
}

impl<'a> Iterator for SiblingIter<'a> {
    type Item = NodeRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let id = opt_node(self.next)?;
        self.next = self.store.nodes[id.index()].next_sibling;
        Some(NodeRef {
            store: self.store,
            id,
        })
    }
}

/// Iterator over the root records of a section.
pub type RootIter<'a> = SiblingIter<'a>;

/// Iterator over the direct children of a record.
pub type ChildrenIter<'a> = SiblingIter<'a>;

impl<'a> NetRef<'a> {
    /// Iterate the root records of this section.
    #[inline]
    pub fn roots(&self) -> RootIter<'a> {
        SiblingIter::new(self.store, self.section().first_root)
    }
}

impl<'a> NodeRef<'a> {
    /// The parent record, or `None` for a root.
    #[inline]
    pub fn parent(&self) -> Option<NodeRef<'a>> {
        opt_node(self.node().parent).map(|id| NodeRef {
            store: self.store,
            id,
        })
    }

    /// The first child, or `None` for a leaf.
    #[inline]
    pub fn first_child(&self) -> Option<NodeRef<'a>> {
        opt_node(self.node().first_child).map(|id| NodeRef {
            store: self.store,
            id,
        })
    }

    /// The next sibling under the same parent, or `None`.
    #[inline]
    pub fn next_sibling(&self) -> Option<NodeRef<'a>> {
        opt_node(self.node().next_sibling).map(|id| NodeRef {
            store: self.store,
            id,
        })
    }

    /// The previous sibling under the same parent, or `None`.
    ///
    /// The previous link is not stored; it is recovered by scanning the parent's
    /// (or section's) child chain, which is `O(number of preceding siblings)`.
    pub fn previous_sibling(&self) -> Option<NodeRef<'a>> {
        let me = self.id.get();
        let first = match opt_node(self.node().parent) {
            Some(parent) => self.store.nodes[parent.index()].first_child,
            None => match self.store.section_of(self.id) {
                Some(net) => self.store.sections[net.index()].first_root,
                None => NO_NODE,
            },
        };
        let mut cur = first;
        let mut prev = NO_NODE;
        while cur != NO_NODE {
            if cur == me {
                return opt_node(prev).map(|id| NodeRef {
                    store: self.store,
                    id,
                });
            }
            prev = cur;
            cur = self.store.nodes[cur as usize].next_sibling;
        }
        None
    }

    /// Iterate the direct children of this record.
    #[inline]
    pub fn children(&self) -> ChildrenIter<'a> {
        SiblingIter::new(self.store, self.node().first_child)
    }

    /// The last child, or `None` for a leaf.
    pub fn last_child(&self) -> Option<NodeRef<'a>> {
        let mut last: Option<NodeId> = None;
        let mut cur = self.node().first_child;
        while let Some(id) = opt_node(cur) {
            last = Some(id);
            cur = self.store.nodes[id.index()].next_sibling;
        }
        last.map(|id| NodeRef {
            store: self.store,
            id,
        })
    }
}
