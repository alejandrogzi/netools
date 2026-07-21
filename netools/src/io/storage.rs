// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the GNU General Public License v3.0.

//! Shared byte storage and the public zero-copy [`ByteSlice`].
//!
//! All textual data (sequence names, undocumented attribute keys and values,
//! and unrecognised type strings) is exposed as [`ByteSlice`] values that
//! borrow reference-counted backing storage. Parsing never requires valid
//! UTF-8, so genomic names with unusual bytes round-trip unchanged.

use std::borrow::Cow;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use crate::model::Span;

/// Reference-counted backing buffer shared by every slice from one reader.
#[derive(Clone)]
pub(crate) enum SharedBytes {
    /// An owned, heap-allocated buffer.
    Owned(Arc<[u8]>),
    /// A memory-mapped file.
    #[cfg(feature = "mmap")]
    Mapped(Arc<memmap2::Mmap>),
}

impl SharedBytes {
    /// Construct owned storage from a byte vector.
    #[inline]
    pub(crate) fn from_vec(data: Vec<u8>) -> SharedBytes {
        SharedBytes::Owned(Arc::from(data.into_boxed_slice()))
    }

    /// The backing bytes.
    #[inline]
    pub(crate) fn as_bytes(&self) -> &[u8] {
        match self {
            SharedBytes::Owned(bytes) => bytes,
            #[cfg(feature = "mmap")]
            SharedBytes::Mapped(map) => map,
        }
    }

    /// Length of the backing buffer.
    #[inline]
    pub(crate) fn len(&self) -> usize {
        self.as_bytes().len()
    }

    /// Build a public slice over a span of this storage.
    #[inline]
    pub(crate) fn slice(&self, span: Span) -> ByteSlice {
        ByteSlice {
            storage: self.clone(),
            start: span.start,
            end: span.end,
        }
    }
}

impl fmt::Debug for SharedBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SharedBytes::Owned(bytes) => {
                f.debug_struct("Owned").field("len", &bytes.len()).finish()
            }
            #[cfg(feature = "mmap")]
            SharedBytes::Mapped(map) => f.debug_struct("Mapped").field("len", &map.len()).finish(),
        }
    }
}

/// A zero-copy view over a textual field, borrowing shared backing storage.
///
/// A `ByteSlice` keeps the backing buffer alive through a reference count, so
/// it remains valid after the reader that produced it is dropped.
#[derive(Clone)]
pub struct ByteSlice {
    storage: SharedBytes,
    start: u32,
    end: u32,
}

impl ByteSlice {
    /// The referenced bytes.
    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        let bytes = self.storage.as_bytes();
        let start = self.start as usize;
        let end = self.end as usize;
        if start <= end && end <= bytes.len() {
            &bytes[start..end]
        } else {
            &[]
        }
    }

    /// The referenced bytes as UTF-8, or `None` if not valid UTF-8.
    #[inline]
    pub fn as_str(&self) -> Option<&str> {
        std::str::from_utf8(self.as_bytes()).ok()
    }

    /// The referenced bytes as UTF-8, replacing invalid sequences.
    #[inline]
    pub fn to_string_lossy(&self) -> Cow<'_, str> {
        String::from_utf8_lossy(self.as_bytes())
    }

    /// Number of referenced bytes.
    #[inline]
    pub fn len(&self) -> usize {
        self.as_bytes().len()
    }

    /// Whether the slice is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.end <= self.start
    }
}

impl AsRef<[u8]> for ByteSlice {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl PartialEq for ByteSlice {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.as_bytes() == other.as_bytes()
    }
}

impl Eq for ByteSlice {}

impl PartialEq<[u8]> for ByteSlice {
    #[inline]
    fn eq(&self, other: &[u8]) -> bool {
        self.as_bytes() == other
    }
}

impl PartialEq<&[u8]> for ByteSlice {
    #[inline]
    fn eq(&self, other: &&[u8]) -> bool {
        self.as_bytes() == *other
    }
}

impl Hash for ByteSlice {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_bytes().hash(state);
    }
}

impl fmt::Debug for ByteSlice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.to_string_lossy())
    }
}

impl fmt::Display for ByteSlice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_string_lossy())
    }
}
