// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the GNU General Public License v3.0.

//! Core data model: identifiers, coordinate types, records, and attributes.
//!
//! The model is deliberately storage-agnostic. Arena nodes reference textual
//! data through internal [`Span`](range::Span) handles that are resolved
//! against a shared byte buffer owned by the reader, so the model never embeds
//! reference-counted storage per record.

pub mod attributes;
pub mod ids;
pub mod net;
pub mod node;
pub mod owned;
pub mod range;
pub mod strand;
pub mod validation;

pub use attributes::{AttributeMask, AttributeView, KnownAttr, NetType, NetTypeKind};
pub use ids::{ChainId, Coord, NetId, NodeId};
pub use net::Net;
pub use node::NodeKind;
pub use owned::OwnedNet;
pub use range::NetRange;
pub use strand::Strand;
pub use validation::{Severity, ValidationCode, ValidationIssue, ValidationMode, ValidationReport};

pub(crate) use net::Section;
pub(crate) use range::Span;
