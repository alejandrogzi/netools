// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the GNU General Public License v3.0.

//! Input/output: byte storage, readers, writers, streaming, and indexes.

pub(crate) mod compression;
pub mod reader;
pub(crate) mod storage;
pub mod streaming;

#[cfg(feature = "index")]
pub mod index;
#[cfg(feature = "write")]
pub mod writer;

pub use reader::{
    NetIter, NetRef, NodeRef, PreorderIter, Reader, ReaderOptions, ReaderOptionsBuilder,
};
pub use storage::ByteSlice;
pub use streaming::{NetEvent, OwnedAttributes, OwnedNodeRecord, StreamingNets, StreamingReader};

#[cfg(feature = "index")]
pub use index::{
    ChainIdIndex, IndexedInterval, NetIndex, NetSpan, NodeLocation, ReferenceIntervalIndex,
};

#[cfg(feature = "write")]
pub use writer::{
    Compression, FieldOrder, FileSink, IndentationStyle, NetLike, NodeLike, Writer, WriterOptions,
};
