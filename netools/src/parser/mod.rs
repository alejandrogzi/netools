// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the GNU General Public License v3.0.

//! NET parsing: tokenisation, section detection, tree construction, and errors.

pub mod common;
pub mod error;
pub mod sequential;

#[cfg(feature = "parallel")]
pub mod parallel;
#[cfg(feature = "parallel")]
pub(crate) mod section_scan;

pub use error::{NetError, NetErrorKind, Result};

pub(crate) use sequential::{Parsed, parse_document};

/// Default defensive maximum nesting depth.
pub(crate) const DEFAULT_MAX_DEPTH: u16 = 4096;

/// Default maximum accepted line length, in bytes.
pub(crate) const DEFAULT_MAX_LINE_LENGTH: usize = 16 * 1024 * 1024;

/// Subset of reader options that the parsers consume.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ParserConfig {
    pub(crate) mode: ParseMode,
    pub(crate) max_depth: u16,
    pub(crate) max_line_length: usize,
}

impl Default for ParserConfig {
    fn default() -> Self {
        ParserConfig {
            mode: ParseMode::Compatible,
            max_depth: DEFAULT_MAX_DEPTH,
            max_line_length: DEFAULT_MAX_LINE_LENGTH,
        }
    }
}

/// Parsing strictness.
///
/// Parsing validity and structural/biological validity are kept separate: even
/// `Strict` parsing only rejects what it can decide locally while building the
/// tree. Deeper structural checks live in
/// [`validation`](crate::model) and can be requested after a permissive parse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ParseMode {
    /// Accept the widest range of inputs; report only impossible structure and
    /// malformed values. Larger indentation jumps map to the next logical
    /// depth.
    Permissive,
    /// The default. Blank lines and comments allowed; children must be indented
    /// deeper than their parent; dedents must match a level already on the
    /// stack; tabs in indentation are rejected.
    #[default]
    Compatible,
    /// Enforce exact one-space-per-level indentation and fill/gap conventions
    /// while parsing; reject undocumented attributes and type values.
    Strict,
}

impl ParseMode {
    /// Whether undocumented attributes and type values are rejected.
    #[inline]
    pub const fn rejects_unknown(self) -> bool {
        matches!(self, ParseMode::Strict)
    }
}
