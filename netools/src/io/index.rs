// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the GNU General Public License v3.0.

//! Independent index structures over NET data.
//!
//! Three indexes are provided: a section index (scans headers only), a chain-id
//! index (fill occurrences by chain), and a per-section reference-interval index
//! (sorted vector plus binary search).

use std::collections::HashMap;
use std::path::Path;

use crate::io::compression::{decompress_gzip, is_gzip};
use crate::io::reader::{NetRef, Reader};
use crate::io::storage::{ByteSlice, SharedBytes};
use crate::model::range::Span;
use crate::model::{ChainId, Coord, Net, NetId, NetRange, NodeId};
use crate::parser::common::{Tokens, measure_indent, parse_u32, strip_cr};
use crate::parser::error::{NetError, NetErrorKind, Result};

/// Byte span and metadata of one section, from the section index.
#[derive(Debug, Clone)]
pub struct NetSpan {
    /// Section id (file order).
    pub net_id: NetId,
    /// Reference sequence name.
    pub reference_name: ByteSlice,
    /// Reference sequence length.
    pub reference_size: Coord,
    /// Byte offset of the header line.
    pub byte_start: u64,
    /// Byte offset where the next section (or EOF) begins.
    pub byte_end: u64,
}

/// An index of section headers and their byte ranges.
///
/// For gzip input the ranges refer to the decompressed bytes; true random access
/// into a compressed stream is not available without decompression.
pub struct NetIndex {
    spans: Vec<NetSpan>,
    by_reference: HashMap<Vec<u8>, NetId>,
}

impl NetIndex {
    /// Build a section index by scanning a file's headers.
    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<NetIndex> {
        let data = std::fs::read(path.as_ref()).map_err(NetError::from)?;
        let storage = if is_gzip(&data) {
            SharedBytes::from_vec(decompress_gzip(&data)?)
        } else {
            SharedBytes::from_vec(data)
        };
        Self::from_storage(storage)
    }

    /// Build a section index from an owned byte buffer.
    pub fn from_bytes(data: Vec<u8>) -> Result<NetIndex> {
        let storage = if is_gzip(&data) {
            SharedBytes::from_vec(decompress_gzip(&data)?)
        } else {
            SharedBytes::from_vec(data)
        };
        Self::from_storage(storage)
    }

    fn from_storage(storage: SharedBytes) -> Result<NetIndex> {
        let bytes = storage.as_bytes();
        if bytes.len() as u64 > u32::MAX as u64 {
            return Err(NetError::new(NetErrorKind::NumericOverflow, 0, 0, 0)
                .with_context(b"input exceeds the 4 GiB limit"));
        }

        // Collect header positions with name span and size.
        struct Header {
            start: usize,
            name: Span,
            size: Coord,
        }
        let mut headers: Vec<Header> = Vec::new();
        let mut pos = 0usize;
        let mut line_no = 1u64;
        while pos < bytes.len() {
            let line_end = memchr::memchr(b'\n', &bytes[pos..]).map_or(bytes.len(), |i| pos + i);
            let line = strip_cr(&bytes[pos..line_end]);
            let indent = measure_indent(line);
            if indent.width == 0
                && line.len() >= 3
                && &line[0..3] == b"net"
                && (line.len() == 3 || matches!(line[3], b' ' | b'\t'))
            {
                let mut toks = Tokens::new(line, 0);
                let _net = toks.next();
                let name = toks.next();
                let size_tok = toks.next();
                let (name, size_tok) = match (name, size_tok) {
                    (Some(n), Some(s)) => (n, s),
                    _ => {
                        return Err(NetError::new(
                            NetErrorKind::InvalidNetHeader,
                            pos as u64,
                            line_no,
                            1,
                        ));
                    }
                };
                let size = parse_u32(&line[size_tok.0..size_tok.1]).map_err(|_| {
                    NetError::new(NetErrorKind::InvalidNetHeader, pos as u64, line_no, 1)
                })?;
                headers.push(Header {
                    start: pos,
                    name: Span::new((pos + name.0) as u32, (pos + name.1) as u32),
                    size,
                });
            }
            pos = line_end + 1;
            line_no += 1;
        }

        let mut spans = Vec::with_capacity(headers.len());
        let mut by_reference = HashMap::with_capacity(headers.len());
        for (i, header) in headers.iter().enumerate() {
            let byte_end = headers.get(i + 1).map_or(bytes.len(), |h| h.start) as u64;
            let net_id = NetId::new(i as u32);
            let name_bytes = header.name.resolve(bytes).to_vec();
            by_reference.entry(name_bytes).or_insert(net_id);
            spans.push(NetSpan {
                net_id,
                reference_name: storage.slice(header.name),
                reference_size: header.size,
                byte_start: header.start as u64,
                byte_end,
            });
        }

        Ok(NetIndex {
            spans,
            by_reference,
        })
    }

    /// The span of a section by reference name (first match).
    pub fn get(&self, reference_name: &[u8]) -> Option<&NetSpan> {
        self.by_reference
            .get(reference_name)
            .map(|id| &self.spans[id.index()])
    }

    /// All section spans, in file order.
    pub fn spans(&self) -> &[NetSpan] {
        &self.spans
    }

    /// The byte range of a section.
    pub fn net_bytes(&self, net_id: NetId) -> Option<(u64, u64)> {
        self.spans
            .get(net_id.index())
            .map(|s| (s.byte_start, s.byte_end))
    }

    /// Number of sections.
    pub fn len(&self) -> usize {
        self.spans.len()
    }

    /// Whether the file has no sections.
    pub fn is_empty(&self) -> bool {
        self.spans.is_empty()
    }
}

/// Location of a record within a reader.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeLocation {
    /// The section.
    pub net_id: NetId,
    /// The record.
    pub node_id: NodeId,
}

/// An index of fill occurrences by chain id.
pub struct ChainIdIndex {
    occurrences: HashMap<ChainId, Vec<NodeLocation>>,
}

impl ChainIdIndex {
    /// Occurrences of a chain, in reference then preorder order.
    pub fn occurrences(&self, chain_id: ChainId) -> &[NodeLocation] {
        self.occurrences
            .get(&chain_id)
            .map_or(&[], |v| v.as_slice())
    }

    /// Whether a chain id appears at all.
    pub fn contains(&self, chain_id: ChainId) -> bool {
        self.occurrences.contains_key(&chain_id)
    }

    /// Number of distinct chain ids.
    pub fn len(&self) -> usize {
        self.occurrences.len()
    }

    /// Whether the index is empty.
    pub fn is_empty(&self) -> bool {
        self.occurrences.is_empty()
    }
}

/// An interval entry in a reference-interval index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexedInterval {
    /// Reference start.
    pub start: Coord,
    /// Reference end.
    pub end: Coord,
    /// The record.
    pub node_id: NodeId,
}

impl IndexedInterval {
    #[inline]
    fn range(&self) -> NetRange {
        NetRange::new(self.start, self.end)
    }
}

/// A per-section index of reference intervals (sorted vector, binary search).
pub struct ReferenceIntervalIndex {
    intervals: Vec<IndexedInterval>,
}

impl ReferenceIntervalIndex {
    /// All indexed intervals, sorted by start.
    pub fn intervals(&self) -> &[IndexedInterval] {
        &self.intervals
    }

    /// Intervals overlapping `range`.
    pub fn overlapping(&self, range: NetRange) -> impl Iterator<Item = &IndexedInterval> {
        self.intervals
            .iter()
            .take_while(move |i| i.start < range.end)
            .filter(move |i| i.range().overlaps(range))
    }

    /// Intervals that fully contain `range`.
    pub fn containing(&self, range: NetRange) -> impl Iterator<Item = &IndexedInterval> {
        self.intervals
            .iter()
            .take_while(move |i| i.start <= range.start)
            .filter(move |i| i.range().contains_range(range))
    }

    /// Intervals starting strictly before `position`.
    pub fn starting_before(&self, position: Coord) -> impl Iterator<Item = &IndexedInterval> {
        self.intervals
            .iter()
            .take_while(move |i| i.start < position)
    }

    /// Intervals ending strictly after `position`.
    pub fn ending_after(&self, position: Coord) -> impl Iterator<Item = &IndexedInterval> {
        self.intervals.iter().filter(move |i| i.end > position)
    }
}

impl Reader<Net> {
    /// Build a chain-id index over every fill in the file.
    pub fn chain_id_index(&self) -> ChainIdIndex {
        let mut occurrences: HashMap<ChainId, Vec<NodeLocation>> = HashMap::new();
        for net in self.nets() {
            let net_id = net.id();
            for fill in net.fills() {
                if let Some(chain_id) = fill.chain_id() {
                    occurrences.entry(chain_id).or_default().push(NodeLocation {
                        net_id,
                        node_id: fill.id(),
                    });
                }
            }
        }
        ChainIdIndex { occurrences }
    }
}

impl NetRef<'_> {
    /// Build a reference-interval index over this section's records.
    pub fn reference_interval_index(&self) -> ReferenceIntervalIndex {
        let mut intervals: Vec<IndexedInterval> = self
            .preorder()
            .map(|n| IndexedInterval {
                start: n.reference_start(),
                end: n.reference_end(),
                node_id: n.id(),
            })
            .collect();
        intervals.sort_by(|a, b| a.start.cmp(&b.start).then(a.end.cmp(&b.end)));
        ReferenceIntervalIndex { intervals }
    }
}
