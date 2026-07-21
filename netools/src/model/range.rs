// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the GNU General Public License v3.0.

//! Coordinate intervals and the internal byte-range handle.

use crate::model::ids::Coord;

/// Half-open coordinate interval `[start, end)`.
///
/// Used for both reference and query axes. Construction does not enforce
/// `start <= end`; length queries saturate so a malformed interval never
/// panics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct NetRange {
    /// Inclusive start coordinate.
    pub start: Coord,
    /// Exclusive end coordinate.
    pub end: Coord,
}

impl NetRange {
    /// Construct a range from `start` and `end`.
    #[inline]
    pub const fn new(start: Coord, end: Coord) -> Self {
        NetRange { start, end }
    }

    /// Construct a range from a start coordinate and a size.
    #[inline]
    pub const fn from_start_size(start: Coord, size: Coord) -> Self {
        NetRange {
            start,
            end: start.saturating_add(size),
        }
    }

    /// Length of the interval (`end - start`), saturating at zero.
    #[inline]
    pub const fn len(&self) -> Coord {
        self.end.saturating_sub(self.start)
    }

    /// Whether the interval is empty.
    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.end <= self.start
    }

    /// Whether `pos` lies within `[start, end)`.
    #[inline]
    pub const fn contains(&self, pos: Coord) -> bool {
        pos >= self.start && pos < self.end
    }

    /// Whether `other` lies entirely within this interval.
    #[inline]
    pub const fn contains_range(&self, other: NetRange) -> bool {
        other.start >= self.start && other.end <= self.end
    }

    /// Whether the two half-open intervals share any position.
    #[inline]
    pub const fn overlaps(&self, other: NetRange) -> bool {
        self.start < other.end && other.start < self.end
    }
}

/// Internal byte-range handle into a shared backing buffer.
///
/// Stored inside arena nodes and attributes instead of an owned slice so that
/// the arena stays compact and cache-friendly. Resolved to real bytes on
/// demand against the reader's shared storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub(crate) struct Span {
    pub(crate) start: u32,
    pub(crate) end: u32,
}

impl Span {
    /// Empty span at offset zero.
    pub(crate) const EMPTY: Span = Span { start: 0, end: 0 };

    #[inline]
    pub(crate) const fn new(start: u32, end: u32) -> Span {
        Span { start, end }
    }

    #[inline]
    pub(crate) const fn is_empty(&self) -> bool {
        self.end <= self.start
    }

    /// Resolve this span against `bytes`. Returns an empty slice if the span is
    /// out of range (defensive; parser-produced spans are always in range).
    #[inline]
    pub(crate) fn resolve<'a>(&self, bytes: &'a [u8]) -> &'a [u8] {
        let start = self.start as usize;
        let end = self.end as usize;
        if start <= end && end <= bytes.len() {
            &bytes[start..end]
        } else {
            &[]
        }
    }
}
