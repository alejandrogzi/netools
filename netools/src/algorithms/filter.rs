// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the GNU General Public License v3.0.

//! Predicate-based, tree-aware filtering.
//!
//! Filtering a hierarchy requires explicit semantics: see [`TreeFilterPolicy`].
//! A predicate is compiled once and evaluated per record; filtering performs one
//! preorder pass and rebuilds a compact arena.

use std::collections::HashSet;

use crate::io::reader::NetRef;
use crate::model::owned::{OwnedNet, build_selected};
use crate::model::{ChainId, Coord, KnownAttr, NetRange, NodeKind, Strand};

/// How the tree is reshaped when a record fails the predicate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TreeFilterPolicy {
    /// Remove a failing record and its entire subtree. The default and the
    /// UCSC-compatible behaviour.
    #[default]
    Prune,
    /// Keep every matching record and all ancestors needed to preserve its path.
    RetainAncestors,
    /// Remove a failing record but reattach retained descendants to the nearest
    /// retained ancestor. May produce non-canonical structure.
    Promote,
}

/// A compiled record predicate. All set conditions must hold (logical AND); an
/// unset condition always passes.
#[derive(Debug, Clone, Default)]
pub struct NetPredicate {
    /// Accepted record kinds.
    pub kinds: Option<HashSet<NodeKind>>,
    /// Accepted query names.
    pub query_names: Option<HashSet<Vec<u8>>>,
    /// Accepted strand.
    pub strand: Option<Strand>,
    /// Accepted chain ids.
    pub chain_ids: Option<HashSet<ChainId>>,
    /// Accepted `type` values (as raw bytes).
    pub types: Option<HashSet<Vec<u8>>>,
    /// Minimum depth.
    pub min_depth: Option<u16>,
    /// Maximum depth.
    pub max_depth: Option<u16>,
    /// Minimum reference size.
    pub min_reference_size: Option<Coord>,
    /// Maximum reference size.
    pub max_reference_size: Option<Coord>,
    /// Minimum score.
    pub min_score: Option<f64>,
    /// Maximum score.
    pub max_score: Option<f64>,
    /// Minimum aligned bases.
    pub min_ali: Option<u32>,
    /// Maximum aligned bases.
    pub max_ali: Option<u32>,
    /// Reference interval the record must overlap.
    pub reference_overlap: Option<NetRange>,
    /// Attributes that must be present.
    pub require_attributes: Vec<KnownAttr>,
    /// Attributes that must be absent.
    pub forbid_attributes: Vec<KnownAttr>,
}

impl NetPredicate {
    /// A predicate that accepts every record.
    pub fn new() -> NetPredicate {
        NetPredicate::default()
    }

    /// Whether `node` satisfies every set condition.
    pub fn matches(&self, node: &crate::io::reader::NodeRef<'_>) -> bool {
        if let Some(kinds) = &self.kinds
            && !kinds.contains(&node.kind())
        {
            return false;
        }
        if let Some(names) = &self.query_names
            && !names.contains(node.query_name_bytes())
        {
            return false;
        }
        if let Some(strand) = self.strand
            && node.query_strand() != strand
        {
            return false;
        }
        if let Some(min) = self.min_depth
            && node.depth() < min
        {
            return false;
        }
        if let Some(max) = self.max_depth
            && node.depth() > max
        {
            return false;
        }
        if let Some(min) = self.min_reference_size
            && node.reference_size() < min
        {
            return false;
        }
        if let Some(max) = self.max_reference_size
            && node.reference_size() > max
        {
            return false;
        }
        if let Some(overlap) = self.reference_overlap
            && !node.reference_range().overlaps(overlap)
        {
            return false;
        }

        let attrs = node.attributes();

        if let Some(ids) = &self.chain_ids {
            match node.chain_id() {
                Some(id) if ids.contains(&id) => {}
                _ => return false,
            }
        }
        if let Some(types) = &self.types {
            match attrs.net_type() {
                Some(t) if types.contains(t.as_bytes()) => {}
                _ => return false,
            }
        }
        if let Some(min) = self.min_score
            && attrs.alignment_score().is_none_or(|s| s < min)
        {
            return false;
        }
        if let Some(max) = self.max_score
            && attrs.alignment_score().is_none_or(|s| s > max)
        {
            return false;
        }
        if let Some(min) = self.min_ali
            && attrs.aligned_bases().is_none_or(|a| a < min)
        {
            return false;
        }
        if let Some(max) = self.max_ali
            && attrs.aligned_bases().is_none_or(|a| a > max)
        {
            return false;
        }
        for attr in &self.require_attributes {
            if !attrs.has(*attr) {
                return false;
            }
        }
        for attr in &self.forbid_attributes {
            if attrs.has(*attr) {
                return false;
            }
        }
        true
    }
}

impl NetRef<'_> {
    /// Filter this section by `predicate` under `policy`, returning a new owned
    /// section.
    pub fn filter(&self, predicate: &NetPredicate, policy: TreeFilterPolicy) -> OwnedNet {
        let count = self.len();
        let base: Vec<bool> = self.preorder().map(|n| predicate.matches(&n)).collect();

        // Local parent index (or NO_PARENT) for each node, for policy propagation.
        const NO_PARENT: u32 = u32::MAX;
        let node_start = self.node_range().start;
        let parent_local: Vec<u32> = self
            .preorder()
            .map(|n| n.parent().map_or(NO_PARENT, |p| p.id().get() - node_start))
            .collect();

        let keep = match policy {
            TreeFilterPolicy::Promote => base,
            TreeFilterPolicy::Prune => {
                let mut keep = vec![false; count];
                for i in 0..count {
                    let parent_ok = match parent_local[i] {
                        NO_PARENT => true,
                        p => keep[p as usize],
                    };
                    keep[i] = base[i] && parent_ok;
                }
                keep
            }
            TreeFilterPolicy::RetainAncestors => {
                let mut keep = base;
                for i in (0..count).rev() {
                    if keep[i] && parent_local[i] != NO_PARENT {
                        keep[parent_local[i] as usize] = true;
                    }
                }
                keep
            }
        };

        OwnedNet::from_store(build_selected(self.store, self.id, &keep))
    }
}
