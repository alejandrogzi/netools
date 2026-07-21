// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the GNU General Public License v3.0.

//! Streaming, low-memory reading.
//!
//! A complete hierarchical section must retain at least one chromosome's
//! records, so streaming owns exactly one section at a time. [`next_net`]
//! yields an [`OwnedNet`]; the event API flattens a section into
//! [`NetEvent`]s. Memory stays proportional to the largest section rather than
//! the whole file.
//!
//! [`next_net`]: StreamingReader::next_net

use std::collections::VecDeque;
use std::io::{BufRead, BufReader};
use std::path::Path;

use crate::io::reader::ReaderOptions;
use crate::model::attributes::KnownAttr;
use crate::model::{ChainId, Coord, NodeKind, OwnedNet, Strand};
use crate::parser::common::strip_cr;
use crate::parser::error::{NetError, NetErrorKind, Result};

/// Owned copy of one record's optional attributes, for event streaming.
#[derive(Debug, Clone, Default)]
pub struct OwnedAttributes {
    /// `id`.
    pub chain_id: Option<ChainId>,
    /// Alignment score (UCSC `score`).
    pub alignment_score: Option<f64>,
    /// Aligned bases (UCSC `ali`).
    pub aligned_bases: Option<u32>,
    /// `type` value, verbatim.
    pub net_type: Option<Vec<u8>>,
    /// Present signed-integer attributes.
    pub ints: Vec<(KnownAttr, i32)>,
    /// Preserved undocumented attributes.
    pub extras: Vec<(Vec<u8>, Vec<u8>)>,
}

/// Owned copy of one record, for event streaming.
#[derive(Debug, Clone)]
pub struct OwnedNodeRecord {
    /// Record class.
    pub kind: NodeKind,
    /// Reference start.
    pub reference_start: Coord,
    /// Reference size.
    pub reference_size: Coord,
    /// Query name.
    pub query_name: Vec<u8>,
    /// Query strand.
    pub query_strand: Strand,
    /// Query start.
    pub query_start: Coord,
    /// Query size.
    pub query_size: Coord,
    /// Optional attributes.
    pub attributes: OwnedAttributes,
}

/// A flat streaming event.
#[derive(Debug, Clone)]
pub enum NetEvent {
    /// Start of a section.
    NetStart {
        /// Reference sequence name.
        reference_name: Vec<u8>,
        /// Reference sequence length.
        reference_size: Coord,
    },
    /// A record within the current section.
    Record {
        /// Nesting depth.
        depth: u16,
        /// The record.
        record: OwnedNodeRecord,
    },
    /// End of the current section.
    NetEnd,
}

/// Integer attributes emitted, in canonical order.
const INT_ATTRS: [KnownAttr; 13] = [
    KnownAttr::QueryFar,
    KnownAttr::QueryOver,
    KnownAttr::QueryDup,
    KnownAttr::ReferenceUnsequenced,
    KnownAttr::QueryUnsequenced,
    KnownAttr::ReferenceMasked,
    KnownAttr::QueryMasked,
    KnownAttr::ReferenceNewMasked,
    KnownAttr::QueryNewMasked,
    KnownAttr::ReferenceOldMasked,
    KnownAttr::QueryOldMasked,
    KnownAttr::ReferenceTandem,
    KnownAttr::QueryTandem,
];

/// A streaming NET reader that owns one section at a time.
pub struct StreamingReader<R: BufRead> {
    reader: R,
    options: ReaderOptions,
    /// A header line read ahead that belongs to the next section.
    pending: Vec<u8>,
    /// Reusable line buffer.
    line: Vec<u8>,
    finished: bool,
    events: VecDeque<NetEvent>,
}

impl<R: BufRead> StreamingReader<R> {
    /// Create a streaming reader with default options.
    pub fn new(reader: R) -> StreamingReader<R> {
        Self::with_options(reader, ReaderOptions::default())
    }

    /// Create a streaming reader with explicit options.
    pub fn with_options(reader: R, options: ReaderOptions) -> StreamingReader<R> {
        StreamingReader {
            reader,
            options,
            pending: Vec::new(),
            line: Vec::new(),
            finished: false,
            events: VecDeque::new(),
        }
    }

    /// Read the next section, or `None` at end of input.
    pub fn next_net(&mut self) -> Result<Option<OwnedNet>> {
        let mut section: Vec<u8> = Vec::new();

        if !self.pending.is_empty() {
            section.append(&mut self.pending);
        } else if !self.seed_first_section(&mut section)? {
            return Ok(None);
        }

        // Accumulate record lines until the next header or EOF.
        loop {
            self.line.clear();
            let n = self
                .reader
                .read_until(b'\n', &mut self.line)
                .map_err(NetError::from)?;
            if n == 0 {
                self.finished = true;
                break;
            }
            if is_header_line(strip_cr(trim_newline(&self.line))) {
                self.pending = self.line.clone();
                break;
            }
            section.extend_from_slice(&self.line);
        }

        let owned = OwnedNet::parse_bytes_with(section, self.options.parser_config())?;
        Ok(Some(owned))
    }

    /// Read lines until the first header, returning whether one was found.
    fn seed_first_section(&mut self, section: &mut Vec<u8>) -> Result<bool> {
        loop {
            self.line.clear();
            let n = self
                .reader
                .read_until(b'\n', &mut self.line)
                .map_err(NetError::from)?;
            if n == 0 {
                self.finished = true;
                return Ok(false);
            }
            let logical = strip_cr(trim_newline(&self.line));
            match line_role(logical) {
                LineRole::Header => {
                    section.extend_from_slice(&self.line);
                    return Ok(true);
                }
                LineRole::Blank | LineRole::Comment => {}
                LineRole::Record => {
                    return Err(NetError::new(NetErrorKind::MissingNetHeader, 0, 0, 1)
                        .with_context(logical));
                }
            }
        }
    }

    /// Read the next flat event, or `None` at end of input.
    pub fn next_event(&mut self) -> Result<Option<NetEvent>> {
        if let Some(event) = self.events.pop_front() {
            return Ok(Some(event));
        }
        match self.next_net()? {
            None => Ok(None),
            Some(owned) => {
                let net = owned.as_ref();
                self.events.push_back(NetEvent::NetStart {
                    reference_name: net.reference_name_bytes().to_vec(),
                    reference_size: net.reference_size(),
                });
                for node in net.preorder() {
                    self.events.push_back(NetEvent::Record {
                        depth: node.depth(),
                        record: owned_record(&node),
                    });
                }
                self.events.push_back(NetEvent::NetEnd);
                Ok(self.events.pop_front())
            }
        }
    }

    /// Consume the reader as an iterator over owned sections.
    pub fn nets(self) -> StreamingNets<R> {
        StreamingNets { inner: self }
    }
}

impl StreamingReader<BufReader<std::fs::File>> {
    /// Open a plain (uncompressed) file for streaming.
    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file = std::fs::File::open(path).map_err(NetError::from)?;
        Ok(StreamingReader::new(BufReader::new(file)))
    }
}

#[cfg(feature = "gzip")]
impl StreamingReader<Box<dyn BufRead>> {
    /// Open a file for streaming, transparently decompressing gzip input.
    pub fn from_path_auto<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file = std::fs::File::open(path).map_err(NetError::from)?;
        let mut buffered = BufReader::new(file);
        let is_gzip = {
            let head = buffered.fill_buf().map_err(NetError::from)?;
            head.len() >= 2 && head[0] == 0x1f && head[1] == 0x8b
        };
        let inner: Box<dyn BufRead> = if is_gzip {
            Box::new(BufReader::new(flate2::read::MultiGzDecoder::new(buffered)))
        } else {
            Box::new(buffered)
        };
        Ok(StreamingReader::new(inner))
    }
}

/// Iterator over owned sections from a [`StreamingReader`].
pub struct StreamingNets<R: BufRead> {
    inner: StreamingReader<R>,
}

impl<R: BufRead> Iterator for StreamingNets<R> {
    type Item = Result<OwnedNet>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next_net().transpose()
    }
}

/// Build an owned record from a borrowed node.
fn owned_record(node: &crate::io::reader::NodeRef<'_>) -> OwnedNodeRecord {
    let view = node.attributes();
    let mut ints = Vec::new();
    for attr in INT_ATTRS {
        if let Some(v) = view.int(attr) {
            ints.push((attr, v));
        }
    }
    OwnedNodeRecord {
        kind: node.kind(),
        reference_start: node.reference_start(),
        reference_size: node.reference_size(),
        query_name: node.query_name_bytes().to_vec(),
        query_strand: node.query_strand(),
        query_start: node.query_start(),
        query_size: node.query_size(),
        attributes: OwnedAttributes {
            chain_id: view.chain_id(),
            alignment_score: view.alignment_score(),
            aligned_bases: view.aligned_bases(),
            net_type: view.net_type().map(|t| t.as_bytes().to_vec()),
            ints,
            extras: view
                .extras()
                .map(|(k, v)| (k.to_vec(), v.to_vec()))
                .collect(),
        },
    }
}

/// Classification of a logical (newline/CR-stripped) line.
enum LineRole {
    Header,
    Comment,
    Blank,
    Record,
}

fn line_role(line: &[u8]) -> LineRole {
    let trimmed = line
        .iter()
        .position(|&b| !matches!(b, b' ' | b'\t'))
        .map(|i| &line[i..]);
    match trimmed {
        None => LineRole::Blank,
        Some(rest) if rest[0] == b'#' => LineRole::Comment,
        Some(_) if is_header_line(line) => LineRole::Header,
        Some(_) => LineRole::Record,
    }
}

/// Whether a logical line is a section header (column zero `net` + whitespace).
#[inline]
fn is_header_line(line: &[u8]) -> bool {
    line.len() >= 3 && &line[0..3] == b"net" && (line.len() == 3 || matches!(line[3], b' ' | b'\t'))
}

/// Strip a single trailing newline (`\n`) if present.
#[inline]
fn trim_newline(line: &[u8]) -> &[u8] {
    match line.split_last() {
        Some((b'\n', rest)) => rest,
        _ => line,
    }
}
