// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the GNU General Public License v3.0.

//! Errors produced while constructing NETs from chains.

use std::fmt;

/// Result type for chain-driven NET construction.
pub type Result<T> = std::result::Result<T, ChainNetError>;

/// A structured chain-NET construction failure.
#[derive(Debug)]
#[non_exhaustive]
pub enum ChainNetError {
    /// An underlying filesystem operation failed.
    Io(std::io::Error),
    /// The chain parser rejected the input.
    Chain(chaintools::ChainError),
    /// A 2bit index could not be read.
    TwoBit(twobit::Error),
    /// A required reference or query size source was not configured.
    MissingSizeSource(&'static str),
    /// A size source contained the same sequence more than once.
    DuplicateSequence(Vec<u8>),
    /// A `.chrom.sizes` record was malformed.
    InvalidSizeRecord {
        /// 1-based source line.
        line: u64,
        /// Brief reason.
        reason: &'static str,
    },
    /// A sequence size cannot be represented by the NET coordinate type.
    SequenceTooLarge(Vec<u8>),
    /// A chain sequence was absent from the corresponding size source.
    MissingSequence {
        /// `reference` or `query`.
        side: &'static str,
        /// Chain ID.
        chain_id: u64,
        /// Sequence name.
        name: Vec<u8>,
    },
    /// A chain header size disagreed with the configured sequence size.
    SizeMismatch {
        /// `reference` or `query`.
        side: &'static str,
        /// Chain ID.
        chain_id: u64,
        /// Sequence name.
        name: Vec<u8>,
        /// Size in the chain header.
        chain_size: u32,
        /// Size in the size source.
        configured_size: u32,
    },
    /// Chain scores were not in descending input order.
    UnsortedScores {
        /// Chain that violates the ordering contract.
        chain_id: u64,
        /// Its score.
        score: i64,
        /// Score of the immediately preceding chain.
        previous_score: i64,
    },
    /// Chain IDs must be unique.
    DuplicateChainId(u64),
    /// UCSC chain NET construction requires a plus-strand reference.
    InvalidReferenceStrand(u64),
    /// Dense blocks did not agree with the chain header or overflowed.
    MalformedBlocks {
        /// Chain ID.
        chain_id: u64,
        /// Brief reason.
        reason: &'static str,
    },
    /// An option value was invalid.
    InvalidOption(&'static str),
    /// Available-space invariants were violated.
    InvalidSpace {
        /// Sequence containing the invalid space.
        sequence: Vec<u8>,
        /// Brief reason.
        reason: &'static str,
    },
    /// A proportional score was non-finite or outside the supported range.
    ScoreOverflow,
    /// Generated topology exceeded the representable NET depth.
    ExcessiveDepth,
    /// Generated topology exceeded the 32-bit arena capacity.
    ExcessiveNodes,
    /// Writing a generated NET failed.
    Net(crate::NetError),
}

impl fmt::Display for ChainNetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "I/O error: {err}"),
            Self::Chain(err) => write!(f, "chain parse error: {err}"),
            Self::TwoBit(err) => write!(f, "2bit error: {err}"),
            Self::MissingSizeSource(side) => write!(f, "missing {side} sequence sizes"),
            Self::DuplicateSequence(name) => write!(
                f,
                "duplicate sequence `{}` in size source",
                String::from_utf8_lossy(name)
            ),
            Self::InvalidSizeRecord { line, reason } => {
                write!(f, "invalid size record at line {line}: {reason}")
            }
            Self::SequenceTooLarge(name) => write!(
                f,
                "sequence `{}` is too large for 32-bit NET coordinates",
                String::from_utf8_lossy(name)
            ),
            Self::MissingSequence {
                side,
                chain_id,
                name,
            } => write!(
                f,
                "chain {chain_id} {side} sequence `{}` is absent from the size source",
                String::from_utf8_lossy(name)
            ),
            Self::SizeMismatch {
                side,
                chain_id,
                name,
                chain_size,
                configured_size,
            } => write!(
                f,
                "chain {chain_id} {side} size mismatch for `{}`: chain has {chain_size}, size source has {configured_size}",
                String::from_utf8_lossy(name)
            ),
            Self::UnsortedScores {
                chain_id,
                score,
                previous_score,
            } => write!(
                f,
                "chain input is not sorted by descending score: chain {chain_id} has score {score} after score {previous_score}"
            ),
            Self::DuplicateChainId(id) => write!(f, "duplicate chain ID {id}"),
            Self::InvalidReferenceStrand(id) => {
                write!(f, "chain {id} has a minus-strand reference")
            }
            Self::MalformedBlocks { chain_id, reason } => {
                write!(
                    f,
                    "malformed block progression in chain {chain_id}: {reason}"
                )
            }
            Self::InvalidOption(reason) => write!(f, "invalid net option: {reason}"),
            Self::InvalidSpace { sequence, reason } => write!(
                f,
                "invalid available space on `{}`: {reason}",
                String::from_utf8_lossy(sequence)
            ),
            Self::ScoreOverflow => write!(f, "generated fill score cannot be represented"),
            Self::ExcessiveDepth => write!(f, "generated NET exceeds the maximum nesting depth"),
            Self::ExcessiveNodes => write!(f, "generated NET exceeds the node arena capacity"),
            Self::Net(err) => write!(f, "NET error: {err}"),
        }
    }
}

impl std::error::Error for ChainNetError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            Self::Chain(err) => Some(err),
            Self::TwoBit(err) => Some(err),
            Self::Net(err) => Some(err),
            _ => None,
        }
    }
}

impl From<std::io::Error> for ChainNetError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<chaintools::ChainError> for ChainNetError {
    fn from(value: chaintools::ChainError) -> Self {
        Self::Chain(value)
    }
}

impl From<twobit::Error> for ChainNetError {
    fn from(value: twobit::Error) -> Self {
        Self::TwoBit(value)
    }
}

impl From<crate::NetError> for ChainNetError {
    fn from(value: crate::NetError) -> Self {
        Self::Net(value)
    }
}
