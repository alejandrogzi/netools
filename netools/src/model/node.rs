// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the GNU General Public License v3.0.

//! The internal arena node.

use std::fmt;

use crate::model::ids::Coord;
use crate::model::range::Span;
use crate::model::strand::Strand;

/// Whether a record is an alignment `fill` or a `gap` between fills.
///
/// The class is represented explicitly and never inferred from the chain ID,
/// so malformed input (e.g. a gap carrying a chain ID) is preserved for
/// validation rather than silently reclassified.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum NodeKind {
    /// An aligned region (`fill`).
    Fill,
    /// An unaligned region between fills (`gap`).
    Gap,
}

impl NodeKind {
    /// Canonical keyword for this record class.
    #[inline]
    pub const fn as_str(self) -> &'static str {
        match self {
            NodeKind::Fill => "fill",
            NodeKind::Gap => "gap",
        }
    }

    /// Canonical keyword bytes for this record class.
    #[inline]
    pub const fn as_bytes(self) -> &'static [u8] {
        match self {
            NodeKind::Fill => b"fill",
            NodeKind::Gap => b"gap",
        }
    }

    /// Whether this is a fill record.
    #[inline]
    pub const fn is_fill(self) -> bool {
        matches!(self, NodeKind::Fill)
    }

    /// Whether this is a gap record.
    #[inline]
    pub const fn is_gap(self) -> bool {
        matches!(self, NodeKind::Gap)
    }
}

impl fmt::Display for NodeKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One record inside the shared preorder arena.
///
/// Nodes are stored in strict preorder, so a node's entire subtree occupies the
/// contiguous index range `(id + 1)..subtree_end`. Topology links use
/// [`NO_NODE`](crate::model::ids::NO_NODE) as an absent sentinel and the
/// attribute slot uses [`NO_ATTRIBUTES`](crate::model::ids::NO_ATTRIBUTES).
///
/// `depth` is stored (rather than recomputed by walking ancestors) because
/// depth-aware analyses query it frequently and two bytes per record is cheap.
#[derive(Debug, Clone, Copy)]
pub(crate) struct NetNode {
    pub(crate) kind: NodeKind,
    pub(crate) query_strand: Strand,
    pub(crate) depth: u16,

    pub(crate) reference_start: Coord,
    pub(crate) reference_size: Coord,

    pub(crate) query_start: Coord,
    pub(crate) query_size: Coord,
    pub(crate) query_name: Span,

    /// Parent node, or `NO_NODE` for a root record.
    pub(crate) parent: u32,
    /// First child in preorder, or `NO_NODE`.
    pub(crate) first_child: u32,
    /// Next sibling under the same parent, or `NO_NODE`.
    pub(crate) next_sibling: u32,
    /// First arena index after this node's subtree in preorder.
    pub(crate) subtree_end: u32,

    /// Index into the attribute arena, or `NO_ATTRIBUTES`.
    pub(crate) attributes: u32,
}
