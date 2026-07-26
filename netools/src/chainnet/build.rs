// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the GNU General Public License v3.0.

//! Chain projection, mutable NET construction, finalization, and compaction.

use std::collections::{BTreeMap, HashMap, HashSet};

use chaintools::{Chain, Reader};

use super::error::{ChainNetError, Result};
use super::sizes::SequenceSizes;
use super::{ChainNetOptions, NetSide, NonNestedFilter, try_kent_display_score};
use crate::io::reader::NetStore;
use crate::io::storage::SharedBytes;
use crate::model::attributes::{KnownAttr, NodeAttributes};
use crate::model::ids::{MAX_NODES, NO_ATTRIBUTES, NO_NODE};
use crate::model::node::NetNode;
use crate::model::{NodeKind, OwnedNet, Section, Span, Strand};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProjectionSide {
    Reference,
    Query,
}

#[derive(Debug, Clone, Copy)]
struct AbsoluteBlock {
    reference_start: u32,
    reference_end: u32,
    query_start: u32,
    query_end: u32,
}

#[derive(Debug)]
struct PreparedChain {
    score: i64,
    reference_name: Vec<u8>,
    reference_start: u32,
    reference_end: u32,
    query_name: Vec<u8>,
    query_size: u32,
    query_strand: Strand,
    query_start: u32,
    query_end: u32,
    id: u64,
    blocks: Vec<AbsoluteBlock>,
    total_aligned_bases: u64,
}

#[derive(Debug, Clone, Copy)]
struct ProjectedBlock {
    primary_start: u32,
    primary_end: u32,
    other_start: u32,
    other_end: u32,
}

/// An allocation-free orientation-specific view over one chain's blocks.
struct ProjectionView<'a> {
    chain: &'a PreparedChain,
    side: ProjectionSide,
}

impl<'a> ProjectionView<'a> {
    fn new(chain: &'a PreparedChain, side: ProjectionSide) -> Self {
        Self { chain, side }
    }

    #[inline]
    fn len(&self) -> usize {
        self.chain.blocks.len()
    }

    #[inline]
    fn other_name(&self) -> &'a [u8] {
        match self.side {
            ProjectionSide::Reference => &self.chain.query_name,
            ProjectionSide::Query => &self.chain.reference_name,
        }
    }

    #[inline]
    fn primary_span(&self) -> (u32, u32) {
        match (self.side, self.chain.query_strand) {
            (ProjectionSide::Reference, _) => {
                (self.chain.reference_start, self.chain.reference_end)
            }
            (ProjectionSide::Query, Strand::Forward) => {
                (self.chain.query_start, self.chain.query_end)
            }
            (ProjectionSide::Query, Strand::Reverse) => (
                self.chain.query_size - self.chain.query_end,
                self.chain.query_size - self.chain.query_start,
            ),
        }
    }

    fn block(&self, projected_index: usize) -> ProjectedBlock {
        let reverse_query_order =
            self.side == ProjectionSide::Query && self.chain.query_strand == Strand::Reverse;
        let source_index = if reverse_query_order {
            self.chain.blocks.len() - 1 - projected_index
        } else {
            projected_index
        };
        let block = self.chain.blocks[source_index];
        match self.side {
            ProjectionSide::Reference => {
                let (other_start, other_end) = match self.chain.query_strand {
                    Strand::Forward => (block.query_start, block.query_end),
                    Strand::Reverse => (
                        self.chain.query_size - block.query_end,
                        self.chain.query_size - block.query_start,
                    ),
                };
                ProjectedBlock {
                    primary_start: block.reference_start,
                    primary_end: block.reference_end,
                    other_start,
                    other_end,
                }
            }
            ProjectionSide::Query => {
                let (primary_start, primary_end) = match self.chain.query_strand {
                    Strand::Forward => (block.query_start, block.query_end),
                    Strand::Reverse => (
                        self.chain.query_size - block.query_end,
                        self.chain.query_size - block.query_start,
                    ),
                };
                ProjectedBlock {
                    primary_start,
                    primary_end,
                    other_start: block.reference_start,
                    other_end: block.reference_end,
                }
            }
        }
    }

    fn first_potentially_overlapping(&self, start: u32) -> usize {
        let mut low = 0usize;
        let mut high = self.len();
        while low < high {
            let middle = low + (high - low) / 2;
            if self.block(middle).primary_end <= start {
                low = middle + 1;
            } else {
                high = middle;
            }
        }
        low
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FillId(usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GapId(usize);

#[derive(Debug, Clone, Copy)]
struct AvailableSpace {
    start: u32,
    end: u32,
    parent_gap: GapId,
}

#[derive(Debug)]
struct FillBuild {
    primary_start: u32,
    primary_end: u32,
    other_start: u32,
    other_end: u32,
    chain_index: usize,
    child_gaps: Vec<GapId>,
    aligned_bases: u32,
    raw_score: f64,
    displayed_score: i64,
}

#[derive(Debug)]
struct GapBuild {
    primary_start: u32,
    primary_end: u32,
    other_start: u32,
    other_end: u32,
    parent_fill: Option<FillId>,
    child_fills: Vec<FillId>,
}

struct ChromNetBuild<'a> {
    name: Vec<u8>,
    size: u32,
    side: ProjectionSide,
    chains: &'a [PreparedChain],
    fills: Vec<FillBuild>,
    gaps: Vec<GapBuild>,
    root_gap: GapId,
    spaces: BTreeMap<u32, AvailableSpace>,
}

impl<'a> ChromNetBuild<'a> {
    fn new(name: Vec<u8>, size: u32, side: ProjectionSide, chains: &'a [PreparedChain]) -> Self {
        let root_gap = GapId(0);
        let root = GapBuild {
            primary_start: 0,
            primary_end: size,
            other_start: 0,
            other_end: 0,
            parent_fill: None,
            child_fills: Vec::new(),
        };
        let mut spaces = BTreeMap::new();
        if size > 0 {
            spaces.insert(
                0,
                AvailableSpace {
                    start: 0,
                    end: size,
                    parent_gap: root_gap,
                },
            );
        }
        Self {
            name,
            size,
            side,
            chains,
            fills: Vec::new(),
            gaps: vec![root],
            root_gap,
            spaces,
        }
    }

    fn insert_chain(&mut self, chain_index: usize, options: &ChainNetOptions) -> Result<()> {
        let view = ProjectionView::new(&self.chains[chain_index], self.side);
        let (chain_start, chain_end) = view.primary_span();
        let overlapping = self.overlapping_spaces(chain_start, chain_end);
        let mut block_cursor = view.first_potentially_overlapping(chain_start);

        for space_start in overlapping {
            let Some(space) = self.spaces.get(&space_start).copied() else {
                continue;
            };
            while block_cursor < view.len() && view.block(block_cursor).primary_end <= space.start {
                block_cursor += 1;
            }
            let Some((fill_start, fill_end)) = inner_bounds(
                &view,
                block_cursor,
                space.start,
                space.end,
                options.min_fill,
            ) else {
                continue;
            };

            self.spaces.remove(&space.start);
            let fill_id = FillId(self.fills.len());
            self.fills.push(FillBuild {
                primary_start: fill_start,
                primary_end: fill_end,
                other_start: 0,
                other_end: 0,
                chain_index,
                child_gaps: Vec::new(),
                aligned_bases: 0,
                raw_score: 0.0,
                displayed_score: 0,
            });
            self.gaps[space.parent_gap.0].child_fills.push(fill_id);

            if fill_start - space.start >= options.min_space {
                self.insert_space(AvailableSpace {
                    start: space.start,
                    end: fill_start,
                    parent_gap: space.parent_gap,
                })?;
            }
            if space.end - fill_end >= options.min_space {
                self.insert_space(AvailableSpace {
                    start: fill_end,
                    end: space.end,
                    parent_gap: space.parent_gap,
                })?;
            }

            self.insert_internal_gaps(fill_id, &view, &space, options.min_space)?;
        }
        Ok(())
    }

    fn overlapping_spaces(&self, start: u32, end: u32) -> Vec<u32> {
        if start >= end {
            return Vec::new();
        }
        let mut keys = Vec::new();
        if let Some((&space_start, space)) = self.spaces.range(..=start).next_back()
            && space.end > start
        {
            keys.push(space_start);
        }
        for (&space_start, _) in self.spaces.range(start..end) {
            if keys.last().copied() != Some(space_start) {
                keys.push(space_start);
            }
        }
        keys
    }

    fn insert_space(&mut self, space: AvailableSpace) -> Result<()> {
        if space.start >= space.end {
            return Err(ChainNetError::InvalidSpace {
                sequence: self.name.clone(),
                reason: "space is empty or inverted",
            });
        }
        if let Some((_, previous)) = self.spaces.range(..=space.start).next_back()
            && previous.end > space.start
        {
            return Err(ChainNetError::InvalidSpace {
                sequence: self.name.clone(),
                reason: "spaces overlap",
            });
        }
        if let Some((&next_start, _)) = self.spaces.range(space.start..).next()
            && next_start < space.end
        {
            return Err(ChainNetError::InvalidSpace {
                sequence: self.name.clone(),
                reason: "spaces overlap",
            });
        }
        self.spaces.insert(space.start, space);
        Ok(())
    }

    fn insert_internal_gaps(
        &mut self,
        fill_id: FillId,
        view: &ProjectionView<'_>,
        original_space: &AvailableSpace,
        min_space: u32,
    ) -> Result<()> {
        if view.len() < 2 {
            return Ok(());
        }
        for index in 0..view.len() - 1 {
            let block = view.block(index);
            let next = view.block(index + 1);
            let gap_start = block.primary_end;
            let gap_end = next.primary_start;
            let long_enough = gap_end.saturating_sub(gap_start) >= min_space;
            if !(original_space.start < gap_start && long_enough && gap_end < original_space.end) {
                continue;
            }
            let (other_start, other_end) = match (
                self.side,
                self.chains[fill_id_chain(self, fill_id)].query_strand,
            ) {
                (_, Strand::Forward) => (block.other_end, next.other_start),
                (ProjectionSide::Reference, Strand::Reverse) => (next.other_end, block.other_start),
                (ProjectionSide::Query, Strand::Reverse) => (next.other_start, block.other_end),
            };
            if other_start > other_end {
                return Err(ChainNetError::MalformedBlocks {
                    chain_id: self.chains[fill_id_chain(self, fill_id)].id,
                    reason: "projected internal-gap coordinates are inverted",
                });
            }
            let gap_id = GapId(self.gaps.len());
            self.gaps.push(GapBuild {
                primary_start: gap_start,
                primary_end: gap_end,
                other_start,
                other_end,
                parent_fill: Some(fill_id),
                child_fills: Vec::new(),
            });
            self.fills[fill_id.0].child_gaps.push(gap_id);
            self.insert_space(AvailableSpace {
                start: gap_start,
                end: gap_end,
                parent_gap: gap_id,
            })?;
        }
        Ok(())
    }

    fn finish(&mut self) -> Result<()> {
        for gap in &mut self.gaps {
            gap.child_fills
                .sort_by_key(|fill| self.fills[fill.0].primary_start);
        }
        for fill in &mut self.fills {
            fill.child_gaps
                .sort_by_key(|gap| self.gaps[gap.0].primary_start);
        }

        for fill_index in 0..self.fills.len() {
            self.finalize_fill(FillId(fill_index))?;
        }
        Ok(())
    }

    fn finalize_fill(&mut self, fill_id: FillId) -> Result<()> {
        let fill = &self.fills[fill_id.0];
        let view = ProjectionView::new(&self.chains[fill.chain_index], self.side);
        let clip_start = fill.primary_start;
        let clip_end = fill.primary_end;
        let mut primary_min = u32::MAX;
        let mut primary_max = 0u32;
        let mut other_min = u32::MAX;
        let mut other_max = 0u32;
        let mut aligned = 0u64;
        let first = view.first_potentially_overlapping(clip_start);

        for index in first..view.len() {
            let block = view.block(index);
            if block.primary_start >= clip_end {
                break;
            }
            let start = block.primary_start.max(clip_start);
            let end = block.primary_end.min(clip_end);
            if start >= end {
                continue;
            }
            let left_clip = start - block.primary_start;
            let right_clip = block.primary_end - end;
            let (other_start, other_end) = match self.chains[fill.chain_index].query_strand {
                Strand::Forward => (block.other_start + left_clip, block.other_end - right_clip),
                Strand::Reverse => (block.other_start + right_clip, block.other_end - left_clip),
            };
            primary_min = primary_min.min(start);
            primary_max = primary_max.max(end);
            other_min = other_min.min(other_start);
            other_max = other_max.max(other_end);
            aligned += u64::from(end - start);
        }

        if primary_min >= primary_max || other_min >= other_max || aligned == 0 {
            return Err(ChainNetError::MalformedBlocks {
                chain_id: self.chains[fill.chain_index].id,
                reason: "fill does not intersect an alignment block",
            });
        }
        let aligned_bases = u32::try_from(aligned).map_err(|_| ChainNetError::MalformedBlocks {
            chain_id: self.chains[fill.chain_index].id,
            reason: "aligned-base count overflows 32 bits",
        })?;
        let chain = &self.chains[fill.chain_index];
        let (chain_start, chain_end) = view.primary_span();
        let raw_score = if primary_min <= chain_start && primary_max >= chain_end {
            chain.score as f64
        } else {
            chain.score as f64 * aligned as f64 / chain.total_aligned_bases as f64
        };
        let displayed_score = try_kent_display_score(raw_score)?;

        let fill = &mut self.fills[fill_id.0];
        fill.primary_start = primary_min;
        fill.primary_end = primary_max;
        fill.other_start = other_min;
        fill.other_end = other_max;
        fill.aligned_bases = aligned_bases;
        fill.raw_score = raw_score;
        fill.displayed_score = displayed_score;
        Ok(())
    }

    fn has_data(&self) -> bool {
        !self.gaps[self.root_gap.0].child_fills.is_empty()
    }

    fn to_owned(
        &self,
        options: &ChainNetOptions,
        post_filter: Option<NonNestedFilter>,
    ) -> Result<OwnedNet> {
        let threshold = post_filter.map(NonNestedFilter::threshold);
        let mut output = Vec::<OutputNode>::new();
        let mut stack = Vec::<Action>::new();
        for &fill in self.gaps[self.root_gap.0].child_fills.iter().rev() {
            stack.push(Action::Fill { fill, parent: None });
        }

        while let Some(action) = stack.pop() {
            match action {
                Action::Fill { fill, parent } => {
                    let source = &self.fills[fill.0];
                    // This is the output-time chainNet predicate. A failing fill
                    // suppresses its complete subtree before the Perl-compatible
                    // non-nested transformation is considered.
                    if source.raw_score < options.min_score
                        || source.aligned_bases < options.min_fill
                    {
                        continue;
                    }
                    if threshold.is_some_and(|minimum| source.displayed_score < minimum) {
                        let descendants = self.direct_descendant_fills(fill);
                        for child in descendants.into_iter().rev() {
                            stack.push(Action::Fill {
                                fill: child,
                                parent,
                            });
                        }
                        continue;
                    }

                    let output_index =
                        push_output_node(&mut output, OutputSource::Fill(fill), parent)?;
                    for &gap in source.child_gaps.iter().rev() {
                        stack.push(Action::Gap {
                            gap,
                            parent: output_index,
                            parent_fill: fill,
                        });
                    }
                }
                Action::Gap {
                    gap,
                    parent,
                    parent_fill,
                } => {
                    let output_index = push_output_node(
                        &mut output,
                        OutputSource::Gap { gap, parent_fill },
                        Some(parent),
                    )?;
                    for &fill in self.gaps[gap.0].child_fills.iter().rev() {
                        stack.push(Action::Fill {
                            fill,
                            parent: Some(output_index),
                        });
                    }
                }
            }
        }

        self.assemble_owned(output)
    }

    fn direct_descendant_fills(&self, fill: FillId) -> Vec<FillId> {
        let mut descendants = Vec::new();
        for &gap in &self.fills[fill.0].child_gaps {
            descendants.extend_from_slice(&self.gaps[gap.0].child_fills);
        }
        descendants
    }

    fn assemble_owned(&self, output: Vec<OutputNode>) -> Result<OwnedNet> {
        if output.len() as u64 > u64::from(MAX_NODES) {
            return Err(ChainNetError::ExcessiveNodes);
        }
        let mut bytes = Vec::<u8>::new();
        let mut interned = HashMap::<Vec<u8>, Span>::new();
        let reference_name = intern(&mut bytes, &mut interned, &self.name)?;
        let mut nodes = Vec::<NetNode>::with_capacity(output.len());
        let mut attrs = Vec::<NodeAttributes>::new();

        for (index, emitted) in output.iter().enumerate() {
            let (
                kind,
                reference_start,
                reference_end,
                query_name,
                query_strand,
                query_start,
                query_end,
                attr,
            ) = match emitted.source {
                OutputSource::Fill(fill_id) => {
                    let fill = &self.fills[fill_id.0];
                    let chain = &self.chains[fill.chain_index];
                    let view = ProjectionView::new(chain, self.side);
                    let mut attributes = NodeAttributes::default();
                    attributes.present.insert(KnownAttr::ChainId);
                    attributes.present.insert(KnownAttr::AlignmentScore);
                    attributes.present.insert(KnownAttr::AlignedBases);
                    attributes.chain_id = chain.id;
                    attributes.alignment_score = fill.displayed_score as f64;
                    attributes.aligned_bases = fill.aligned_bases;
                    (
                        NodeKind::Fill,
                        fill.primary_start,
                        fill.primary_end,
                        view.other_name(),
                        chain.query_strand,
                        fill.other_start,
                        fill.other_end,
                        Some(attributes),
                    )
                }
                OutputSource::Gap {
                    gap: gap_id,
                    parent_fill,
                } => {
                    let gap = &self.gaps[gap_id.0];
                    debug_assert_eq!(gap.parent_fill, Some(parent_fill));
                    let parent = &self.fills[parent_fill.0];
                    let chain = &self.chains[parent.chain_index];
                    let view = ProjectionView::new(chain, self.side);
                    (
                        NodeKind::Gap,
                        gap.primary_start,
                        gap.primary_end,
                        view.other_name(),
                        chain.query_strand,
                        gap.other_start,
                        gap.other_end,
                        None,
                    )
                }
            };
            let query_name = intern(&mut bytes, &mut interned, query_name)?;
            let attributes = if let Some(attribute) = attr {
                let slot = u32::try_from(attrs.len()).map_err(|_| ChainNetError::ExcessiveNodes)?;
                attrs.push(attribute);
                slot
            } else {
                NO_ATTRIBUTES
            };
            nodes.push(NetNode {
                kind,
                query_strand,
                depth: emitted.depth,
                reference_start,
                reference_size: reference_end - reference_start,
                query_start,
                query_size: query_end - query_start,
                query_name,
                parent: emitted.parent.unwrap_or(NO_NODE),
                first_child: NO_NODE,
                next_sibling: NO_NODE,
                subtree_end: index as u32 + 1,
                attributes,
            });
        }

        let mut first_root = NO_NODE;
        let mut last_root = NO_NODE;
        let mut root_count = 0u32;
        let mut last_child = vec![NO_NODE; nodes.len()];
        for index in 0..nodes.len() {
            let parent = nodes[index].parent;
            if parent == NO_NODE {
                if first_root == NO_NODE {
                    first_root = index as u32;
                } else {
                    nodes[last_root as usize].next_sibling = index as u32;
                }
                last_root = index as u32;
                root_count += 1;
            } else {
                let previous = last_child[parent as usize];
                if previous == NO_NODE {
                    nodes[parent as usize].first_child = index as u32;
                } else {
                    nodes[previous as usize].next_sibling = index as u32;
                }
                last_child[parent as usize] = index as u32;
            }
        }
        for index in (0..nodes.len()).rev() {
            let mut end = index + 1;
            while end < nodes.len() && nodes[end].depth > nodes[index].depth {
                end = nodes[end].subtree_end as usize;
            }
            nodes[index].subtree_end =
                u32::try_from(end).map_err(|_| ChainNetError::ExcessiveNodes)?;
        }

        let section = Section {
            reference_name,
            reference_size: self.size,
            node_start: 0,
            node_end: nodes.len() as u32,
            first_root,
            root_count,
        };
        Ok(OwnedNet::from_store(NetStore {
            storage: SharedBytes::from_vec(bytes),
            sections: vec![section],
            nodes,
            attrs,
            extras: Vec::new(),
        }))
    }
}

fn fill_id_chain(build: &ChromNetBuild<'_>, fill: FillId) -> usize {
    build.fills[fill.0].chain_index
}

fn inner_bounds(
    view: &ProjectionView<'_>,
    first: usize,
    space_start: u32,
    space_end: u32,
    min_fill: u32,
) -> Option<(u32, u32)> {
    let mut start = u32::MAX;
    let mut end = 0u32;
    for index in first..view.len() {
        let block = view.block(index);
        if block.primary_start >= space_end {
            break;
        }
        let clipped_start = block.primary_start.max(space_start);
        let clipped_end = block.primary_end.min(space_end);
        if clipped_start < clipped_end {
            start = start.min(clipped_start);
            end = end.max(clipped_end);
        }
    }
    (start < end && end - start >= min_fill).then_some((start, end))
}

#[derive(Debug, Clone, Copy)]
enum OutputSource {
    Fill(FillId),
    Gap { gap: GapId, parent_fill: FillId },
}

#[derive(Debug, Clone, Copy)]
struct OutputNode {
    source: OutputSource,
    parent: Option<u32>,
    depth: u16,
}

#[derive(Debug, Clone, Copy)]
enum Action {
    Fill {
        fill: FillId,
        parent: Option<u32>,
    },
    Gap {
        gap: GapId,
        parent: u32,
        parent_fill: FillId,
    },
}

fn push_output_node(
    output: &mut Vec<OutputNode>,
    source: OutputSource,
    parent: Option<u32>,
) -> Result<u32> {
    let depth = match parent {
        Some(parent_index) => output[parent_index as usize]
            .depth
            .checked_add(1)
            .ok_or(ChainNetError::ExcessiveDepth)?,
        None => 0,
    };
    let index = u32::try_from(output.len()).map_err(|_| ChainNetError::ExcessiveNodes)?;
    output.push(OutputNode {
        source,
        parent,
        depth,
    });
    Ok(index)
}

fn intern(bytes: &mut Vec<u8>, cache: &mut HashMap<Vec<u8>, Span>, value: &[u8]) -> Result<Span> {
    if let Some(span) = cache.get(value) {
        return Ok(*span);
    }
    let start = u32::try_from(bytes.len()).map_err(|_| ChainNetError::ExcessiveNodes)?;
    bytes.extend_from_slice(value);
    let end = u32::try_from(bytes.len()).map_err(|_| ChainNetError::ExcessiveNodes)?;
    let span = Span::new(start, end);
    cache.insert(value.to_vec(), span);
    Ok(span)
}

pub(crate) struct BuildProducts {
    pub(crate) reference: Vec<OwnedNet>,
    pub(crate) query: Vec<OwnedNet>,
    pub(crate) raw_reference: Option<Vec<OwnedNet>>,
    pub(crate) raw_query: Option<Vec<OwnedNet>>,
}

pub(crate) fn generate(
    options: &ChainNetOptions,
    reference_sizes: &SequenceSizes,
    query_sizes: &SequenceSizes,
    reader: &Reader<Chain>,
) -> Result<BuildProducts> {
    validate_options(options)?;
    let chains = prepare_chains(options, reference_sizes, query_sizes, reader)?;
    let build_reference = matches!(options.side, NetSide::ReferenceOnly | NetSide::Both);
    let build_query = matches!(options.side, NetSide::QueryOnly | NetSide::Both);

    let (reference, raw_reference) = if build_reference {
        build_side(options, reference_sizes, &chains, ProjectionSide::Reference)?
    } else {
        (Vec::new(), None)
    };
    let (query, raw_query) = if build_query {
        build_side(options, query_sizes, &chains, ProjectionSide::Query)?
    } else {
        (Vec::new(), None)
    };

    Ok(BuildProducts {
        reference,
        query,
        raw_reference,
        raw_query,
    })
}

fn validate_options(options: &ChainNetOptions) -> Result<()> {
    if !options.min_score.is_finite() {
        return Err(ChainNetError::InvalidOption("min_score must be finite"));
    }
    if options.threads == Some(0) {
        return Err(ChainNetError::InvalidOption(
            "threads must be greater than zero",
        ));
    }
    Ok(())
}

fn prepare_chains(
    options: &ChainNetOptions,
    reference_sizes: &SequenceSizes,
    query_sizes: &SequenceSizes,
    reader: &Reader<Chain>,
) -> Result<Vec<PreparedChain>> {
    let mut prepared = Vec::with_capacity(reader.len());
    let mut previous_score = None;
    let mut ids = HashSet::with_capacity(reader.len());

    for chain in reader.chains() {
        if options.validate_score_order
            && previous_score.is_some_and(|previous| chain.score > previous)
        {
            return Err(ChainNetError::UnsortedScores {
                chain_id: chain.id,
                score: chain.score,
                previous_score: previous_score.unwrap_or(chain.score),
            });
        }
        previous_score = Some(chain.score);
        if !ids.insert(chain.id) {
            return Err(ChainNetError::DuplicateChainId(chain.id));
        }
        if chain.reference_strand != chaintools::Strand::Plus {
            return Err(ChainNetError::InvalidReferenceStrand(chain.id));
        }
        validate_size(
            reference_sizes,
            "reference",
            chain.id,
            chain.reference_name.as_bytes(),
            chain.reference_size,
        )?;
        validate_size(
            query_sizes,
            "query",
            chain.id,
            chain.query_name.as_bytes(),
            chain.query_size,
        )?;

        let mut reference_cursor = chain.reference_start;
        let mut query_cursor = chain.query_start;
        let dense = chain.blocks.as_slice();
        if dense.is_empty() {
            return Err(ChainNetError::MalformedBlocks {
                chain_id: chain.id,
                reason: "chain contains no alignment blocks",
            });
        }
        let mut blocks = Vec::with_capacity(dense.len());
        let mut total_aligned_bases = 0u64;
        for (index, block) in dense.iter().copied().enumerate() {
            if block.size == 0 {
                return Err(ChainNetError::MalformedBlocks {
                    chain_id: chain.id,
                    reason: "alignment block has zero size",
                });
            }
            let reference_end =
                reference_cursor
                    .checked_add(block.size)
                    .ok_or(ChainNetError::MalformedBlocks {
                        chain_id: chain.id,
                        reason: "reference block coordinate overflow",
                    })?;
            let query_end =
                query_cursor
                    .checked_add(block.size)
                    .ok_or(ChainNetError::MalformedBlocks {
                        chain_id: chain.id,
                        reason: "query block coordinate overflow",
                    })?;
            blocks.push(AbsoluteBlock {
                reference_start: reference_cursor,
                reference_end,
                query_start: query_cursor,
                query_end,
            });
            total_aligned_bases += u64::from(block.size);
            if index + 1 < dense.len() {
                reference_cursor = reference_end.checked_add(block.gap_reference).ok_or(
                    ChainNetError::MalformedBlocks {
                        chain_id: chain.id,
                        reason: "reference gap coordinate overflow",
                    },
                )?;
                query_cursor = query_end.checked_add(block.gap_query).ok_or(
                    ChainNetError::MalformedBlocks {
                        chain_id: chain.id,
                        reason: "query gap coordinate overflow",
                    },
                )?;
            } else {
                if block.gap_reference != 0 || block.gap_query != 0 {
                    return Err(ChainNetError::MalformedBlocks {
                        chain_id: chain.id,
                        reason: "terminal dense block carries a gap",
                    });
                }
                reference_cursor = reference_end;
                query_cursor = query_end;
            }
        }
        if reference_cursor != chain.reference_end || query_cursor != chain.query_end {
            return Err(ChainNetError::MalformedBlocks {
                chain_id: chain.id,
                reason: "dense blocks do not end at the chain header bounds",
            });
        }
        if chain.reference_start >= chain.reference_end
            || chain.reference_end > chain.reference_size
            || chain.query_start >= chain.query_end
            || chain.query_end > chain.query_size
        {
            return Err(ChainNetError::MalformedBlocks {
                chain_id: chain.id,
                reason: "chain bounds lie outside the declared sequence",
            });
        }
        prepared.push(PreparedChain {
            score: chain.score,
            reference_name: chain.reference_name.as_bytes().to_vec(),
            reference_start: chain.reference_start,
            reference_end: chain.reference_end,
            query_name: chain.query_name.as_bytes().to_vec(),
            query_size: chain.query_size,
            query_strand: match chain.query_strand {
                chaintools::Strand::Plus => Strand::Forward,
                chaintools::Strand::Minus => Strand::Reverse,
            },
            query_start: chain.query_start,
            query_end: chain.query_end,
            id: chain.id,
            blocks,
            total_aligned_bases,
        });
    }
    Ok(prepared)
}

fn validate_size(
    sizes: &SequenceSizes,
    side: &'static str,
    chain_id: u64,
    name: &[u8],
    chain_size: u32,
) -> Result<()> {
    let configured = sizes
        .get(name)
        .ok_or_else(|| ChainNetError::MissingSequence {
            side,
            chain_id,
            name: name.to_vec(),
        })?;
    if configured.size != chain_size {
        return Err(ChainNetError::SizeMismatch {
            side,
            chain_id,
            name: name.to_vec(),
            chain_size,
            configured_size: configured.size,
        });
    }
    Ok(())
}

fn build_side(
    options: &ChainNetOptions,
    sizes: &SequenceSizes,
    chains: &[PreparedChain],
    side: ProjectionSide,
) -> Result<(Vec<OwnedNet>, Option<Vec<OwnedNet>>)> {
    let mut grouped = HashMap::<Vec<u8>, Vec<usize>>::new();
    for (index, chain) in chains.iter().enumerate() {
        if (chain.score as f64) < options.min_score {
            break;
        }
        if !options.include_haplotypes && is_haplotype(&chain.query_name) {
            continue;
        }
        let name = match side {
            ProjectionSide::Reference => &chain.reference_name,
            ProjectionSide::Query => &chain.query_name,
        };
        grouped.entry(name.clone()).or_default().push(index);
    }

    let build_one = |sequence: &super::SequenceSize| -> Result<SectionProducts> {
        let Some(indices) = grouped.get(sequence.name.as_slice()) else {
            return Ok(SectionProducts::default());
        };
        let mut build = ChromNetBuild::new(sequence.name.clone(), sequence.size, side, chains);
        for &index in indices {
            build.insert_chain(index, options)?;
        }
        if !build.has_data() {
            return Ok(SectionProducts::default());
        }
        build.finish()?;
        let final_net = build.to_owned(options, options.post_filter)?;
        let final_net = if options.post_filter.is_some() && final_net.is_empty() {
            None
        } else {
            Some(final_net)
        };
        let raw_net = if options.capture_raw {
            Some(build.to_owned(options, None)?)
        } else {
            None
        };
        Ok(SectionProducts { final_net, raw_net })
    };

    #[cfg(feature = "parallel")]
    let products: Vec<SectionProducts> = {
        use rayon::prelude::*;
        if options.threads == Some(1) {
            sizes
                .entries()
                .iter()
                .map(build_one)
                .collect::<Result<Vec<_>>>()?
        } else if let Some(threads) = options.threads {
            rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .build()
                .map_err(|_| ChainNetError::InvalidOption("could not create thread pool"))?
                .install(|| {
                    sizes
                        .entries()
                        .par_iter()
                        .map(build_one)
                        .collect::<Result<Vec<_>>>()
                })?
        } else {
            sizes
                .entries()
                .par_iter()
                .map(build_one)
                .collect::<Result<Vec<_>>>()?
        }
    };
    #[cfg(not(feature = "parallel"))]
    let products: Vec<SectionProducts> = sizes
        .entries()
        .iter()
        .map(build_one)
        .collect::<Result<Vec<_>>>()?;

    let mut final_nets = Vec::new();
    let mut raw_nets = options.capture_raw.then(Vec::new);
    for product in products {
        if let Some(net) = product.final_net {
            final_nets.push(net);
        }
        if let (Some(raw), Some(output)) = (product.raw_net, raw_nets.as_mut()) {
            output.push(raw);
        }
    }
    Ok((final_nets, raw_nets))
}

#[derive(Default)]
struct SectionProducts {
    final_net: Option<OwnedNet>,
    raw_net: Option<OwnedNet>,
}

fn is_haplotype(name: &[u8]) -> bool {
    name.windows(4)
        .any(|window| window == b"_hap" || window == b"_alt")
}
