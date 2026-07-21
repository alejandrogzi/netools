// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the GNU General Public License v3.0.

//! Canonical NET serialisation.
//!
//! Records are written in preorder with `depth + 1` leading spaces. Documented
//! optional attributes are emitted in canonical order, followed by any preserved
//! undocumented attributes in their original order. Output is deterministic and
//! never requires the textual fields to be valid UTF-8.

use std::io::{self, BufWriter, Write};
use std::path::Path;

use crate::io::reader::{NetRef, NodeRef};
use crate::model::attributes::{AttributeView, KnownAttr};
use crate::model::{Coord, NodeKind, Strand};
use crate::parser::error::{NetError, Result};

/// Output compression.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Compression {
    /// Uncompressed output.
    #[default]
    None,
    /// Gzip output (requires the `gzip` feature).
    Gzip,
}

/// Indentation style. Canonical NET uses one space per nesting level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IndentationStyle {
    /// One leading space for roots, one more per level (canonical).
    #[default]
    Canonical,
}

/// Ordering of optional fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FieldOrder {
    /// Documented fields in canonical UCSC order, then preserved unknowns.
    #[default]
    Canonical,
}

/// Options controlling serialisation.
#[derive(Debug, Clone, Copy, Default)]
pub struct WriterOptions {
    /// Output compression.
    pub compression: Compression,
    /// Indentation style.
    pub indentation: IndentationStyle,
    /// Optional-field ordering.
    pub field_order: FieldOrder,
    /// Whether to emit comment lines (reserved).
    pub include_comments: bool,
}

/// A view that can be written by a [`Writer`].
pub trait NetLike {
    /// The node view type.
    type Node: NodeLike;

    /// The reference sequence name.
    fn reference_name(&self) -> &[u8];
    /// The reference sequence length.
    fn reference_size(&self) -> Coord;
    /// The root records, in order.
    fn roots(&self) -> impl Iterator<Item = Self::Node>;
}

/// A node view that can be written by a [`Writer`].
pub trait NodeLike: Copy {
    /// Record class.
    fn kind(&self) -> NodeKind;
    /// Reference start.
    fn reference_start(&self) -> Coord;
    /// Reference size.
    fn reference_size(&self) -> Coord;
    /// Query name.
    fn query_name(&self) -> &[u8];
    /// Query strand.
    fn query_strand(&self) -> Strand;
    /// Query start.
    fn query_start(&self) -> Coord;
    /// Query size.
    fn query_size(&self) -> Coord;
    /// Optional attributes.
    fn attributes(&self) -> AttributeView<'_>;
    /// Direct children, in order.
    fn children(&self) -> impl Iterator<Item = Self>;
}

impl<'a> NetLike for NetRef<'a> {
    type Node = NodeRef<'a>;

    #[inline]
    fn reference_name(&self) -> &[u8] {
        self.reference_name_bytes()
    }
    #[inline]
    fn reference_size(&self) -> Coord {
        NetRef::reference_size(self)
    }
    #[inline]
    fn roots(&self) -> impl Iterator<Item = Self::Node> {
        NetRef::roots(self)
    }
}

impl<'a> NodeLike for NodeRef<'a> {
    #[inline]
    fn kind(&self) -> NodeKind {
        NodeRef::kind(self)
    }
    #[inline]
    fn reference_start(&self) -> Coord {
        NodeRef::reference_start(self)
    }
    #[inline]
    fn reference_size(&self) -> Coord {
        NodeRef::reference_size(self)
    }
    #[inline]
    fn query_name(&self) -> &[u8] {
        self.query_name_bytes()
    }
    #[inline]
    fn query_strand(&self) -> Strand {
        NodeRef::query_strand(self)
    }
    #[inline]
    fn query_start(&self) -> Coord {
        NodeRef::query_start(self)
    }
    #[inline]
    fn query_size(&self) -> Coord {
        NodeRef::query_size(self)
    }
    #[inline]
    fn attributes(&self) -> AttributeView<'_> {
        NodeRef::attributes(self)
    }
    #[inline]
    fn children(&self) -> impl Iterator<Item = Self> {
        NodeRef::children(self)
    }
}

/// 256 spaces used to emit indentation in bulk.
const SPACES: [u8; 256] = [b' '; 256];

/// Write `n` leading spaces.
fn write_indent<W: Write>(out: &mut W, mut n: usize) -> io::Result<()> {
    while n > 0 {
        let chunk = n.min(SPACES.len());
        out.write_all(&SPACES[..chunk])?;
        n -= chunk;
    }
    Ok(())
}

/// Write one optional attribute in canonical `key value` form.
fn write_attribute<W: Write>(
    out: &mut W,
    attr: KnownAttr,
    attrs: &AttributeView<'_>,
) -> io::Result<()> {
    match attr {
        KnownAttr::ChainId => write!(out, " id {}", attrs.chain_id().unwrap_or(0)),
        KnownAttr::AlignmentScore => {
            write!(out, " score {}", attrs.alignment_score().unwrap_or(0.0))
        }
        KnownAttr::AlignedBases => write!(out, " ali {}", attrs.aligned_bases().unwrap_or(0)),
        KnownAttr::Type => {
            out.write_all(b" type ")?;
            let value = attrs.net_type().map(|t| t.as_bytes()).unwrap_or(b"top");
            out.write_all(value)
        }
        other => write!(out, " {} {}", other.as_str(), attrs.int(other).unwrap_or(0)),
    }
}

/// Write one record line for any [`NodeLike`] at the given depth.
fn write_record<N: NodeLike, W: Write>(
    out: &mut W,
    node: &N,
    depth: u16,
    _options: &WriterOptions,
) -> io::Result<()> {
    write_indent(out, depth as usize + 1)?;
    out.write_all(node.kind().as_bytes())?;
    write!(
        out,
        " {} {} ",
        node.reference_start(),
        node.reference_size()
    )?;
    out.write_all(node.query_name())?;
    write!(
        out,
        " {} {} {}",
        node.query_strand().as_char(),
        node.query_start(),
        node.query_size()
    )?;

    let attrs = node.attributes();
    for attr in KnownAttr::ALL {
        if attrs.has(attr) {
            write_attribute(out, attr, &attrs)?;
        }
    }
    for (key, value) in attrs.extras() {
        out.write_all(b" ")?;
        out.write_all(key)?;
        out.write_all(b" ")?;
        out.write_all(value)?;
    }
    out.write_all(b"\n")
}

/// Write a whole section (fast path over the preorder arena).
pub(crate) fn write_net_ref<W: Write>(
    out: &mut W,
    net: NetRef<'_>,
    options: &WriterOptions,
) -> io::Result<()> {
    out.write_all(b"net ")?;
    out.write_all(net.reference_name_bytes())?;
    write!(out, " {}", net.reference_size())?;
    out.write_all(b"\n")?;
    for node in net.preorder() {
        write_record(out, &node, node.depth(), options)?;
    }
    Ok(())
}

/// Write any [`NetLike`] via an explicit-stack depth-first walk.
fn write_net_generic<N: NetLike, W: Write>(
    out: &mut W,
    net: &N,
    options: &WriterOptions,
) -> io::Result<()> {
    out.write_all(b"net ")?;
    out.write_all(net.reference_name())?;
    write!(out, " {}", net.reference_size())?;
    out.write_all(b"\n")?;

    let mut stack: Vec<(N::Node, u16)> = Vec::new();
    let roots: Vec<N::Node> = net.roots().collect();
    for root in roots.into_iter().rev() {
        stack.push((root, 0));
    }
    while let Some((node, depth)) = stack.pop() {
        write_record(out, &node, depth, options)?;
        let children: Vec<N::Node> = node.children().collect();
        for child in children.into_iter().rev() {
            stack.push((child, depth + 1));
        }
    }
    Ok(())
}

/// A destination for one of the concrete output sinks used by
/// [`Writer::from_path`].
pub enum FileSink {
    /// Plain buffered file output.
    Plain(BufWriter<std::fs::File>),
    /// Gzip-compressed file output.
    #[cfg(feature = "gzip")]
    Gzip(Box<flate2::write::GzEncoder<BufWriter<std::fs::File>>>),
}

impl Write for FileSink {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            FileSink::Plain(w) => w.write(buf),
            #[cfg(feature = "gzip")]
            FileSink::Gzip(w) => w.write(buf),
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        match self {
            FileSink::Plain(w) => w.flush(),
            #[cfg(feature = "gzip")]
            FileSink::Gzip(w) => w.flush(),
        }
    }
}

impl FileSink {
    /// Finalise the sink, writing the gzip footer if applicable.
    fn finish(self) -> io::Result<()> {
        match self {
            FileSink::Plain(mut w) => w.flush(),
            #[cfg(feature = "gzip")]
            FileSink::Gzip(w) => {
                w.finish()?;
                Ok(())
            }
        }
    }
}

/// A canonical NET writer over an arbitrary sink.
pub struct Writer<W: Write> {
    inner: W,
    options: WriterOptions,
}

impl<W: Write> Writer<W> {
    /// Create a writer with default options.
    pub fn new(inner: W) -> Writer<W> {
        Writer {
            inner,
            options: WriterOptions::default(),
        }
    }

    /// Create a writer with explicit options.
    pub fn with_options(inner: W, options: WriterOptions) -> Writer<W> {
        Writer { inner, options }
    }

    /// The current options.
    pub fn options(&self) -> &WriterOptions {
        &self.options
    }

    /// Mutable access to the underlying sink.
    pub fn get_mut(&mut self) -> &mut W {
        &mut self.inner
    }

    /// Write one borrowed section.
    pub fn write_net(&mut self, net: NetRef<'_>) -> Result<()> {
        write_net_ref(&mut self.inner, net, &self.options).map_err(NetError::from)
    }

    /// Write any [`NetLike`] value (borrowed or owned).
    pub fn write_net_like<N: NetLike>(&mut self, net: &N) -> Result<()> {
        write_net_generic(&mut self.inner, net, &self.options).map_err(NetError::from)
    }

    /// Flush buffered output.
    pub fn flush(&mut self) -> Result<()> {
        self.inner.flush().map_err(NetError::from)
    }

    /// Serialise many sections with bounded-memory parallelism.
    ///
    /// Sections are processed in chunks of `chunk_size`; each chunk is serialised
    /// concurrently into per-section buffers, then those buffers are written in
    /// original order. At most `chunk_size` section buffers exist at once, so a
    /// file with very large sections does not duplicate the whole output in
    /// memory. Output is byte-identical to sequential writing.
    #[cfg(feature = "parallel")]
    pub fn write_all_parallel(&mut self, nets: &[NetRef<'_>], chunk_size: usize) -> Result<()> {
        use rayon::prelude::*;

        let chunk_size = chunk_size.max(1);
        let options = self.options;
        for chunk in nets.chunks(chunk_size) {
            let buffers: Vec<Vec<u8>> = chunk
                .par_iter()
                .map(|net| {
                    let mut buffer = Vec::new();
                    // Writing to a Vec is infallible.
                    let _ = write_net_ref(&mut buffer, *net, &options);
                    buffer
                })
                .collect();
            for buffer in buffers {
                self.inner.write_all(&buffer).map_err(NetError::from)?;
            }
        }
        Ok(())
    }

    /// Recover the underlying sink.
    pub fn into_inner(self) -> W {
        self.inner
    }
}

impl Writer<FileSink> {
    /// Open a file for writing with default options, choosing gzip from the
    /// `.gz` extension.
    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<Writer<FileSink>> {
        let gz = path
            .as_ref()
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("gz"));
        let options = WriterOptions {
            compression: if gz {
                Compression::Gzip
            } else {
                Compression::None
            },
            ..Default::default()
        };
        Self::from_path_with(path, options)
    }

    /// Open a file for writing with explicit options.
    pub fn from_path_with<P: AsRef<Path>>(
        path: P,
        options: WriterOptions,
    ) -> Result<Writer<FileSink>> {
        let file = std::fs::File::create(path.as_ref()).map_err(NetError::from)?;
        let buffered = BufWriter::new(file);
        let sink = match options.compression {
            Compression::None => FileSink::Plain(buffered),
            Compression::Gzip => make_gzip_sink(buffered)?,
        };
        Ok(Writer {
            inner: sink,
            options,
        })
    }

    /// Flush and finalise, writing the gzip footer when applicable.
    pub fn finish(mut self) -> Result<()> {
        self.inner.flush().map_err(NetError::from)?;
        self.inner.finish().map_err(NetError::from)
    }
}

#[cfg(feature = "gzip")]
fn make_gzip_sink(buffered: BufWriter<std::fs::File>) -> Result<FileSink> {
    let encoder = flate2::write::GzEncoder::new(buffered, flate2::Compression::default());
    Ok(FileSink::Gzip(Box::new(encoder)))
}

#[cfg(not(feature = "gzip"))]
fn make_gzip_sink(_buffered: BufWriter<std::fs::File>) -> Result<FileSink> {
    Err(
        NetError::new(crate::parser::NetErrorKind::UnsupportedCompression, 0, 0, 0)
            .with_context(b"gzip output requires the `gzip` feature"),
    )
}

impl NetRef<'_> {
    /// Serialise this section to `out`.
    pub fn write_to<W: Write>(&self, out: &mut W) -> Result<()> {
        write_net_ref(out, *self, &WriterOptions::default()).map_err(NetError::from)
    }

    /// Serialise this section to a `String` (lossy for non-UTF-8 fields).
    ///
    /// This materialises the whole section; prefer [`Writer`] for streaming.
    pub fn to_net_string(&self) -> String {
        let mut buffer = Vec::new();
        let _ = write_net_ref(&mut buffer, *self, &WriterOptions::default());
        String::from_utf8_lossy(&buffer).into_owned()
    }
}

impl std::fmt::Display for NetRef<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut buffer = Vec::new();
        write_net_ref(&mut buffer, *self, &WriterOptions::default())
            .map_err(|_| std::fmt::Error)?;
        f.write_str(&String::from_utf8_lossy(&buffer))
    }
}

impl NodeRef<'_> {
    /// Serialise this record (without its subtree) at `depth` to `out`.
    pub fn write_record_to<W: Write>(&self, out: &mut W, depth: u16) -> Result<()> {
        write_record(out, self, depth, &WriterOptions::default()).map_err(NetError::from)
    }
}

impl std::fmt::Display for NodeRef<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut buffer = Vec::new();
        write_record(&mut buffer, self, self.depth(), &WriterOptions::default())
            .map_err(|_| std::fmt::Error)?;
        f.write_str(&String::from_utf8_lossy(&buffer))
    }
}
