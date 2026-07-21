// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the GNU General Public License v3.0.

//! Per-section descriptor and the public `Net` format marker.

use crate::model::ids::Coord;
use crate::model::range::Span;

/// Marker type naming the UCSC NET record format.
///
/// It parameterises [`Reader<Net>`](crate::Reader) in the same way `chaintools`
/// parameterises its reader by record type. Section data is accessed through
/// [`NetRef`](crate::io::NetRef) values yielded by the reader, not through this
/// type directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Net;

/// Internal descriptor for one reference-sequence section.
///
/// The section owns a contiguous `[node_start, node_end)` slice of the shared
/// arena. Root records are chained through `first_root` / `next_sibling`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Section {
    pub(crate) reference_name: Span,
    pub(crate) reference_size: Coord,

    pub(crate) node_start: u32,
    pub(crate) node_end: u32,

    /// First root record, or `NO_NODE` if the section is empty.
    pub(crate) first_root: u32,
    pub(crate) root_count: u32,
}

impl Section {
    /// Number of records (nodes) in this section.
    #[inline]
    pub(crate) fn node_count(&self) -> usize {
        (self.node_end - self.node_start) as usize
    }

    /// Whether this section contains no records.
    #[inline]
    pub(crate) fn is_empty(&self) -> bool {
        self.node_end == self.node_start
    }
}
