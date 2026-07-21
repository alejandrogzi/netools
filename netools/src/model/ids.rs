// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the GNU General Public License v3.0.

//! Numeric aliases and compact, stable identifiers used throughout the crate.
//!
//! Coordinates are 32-bit for the first release (see the crate plan). Chain IDs
//! are 64-bit because UCSC chain identifiers routinely exceed 32 bits in
//! whole-genome nets.

/// Width of all NET reference/query coordinate values in the first release.
pub type Coord = u32;

/// Chain identifier type.
pub type ChainId = u64;

/// Sentinel stored in topology links to mean "no node". Equal to `u32::MAX`.
pub(crate) const NO_NODE: u32 = u32::MAX;

/// Sentinel stored in a node's attribute slot to mean "no optional attributes".
pub(crate) const NO_ATTRIBUTES: u32 = u32::MAX;

/// Largest number of nodes representable in a single shared arena.
///
/// One slot below `u32::MAX` is reserved for the [`NO_NODE`] sentinel.
pub(crate) const MAX_NODES: u32 = u32::MAX - 1;

/// Stable handle to a NET section (one reference sequence) within a
/// [`Reader`](crate::Reader).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(transparent)]
pub struct NetId(pub(crate) u32);

impl NetId {
    /// Raw index of this section.
    #[inline]
    pub const fn get(self) -> u32 {
        self.0
    }

    /// Index usable directly against a slice of sections.
    #[inline]
    pub const fn index(self) -> usize {
        self.0 as usize
    }

    #[inline]
    pub(crate) const fn new(value: u32) -> Self {
        NetId(value)
    }
}

/// Stable handle to a node within the shared preorder arena.
///
/// Node IDs are global across every section owned by a single reader. A node's
/// descendants form the contiguous preorder range
/// `(id.get() + 1)..node.subtree_end()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(transparent)]
pub struct NodeId(pub(crate) u32);

impl NodeId {
    /// Raw arena index of this node.
    #[inline]
    pub const fn get(self) -> u32 {
        self.0
    }

    /// Index usable directly against the node arena.
    #[inline]
    pub const fn index(self) -> usize {
        self.0 as usize
    }

    #[inline]
    pub(crate) const fn new(value: u32) -> Self {
        NodeId(value)
    }
}

/// Convert a raw topology link into an optional [`NodeId`].
#[inline]
pub(crate) const fn opt_node(raw: u32) -> Option<NodeId> {
    if raw == NO_NODE {
        None
    } else {
        Some(NodeId::new(raw))
    }
}
