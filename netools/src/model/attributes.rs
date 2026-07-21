// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the GNU General Public License v3.0.

//! Optional NET record attributes.
//!
//! Known optional fields are stored in a compact sidecar arena so that the hot
//! coordinate/topology scan over [`NetNode`](crate::model::node::NetNode) stays
//! cache friendly. Presence is tracked with a bitmask rather than `Option<T>`
//! per field, which keeps a genuine zero distinct from an absent field.
//!
//! # Nomenclature
//!
//! Field names are descriptive rather than the terse UCSC keys, and "reference"
//! replaces UCSC's "target" throughout. The on-disk keys are unchanged; each
//! variant documents its UCSC equivalent. The mapping is:
//!
//! | descriptive name        | UCSC key |
//! |-------------------------|----------|
//! | `ChainId`               | `id`     |
//! | `AlignmentScore`        | `score`  |
//! | `AlignedBases`          | `ali`    |
//! | `QueryFar`              | `qFar`   |
//! | `QueryOver`             | `qOver`  |
//! | `QueryDup`              | `qDup`   |
//! | `Type`                  | `type`   |
//! | `ReferenceUnsequenced`  | `tN`     |
//! | `QueryUnsequenced`      | `qN`     |
//! | `ReferenceMasked`       | `tR`     |
//! | `QueryMasked`           | `qR`     |
//! | `ReferenceNewMasked`    | `tNewR`  |
//! | `QueryNewMasked`        | `qNewR`  |
//! | `ReferenceOldMasked`    | `tOldR`  |
//! | `QueryOldMasked`        | `qOldR`  |
//! | `ReferenceTandem`       | `tTrf`   |
//! | `QueryTandem`           | `qTrf`   |

use std::fmt;

use crate::model::ids::ChainId;
use crate::model::range::Span;

/// One documented optional attribute, in canonical write order.
///
/// Names are descriptive; the on-disk UCSC key for each is given in the doc and
/// returned by [`as_str`](KnownAttr::as_str). The numeric discriminant doubles
/// as the bit index inside [`AttributeMask`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum KnownAttr {
    /// Chain identifier of a fill (UCSC `id`).
    ChainId = 0,
    /// Alignment score, floating point (UCSC `score`).
    AlignmentScore,
    /// Number of bases in gap-free alignments (UCSC `ali`).
    AlignedBases,
    /// Distance to the nearest same-chain block on the query (UCSC `qFar`).
    QueryFar,
    /// Bases of query overlap with the parent (UCSC `qOver`).
    QueryOver,
    /// Bases in the query already used by higher-level fills (UCSC `qDup`).
    QueryDup,
    /// Top / syn / inv / nonSyn classification (UCSC `type`).
    Type,
    /// Unsequenced (N) bases on the reference (UCSC `tN`).
    ReferenceUnsequenced,
    /// Unsequenced (N) bases on the query (UCSC `qN`).
    QueryUnsequenced,
    /// Repeat-masked bases on the reference (UCSC `tR`).
    ReferenceMasked,
    /// Repeat-masked bases on the query (UCSC `qR`).
    QueryMasked,
    /// Lineage-specific repeat bases on the reference (UCSC `tNewR`).
    ReferenceNewMasked,
    /// Lineage-specific repeat bases on the query (UCSC `qNewR`).
    QueryNewMasked,
    /// Ancestral repeat bases on the reference (UCSC `tOldR`).
    ReferenceOldMasked,
    /// Ancestral repeat bases on the query (UCSC `qOldR`).
    QueryOldMasked,
    /// Tandem-repeat-finder bases on the reference (UCSC `tTrf`).
    ReferenceTandem,
    /// Tandem-repeat-finder bases on the query (UCSC `qTrf`).
    QueryTandem,
}

impl KnownAttr {
    /// Number of documented optional attributes.
    #[allow(dead_code)]
    pub(crate) const COUNT: usize = 17;

    /// All documented attributes in canonical write order.
    ///
    /// Consumed by the canonical writer; unused when the `write` feature is off.
    #[allow(dead_code)]
    pub(crate) const ALL: [KnownAttr; Self::COUNT] = [
        KnownAttr::ChainId,
        KnownAttr::AlignmentScore,
        KnownAttr::AlignedBases,
        KnownAttr::QueryFar,
        KnownAttr::QueryOver,
        KnownAttr::QueryDup,
        KnownAttr::Type,
        KnownAttr::ReferenceUnsequenced,
        KnownAttr::QueryUnsequenced,
        KnownAttr::ReferenceMasked,
        KnownAttr::QueryMasked,
        KnownAttr::ReferenceNewMasked,
        KnownAttr::QueryNewMasked,
        KnownAttr::ReferenceOldMasked,
        KnownAttr::QueryOldMasked,
        KnownAttr::ReferenceTandem,
        KnownAttr::QueryTandem,
    ];

    /// The on-disk UCSC key for this attribute.
    #[inline]
    pub const fn as_str(self) -> &'static str {
        match self {
            KnownAttr::ChainId => "id",
            KnownAttr::AlignmentScore => "score",
            KnownAttr::AlignedBases => "ali",
            KnownAttr::QueryFar => "qFar",
            KnownAttr::QueryOver => "qOver",
            KnownAttr::QueryDup => "qDup",
            KnownAttr::Type => "type",
            KnownAttr::ReferenceUnsequenced => "tN",
            KnownAttr::QueryUnsequenced => "qN",
            KnownAttr::ReferenceMasked => "tR",
            KnownAttr::QueryMasked => "qR",
            KnownAttr::ReferenceNewMasked => "tNewR",
            KnownAttr::QueryNewMasked => "qNewR",
            KnownAttr::ReferenceOldMasked => "tOldR",
            KnownAttr::QueryOldMasked => "qOldR",
            KnownAttr::ReferenceTandem => "tTrf",
            KnownAttr::QueryTandem => "qTrf",
        }
    }

    /// The on-disk UCSC key bytes for this attribute.
    #[inline]
    pub const fn as_bytes(self) -> &'static [u8] {
        self.as_str().as_bytes()
    }

    /// Resolve an on-disk UCSC key to a known attribute, if documented.
    #[inline]
    pub(crate) fn from_key(key: &[u8]) -> Option<KnownAttr> {
        Some(match key {
            b"id" => KnownAttr::ChainId,
            b"score" => KnownAttr::AlignmentScore,
            b"ali" => KnownAttr::AlignedBases,
            b"qFar" => KnownAttr::QueryFar,
            b"qOver" => KnownAttr::QueryOver,
            b"qDup" => KnownAttr::QueryDup,
            b"type" => KnownAttr::Type,
            b"tN" => KnownAttr::ReferenceUnsequenced,
            b"qN" => KnownAttr::QueryUnsequenced,
            b"tR" => KnownAttr::ReferenceMasked,
            b"qR" => KnownAttr::QueryMasked,
            b"tNewR" => KnownAttr::ReferenceNewMasked,
            b"qNewR" => KnownAttr::QueryNewMasked,
            b"tOldR" => KnownAttr::ReferenceOldMasked,
            b"qOldR" => KnownAttr::QueryOldMasked,
            b"tTrf" => KnownAttr::ReferenceTandem,
            b"qTrf" => KnownAttr::QueryTandem,
            _ => return None,
        })
    }

    #[inline]
    const fn bit(self) -> u32 {
        1u32 << (self as u32)
    }
}

/// Presence bitmask over the documented optional attributes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
#[repr(transparent)]
pub struct AttributeMask(pub(crate) u32);

impl AttributeMask {
    /// Empty mask.
    pub const EMPTY: AttributeMask = AttributeMask(0);

    /// Whether `attr` is present.
    #[inline]
    pub const fn has(self, attr: KnownAttr) -> bool {
        self.0 & attr.bit() != 0
    }

    /// Record `attr` as present.
    #[inline]
    pub(crate) fn insert(&mut self, attr: KnownAttr) {
        self.0 |= attr.bit();
    }

    /// Whether no documented attribute is present.
    #[inline]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Number of documented attributes present.
    #[inline]
    pub const fn count(self) -> u32 {
        self.0.count_ones()
    }
}

/// Recognised value of the `type` attribute.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NetTypeKind {
    /// `top`
    Top,
    /// `syn`
    Syn,
    /// `inv`
    Inv,
    /// `nonSyn`
    NonSyn,
}

impl NetTypeKind {
    /// Canonical text for this net type.
    #[inline]
    pub const fn as_str(self) -> &'static str {
        match self {
            NetTypeKind::Top => "top",
            NetTypeKind::Syn => "syn",
            NetTypeKind::Inv => "inv",
            NetTypeKind::NonSyn => "nonSyn",
        }
    }

    /// Parse a documented net type value.
    #[inline]
    pub(crate) fn from_bytes(b: &[u8]) -> Option<NetTypeKind> {
        Some(match b {
            b"top" => NetTypeKind::Top,
            b"syn" => NetTypeKind::Syn,
            b"inv" => NetTypeKind::Inv,
            b"nonSyn" => NetTypeKind::NonSyn,
            _ => return None,
        })
    }
}

/// A resolved `type` attribute value borrowed from backing storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NetType<'a> {
    /// `top`
    Top,
    /// `syn`
    Syn,
    /// `inv`
    Inv,
    /// `nonSyn`
    NonSyn,
    /// An undocumented type value, preserved verbatim.
    Other(&'a [u8]),
}

impl<'a> NetType<'a> {
    /// The type value as raw bytes.
    #[inline]
    pub fn as_bytes(&self) -> &'a [u8] {
        match self {
            NetType::Top => b"top",
            NetType::Syn => b"syn",
            NetType::Inv => b"inv",
            NetType::NonSyn => b"nonSyn",
            NetType::Other(b) => b,
        }
    }

    /// The type value as UTF-8, if valid.
    #[inline]
    pub fn as_str(&self) -> Option<&'a str> {
        std::str::from_utf8(self.as_bytes()).ok()
    }

    /// The documented kind, if this is a recognised type.
    #[inline]
    pub fn kind(&self) -> Option<NetTypeKind> {
        match self {
            NetType::Top => Some(NetTypeKind::Top),
            NetType::Syn => Some(NetTypeKind::Syn),
            NetType::Inv => Some(NetTypeKind::Inv),
            NetType::NonSyn => Some(NetTypeKind::NonSyn),
            NetType::Other(_) => None,
        }
    }
}

impl fmt::Display for NetType<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&String::from_utf8_lossy(self.as_bytes()))
    }
}

/// Internal storage for the `type` attribute value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NetTypeRepr {
    Known(NetTypeKind),
    /// Undocumented value, held as a span into backing storage.
    Other(Span),
}

impl Default for NetTypeRepr {
    fn default() -> Self {
        NetTypeRepr::Known(NetTypeKind::Top)
    }
}

impl NetTypeRepr {
    #[inline]
    pub(crate) fn resolve<'a>(&self, bytes: &'a [u8]) -> NetType<'a> {
        match self {
            NetTypeRepr::Known(NetTypeKind::Top) => NetType::Top,
            NetTypeRepr::Known(NetTypeKind::Syn) => NetType::Syn,
            NetTypeRepr::Known(NetTypeKind::Inv) => NetType::Inv,
            NetTypeRepr::Known(NetTypeKind::NonSyn) => NetType::NonSyn,
            NetTypeRepr::Other(span) => NetType::Other(span.resolve(bytes)),
        }
    }
}

/// One preserved undocumented `key value` attribute pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ExtraAttribute {
    pub(crate) key: Span,
    pub(crate) value: Span,
}

/// Range of preserved undocumented attributes for one node, into the shared
/// extra-attribute arena.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct ExtraAttributeRange {
    pub(crate) start: u32,
    pub(crate) len: u16,
}

impl ExtraAttributeRange {
    #[inline]
    pub(crate) const fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[inline]
    pub(crate) fn as_slice<'a>(&self, extras: &'a [ExtraAttribute]) -> &'a [ExtraAttribute] {
        let start = self.start as usize;
        let end = start + self.len as usize;
        extras.get(start..end).unwrap_or(&[])
    }
}

/// Scalar sidecar block holding one node's documented optional attributes.
///
/// Field names are descriptive; see the module-level table for their UCSC
/// equivalents.
#[derive(Debug, Clone, Copy)]
pub(crate) struct NodeAttributes {
    pub(crate) present: AttributeMask,

    pub(crate) chain_id: ChainId,
    pub(crate) alignment_score: f64,
    pub(crate) aligned_bases: u32,

    pub(crate) query_far: i32,
    pub(crate) query_over: i32,
    pub(crate) query_dup: i32,

    pub(crate) net_type: NetTypeRepr,

    pub(crate) reference_unsequenced: i32,
    pub(crate) query_unsequenced: i32,
    pub(crate) reference_masked: i32,
    pub(crate) query_masked: i32,
    pub(crate) reference_new_masked: i32,
    pub(crate) query_new_masked: i32,
    pub(crate) reference_old_masked: i32,
    pub(crate) query_old_masked: i32,
    pub(crate) reference_tandem: i32,
    pub(crate) query_tandem: i32,

    pub(crate) extra: ExtraAttributeRange,
}

impl Default for NodeAttributes {
    fn default() -> Self {
        NodeAttributes {
            present: AttributeMask::EMPTY,
            chain_id: 0,
            alignment_score: 0.0,
            aligned_bases: 0,
            query_far: 0,
            query_over: 0,
            query_dup: 0,
            net_type: NetTypeRepr::default(),
            reference_unsequenced: 0,
            query_unsequenced: 0,
            reference_masked: 0,
            query_masked: 0,
            reference_new_masked: 0,
            query_new_masked: 0,
            reference_old_masked: 0,
            query_old_masked: 0,
            reference_tandem: 0,
            query_tandem: 0,
            extra: ExtraAttributeRange::default(),
        }
    }
}

impl NodeAttributes {
    /// Whether this block carries no documented attribute and no extras.
    #[inline]
    pub(crate) fn is_empty(&self) -> bool {
        self.present.is_empty() && self.extra.is_empty()
    }

    /// Store the value of a documented signed-integer attribute and mark it
    /// present.
    #[inline]
    pub(crate) fn set_i32(&mut self, attr: KnownAttr, value: i32) {
        self.present.insert(attr);
        match attr {
            KnownAttr::QueryFar => self.query_far = value,
            KnownAttr::QueryOver => self.query_over = value,
            KnownAttr::QueryDup => self.query_dup = value,
            KnownAttr::ReferenceUnsequenced => self.reference_unsequenced = value,
            KnownAttr::QueryUnsequenced => self.query_unsequenced = value,
            KnownAttr::ReferenceMasked => self.reference_masked = value,
            KnownAttr::QueryMasked => self.query_masked = value,
            KnownAttr::ReferenceNewMasked => self.reference_new_masked = value,
            KnownAttr::QueryNewMasked => self.query_new_masked = value,
            KnownAttr::ReferenceOldMasked => self.reference_old_masked = value,
            KnownAttr::QueryOldMasked => self.query_old_masked = value,
            KnownAttr::ReferenceTandem => self.reference_tandem = value,
            KnownAttr::QueryTandem => self.query_tandem = value,
            // Non-i32 attributes handled by dedicated setters.
            KnownAttr::ChainId
            | KnownAttr::AlignmentScore
            | KnownAttr::AlignedBases
            | KnownAttr::Type => {}
        }
    }

    /// Read a documented signed-integer attribute if present.
    #[inline]
    pub(crate) fn get_i32(&self, attr: KnownAttr) -> Option<i32> {
        if !self.present.has(attr) {
            return None;
        }
        Some(match attr {
            KnownAttr::QueryFar => self.query_far,
            KnownAttr::QueryOver => self.query_over,
            KnownAttr::QueryDup => self.query_dup,
            KnownAttr::ReferenceUnsequenced => self.reference_unsequenced,
            KnownAttr::QueryUnsequenced => self.query_unsequenced,
            KnownAttr::ReferenceMasked => self.reference_masked,
            KnownAttr::QueryMasked => self.query_masked,
            KnownAttr::ReferenceNewMasked => self.reference_new_masked,
            KnownAttr::QueryNewMasked => self.query_new_masked,
            KnownAttr::ReferenceOldMasked => self.reference_old_masked,
            KnownAttr::QueryOldMasked => self.query_old_masked,
            KnownAttr::ReferenceTandem => self.reference_tandem,
            KnownAttr::QueryTandem => self.query_tandem,
            _ => return None,
        })
    }
}

/// Borrowed, read-only view over one node's optional attributes.
///
/// The same view type serves both mmap/owned-buffer readers and the owned
/// [`OwnedNet`](crate::model::owned::OwnedNet) representation: spans are always
/// interpreted against the `bytes` the view was constructed with.
#[derive(Clone, Copy)]
pub struct AttributeView<'a> {
    scalar: Option<&'a NodeAttributes>,
    extras: &'a [ExtraAttribute],
    bytes: &'a [u8],
}

impl<'a> AttributeView<'a> {
    #[inline]
    pub(crate) fn new(
        scalar: Option<&'a NodeAttributes>,
        extras: &'a [ExtraAttribute],
        bytes: &'a [u8],
    ) -> Self {
        AttributeView {
            scalar,
            extras,
            bytes,
        }
    }

    /// An empty view carrying no attributes.
    #[inline]
    pub(crate) fn empty(bytes: &'a [u8]) -> Self {
        AttributeView {
            scalar: None,
            extras: &[],
            bytes,
        }
    }

    /// The presence mask for documented attributes.
    #[inline]
    pub fn mask(&self) -> AttributeMask {
        self.scalar.map_or(AttributeMask::EMPTY, |s| s.present)
    }

    /// Whether a documented attribute is present.
    #[inline]
    pub fn has(&self, attr: KnownAttr) -> bool {
        self.mask().has(attr)
    }

    /// Whether the view carries no documented attribute and no extras.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.scalar.is_none_or(|s| s.is_empty())
    }

    /// Chain identifier (UCSC `id`).
    #[inline]
    pub fn chain_id(&self) -> Option<ChainId> {
        let s = self.scalar?;
        s.present.has(KnownAttr::ChainId).then_some(s.chain_id)
    }

    /// Alignment score (UCSC `score`).
    #[inline]
    pub fn alignment_score(&self) -> Option<f64> {
        let s = self.scalar?;
        s.present
            .has(KnownAttr::AlignmentScore)
            .then_some(s.alignment_score)
    }

    /// Aligned bases (UCSC `ali`).
    #[inline]
    pub fn aligned_bases(&self) -> Option<u32> {
        let s = self.scalar?;
        s.present
            .has(KnownAttr::AlignedBases)
            .then_some(s.aligned_bases)
    }

    /// The resolved `type` value.
    #[inline]
    pub fn net_type(&self) -> Option<NetType<'a>> {
        let s = self.scalar?;
        s.present
            .has(KnownAttr::Type)
            .then(|| s.net_type.resolve(self.bytes))
    }

    /// A documented signed-integer attribute (`QueryFar`, `ReferenceMasked`, ...).
    #[inline]
    pub fn int(&self, attr: KnownAttr) -> Option<i32> {
        self.scalar?.get_i32(attr)
    }

    /// Distance to the nearest same-chain query block (UCSC `qFar`).
    #[inline]
    pub fn query_far(&self) -> Option<i32> {
        self.int(KnownAttr::QueryFar)
    }

    /// Query overlap bases (UCSC `qOver`).
    #[inline]
    pub fn query_over(&self) -> Option<i32> {
        self.int(KnownAttr::QueryOver)
    }

    /// Query duplicated bases (UCSC `qDup`).
    #[inline]
    pub fn query_dup(&self) -> Option<i32> {
        self.int(KnownAttr::QueryDup)
    }

    /// Number of preserved undocumented attributes.
    #[inline]
    pub fn extra_count(&self) -> usize {
        self.extras.len()
    }

    /// Iterate preserved undocumented `(key, value)` pairs in input order.
    pub fn extras(&self) -> impl Iterator<Item = (&'a [u8], &'a [u8])> + '_ {
        let bytes = self.bytes;
        self.extras
            .iter()
            .map(move |e| (e.key.resolve(bytes), e.value.resolve(bytes)))
    }
}

impl fmt::Debug for AttributeView<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut dbg = f.debug_struct("AttributeView");
        dbg.field("mask", &self.mask());
        if let Some(id) = self.chain_id() {
            dbg.field("chain_id", &id);
        }
        if let Some(score) = self.alignment_score() {
            dbg.field("alignment_score", &score);
        }
        dbg.field("extras", &self.extra_count());
        dbg.finish()
    }
}
