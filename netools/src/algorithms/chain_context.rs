// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the GNU General Public License v3.0.

//! NET-derived contexts for chain-breaking analysis.
//!
//! These are generic primitives a future `chainCleaner` reimplementation can
//! build on without reparsing indentation or reconstructing parent links. No
//! chain-scoring or chain-specific ranking lives here: ranking is always
//! supplied by the caller (see [`AlignmentSpanIndex::any_overlap_where`]).

use std::collections::BTreeSet;

use crate::Reader;
use crate::io::reader::{NetRef, NodeRef};
use crate::model::{ChainId, Net, NetId, NetRange, NodeId};
use crate::parser::error::{NetError, NetErrorKind, Result};

/// Context surrounding one fill record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FillContext {
    /// The section this fill belongs to.
    pub net_id: NetId,
    /// The fill record.
    pub fill: NodeId,
    /// Nesting depth of the fill.
    pub depth: u16,
    /// The fill's chain id (0 if absent).
    pub chain_id: ChainId,
    /// The immediate parent gap, if the parent is a gap.
    pub parent_gap: Option<NodeId>,
    /// The nearest enclosing fill.
    pub enclosing_fill: Option<NodeId>,
    /// The nearest enclosing fill's chain id, if any.
    pub enclosing_chain_id: Option<ChainId>,
    /// Reference interval.
    pub reference_range: NetRange,
    /// Query interval.
    pub query_range: NetRange,
}

/// One occurrence of a chain as a fill within a section.
#[derive(Clone, Copy)]
pub struct ChainOccurrence<'a> {
    /// The section.
    pub net: NetRef<'a>,
    /// The fill record.
    pub fill: NodeRef<'a>,
    /// The fill's context.
    pub context: FillContext,
}

/// An uninterrupted aligned reference interval of a fill.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AlignmentSpan {
    /// The section.
    pub net_id: NetId,
    /// The fill this span belongs to.
    pub fill: NodeId,
    /// The fill's chain id.
    pub chain_id: ChainId,
    /// The uninterrupted reference interval.
    pub reference_range: NetRange,
}

/// Context linking a nested fill, its parent gap, and the enclosing fill.
///
/// This is the most directly useful primitive for chain-breaking discovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NestedFillGapContext {
    /// The section.
    pub net_id: NetId,
    /// Depth of the nested fill.
    pub depth: u16,
    /// The nested fill.
    pub fill_node: NodeId,
    /// The nested fill's chain id.
    pub fill_chain_id: ChainId,
    /// The nested fill's reference interval.
    pub fill_range: NetRange,
    /// The parent gap.
    pub parent_gap_node: NodeId,
    /// The parent gap's reference interval.
    pub parent_gap_range: NetRange,
    /// The enclosing fill.
    pub parent_fill_node: NodeId,
    /// The enclosing fill's chain id.
    pub parent_chain_id: ChainId,
}

/// Build a fill context from a borrowed fill node.
fn build_fill_context(net: NetRef<'_>, fill: NodeRef<'_>) -> FillContext {
    let parent_gap = fill.parent().filter(NodeRef::is_gap).map(|p| p.id());
    let enclosing = fill.enclosing_fill();
    FillContext {
        net_id: net.id(),
        fill: fill.id(),
        depth: fill.depth(),
        chain_id: fill.chain_id().unwrap_or(0),
        parent_gap,
        enclosing_fill: enclosing.map(|n| n.id()),
        enclosing_chain_id: enclosing.and_then(|n| n.chain_id()),
        reference_range: fill.reference_range(),
        query_range: fill.query_range(),
    }
}

impl<'a> NetRef<'a> {
    /// The context of a specific fill.
    pub fn fill_context(&self, fill: NodeId) -> Result<FillContext> {
        let node = self
            .node(fill)
            .ok_or_else(|| NetError::new(NetErrorKind::StructuralViolation, 0, 0, 0))?;
        if !node.is_fill() {
            return Err(NetError::new(NetErrorKind::StructuralViolation, 0, 0, 0)
                .with_context(b"node is not a fill"));
        }
        Ok(build_fill_context(*self, node))
    }

    /// Contexts for every fill in the section, in preorder.
    pub fn fill_contexts(&self) -> impl Iterator<Item = FillContext> + 'a {
        let net = *self;
        net.fills().map(move |fill| build_fill_context(net, fill))
    }

    /// Contexts for fills below the top level (depth > 0) — the records relevant
    /// to chain-breaking discovery.
    pub fn nested_fill_contexts(&self) -> impl Iterator<Item = FillContext> + 'a {
        let net = *self;
        net.fills()
            .filter(|f| f.depth() > 0)
            .map(move |fill| build_fill_context(net, fill))
    }

    /// Occurrences of a chain (as fills) within this section, in preorder.
    pub fn chain_occurrences(
        &self,
        chain_id: ChainId,
    ) -> impl Iterator<Item = ChainOccurrence<'a>> + 'a {
        let net = *self;
        net.fills()
            .filter(move |fill| fill.chain_id() == Some(chain_id))
            .map(move |fill| ChainOccurrence {
                net,
                fill,
                context: build_fill_context(net, fill),
            })
    }

    /// Chain ids used by fills in this section, sorted and distinct.
    pub fn used_chain_ids(&self) -> Vec<ChainId> {
        let mut set = BTreeSet::new();
        self.mark_used_chain_ids(&mut set);
        set.into_iter().collect()
    }

    /// Add every fill chain id in this section to `sink`.
    pub fn mark_used_chain_ids<S: Extend<ChainId>>(&self, sink: &mut S) {
        for fill in self.fills() {
            if let Some(id) = fill.chain_id() {
                sink.extend(std::iter::once(id));
            }
        }
    }

    /// Contexts linking nested fills, their parent gaps, and enclosing fills.
    ///
    /// Only fills that have a parent gap and an enclosing fill, both carrying a
    /// chain id, are returned, in reference (preorder) order.
    pub fn nested_fill_gap_contexts(&self) -> impl Iterator<Item = NestedFillGapContext> + 'a {
        let net_id = self.id();
        self.fills().filter_map(move |fill| {
            let parent = fill.parent()?;
            if !parent.is_gap() {
                return None;
            }
            let enclosing = parent.enclosing_fill()?;
            let fill_chain_id = fill.chain_id()?;
            let parent_chain_id = enclosing.chain_id()?;
            Some(NestedFillGapContext {
                net_id,
                depth: fill.depth(),
                fill_node: fill.id(),
                fill_chain_id,
                fill_range: fill.reference_range(),
                parent_gap_node: parent.id(),
                parent_gap_range: parent.reference_range(),
                parent_fill_node: enclosing.id(),
                parent_chain_id,
            })
        })
    }

    /// Build an index over the uninterrupted alignment spans of every fill in
    /// this section.
    pub fn alignment_span_index(&self) -> AlignmentSpanIndex {
        let mut spans: Vec<AlignmentSpan> = Vec::new();
        for fill in self.fills() {
            spans.extend(fill.uninterrupted_reference_spans());
        }
        spans.sort_by(|a, b| {
            a.reference_range
                .start
                .cmp(&b.reference_range.start)
                .then(a.reference_range.end.cmp(&b.reference_range.end))
        });
        AlignmentSpanIndex { spans }
    }
}

impl<'a> NodeRef<'a> {
    /// The reference intervals of this fill that are not interrupted by child
    /// gaps that themselves contain lower-level fills.
    ///
    /// Returns an empty vector for non-fill records.
    pub fn uninterrupted_reference_spans(&self) -> Vec<AlignmentSpan> {
        if !self.is_fill() {
            return Vec::new();
        }
        let net_id = self.store.section_of(self.id).unwrap_or(NetId::new(0));
        let chain_id = self.chain_id().unwrap_or(0);

        let mut cuts: Vec<NetRange> = self
            .child_gaps_with_fills()
            .map(|g| g.reference_range())
            .collect();
        cuts.sort_by_key(|r| r.start);

        let mut spans = Vec::new();
        let mut cursor = self.reference_start();
        let end = self.reference_end();
        let mut push = |start: crate::model::Coord, stop: crate::model::Coord| {
            if stop > start {
                spans.push(AlignmentSpan {
                    net_id,
                    fill: self.id(),
                    chain_id,
                    reference_range: NetRange::new(start, stop),
                });
            }
        };
        for cut in cuts {
            if cut.start > cursor {
                push(cursor, cut.start);
            }
            cursor = cursor.max(cut.end);
        }
        push(cursor, end);
        spans
    }
}

/// An index over alignment spans supporting overlap queries with chain metadata.
///
/// Ranking is intentionally external: [`any_overlap_where`](Self::any_overlap_where)
/// lets the caller decide, per span, whether it counts.
pub struct AlignmentSpanIndex {
    /// Spans sorted by reference start, then end.
    spans: Vec<AlignmentSpan>,
}

impl AlignmentSpanIndex {
    /// All indexed spans, sorted by reference start.
    pub fn spans(&self) -> &[AlignmentSpan] {
        &self.spans
    }

    /// Spans overlapping `range`.
    pub fn overlapping(&self, range: NetRange) -> impl Iterator<Item = &AlignmentSpan> {
        self.spans
            .iter()
            .take_while(move |s| s.reference_range.start < range.end)
            .filter(move |s| s.reference_range.overlaps(range))
    }

    /// Alias of [`overlapping`](Self::overlapping).
    pub fn spans_overlapping(&self, range: NetRange) -> impl Iterator<Item = &AlignmentSpan> {
        self.overlapping(range)
    }

    /// Chain ids of spans overlapping `range`.
    pub fn chain_ids_overlapping(&self, range: NetRange) -> impl Iterator<Item = ChainId> + '_ {
        self.overlapping(range).map(|s| s.chain_id)
    }

    /// Whether any span overlapping `range` satisfies `predicate`.
    pub fn any_overlap_where<F>(&self, range: NetRange, predicate: F) -> bool
    where
        F: FnMut(&AlignmentSpan) -> bool,
    {
        self.overlapping(range).any(predicate)
    }
}

impl Reader<Net> {
    /// Occurrences of a chain across every section, in section then preorder
    /// order.
    pub fn chain_occurrences(
        &self,
        chain_id: ChainId,
    ) -> impl Iterator<Item = ChainOccurrence<'_>> {
        self.nets()
            .flat_map(move |net| net.chain_occurrences(chain_id).collect::<Vec<_>>())
    }

    /// Adjacent pairs of the same chain's occurrences, ordered by section,
    /// reference start, reference end, then node id.
    pub fn adjacent_chain_occurrences(
        &self,
        chain_id: ChainId,
    ) -> impl Iterator<Item = (ChainOccurrence<'_>, ChainOccurrence<'_>)> {
        let mut occ: Vec<ChainOccurrence<'_>> = self.chain_occurrences(chain_id).collect();
        occ.sort_by(|a, b| {
            a.context
                .net_id
                .cmp(&b.context.net_id)
                .then(
                    a.context
                        .reference_range
                        .start
                        .cmp(&b.context.reference_range.start),
                )
                .then(
                    a.context
                        .reference_range
                        .end
                        .cmp(&b.context.reference_range.end),
                )
                .then(a.fill.id().cmp(&b.fill.id()))
        });
        let mut pairs = Vec::new();
        for window in occ.windows(2) {
            pairs.push((window[0], window[1]));
        }
        pairs.into_iter()
    }

    /// Chain ids used by fills across the whole file, sorted and distinct.
    pub fn used_chain_ids(&self) -> Vec<ChainId> {
        let mut set = BTreeSet::new();
        self.mark_used_chain_ids(&mut set);
        set.into_iter().collect()
    }

    /// Add every fill chain id across the whole file to `sink`.
    pub fn mark_used_chain_ids<S: Extend<ChainId>>(&self, sink: &mut S) {
        for net in self.nets() {
            net.mark_used_chain_ids(sink);
        }
    }
}
