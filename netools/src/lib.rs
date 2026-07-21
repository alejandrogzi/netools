// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the GNU General Public License v3.0.

//! # netools
//!
//! A library and CLI suite for reading, writing, validating, indexing,
//! transforming, and analysing UCSC `.net` files.
//!
//! A NET file describes, for one reference genome, how a query genome aligns to
//! it as a hierarchy of alignment `fill` records and the `gap` records between
//! them. Each reference sequence is one section beginning with a
//! `net <name> <size>` header; nested records are indented one space per level.
//!
//! One [`Net`] section corresponds to one reference sequence. A whole file is
//! read through a [`Reader<Net>`](Reader), which owns a single shared byte
//! buffer and a contiguous preorder arena of records.
//!
//! ```no_run
//! use netools::{Net, Reader};
//!
//! # fn main() -> netools::Result<()> {
//! let reader = Reader::<Net>::from_path("example.net")?;
//! println!("reference nets: {}", reader.len());
//! for net in reader.nets() {
//!     println!(
//!         "{}\t{}",
//!         net.reference_name().to_string_lossy(),
//!         net.reference_size(),
//!     );
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## Design highlights
//!
//! * Textual fields are exposed zero-copy for mmap and owned-buffer readers.
//! * The record hierarchy is a contiguous preorder arena, not boxed nodes.
//! * Parsing never requires valid UTF-8.
//! * Parsing and structural validation are separate concerns.

#![deny(unsafe_op_in_unsafe_fn)]
#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod algorithms;
pub mod io;
pub mod model;
pub mod parser;
pub mod traversal;

#[cfg(feature = "cli")]
pub mod cli;

pub use crate::algorithms::{
    AlignmentSpan, AlignmentSpanIndex, ChainOccurrence, FillContext, NestedFillGapContext,
    NetPredicate, Stats, TreeFilterPolicy,
};
pub use crate::io::{
    ByteSlice, NetEvent, NetIter, NetRef, NodeRef, OwnedAttributes, OwnedNodeRecord, PreorderIter,
    Reader, ReaderOptions, ReaderOptionsBuilder, StreamingNets, StreamingReader,
};
#[cfg(feature = "index")]
pub use crate::io::{
    ChainIdIndex, IndexedInterval, NetIndex, NetSpan, NodeLocation, ReferenceIntervalIndex,
};
#[cfg(feature = "write")]
pub use crate::io::{
    Compression, FieldOrder, FileSink, IndentationStyle, NetLike, NodeLike, Writer, WriterOptions,
};
pub use crate::model::{
    AttributeMask, AttributeView, ChainId, Coord, KnownAttr, Net, NetId, NetRange, NetType,
    NetTypeKind, NodeId, NodeKind, OwnedNet, Severity, Strand, ValidationCode, ValidationIssue,
    ValidationMode, ValidationReport,
};
pub use crate::parser::{NetError, NetErrorKind, ParseMode, Result};
pub use crate::traversal::{
    AncestorIter, ChildrenIter, DepthIter, DescendantIter, EventIter, FillIter, GapIter,
    NetVisitor, RootIter, SiblingIter, SubtreeIter, TraversalEvent,
};
