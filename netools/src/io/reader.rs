// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the GNU General Public License v3.0.

//! The public [`Reader`] and the borrowed [`NetRef`] / [`NodeRef`] views.
//!
//! A reader owns one shared byte buffer and a contiguous preorder arena shared
//! by every section it read. Views are lightweight `(store, id)` handles.

use std::marker::PhantomData;
use std::path::Path;
use std::sync::Arc;

use crate::io::compression::{decompress_gzip, is_gzip};
use crate::io::storage::{ByteSlice, SharedBytes};
use crate::model::attributes::{AttributeView, ExtraAttribute, NodeAttributes};
use crate::model::ids::{NO_ATTRIBUTES, opt_node};
use crate::model::node::NetNode;
use crate::model::{Coord, Net, NetId, NetRange, NodeId, NodeKind, Section, Strand};
use crate::parser::error::{NetError, NetErrorKind, Result};
use crate::parser::{ParseMode, Parsed, ParserConfig, parse_document};

/// Maximum input size addressable by 32-bit spans.
const MAX_INPUT_LEN: u64 = u32::MAX as u64;

/// Shared backing owned by a [`Reader`]: the byte buffer plus the contiguous
/// section, node, attribute, and extra-attribute arenas.
pub(crate) struct NetStore {
    pub(crate) storage: SharedBytes,
    pub(crate) sections: Vec<Section>,
    pub(crate) nodes: Vec<NetNode>,
    pub(crate) attrs: Vec<NodeAttributes>,
    pub(crate) extras: Vec<ExtraAttribute>,
}

impl NetStore {
    /// The backing bytes.
    #[inline]
    pub(crate) fn bytes(&self) -> &[u8] {
        self.storage.as_bytes()
    }

    /// Build a fresh single-section store containing only `net`'s records.
    ///
    /// Node/attribute/extra indices are rebased to zero; textual spans are kept
    /// as offsets into the shared byte buffer (cloned cheaply through the `Arc`),
    /// so this is inexpensive. Call [`OwnedNet::compact`](crate::model::owned::OwnedNet::compact)
    /// to additionally shrink the byte buffer to only the referenced bytes.
    pub(crate) fn extract_section(&self, net: NetId) -> NetStore {
        use crate::model::ids::{NO_ATTRIBUTES, NO_NODE};

        let section = &self.sections[net.index()];
        let ns = section.node_start;
        let ne = section.node_end;

        let rebase = |raw: u32| if raw == NO_NODE { NO_NODE } else { raw - ns };

        let mut nodes = Vec::with_capacity((ne - ns) as usize);
        let mut attrs = Vec::new();
        let mut extras = Vec::new();

        for gi in ns..ne {
            let mut node = self.nodes[gi as usize];
            node.parent = rebase(node.parent);
            node.first_child = rebase(node.first_child);
            node.next_sibling = rebase(node.next_sibling);
            node.subtree_end -= ns;

            if node.attributes != NO_ATTRIBUTES {
                let mut block = self.attrs[node.attributes as usize];
                if block.extra.len > 0 {
                    let start = block.extra.start as usize;
                    let len = block.extra.len as usize;
                    let new_start = extras.len() as u32;
                    extras.extend_from_slice(&self.extras[start..start + len]);
                    block.extra.start = new_start;
                }
                node.attributes = attrs.len() as u32;
                attrs.push(block);
            }
            nodes.push(node);
        }

        let new_section = Section {
            reference_name: section.reference_name,
            reference_size: section.reference_size,
            node_start: 0,
            node_end: nodes.len() as u32,
            first_root: rebase(section.first_root),
            root_count: section.root_count,
        };

        NetStore {
            storage: self.storage.clone(),
            sections: vec![new_section],
            nodes,
            attrs,
            extras,
        }
    }

    /// The section that owns a global node id, via binary search over the
    /// section node ranges (which partition the arena in order).
    pub(crate) fn section_of(&self, id: NodeId) -> Option<NetId> {
        let idx = id.get();
        let candidate = self.sections.partition_point(|s| s.node_start <= idx);
        if candidate == 0 {
            return None;
        }
        let i = candidate - 1;
        (idx < self.sections[i].node_end).then(|| NetId::new(i as u32))
    }

    /// Parse `storage` into a store using `config`, optionally in parallel.
    pub(crate) fn parse(
        storage: SharedBytes,
        config: ParserConfig,
        parallel: bool,
    ) -> Result<NetStore> {
        if storage.len() as u64 > MAX_INPUT_LEN {
            return Err(NetError::new(NetErrorKind::NumericOverflow, 0, 0, 0)
                .with_context(b"input exceeds the 4 GiB limit for 32-bit offsets"));
        }
        let parsed = parse_bytes(storage.as_bytes(), config, parallel)?;
        Ok(NetStore {
            storage,
            sections: parsed.sections,
            nodes: parsed.nodes,
            attrs: parsed.attrs,
            extras: parsed.extras,
        })
    }
}

/// Parse bytes into arenas, using the parallel section parser when requested and
/// available.
fn parse_bytes(bytes: &[u8], config: ParserConfig, parallel: bool) -> Result<Parsed> {
    #[cfg(feature = "parallel")]
    if parallel {
        return crate::parser::parallel::parse_document_parallel(bytes, config);
    }
    let _ = parallel;
    parse_document(bytes, config)
}

/// A reader over a NET file.
///
/// The type parameter names the record format; only [`Net`] is supported. Use
/// [`Reader::<Net>::from_path`](Reader::from_path) to read a file.
pub struct Reader<T = Net> {
    store: Arc<NetStore>,
    _marker: PhantomData<T>,
}

impl<T> Clone for Reader<T> {
    fn clone(&self) -> Self {
        Reader {
            store: Arc::clone(&self.store),
            _marker: PhantomData,
        }
    }
}

impl Reader<Net> {
    /// Begin configuring a reader.
    pub fn options() -> ReaderOptionsBuilder {
        ReaderOptionsBuilder::default()
    }

    /// Read a file, choosing storage automatically: mmap for plain files (when
    /// the `mmap` feature is enabled), decompression for gzip input, otherwise
    /// an owned buffer.
    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<Self> {
        Self::options().from_path(path)
    }

    /// Memory-map a file explicitly. Gzip input is decompressed into an owned
    /// buffer even through this entry point.
    #[cfg(feature = "mmap")]
    pub fn from_mmap<P: AsRef<Path>>(path: P) -> Result<Self> {
        Self::options().from_path(path)
    }

    /// Read a file, parsing independent sections in parallel.
    #[cfg(feature = "parallel")]
    pub fn from_path_parallel<P: AsRef<Path>>(path: P) -> Result<Self> {
        Self::options().parallel(true).from_path(path)
    }

    /// Parse an owned byte buffer.
    pub fn from_owned_bytes(data: Vec<u8>) -> Result<Self> {
        Self::options().from_owned_bytes(data)
    }

    /// Parse a shared byte buffer without copying.
    pub fn from_bytes(data: Arc<[u8]>) -> Result<Self> {
        Self::options().from_shared(SharedBytes::Owned(data))
    }

    /// Read everything from `reader` into memory and parse it.
    pub fn from_reader<R: std::io::Read>(reader: R) -> Result<Self> {
        Self::options().from_reader(reader)
    }

    fn from_store(store: NetStore) -> Self {
        Reader {
            store: Arc::new(store),
            _marker: PhantomData,
        }
    }

    /// Number of reference-sequence sections.
    #[inline]
    pub fn len(&self) -> usize {
        self.store.sections.len()
    }

    /// Whether the file contained no sections.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.store.sections.is_empty()
    }

    /// Total number of records across all sections.
    #[inline]
    pub fn node_count(&self) -> usize {
        self.store.nodes.len()
    }

    /// Iterate the sections in file order.
    #[inline]
    pub fn nets(&self) -> NetIter<'_> {
        NetIter {
            store: &self.store,
            pos: 0,
            end: self.store.sections.len(),
        }
    }

    /// Alias for [`nets`](Self::nets).
    #[inline]
    pub fn iter(&self) -> NetIter<'_> {
        self.nets()
    }

    /// A parallel iterator over the sections, in file order.
    #[cfg(feature = "parallel")]
    pub fn par_nets(&self) -> impl rayon::iter::IndexedParallelIterator<Item = NetRef<'_>> {
        use rayon::prelude::*;
        let store: &NetStore = &self.store;
        (0..store.sections.len())
            .into_par_iter()
            .map(move |i| NetRef {
                store,
                id: NetId::new(i as u32),
            })
    }

    /// Access a section by index.
    #[inline]
    pub fn net(&self, index: usize) -> Option<NetRef<'_>> {
        (index < self.store.sections.len()).then(|| NetRef {
            store: &self.store,
            id: NetId::new(index as u32),
        })
    }

    /// Access a node by global arena id.
    #[inline]
    pub fn node(&self, id: NodeId) -> Option<NodeRef<'_>> {
        (id.index() < self.store.nodes.len()).then(|| NodeRef {
            store: &self.store,
            id,
        })
    }
}

impl<'a> IntoIterator for &'a Reader<Net> {
    type Item = NetRef<'a>;
    type IntoIter = NetIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.nets()
    }
}

impl std::fmt::Debug for Reader<Net> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Reader")
            .field("nets", &self.len())
            .field("nodes", &self.node_count())
            .finish()
    }
}

/// Options controlling how a file is read.
#[derive(Debug, Clone, Copy)]
pub struct ReaderOptions {
    /// Parsing strictness.
    pub parse_mode: ParseMode,
    /// Whether to parse sections in parallel (requires the `parallel` feature).
    pub parallel: bool,
    /// Maximum nesting depth accepted before erroring.
    pub max_depth: u16,
    /// Maximum accepted line length in bytes.
    pub max_line_length: usize,
    /// Whether to retain comment lines (reserved for streaming/event APIs).
    pub preserve_comments: bool,
    /// Whether to retain undocumented attributes.
    pub preserve_unknown_attributes: bool,
}

impl Default for ReaderOptions {
    fn default() -> Self {
        ReaderOptions {
            parse_mode: ParseMode::Compatible,
            parallel: false,
            max_depth: crate::parser::DEFAULT_MAX_DEPTH,
            max_line_length: crate::parser::DEFAULT_MAX_LINE_LENGTH,
            preserve_comments: false,
            preserve_unknown_attributes: true,
        }
    }
}

impl ReaderOptions {
    pub(crate) fn parser_config(&self) -> ParserConfig {
        ParserConfig {
            mode: self.parse_mode,
            max_depth: self.max_depth,
            max_line_length: self.max_line_length,
        }
    }
}

/// Builder for [`ReaderOptions`] that finishes by reading input.
#[derive(Debug, Clone, Default)]
pub struct ReaderOptionsBuilder {
    options: ReaderOptions,
}

// `from_*` finalizers intentionally consume the builder.
#[allow(clippy::wrong_self_convention)]
impl ReaderOptionsBuilder {
    /// Set the parsing strictness.
    pub fn parse_mode(mut self, mode: ParseMode) -> Self {
        self.options.parse_mode = mode;
        self
    }

    /// Request parallel section parsing (requires the `parallel` feature).
    pub fn parallel(mut self, parallel: bool) -> Self {
        self.options.parallel = parallel;
        self
    }

    /// Set the maximum nesting depth.
    pub fn max_depth(mut self, max_depth: u16) -> Self {
        self.options.max_depth = max_depth;
        self
    }

    /// Set the maximum accepted line length.
    pub fn max_line_length(mut self, len: usize) -> Self {
        self.options.max_line_length = len;
        self
    }

    /// Set whether comment lines are retained.
    pub fn preserve_comments(mut self, preserve: bool) -> Self {
        self.options.preserve_comments = preserve;
        self
    }

    /// Set whether undocumented attributes are retained.
    pub fn preserve_unknown_attributes(mut self, preserve: bool) -> Self {
        self.options.preserve_unknown_attributes = preserve;
        self
    }

    /// The configured options.
    pub fn build(&self) -> ReaderOptions {
        self.options
    }

    /// Read a file with these options.
    pub fn from_path<P: AsRef<Path>>(self, path: P) -> Result<Reader<Net>> {
        let storage = open_path(path.as_ref())?;
        self.from_shared(storage)
    }

    /// Parse an owned byte buffer with these options.
    pub fn from_owned_bytes(self, data: Vec<u8>) -> Result<Reader<Net>> {
        let storage = if is_gzip(&data) {
            SharedBytes::from_vec(decompress_gzip(&data)?)
        } else {
            SharedBytes::from_vec(data)
        };
        self.from_shared(storage)
    }

    /// Read everything from `reader` and parse it with these options.
    pub fn from_reader<R: std::io::Read>(self, mut reader: R) -> Result<Reader<Net>> {
        let mut data = Vec::new();
        reader.read_to_end(&mut data).map_err(NetError::from)?;
        self.from_owned_bytes(data)
    }

    fn from_shared(self, storage: SharedBytes) -> Result<Reader<Net>> {
        let storage = if is_gzip(storage.as_bytes()) {
            SharedBytes::from_vec(decompress_gzip(storage.as_bytes())?)
        } else {
            storage
        };

        let config = self.options.parser_config();
        let store = NetStore::parse(storage, config, self.options.parallel)?;
        Ok(Reader::from_store(store))
    }
}

/// Load a path into shared storage, mmapping plain files and decompressing gzip.
fn open_path(path: &Path) -> Result<SharedBytes> {
    #[cfg(feature = "mmap")]
    {
        let file = std::fs::File::open(path).map_err(NetError::from)?;
        // SAFETY: the file is opened read-only for the duration of the map. As
        // with any mmap, concurrent external truncation is a caller concern; the
        // mapping itself is a read-only view over the file's bytes.
        let mmap = unsafe { memmap2::Mmap::map(&file) }.map_err(NetError::from)?;
        if is_gzip(&mmap) {
            return Ok(SharedBytes::from_vec(decompress_gzip(&mmap)?));
        }
        Ok(SharedBytes::Mapped(Arc::new(mmap)))
    }
    #[cfg(not(feature = "mmap"))]
    {
        let data = std::fs::read(path).map_err(NetError::from)?;
        if is_gzip(&data) {
            return Ok(SharedBytes::from_vec(decompress_gzip(&data)?));
        }
        Ok(SharedBytes::from_vec(data))
    }
}

/// Iterator over the sections of a reader, in file order.
#[derive(Clone)]
pub struct NetIter<'a> {
    store: &'a NetStore,
    pos: usize,
    end: usize,
}

impl<'a> Iterator for NetIter<'a> {
    type Item = NetRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos >= self.end {
            return None;
        }
        let id = NetId::new(self.pos as u32);
        self.pos += 1;
        Some(NetRef {
            store: self.store,
            id,
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let n = self.end - self.pos;
        (n, Some(n))
    }
}

impl ExactSizeIterator for NetIter<'_> {}

impl DoubleEndedIterator for NetIter<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.pos >= self.end {
            return None;
        }
        self.end -= 1;
        Some(NetRef {
            store: self.store,
            id: NetId::new(self.end as u32),
        })
    }
}

/// A borrowed view over one reference-sequence section.
#[derive(Clone, Copy)]
pub struct NetRef<'a> {
    pub(crate) store: &'a NetStore,
    pub(crate) id: NetId,
}

impl<'a> NetRef<'a> {
    #[inline]
    pub(crate) fn section(&self) -> &'a Section {
        &self.store.sections[self.id.index()]
    }

    /// This section's stable id.
    #[inline]
    pub fn id(&self) -> NetId {
        self.id
    }

    /// The reference sequence name (zero-copy).
    #[inline]
    pub fn reference_name(&self) -> ByteSlice {
        self.store.storage.slice(self.section().reference_name)
    }

    /// The reference sequence name as borrowed bytes.
    #[inline]
    pub fn reference_name_bytes(&self) -> &'a [u8] {
        self.section().reference_name.resolve(self.store.bytes())
    }

    /// The reference sequence length.
    #[inline]
    pub fn reference_size(&self) -> Coord {
        self.section().reference_size
    }

    /// Number of records in this section.
    #[inline]
    pub fn len(&self) -> usize {
        self.section().node_count()
    }

    /// Whether the section has no records.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.section().is_empty()
    }

    /// Number of root records.
    #[inline]
    pub fn root_count(&self) -> usize {
        self.section().root_count as usize
    }

    /// Global arena range covered by this section.
    #[inline]
    pub(crate) fn node_range(&self) -> std::ops::Range<u32> {
        let s = self.section();
        s.node_start..s.node_end
    }

    /// Wrap a node id known to belong to this store.
    #[inline]
    pub(crate) fn node_ref(&self, id: NodeId) -> NodeRef<'a> {
        NodeRef {
            store: self.store,
            id,
        }
    }

    /// Access a node by global id, if it lies within this section.
    pub fn node(&self, id: NodeId) -> Option<NodeRef<'a>> {
        let range = self.node_range();
        (range.start..range.end)
            .contains(&id.get())
            .then(|| self.node_ref(id))
    }

    /// Iterate every record of this section in preorder.
    ///
    /// Because the arena is stored in preorder, this is a contiguous scan.
    pub fn preorder(&self) -> PreorderIter<'a> {
        let range = self.node_range();
        PreorderIter {
            store: self.store,
            pos: range.start,
            end: range.end,
        }
    }
}

impl std::fmt::Debug for NetRef<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NetRef")
            .field("reference_name", &self.reference_name().to_string_lossy())
            .field("reference_size", &self.reference_size())
            .field("records", &self.len())
            .finish()
    }
}

/// Preorder iterator over a contiguous arena range.
#[derive(Clone)]
pub struct PreorderIter<'a> {
    store: &'a NetStore,
    pos: u32,
    end: u32,
}

impl<'a> PreorderIter<'a> {
    /// Construct a preorder iterator over the arena range `[start, end)`.
    #[inline]
    pub(crate) fn over(store: &'a NetStore, start: u32, end: u32) -> PreorderIter<'a> {
        PreorderIter {
            store,
            pos: start,
            end,
        }
    }
}

impl<'a> Iterator for PreorderIter<'a> {
    type Item = NodeRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos >= self.end {
            return None;
        }
        let id = NodeId::new(self.pos);
        self.pos += 1;
        Some(NodeRef {
            store: self.store,
            id,
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let n = (self.end - self.pos) as usize;
        (n, Some(n))
    }
}

impl ExactSizeIterator for PreorderIter<'_> {}

/// A borrowed view over one record.
#[derive(Clone, Copy)]
pub struct NodeRef<'a> {
    pub(crate) store: &'a NetStore,
    pub(crate) id: NodeId,
}

impl<'a> NodeRef<'a> {
    #[inline]
    pub(crate) fn node(&self) -> &'a NetNode {
        &self.store.nodes[self.id.index()]
    }

    /// Wrap a raw arena index known to be valid for `store`.
    #[inline]
    pub(crate) fn from_raw(store: &'a NetStore, raw: u32) -> NodeRef<'a> {
        NodeRef {
            store,
            id: NodeId::new(raw),
        }
    }

    /// This record's stable global id.
    #[inline]
    pub fn id(&self) -> NodeId {
        self.id
    }

    /// Whether this is a fill or a gap.
    #[inline]
    pub fn kind(&self) -> NodeKind {
        self.node().kind
    }

    /// Whether this record is a fill.
    #[inline]
    pub fn is_fill(&self) -> bool {
        self.kind().is_fill()
    }

    /// Whether this record is a gap.
    #[inline]
    pub fn is_gap(&self) -> bool {
        self.kind().is_gap()
    }

    /// Nesting depth (roots are depth 0).
    #[inline]
    pub fn depth(&self) -> u16 {
        self.node().depth
    }

    /// Reference start coordinate.
    #[inline]
    pub fn reference_start(&self) -> Coord {
        self.node().reference_start
    }

    /// Reference size.
    #[inline]
    pub fn reference_size(&self) -> Coord {
        self.node().reference_size
    }

    /// Reference end coordinate (`start + size`).
    #[inline]
    pub fn reference_end(&self) -> Coord {
        let n = self.node();
        n.reference_start.saturating_add(n.reference_size)
    }

    /// Reference interval.
    #[inline]
    pub fn reference_range(&self) -> NetRange {
        NetRange::new(self.reference_start(), self.reference_end())
    }

    /// Query start coordinate.
    #[inline]
    pub fn query_start(&self) -> Coord {
        self.node().query_start
    }

    /// Query size.
    #[inline]
    pub fn query_size(&self) -> Coord {
        self.node().query_size
    }

    /// Query end coordinate (`start + size`).
    #[inline]
    pub fn query_end(&self) -> Coord {
        let n = self.node();
        n.query_start.saturating_add(n.query_size)
    }

    /// Query interval.
    #[inline]
    pub fn query_range(&self) -> NetRange {
        NetRange::new(self.query_start(), self.query_end())
    }

    /// Query strand.
    #[inline]
    pub fn query_strand(&self) -> Strand {
        self.node().query_strand
    }

    /// Query sequence name (zero-copy).
    #[inline]
    pub fn query_name(&self) -> ByteSlice {
        self.store.storage.slice(self.node().query_name)
    }

    /// Query sequence name as borrowed bytes.
    #[inline]
    pub fn query_name_bytes(&self) -> &'a [u8] {
        self.node().query_name.resolve(self.store.bytes())
    }

    /// Optional attributes attached to this record.
    #[inline]
    pub fn attributes(&self) -> AttributeView<'a> {
        let n = self.node();
        let bytes = self.store.bytes();
        if n.attributes == NO_ATTRIBUTES {
            AttributeView::empty(bytes)
        } else {
            let block = &self.store.attrs[n.attributes as usize];
            let extras = block.extra.as_slice(&self.store.extras);
            AttributeView::new(Some(block), extras, bytes)
        }
    }

    /// The chain id, if present.
    #[inline]
    pub fn chain_id(&self) -> Option<crate::model::ChainId> {
        self.attributes().chain_id()
    }

    /// Whether this record has any children.
    #[inline]
    pub fn has_children(&self) -> bool {
        opt_node(self.node().first_child).is_some()
    }

    /// First arena index after this record's subtree.
    #[inline]
    pub(crate) fn subtree_end(&self) -> u32 {
        self.node().subtree_end
    }
}

impl std::fmt::Debug for NodeRef<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NodeRef")
            .field("id", &self.id.get())
            .field("kind", &self.kind())
            .field("depth", &self.depth())
            .field("reference", &self.reference_range())
            .field("query_name", &self.query_name().to_string_lossy())
            .finish()
    }
}
