// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the GNU General Public License v3.0.

//! Summary statistics over a reader.

use std::collections::{BTreeMap, HashSet};

use crate::io::reader::Reader;
use crate::model::{ChainId, Net, NodeKind};

/// Aggregate statistics over every section of a reader.
#[derive(Debug, Clone, Default)]
pub struct Stats {
    /// Number of reference-sequence sections.
    pub nets: usize,
    /// Total records.
    pub records: usize,
    /// Number of fill records.
    pub fills: usize,
    /// Number of gap records.
    pub gaps: usize,
    /// Greatest nesting depth.
    pub max_depth: u16,
    /// Record count per depth (index = depth).
    pub records_per_depth: Vec<usize>,
    /// Number of distinct chain ids.
    pub distinct_chain_ids: usize,
    /// Number of distinct query names.
    pub distinct_query_names: usize,
    /// Reference bases summed over fills.
    pub reference_bases: u64,
    /// Query bases summed over fills.
    pub query_bases: u64,
    /// Record counts by `type` value.
    pub type_counts: BTreeMap<String, usize>,
    /// Counts of undocumented attribute names.
    pub unknown_attribute_names: BTreeMap<String, usize>,
}

impl Stats {
    /// Compute statistics over `reader`.
    pub fn compute(reader: &Reader<Net>) -> Stats {
        let mut stats = Stats {
            nets: reader.len(),
            ..Stats::default()
        };
        let mut chain_ids: HashSet<ChainId> = HashSet::new();
        let mut query_names: HashSet<Vec<u8>> = HashSet::new();

        for net in reader.nets() {
            for node in net.preorder() {
                stats.records += 1;
                match node.kind() {
                    NodeKind::Fill => {
                        stats.fills += 1;
                        stats.reference_bases += node.reference_size() as u64;
                        stats.query_bases += node.query_size() as u64;
                    }
                    NodeKind::Gap => stats.gaps += 1,
                }

                let depth = node.depth();
                stats.max_depth = stats.max_depth.max(depth);
                if depth as usize >= stats.records_per_depth.len() {
                    stats.records_per_depth.resize(depth as usize + 1, 0);
                }
                stats.records_per_depth[depth as usize] += 1;

                query_names.insert(node.query_name_bytes().to_vec());

                let attrs = node.attributes();
                if let Some(id) = attrs.chain_id() {
                    chain_ids.insert(id);
                }
                if let Some(net_type) = attrs.net_type() {
                    *stats
                        .type_counts
                        .entry(String::from_utf8_lossy(net_type.as_bytes()).into_owned())
                        .or_default() += 1;
                }
                for (key, _value) in attrs.extras() {
                    *stats
                        .unknown_attribute_names
                        .entry(String::from_utf8_lossy(key).into_owned())
                        .or_default() += 1;
                }
            }
        }

        stats.distinct_chain_ids = chain_ids.len();
        stats.distinct_query_names = query_names.len();
        stats
    }
}
