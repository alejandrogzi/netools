// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the GNU General Public License v3.0.

//! Subtree, descendant, and filtered preorder traversal.
//!
//! Because the arena is stored in preorder and each node records the first index
//! after its subtree, subtree and descendant traversal are contiguous scans.

use crate::io::reader::{NetRef, NodeRef, PreorderIter};
use crate::model::NodeKind;

/// Iterator over a record and all its descendants, in preorder.
pub type SubtreeIter<'a> = PreorderIter<'a>;

/// Iterator over a record's descendants (excluding itself), in preorder.
pub type DescendantIter<'a> = PreorderIter<'a>;

/// Preorder iterator yielding only fill records.
#[derive(Clone)]
pub struct FillIter<'a>(PreorderIter<'a>);

impl<'a> Iterator for FillIter<'a> {
    type Item = NodeRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.by_ref().find(NodeRef::is_fill)
    }
}

/// Preorder iterator yielding only gap records.
#[derive(Clone)]
pub struct GapIter<'a>(PreorderIter<'a>);

impl<'a> Iterator for GapIter<'a> {
    type Item = NodeRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.by_ref().find(NodeRef::is_gap)
    }
}

/// Preorder iterator yielding only records at a given depth.
#[derive(Clone)]
pub struct DepthIter<'a> {
    inner: PreorderIter<'a>,
    depth: u16,
}

impl<'a> Iterator for DepthIter<'a> {
    type Item = NodeRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let depth = self.depth;
        self.inner.by_ref().find(|n| n.depth() == depth)
    }
}

impl<'a> NetRef<'a> {
    /// Iterate only the fill records of this section, in preorder.
    #[inline]
    pub fn fills(&self) -> FillIter<'a> {
        FillIter(self.preorder())
    }

    /// Iterate only the gap records of this section, in preorder.
    #[inline]
    pub fn gaps(&self) -> GapIter<'a> {
        GapIter(self.preorder())
    }

    /// Iterate the records at exactly `depth`, in preorder.
    #[inline]
    pub fn nodes_at_depth(&self, depth: u16) -> DepthIter<'a> {
        DepthIter {
            inner: self.preorder(),
            depth,
        }
    }

    /// The greatest nesting depth present, or 0 for an empty section.
    pub fn max_depth(&self) -> u16 {
        self.preorder().map(|n| n.depth()).max().unwrap_or(0)
    }
}

impl<'a> NodeRef<'a> {
    /// Iterate this record and all its descendants, in preorder.
    #[inline]
    pub fn subtree(&self) -> SubtreeIter<'a> {
        PreorderIter::over(self.store, self.id.get(), self.subtree_end())
    }

    /// Iterate this record's descendants (excluding itself), in preorder.
    #[inline]
    pub fn descendants(&self) -> DescendantIter<'a> {
        PreorderIter::over(self.store, self.id.get() + 1, self.subtree_end())
    }

    /// Number of records in this record's subtree, including itself.
    #[inline]
    pub fn subtree_len(&self) -> usize {
        (self.subtree_end() - self.id.get()) as usize
    }

    /// Whether `other` lies within this record's subtree.
    pub fn contains(&self, other: NodeRef<'a>) -> bool {
        let id = other.id().get();
        id >= self.id.get() && id < self.subtree_end()
    }

    /// Iterate only the child records of the given kind.
    pub fn child_gaps(&self) -> impl Iterator<Item = NodeRef<'a>> {
        self.children().filter(|n| n.kind() == NodeKind::Gap)
    }

    /// Iterate child gaps that themselves contain at least one fill.
    ///
    /// `chainCleaner` distinguishes gaps that hold lower-level alignment fills
    /// from ordinary internal gaps.
    pub fn child_gaps_with_fills(&self) -> impl Iterator<Item = NodeRef<'a>> {
        self.children()
            .filter(|n| n.kind() == NodeKind::Gap && n.first_child().is_some())
    }
}
