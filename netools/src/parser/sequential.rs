// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the GNU General Public License v3.0.

//! Sequential, single-pass NET parser.
//!
//! The parser walks the input byte-by-byte using `memchr` for line boundaries,
//! maintains an explicit indentation stack (no Rust recursion), and appends
//! records to a contiguous preorder arena. Textual fields are recorded as byte
//! spans into the backing buffer; nothing is copied except the reference name
//! kept for error messages.

use crate::model::attributes::{
    ExtraAttribute, KnownAttr, NetTypeKind, NetTypeRepr, NodeAttributes,
};
use crate::model::ids::{MAX_NODES, NO_ATTRIBUTES, NO_NODE};
use crate::model::node::{NetNode, NodeKind};
use crate::model::range::Span;
use crate::model::strand::Strand;
use crate::model::{Coord, Section};
use crate::parser::ParseMode;
use crate::parser::ParserConfig;
use crate::parser::common::{
    Indent, NumErr, Tokens, measure_indent, parse_f64, parse_i32, parse_u32, parse_u64, strip_cr,
};
use crate::parser::error::{NetError, NetErrorKind, Result};

/// The storage-agnostic output of a parse: contiguous arenas plus section
/// descriptors. Spans are byte offsets into the shared backing buffer.
pub(crate) struct Parsed {
    pub(crate) sections: Vec<Section>,
    pub(crate) nodes: Vec<NetNode>,
    pub(crate) attrs: Vec<NodeAttributes>,
    pub(crate) extras: Vec<ExtraAttribute>,
}

/// One currently-open ancestor on the indentation stack.
struct OpenLevel {
    /// Physical indentation width of this record.
    indent: usize,
    /// Arena index of this record.
    node: u32,
    /// Arena index of this record's most recent child, or `NO_NODE`.
    last_child: u32,
    /// Indentation width used by this record's children (`usize::MAX` = unset).
    child_indent: usize,
}

/// Parse a complete NET document (possibly many sections).
pub(crate) fn parse_document(bytes: &[u8], config: ParserConfig) -> Result<Parsed> {
    if bytes.is_empty() {
        return Err(NetError::new(NetErrorKind::EmptyInput, 0, 1, 1));
    }
    let mut doc = Doc::new(bytes, 0, 1, config);
    doc.run()?;
    if doc.sections.is_empty() {
        // No header was ever seen. If there was any non-whitespace, `run` would
        // already have failed on the stray record; otherwise the input is
        // effectively empty.
        if bytes
            .iter()
            .all(|b| matches!(b, b' ' | b'\t' | b'\r' | b'\n'))
        {
            return Err(NetError::new(NetErrorKind::EmptyInput, 0, 1, 1));
        }
    }
    Ok(Parsed {
        sections: doc.sections,
        nodes: doc.nodes,
        attrs: doc.attrs,
        extras: doc.extras,
    })
}

/// Parse a single section slice (no empty-input check), for the parallel path.
///
/// `base` is the slice's global byte offset and `start_line` the 1-based line
/// number of the slice's first line, so spans and error positions are global.
#[cfg(feature = "parallel")]
pub(crate) fn parse_slice(
    bytes: &[u8],
    base: u32,
    start_line: u64,
    config: ParserConfig,
) -> Result<Parsed> {
    let mut doc = Doc::new(bytes, base, start_line, config);
    doc.run()?;
    Ok(Parsed {
        sections: doc.sections,
        nodes: doc.nodes,
        attrs: doc.attrs,
        extras: doc.extras,
    })
}

/// Mutable parser state for one input slice.
struct Doc<'a> {
    bytes: &'a [u8],
    /// Global byte offset added to every span (0 for whole-file parsing).
    base: u32,
    /// Line number of the first line of `bytes` (1 for whole-file parsing).
    start_line: u64,
    config: ParserConfig,

    nodes: Vec<NetNode>,
    attrs: Vec<NodeAttributes>,
    extras: Vec<ExtraAttribute>,
    sections: Vec<Section>,

    stack: Vec<OpenLevel>,

    sec_active: bool,
    sec_name: Span,
    sec_name_owned: Vec<u8>,
    sec_size: Coord,
    sec_node_start: u32,
    sec_first_root: u32,
    sec_last_root: u32,
    sec_root_count: u32,
    root_indent: usize,
}

impl<'a> Doc<'a> {
    fn new(bytes: &'a [u8], base: u32, start_line: u64, config: ParserConfig) -> Doc<'a> {
        Doc {
            bytes,
            base,
            start_line,
            config,
            nodes: Vec::new(),
            attrs: Vec::new(),
            extras: Vec::new(),
            sections: Vec::new(),
            stack: Vec::new(),
            sec_active: false,
            sec_name: Span::EMPTY,
            sec_name_owned: Vec::new(),
            sec_size: 0,
            sec_node_start: 0,
            sec_first_root: NO_NODE,
            sec_last_root: NO_NODE,
            sec_root_count: 0,
            root_indent: usize::MAX,
        }
    }

    /// Global byte offset of a token, given the line's local start and the
    /// token's offset within the line.
    #[inline]
    fn make_span(&self, line_abs: usize, rel_start: usize, rel_end: usize) -> Span {
        let base = self.base as usize;
        Span::new(
            (base + line_abs + rel_start) as u32,
            (base + line_abs + rel_end) as u32,
        )
    }

    /// Build a positioned error, attaching the current net name if known.
    fn error(
        &self,
        kind: NetErrorKind,
        line_no: u64,
        line_global_start: usize,
        rel: usize,
        ctx: &[u8],
    ) -> NetError {
        let abs = (line_global_start + rel) as u64;
        let col = (rel + 1) as u32;
        let mut err = NetError::new(kind, abs, line_no, col);
        if self.sec_active {
            err = err.with_net_name(&self.sec_name_owned);
        }
        if !ctx.is_empty() {
            err = err.with_context(ctx);
        }
        err
    }

    /// Whether a line at column zero opens a new section.
    #[inline]
    fn is_header(line: &[u8], indent: &Indent) -> bool {
        indent.width == 0
            && line.len() >= 3
            && &line[0..3] == b"net"
            && (line.len() == 3 || matches!(line[3], b' ' | b'\t'))
    }

    /// Drive the parse over every line of `bytes`.
    fn run(&mut self) -> Result<()> {
        let bytes = self.bytes;
        let mut pos = 0usize;
        let mut line_no = self.start_line;

        while pos < bytes.len() {
            let (line_end, next) = match memchr::memchr(b'\n', &bytes[pos..]) {
                Some(i) => (pos + i, pos + i + 1),
                None => (bytes.len(), bytes.len()),
            };
            let line = strip_cr(&bytes[pos..line_end]);
            let line_abs = pos;
            let line_global_start = self.base as usize + line_abs;

            if line.len() > self.config.max_line_length {
                return Err(self.error(
                    NetErrorKind::LineTooLong,
                    line_no,
                    line_global_start,
                    0,
                    b"",
                ));
            }

            let indent = measure_indent(line);
            if indent.content_start >= line.len() {
                // Blank line.
            } else if line[indent.content_start] == b'#' {
                // Comment line.
            } else if Self::is_header(line, &indent) {
                self.start_section(line, line_abs, line_no)?;
            } else {
                self.handle_record(line, &indent, line_abs, line_no)?;
            }

            pos = next;
            line_no += 1;
        }

        if self.sec_active {
            self.finish_section();
        }
        Ok(())
    }

    /// Open a new section from a `net <name> <size>` header.
    fn start_section(&mut self, line: &[u8], line_abs: usize, line_no: u64) -> Result<()> {
        if self.sec_active {
            self.finish_section();
        }
        let line_global_start = self.base as usize + line_abs;

        let mut toks = Tokens::new(line, 0);
        let _net = toks.next(); // "net", guaranteed by `is_header`.
        let name = match toks.next() {
            Some(t) => t,
            None => {
                return Err(self.error(
                    NetErrorKind::InvalidNetHeader,
                    line_no,
                    line_global_start,
                    line.len(),
                    b"",
                ));
            }
        };
        let size_tok = match toks.next() {
            Some(t) => t,
            None => {
                return Err(self.error(
                    NetErrorKind::InvalidNetHeader,
                    line_no,
                    line_global_start,
                    line.len(),
                    b"",
                ));
            }
        };
        let size = parse_u32(&line[size_tok.0..size_tok.1]).map_err(|_| {
            self.error(
                NetErrorKind::InvalidNetHeader,
                line_no,
                line_global_start,
                size_tok.0,
                &line[size_tok.0..size_tok.1],
            )
        })?;

        self.sec_name = self.make_span(line_abs, name.0, name.1);
        self.sec_name_owned = line[name.0..name.1].to_vec();
        self.sec_size = size;
        self.sec_active = true;
        self.sec_node_start = self.nodes.len() as u32;
        self.sec_first_root = NO_NODE;
        self.sec_last_root = NO_NODE;
        self.sec_root_count = 0;
        self.root_indent = usize::MAX;
        self.stack.clear();
        Ok(())
    }

    /// Close the current section, finalising subtree boundaries.
    fn finish_section(&mut self) {
        let end = self.nodes.len() as u32;
        for lvl in self.stack.drain(..) {
            self.nodes[lvl.node as usize].subtree_end = end;
        }
        self.sections.push(Section {
            reference_name: self.sec_name,
            reference_size: self.sec_size,
            node_start: self.sec_node_start,
            node_end: end,
            first_root: self.sec_first_root,
            root_count: self.sec_root_count,
        });
        self.sec_active = false;
    }

    /// Parse and attach one `fill`/`gap` record.
    fn handle_record(
        &mut self,
        line: &[u8],
        indent: &Indent,
        line_abs: usize,
        line_no: u64,
    ) -> Result<()> {
        let mode = self.config.mode;
        let line_global_start = self.base as usize + line_abs;

        if !self.sec_active {
            return Err(self.error(
                NetErrorKind::MissingNetHeader,
                line_no,
                line_global_start,
                indent.content_start,
                &line[indent.content_start..],
            ));
        }

        if indent.has_tab && mode != ParseMode::Permissive {
            return Err(self.error(
                NetErrorKind::TabIndentation,
                line_no,
                line_global_start,
                0,
                b"",
            ));
        }

        // --- Fixed fields ------------------------------------------------
        let mut toks = Tokens::new(line, indent.content_start);
        let class = toks.next().expect("non-empty content yields a token");
        let kind = match &line[class.0..class.1] {
            b"fill" => NodeKind::Fill,
            b"gap" => NodeKind::Gap,
            _ => {
                return Err(self.error(
                    NetErrorKind::InvalidRecordKind,
                    line_no,
                    line_global_start,
                    class.0,
                    &line[class.0..class.1],
                ));
            }
        };

        let reference_start =
            self.field_u32(line, &mut toks, line_no, line_global_start, line.len())?;
        let reference_size =
            self.field_u32(line, &mut toks, line_no, line_global_start, line.len())?;

        let q_name_tok = self.require(&mut toks, line_no, line_global_start, line.len())?;
        let q_name = self.make_span(line_abs, q_name_tok.0, q_name_tok.1);

        let strand_tok = self.require(&mut toks, line_no, line_global_start, line.len())?;
        let strand_bytes = &line[strand_tok.0..strand_tok.1];
        let query_strand = if strand_bytes.len() == 1 {
            Strand::from_byte(strand_bytes[0])
        } else {
            None
        }
        .ok_or_else(|| {
            self.error(
                NetErrorKind::InvalidStrand,
                line_no,
                line_global_start,
                strand_tok.0,
                strand_bytes,
            )
        })?;

        let query_start =
            self.field_u32(line, &mut toks, line_no, line_global_start, line.len())?;
        let query_size = self.field_u32(line, &mut toks, line_no, line_global_start, line.len())?;

        // Checked coordinate ends.
        if reference_start.checked_add(reference_size).is_none() {
            return Err(self.error(
                NetErrorKind::CoordinateOverflow,
                line_no,
                line_global_start,
                class.0,
                b"",
            ));
        }
        if query_start.checked_add(query_size).is_none() {
            return Err(self.error(
                NetErrorKind::CoordinateOverflow,
                line_no,
                line_global_start,
                class.0,
                b"",
            ));
        }

        // --- Optional attributes ----------------------------------------
        let attrs = self.parse_attributes(line, &mut toks, line_abs, line_no, line_global_start)?;

        // --- Indentation → parent / depth -------------------------------
        while let Some(top) = self.stack.last() {
            if top.indent >= indent.width {
                let popped = self.stack.pop().unwrap();
                self.nodes[popped.node as usize].subtree_end = self.nodes.len() as u32;
            } else {
                break;
            }
        }

        let (parent, depth_usize) = match self.stack.last() {
            Some(top) => (top.node, self.stack.len()),
            None => (NO_NODE, 0),
        };

        if depth_usize > self.config.max_depth as usize {
            return Err(self.error(
                NetErrorKind::ExcessiveDepth,
                line_no,
                line_global_start,
                indent.content_start,
                b"",
            ));
        }
        let depth = depth_usize as u16;

        // Strict-mode local structure: fill/gap alternation by depth and the
        // id-presence rules. Alternation is decided by depth parity: roots and
        // even depths are fills, odd depths are gaps.
        if mode == ParseMode::Strict {
            let want = if depth % 2 == 0 {
                NodeKind::Fill
            } else {
                NodeKind::Gap
            };
            if kind != want {
                return Err(self.error(
                    NetErrorKind::StructuralViolation,
                    line_no,
                    line_global_start,
                    class.0,
                    b"",
                ));
            }
            let has_id = attrs.present.has(KnownAttr::ChainId);
            if (kind == NodeKind::Fill) != has_id {
                return Err(self.error(
                    NetErrorKind::StructuralViolation,
                    line_no,
                    line_global_start,
                    class.0,
                    b"",
                ));
            }
        }

        // Indentation-consistency validation for non-permissive modes.
        if !matches!(mode, ParseMode::Permissive) {
            self.check_indentation(indent.width, mode, line_no, line_global_start)?;
        } else {
            // Still record child/root indent so canonical remaps are stable.
            self.record_indent(indent.width);
        }

        // --- Commit node -------------------------------------------------
        if self.nodes.len() >= MAX_NODES as usize {
            return Err(self.error(
                NetErrorKind::ExcessiveNodes,
                line_no,
                line_global_start,
                0,
                b"",
            ));
        }
        let idx = self.nodes.len() as u32;

        let attr_index = if attrs.is_empty() {
            NO_ATTRIBUTES
        } else {
            if self.attrs.len() >= MAX_NODES as usize {
                return Err(self.error(
                    NetErrorKind::ExcessiveNodes,
                    line_no,
                    line_global_start,
                    0,
                    b"",
                ));
            }
            let ai = self.attrs.len() as u32;
            self.attrs.push(attrs);
            ai
        };

        self.nodes.push(NetNode {
            kind,
            query_strand,
            depth,
            reference_start,
            reference_size,
            query_start,
            query_size,
            query_name: q_name,
            parent,
            first_child: NO_NODE,
            next_sibling: NO_NODE,
            subtree_end: idx + 1,
            attributes: attr_index,
        });

        // --- Link topology ----------------------------------------------
        if parent == NO_NODE {
            if self.sec_first_root == NO_NODE {
                self.sec_first_root = idx;
            } else {
                self.nodes[self.sec_last_root as usize].next_sibling = idx;
            }
            self.sec_last_root = idx;
            self.sec_root_count += 1;
        } else {
            let last_child = self.stack.last().unwrap().last_child;
            if last_child == NO_NODE {
                self.nodes[parent as usize].first_child = idx;
            } else {
                self.nodes[last_child as usize].next_sibling = idx;
            }
            self.stack.last_mut().unwrap().last_child = idx;
        }

        self.stack.push(OpenLevel {
            indent: indent.width,
            node: idx,
            last_child: NO_NODE,
            child_indent: usize::MAX,
        });

        Ok(())
    }

    /// Pull the next token, mapping absence to `TooFewFields`.
    #[inline]
    fn require(
        &self,
        toks: &mut Tokens<'_>,
        line_no: u64,
        lgs: usize,
        line_len: usize,
    ) -> Result<(usize, usize)> {
        toks.next()
            .ok_or_else(|| self.error(NetErrorKind::TooFewFields, line_no, lgs, line_len, b""))
    }

    /// Pull and parse the next `u32` fixed field.
    #[inline]
    fn field_u32(
        &self,
        line: &[u8],
        toks: &mut Tokens<'_>,
        line_no: u64,
        lgs: usize,
        line_len: usize,
    ) -> Result<u32> {
        let tok = self.require(toks, line_no, lgs, line_len)?;
        parse_u32(&line[tok.0..tok.1])
            .map_err(|e| self.error(num_int_kind(e), line_no, lgs, tok.0, &line[tok.0..tok.1]))
    }

    /// Parse the trailing `key value` pairs of a record.
    fn parse_attributes(
        &mut self,
        line: &[u8],
        toks: &mut Tokens<'_>,
        line_abs: usize,
        line_no: u64,
        lgs: usize,
    ) -> Result<NodeAttributes> {
        let mode = self.config.mode;
        let extra_start = self.extras.len() as u32;
        let mut extra_len: u32 = 0;
        let mut attrs = NodeAttributes::default();

        while let Some(key) = toks.next() {
            let value = match toks.next() {
                Some(v) => v,
                None => {
                    return Err(self.error(
                        NetErrorKind::OddAttributeCount,
                        line_no,
                        lgs,
                        key.0,
                        &line[key.0..key.1],
                    ));
                }
            };
            let key_bytes = &line[key.0..key.1];
            let value_bytes = &line[value.0..value.1];

            if let Some(attr) = KnownAttr::from_key(key_bytes) {
                if attrs.present.has(attr) && mode != ParseMode::Permissive {
                    return Err(self.error(
                        NetErrorKind::DuplicateAttribute,
                        line_no,
                        lgs,
                        key.0,
                        key_bytes,
                    ));
                }
                match attr {
                    KnownAttr::ChainId => {
                        let v = parse_u64(value_bytes).map_err(|e| {
                            self.error(num_int_kind(e), line_no, lgs, value.0, value_bytes)
                        })?;
                        attrs.chain_id = v;
                        attrs.present.insert(KnownAttr::ChainId);
                    }
                    KnownAttr::AlignmentScore => {
                        let v = parse_f64(value_bytes).map_err(|_| {
                            self.error(
                                NetErrorKind::InvalidFloat,
                                line_no,
                                lgs,
                                value.0,
                                value_bytes,
                            )
                        })?;
                        attrs.alignment_score = v;
                        attrs.present.insert(KnownAttr::AlignmentScore);
                    }
                    KnownAttr::AlignedBases => {
                        let v = parse_u32(value_bytes).map_err(|e| {
                            self.error(num_int_kind(e), line_no, lgs, value.0, value_bytes)
                        })?;
                        attrs.aligned_bases = v;
                        attrs.present.insert(KnownAttr::AlignedBases);
                    }
                    KnownAttr::Type => {
                        if let Some(known) = NetTypeKind::from_bytes(value_bytes) {
                            attrs.net_type = NetTypeRepr::Known(known);
                        } else if mode == ParseMode::Strict {
                            return Err(self.error(
                                NetErrorKind::UnknownNetType,
                                line_no,
                                lgs,
                                value.0,
                                value_bytes,
                            ));
                        } else {
                            attrs.net_type =
                                NetTypeRepr::Other(self.make_span(line_abs, value.0, value.1));
                        }
                        attrs.present.insert(KnownAttr::Type);
                    }
                    other => {
                        let v = parse_i32(value_bytes).map_err(|e| {
                            self.error(num_int_kind(e), line_no, lgs, value.0, value_bytes)
                        })?;
                        attrs.set_i32(other, v);
                    }
                }
            } else {
                if mode == ParseMode::Strict {
                    return Err(self.error(
                        NetErrorKind::UnknownAttribute,
                        line_no,
                        lgs,
                        key.0,
                        key_bytes,
                    ));
                }
                if extra_len >= u16::MAX as u32 {
                    return Err(self.error(
                        NetErrorKind::StructuralViolation,
                        line_no,
                        lgs,
                        key.0,
                        key_bytes,
                    ));
                }
                self.extras.push(ExtraAttribute {
                    key: self.make_span(line_abs, key.0, key.1),
                    value: self.make_span(line_abs, value.0, value.1),
                });
                extra_len += 1;
            }
        }

        attrs.extra.start = extra_start;
        attrs.extra.len = extra_len as u16;
        Ok(attrs)
    }

    /// Validate indentation for non-permissive modes and record child/root
    /// indentation.
    fn check_indentation(
        &mut self,
        width: usize,
        mode: ParseMode,
        line_no: u64,
        lgs: usize,
    ) -> Result<()> {
        if let Some(top) = self.stack.last_mut() {
            if top.child_indent == usize::MAX {
                if mode == ParseMode::Strict && width != top.indent + 1 {
                    return Err(self.error(NetErrorKind::InvalidIndentation, line_no, lgs, 0, b""));
                }
                top.child_indent = width;
            } else if width != top.child_indent {
                return Err(self.error(NetErrorKind::IndentationJump, line_no, lgs, 0, b""));
            }
        } else if self.root_indent == usize::MAX {
            if mode == ParseMode::Strict && width != 1 {
                return Err(self.error(NetErrorKind::InvalidIndentation, line_no, lgs, 0, b""));
            }
            self.root_indent = width;
        } else if width != self.root_indent {
            return Err(self.error(NetErrorKind::IndentationJump, line_no, lgs, 0, b""));
        }
        Ok(())
    }

    /// Record child/root indentation without validating (permissive mode).
    fn record_indent(&mut self, width: usize) {
        if let Some(top) = self.stack.last_mut() {
            if top.child_indent == usize::MAX {
                top.child_indent = width;
            }
        } else if self.root_indent == usize::MAX {
            self.root_indent = width;
        }
    }
}

/// Map an integer parse error to its error kind.
#[inline]
fn num_int_kind(e: NumErr) -> NetErrorKind {
    match e {
        NumErr::Invalid => NetErrorKind::InvalidInteger,
        NumErr::Overflow => NetErrorKind::NumericOverflow,
    }
}
