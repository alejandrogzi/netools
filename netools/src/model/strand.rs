// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the GNU General Public License v3.0.

//! Query strand orientation.

use std::fmt;

/// Orientation of the query sequence relative to the reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Strand {
    /// Forward strand (`+`).
    Forward,
    /// Reverse strand (`-`).
    Reverse,
}

impl Strand {
    /// ASCII byte for this strand.
    #[inline]
    pub const fn as_byte(self) -> u8 {
        match self {
            Strand::Forward => b'+',
            Strand::Reverse => b'-',
        }
    }

    /// Character for this strand.
    #[inline]
    pub const fn as_char(self) -> char {
        match self {
            Strand::Forward => '+',
            Strand::Reverse => '-',
        }
    }

    /// Parse a single strand byte. Only `+` and `-` are accepted.
    #[inline]
    pub const fn from_byte(b: u8) -> Option<Strand> {
        match b {
            b'+' => Some(Strand::Forward),
            b'-' => Some(Strand::Reverse),
            _ => None,
        }
    }

    /// Whether this is the forward strand.
    #[inline]
    pub const fn is_forward(self) -> bool {
        matches!(self, Strand::Forward)
    }

    /// Whether this is the reverse strand.
    #[inline]
    pub const fn is_reverse(self) -> bool {
        matches!(self, Strand::Reverse)
    }
}

impl fmt::Display for Strand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Strand::Forward => "+",
            Strand::Reverse => "-",
        })
    }
}
