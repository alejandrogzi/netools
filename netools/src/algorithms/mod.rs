// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the GNU General Public License v3.0.

//! Analyses and transformations over parsed nets.
//!
//! Chain-specific scoring deliberately lives outside `model` and `parser`; this
//! module provides only generic NET-derived contexts and transformations.

pub mod chain_context;
pub mod filter;
pub mod stats;

pub use chain_context::{
    AlignmentSpan, AlignmentSpanIndex, ChainOccurrence, FillContext, NestedFillGapContext,
};
pub use filter::{NetPredicate, TreeFilterPolicy};
pub use stats::Stats;
