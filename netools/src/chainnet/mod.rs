// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the GNU General Public License v3.0.

//! Construct UCSC alignment NETs directly from score-sorted chain records.
//!
//! Construction is order-sensitive within a chromosome and parallel only
//! across independent chromosomes. Query-minus chains are projected without
//! mutating their parsed records. Optional post-filtering implements the exact
//! descendant-promotion behavior of `NetFilterNonNested.perl -minScore1`.

mod build;
mod error;
mod sizes;

#[cfg(feature = "write")]
use std::io::Write;
#[cfg(feature = "write")]
use std::path::Path;

use chaintools::{Chain, Reader};

pub use error::{ChainNetError, Result};
pub use sizes::{SequenceSize, SequenceSizes, SizeSource};

use crate::OwnedNet;

/// Which independently constructed side(s) to return.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NetSide {
    /// Construct only the reference/target NET.
    ReferenceOnly,
    /// Construct only the query NET.
    QueryOnly,
    /// Construct both NETs.
    #[default]
    Both,
}

/// Exact post-construction non-nested filtering supported by this release.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NonNestedFilter {
    /// Retain fills whose Kent-displayed score is at least `set1`.
    MinScore {
        /// Equivalent to Perl `-minScore1`.
        set1: i64,
    },
}

/// Options for applying the exact non-nested transformation to a finalized NET.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NonNestedFilterOptions {
    /// Minimum printed fill score (`NetFilterNonNested.perl -minScore1`).
    pub min_score1: i64,
}

impl NonNestedFilter {
    #[inline]
    pub(crate) const fn threshold(self) -> i64 {
        match self {
            Self::MinScore { set1 } => set1,
        }
    }
}

/// Construction and compatibility options.
#[derive(Debug, Clone)]
pub struct ChainNetOptions {
    /// Smallest available gap retained for lower-ranking chains.
    pub min_space: u32,
    /// Smallest coordinate-span fill accepted during construction and smallest
    /// aligned-base count emitted during finalization.
    pub min_fill: u32,
    /// Lowest chain/input and proportional output score considered.
    pub min_score: f64,
    /// Include query names containing `_hap` or `_alt`.
    pub include_haplotypes: bool,
    /// Side(s) to construct.
    pub side: NetSide,
    /// Optional exact non-nested post-filter.
    pub post_filter: Option<NonNestedFilter>,
    /// Reject input whose scores increase.
    pub validate_score_order: bool,
    /// Worker count for chromosome-level parallel construction.
    pub threads: Option<usize>,
    /// Retain an additional pre-post-filter arena for diagnostic output.
    ///
    /// This is disabled by default so the fused cleaner-compatible path never
    /// holds raw and filtered NET copies simultaneously.
    pub capture_raw: bool,
}

impl Default for ChainNetOptions {
    fn default() -> Self {
        Self {
            min_space: 25,
            min_fill: 12,
            min_score: 2000.0,
            include_haplotypes: false,
            side: NetSide::Both,
            post_filter: None,
            validate_score_order: true,
            threads: None,
            capture_raw: false,
        }
    }
}

impl ChainNetOptions {
    /// Exact `chainCleaner`-compatible generation preset.
    pub fn chaincleaner_compatible() -> Self {
        Self {
            min_space: 25,
            min_fill: 12,
            min_score: 0.0,
            include_haplotypes: false,
            side: NetSide::ReferenceOnly,
            post_filter: Some(NonNestedFilter::MinScore { set1: 3000 }),
            validate_score_order: true,
            threads: None,
            capture_raw: false,
        }
    }
}

/// Builder tying options, size sources, and optional chain metadata together.
#[derive(Debug, Clone)]
pub struct ChainNetBuilder {
    options: ChainNetOptions,
    reference_sizes: Option<SizeSource>,
    query_sizes: Option<SizeSource>,
    metadata: Vec<Vec<u8>>,
}

impl ChainNetBuilder {
    /// Begin a chain-NET build with explicit options.
    pub fn new(options: ChainNetOptions) -> Self {
        Self {
            options,
            reference_sizes: None,
            query_sizes: None,
            metadata: Vec::new(),
        }
    }

    /// Configure ordered reference/target sizes.
    pub fn reference_sizes(mut self, source: SizeSource) -> Self {
        self.reference_sizes = Some(source);
        self
    }

    /// Configure ordered query sizes.
    pub fn query_sizes(mut self, source: SizeSource) -> Self {
        self.query_sizes = Some(source);
        self
    }

    /// Attach chain metadata lines (normally lines beginning with `#`).
    ///
    /// Raw chainNet output preserves these lines; a non-nested filtered result
    /// intentionally omits them to match the Perl pipeline.
    pub fn metadata_lines<I, B>(mut self, lines: I) -> Self
    where
        I: IntoIterator<Item = B>,
        B: Into<Vec<u8>>,
    {
        self.metadata = lines
            .into_iter()
            .map(Into::into)
            .map(|mut line: Vec<u8>| {
                while line
                    .last()
                    .is_some_and(|byte| matches!(byte, b'\n' | b'\r'))
                {
                    line.pop();
                }
                line
            })
            .collect();
        self
    }

    /// Construct the requested NET side(s) from chains in input order.
    pub fn build(&self, chains: &Reader<Chain>) -> Result<GeneratedNets> {
        let reference_sizes = self
            .reference_sizes
            .as_ref()
            .ok_or(ChainNetError::MissingSizeSource("reference"))?
            .load()?;
        let query_sizes = self
            .query_sizes
            .as_ref()
            .ok_or(ChainNetError::MissingSizeSource("query"))?
            .load()?;
        let products = build::generate(&self.options, &reference_sizes, &query_sizes, chains)?;
        let filtered_metadata = if self.options.post_filter.is_some() {
            Vec::new()
        } else {
            self.metadata.clone()
        };
        Ok(GeneratedNets {
            reference: products.reference,
            query: products.query,
            raw_reference: products.raw_reference,
            raw_query: products.raw_query,
            metadata: filtered_metadata,
            raw_metadata: self.metadata.clone(),
        })
    }
}

/// Generated, finalized NET sections.
pub struct GeneratedNets {
    reference: Vec<OwnedNet>,
    query: Vec<OwnedNet>,
    raw_reference: Option<Vec<OwnedNet>>,
    raw_query: Option<Vec<OwnedNet>>,
    metadata: Vec<Vec<u8>>,
    raw_metadata: Vec<Vec<u8>>,
}

impl GeneratedNets {
    /// Final reference-side sections in size-source order.
    #[inline]
    pub fn reference_nets(&self) -> &[OwnedNet] {
        &self.reference
    }

    /// Final query-side sections in size-source order.
    #[inline]
    pub fn query_nets(&self) -> &[OwnedNet] {
        &self.query
    }

    /// Diagnostic raw reference sections, when `capture_raw` was enabled.
    #[inline]
    pub fn raw_reference_nets(&self) -> Option<&[OwnedNet]> {
        self.raw_reference.as_deref()
    }

    /// Diagnostic raw query sections, when `capture_raw` was enabled.
    #[inline]
    pub fn raw_query_nets(&self) -> Option<&[OwnedNet]> {
        self.raw_query.as_deref()
    }

    /// Metadata emitted before final sections.
    #[inline]
    pub fn metadata(&self) -> &[Vec<u8>] {
        &self.metadata
    }

    /// Metadata associated with captured raw sections.
    #[inline]
    pub fn raw_metadata(&self) -> &[Vec<u8>] {
        &self.raw_metadata
    }

    /// Consume the result and return reference sections.
    #[inline]
    pub fn into_reference(self) -> Vec<OwnedNet> {
        self.reference
    }

    /// Consume the result and return query sections.
    #[inline]
    pub fn into_query(self) -> Vec<OwnedNet> {
        self.query
    }

    /// Write final reference sections to an arbitrary sink.
    #[cfg(feature = "write")]
    pub fn write_reference_to<W: Write>(&self, output: &mut W) -> Result<()> {
        write_nets(output, &self.metadata, &self.reference)
    }

    /// Write final query sections to an arbitrary sink.
    #[cfg(feature = "write")]
    pub fn write_query_to<W: Write>(&self, output: &mut W) -> Result<()> {
        write_nets(output, &self.metadata, &self.query)
    }

    /// Write diagnostic raw reference sections to an arbitrary sink.
    #[cfg(feature = "write")]
    pub fn write_raw_reference_to<W: Write>(&self, output: &mut W) -> Result<()> {
        let nets = self
            .raw_reference
            .as_deref()
            .ok_or(ChainNetError::InvalidOption(
                "raw reference NET was not captured",
            ))?;
        write_nets(output, &self.raw_metadata, nets)
    }

    /// Write diagnostic raw query sections to an arbitrary sink.
    #[cfg(feature = "write")]
    pub fn write_raw_query_to<W: Write>(&self, output: &mut W) -> Result<()> {
        let nets = self
            .raw_query
            .as_deref()
            .ok_or(ChainNetError::InvalidOption(
                "raw query NET was not captured",
            ))?;
        write_nets(output, &self.raw_metadata, nets)
    }

    /// Write final reference sections to a path, inferring gzip from `.gz`.
    #[cfg(feature = "write")]
    pub fn write_reference<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        write_nets_path(path, &self.metadata, &self.reference)
    }

    /// Write final query sections to a path, inferring gzip from `.gz`.
    #[cfg(feature = "write")]
    pub fn write_query<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        write_nets_path(path, &self.metadata, &self.query)
    }

    /// Write diagnostic raw reference sections to a path.
    #[cfg(feature = "write")]
    pub fn write_raw_reference<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let nets = self
            .raw_reference
            .as_deref()
            .ok_or(ChainNetError::InvalidOption(
                "raw reference NET was not captured",
            ))?;
        write_nets_path(path, &self.raw_metadata, nets)
    }

    /// Write diagnostic raw query sections to a path.
    #[cfg(feature = "write")]
    pub fn write_raw_query<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let nets = self
            .raw_query
            .as_deref()
            .ok_or(ChainNetError::InvalidOption(
                "raw query NET was not captured",
            ))?;
        write_nets_path(path, &self.raw_metadata, nets)
    }
}

/// Apply `NetFilterNonNested.perl -minScore1` semantics to one finalized NET.
///
/// Rejected fills and their direct gaps disappear, while fills below those
/// gaps are promoted to the rejected fill's output parent. An empty result is
/// returned as `None`, matching whole-section removal by the Perl filter.
pub fn filter_non_nested(
    net: &OwnedNet,
    options: &NonNestedFilterOptions,
) -> Result<Option<OwnedNet>> {
    use crate::model::ids::NO_NODE;

    let view = net.as_ref();
    let node_start = view.section().node_start;
    let node_count = view.len();
    let mut keep = vec![false; node_count];
    let mut surviving_fills = 0usize;

    for node in view.preorder() {
        let local = (node.id().get() - node_start) as usize;
        if node.is_fill() {
            let displayed = node
                .attributes()
                .alignment_score()
                .map(kent_display_score)
                .unwrap_or(0);
            keep[local] = displayed >= options.min_score1;
            surviving_fills += usize::from(keep[local]);
        } else {
            let parent = node.node().parent;
            keep[local] = if parent == NO_NODE {
                false
            } else {
                let parent_local = (parent - node_start) as usize;
                view.node(crate::NodeId::new(parent))
                    .is_some_and(|parent_node| parent_node.is_fill() && keep[parent_local])
            };
        }
    }
    if surviving_fills == 0 {
        return Ok(None);
    }
    Ok(Some(OwnedNet::from_store(
        crate::model::owned::build_selected(view.store, view.id, &keep),
    )))
}

impl std::fmt::Debug for GeneratedNets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GeneratedNets")
            .field("reference_sections", &self.reference.len())
            .field("query_sections", &self.query.len())
            .field(
                "raw_reference_sections",
                &self.raw_reference.as_ref().map(Vec::len),
            )
            .field("raw_query_sections", &self.raw_query.as_ref().map(Vec::len))
            .finish()
    }
}

/// Format a chainNet proportional score as Kent's `%1.0f` integer.
///
/// C's default floating-point environment rounds halfway cases to even. Rust's
/// integer cast saturates values outside the `i64` domain; construction itself
/// rejects non-finite/out-of-range scores before calling this function.
#[inline]
pub fn kent_display_score(score: f64) -> i64 {
    score.round_ties_even() as i64
}

pub(crate) fn try_kent_display_score(score: f64) -> Result<i64> {
    if !score.is_finite() {
        return Err(ChainNetError::ScoreOverflow);
    }
    let rounded = score.round_ties_even();
    // `i64::MAX as f64` rounds upward to 2^63, so the upper boundary itself
    // is already outside the integer domain.
    if rounded < i64::MIN as f64 || rounded >= i64::MAX as f64 {
        return Err(ChainNetError::ScoreOverflow);
    }
    Ok(rounded as i64)
}

#[cfg(feature = "write")]
fn write_metadata<W: Write>(output: &mut W, metadata: &[Vec<u8>]) -> Result<()> {
    for line in metadata {
        output.write_all(line)?;
        output.write_all(b"\n")?;
    }
    Ok(())
}

#[cfg(feature = "write")]
fn write_nets<W: Write>(output: &mut W, metadata: &[Vec<u8>], nets: &[OwnedNet]) -> Result<()> {
    write_metadata(output, metadata)?;
    let mut writer = crate::Writer::new(output);
    for net in nets {
        writer.write_net(net.as_ref())?;
    }
    writer.flush()?;
    Ok(())
}

#[cfg(feature = "write")]
fn write_nets_path<P: AsRef<Path>>(path: P, metadata: &[Vec<u8>], nets: &[OwnedNet]) -> Result<()> {
    let mut writer = crate::Writer::from_path(path)?;
    write_metadata(writer.get_mut(), metadata)?;
    for net in nets {
        writer.write_net(net.as_ref())?;
    }
    writer.finish()?;
    Ok(())
}
