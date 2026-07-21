// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the GNU General Public License v3.0.

//! Structured parse and I/O errors.
//!
//! No malformed input may panic; every failure produces a [`NetError`] carrying
//! the absolute byte offset, line, and column of the problem, plus the
//! reference net name once it is known and a bounded, lossy copy of the
//! offending bytes.

use std::fmt;

/// Convenience result type for the crate.
pub type Result<T> = std::result::Result<T, NetError>;

/// Maximum number of offending bytes retained in an error's context field.
const MAX_CONTEXT: usize = 64;

/// Category of a parse or I/O failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum NetErrorKind {
    /// Underlying I/O error.
    Io,
    /// The input contained no data.
    EmptyInput,
    /// Expected a `net` header but found none.
    MissingNetHeader,
    /// A `net` header was malformed.
    InvalidNetHeader,
    /// A record class other than `fill` or `gap`.
    InvalidRecordKind,
    /// Fewer than the seven required fixed fields.
    TooFewFields,
    /// An odd number of optional key/value tokens.
    OddAttributeCount,
    /// An integer field could not be parsed.
    InvalidInteger,
    /// A floating-point field could not be parsed.
    InvalidFloat,
    /// A numeric value overflowed its target type.
    NumericOverflow,
    /// `start + size` overflowed the coordinate type.
    CoordinateOverflow,
    /// A strand field was neither `+` nor `-`.
    InvalidStrand,
    /// Indentation could not be interpreted.
    InvalidIndentation,
    /// Indentation increased by more than the mode permits.
    IndentationJump,
    /// A tab character appeared in indentation.
    TabIndentation,
    /// A record could not be attached to any parent.
    OrphanRecord,
    /// Nesting exceeded the configured maximum depth.
    ExcessiveDepth,
    /// The arena would exceed the maximum node count.
    ExcessiveNodes,
    /// A documented attribute appeared more than once.
    DuplicateAttribute,
    /// An undocumented attribute in a mode that forbids them.
    UnknownAttribute,
    /// An unrecognised `type` value in a mode that forbids them.
    UnknownNetType,
    /// A structural (biological/topological) rule was violated.
    StructuralViolation,
    /// The input used a compression scheme without the required feature.
    UnsupportedCompression,
    /// A line exceeded the configured maximum length.
    LineTooLong,
}

impl NetErrorKind {
    /// A short, stable human-readable description.
    pub const fn message(self) -> &'static str {
        match self {
            NetErrorKind::Io => "I/O error",
            NetErrorKind::EmptyInput => "input contained no data",
            NetErrorKind::MissingNetHeader => "expected a `net` header",
            NetErrorKind::InvalidNetHeader => "malformed `net` header",
            NetErrorKind::InvalidRecordKind => "record class is not `fill` or `gap`",
            NetErrorKind::TooFewFields => "record has fewer than seven fixed fields",
            NetErrorKind::OddAttributeCount => "optional attributes have an odd token count",
            NetErrorKind::InvalidInteger => "invalid integer",
            NetErrorKind::InvalidFloat => "invalid floating-point value",
            NetErrorKind::NumericOverflow => "numeric value overflowed its type",
            NetErrorKind::CoordinateOverflow => "start + size overflowed the coordinate type",
            NetErrorKind::InvalidStrand => "strand is not `+` or `-`",
            NetErrorKind::InvalidIndentation => "indentation could not be interpreted",
            NetErrorKind::IndentationJump => "indentation increased too far",
            NetErrorKind::TabIndentation => "tab character in indentation",
            NetErrorKind::OrphanRecord => "record could not be attached to a parent",
            NetErrorKind::ExcessiveDepth => "nesting exceeded the maximum depth",
            NetErrorKind::ExcessiveNodes => "too many records for the arena",
            NetErrorKind::DuplicateAttribute => "documented attribute appeared twice",
            NetErrorKind::UnknownAttribute => "undocumented attribute rejected",
            NetErrorKind::UnknownNetType => "unrecognised `type` value rejected",
            NetErrorKind::StructuralViolation => "structural rule violated",
            NetErrorKind::UnsupportedCompression => "unsupported compression",
            NetErrorKind::LineTooLong => "line exceeded the maximum length",
        }
    }
}

/// A structured parse or I/O error.
#[derive(Debug, Clone)]
pub struct NetError {
    /// What went wrong.
    pub kind: NetErrorKind,
    /// Absolute byte offset of the problem within the input.
    pub byte_offset: u64,
    /// 1-based line number.
    pub line: u64,
    /// 1-based column number.
    pub column: u32,
    /// Reference net name, once known.
    pub net_name: Option<Vec<u8>>,
    /// A bounded, lossy copy of the offending bytes.
    pub context: Option<Vec<u8>>,
}

impl NetError {
    /// Construct an error with position but no name/context yet.
    pub(crate) fn new(kind: NetErrorKind, byte_offset: u64, line: u64, column: u32) -> NetError {
        NetError {
            kind,
            byte_offset,
            line,
            column,
            net_name: None,
            context: None,
        }
    }

    /// Attach the reference net name (truncated defensively).
    pub(crate) fn with_net_name(mut self, name: &[u8]) -> NetError {
        if self.net_name.is_none() {
            self.net_name = Some(name.iter().copied().take(MAX_CONTEXT).collect());
        }
        self
    }

    /// Attach a bounded copy of the offending bytes.
    pub(crate) fn with_context(mut self, context: &[u8]) -> NetError {
        self.context = Some(context.iter().copied().take(MAX_CONTEXT).collect());
        self
    }

    /// The error category.
    #[inline]
    pub fn kind(&self) -> NetErrorKind {
        self.kind
    }
}

impl fmt::Display for NetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} at line {}, column {} (byte {})",
            self.kind.message(),
            self.line,
            self.column,
            self.byte_offset
        )?;
        if let Some(name) = &self.net_name {
            write!(f, " in net `{}`", String::from_utf8_lossy(name))?;
        }
        if let Some(context) = &self.context {
            write!(f, ": `{}`", String::from_utf8_lossy(context))?;
        }
        Ok(())
    }
}

impl std::error::Error for NetError {}

impl From<std::io::Error> for NetError {
    fn from(err: std::io::Error) -> NetError {
        NetError {
            kind: NetErrorKind::Io,
            byte_offset: 0,
            line: 0,
            column: 0,
            net_name: None,
            context: Some(err.to_string().into_bytes()),
        }
    }
}
