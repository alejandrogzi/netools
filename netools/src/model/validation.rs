// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the GNU General Public License v3.0.

//! Structural validation, separate from parse validity.
//!
//! A successful parse guarantees syntactic well-formedness. Validation adds the
//! biological/topological checks (containment, ordering, alternation, id rules)
//! and returns them as structured issues rather than printing to stderr.

use crate::io::reader::NetRef;
use crate::model::attributes::KnownAttr;
use crate::model::ids::{NetId, NodeId};
use crate::model::node::NodeKind;

/// How strict validation should be.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ValidationMode {
    /// Only checks implied by a successful parse plus obvious anomalies.
    Syntax,
    /// Structural anomalies reported as warnings.
    #[default]
    Compatible,
    /// Structural anomalies reported as errors.
    Strict,
}

/// Severity of a validation issue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Severity {
    /// Informational.
    Info,
    /// A warning: unusual but not necessarily invalid.
    Warning,
    /// An error: violates the format's structural rules.
    Error,
}

/// Machine-readable classification of a validation issue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ValidationCode {
    /// A reference interval extends past the reference sequence length.
    ReferenceOutOfBounds,
    /// A child's reference interval is not contained in its parent's.
    ChildOutsideParent,
    /// Fill/gap records do not alternate by depth.
    FillGapAlternation,
    /// A fill record has no chain id.
    MissingFillId,
    /// A gap record carries a chain id.
    GapHasId,
    /// Sibling records are not sorted by reference start.
    SiblingOrder,
    /// Sibling reference intervals overlap.
    SiblingOverlap,
    /// A record has zero reference size.
    ZeroSizedRecord,
    /// A count-like attribute is negative.
    NegativeCount,
}

impl ValidationCode {
    /// A short, stable slug for this code.
    pub const fn as_str(self) -> &'static str {
        match self {
            ValidationCode::ReferenceOutOfBounds => "reference-out-of-bounds",
            ValidationCode::ChildOutsideParent => "child-outside-parent",
            ValidationCode::FillGapAlternation => "fill-gap-alternation",
            ValidationCode::MissingFillId => "missing-fill-id",
            ValidationCode::GapHasId => "gap-has-id",
            ValidationCode::SiblingOrder => "sibling-order",
            ValidationCode::SiblingOverlap => "sibling-overlap",
            ValidationCode::ZeroSizedRecord => "zero-sized-record",
            ValidationCode::NegativeCount => "negative-count",
        }
    }
}

/// A single validation finding.
#[derive(Debug, Clone)]
pub struct ValidationIssue {
    /// Severity of the finding.
    pub severity: Severity,
    /// Machine-readable classification.
    pub code: ValidationCode,
    /// The section the issue was found in.
    pub net: NetId,
    /// The record the issue concerns, if applicable.
    pub node: Option<NodeId>,
    /// Human-readable description.
    pub message: String,
}

/// The result of validating a reader or section.
#[derive(Debug, Clone, Default)]
pub struct ValidationReport {
    /// All findings, in discovery order.
    pub issues: Vec<ValidationIssue>,
}

impl ValidationReport {
    /// Whether no issues were found.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.issues.is_empty()
    }

    /// Number of issues at [`Severity::Error`].
    pub fn error_count(&self) -> usize {
        self.issues
            .iter()
            .filter(|i| i.severity == Severity::Error)
            .count()
    }

    /// Number of issues at [`Severity::Warning`].
    pub fn warning_count(&self) -> usize {
        self.issues
            .iter()
            .filter(|i| i.severity == Severity::Warning)
            .count()
    }

    /// Whether any issue is an error.
    #[inline]
    pub fn has_errors(&self) -> bool {
        self.error_count() > 0
    }

    /// The findings.
    #[inline]
    pub fn issues(&self) -> &[ValidationIssue] {
        &self.issues
    }

    fn push(
        &mut self,
        severity: Severity,
        code: ValidationCode,
        net: NetId,
        node: Option<NodeId>,
        message: String,
    ) {
        self.issues.push(ValidationIssue {
            severity,
            code,
            net,
            node,
            message,
        });
    }
}

/// Severity for structural checks: skipped in `Syntax`, warning in
/// `Compatible`, error in `Strict`.
fn structural(mode: ValidationMode) -> Option<Severity> {
    match mode {
        ValidationMode::Syntax => None,
        ValidationMode::Compatible => Some(Severity::Warning),
        ValidationMode::Strict => Some(Severity::Error),
    }
}

/// Severity for out-of-bounds checks: warning in `Syntax`, error otherwise.
fn bounds(mode: ValidationMode) -> Severity {
    match mode {
        ValidationMode::Syntax => Severity::Warning,
        _ => Severity::Error,
    }
}

/// Severity for zero-size checks: error in `Strict`, warning otherwise.
fn zero_size(mode: ValidationMode) -> Severity {
    match mode {
        ValidationMode::Strict => Severity::Error,
        _ => Severity::Warning,
    }
}

impl<'a> NetRef<'a> {
    /// Validate this section, appending findings to `report`.
    pub fn validate_into(&self, mode: ValidationMode, report: &mut ValidationReport) {
        let net_id = self.id();
        let reference_size = self.reference_size();

        // Sibling checks over roots.
        check_siblings(self.roots(), mode, net_id, report);

        for node in self.preorder() {
            let node_id = Some(node.id());
            let range = node.reference_range();

            if reference_size != 0 && node.reference_end() > reference_size {
                report.push(
                    bounds(mode),
                    ValidationCode::ReferenceOutOfBounds,
                    net_id,
                    node_id,
                    format!(
                        "reference end {} exceeds reference size {}",
                        node.reference_end(),
                        reference_size
                    ),
                );
            }

            if node.reference_size() == 0 {
                report.push(
                    zero_size(mode),
                    ValidationCode::ZeroSizedRecord,
                    net_id,
                    node_id,
                    "record has zero reference size".to_string(),
                );
            }

            if let Some(parent) = node.parent()
                && !parent.reference_range().contains_range(range)
                && let Some(sev) = structural(mode)
            {
                report.push(
                    sev,
                    ValidationCode::ChildOutsideParent,
                    net_id,
                    node_id,
                    format!(
                        "child reference {}..{} not contained in parent {}..{}",
                        range.start,
                        range.end,
                        parent.reference_start(),
                        parent.reference_end()
                    ),
                );
            }

            if let Some(sev) = structural(mode) {
                let expected = if node.depth() % 2 == 0 {
                    NodeKind::Fill
                } else {
                    NodeKind::Gap
                };
                if node.kind() != expected {
                    report.push(
                        sev,
                        ValidationCode::FillGapAlternation,
                        net_id,
                        node_id,
                        format!(
                            "record kind {} does not match depth {} parity",
                            node.kind(),
                            node.depth()
                        ),
                    );
                }

                let has_id = node.chain_id().is_some();
                if node.kind() == NodeKind::Fill && !has_id {
                    report.push(
                        sev,
                        ValidationCode::MissingFillId,
                        net_id,
                        node_id,
                        "fill record has no chain id".to_string(),
                    );
                }
                if node.kind() == NodeKind::Gap && has_id {
                    report.push(
                        sev,
                        ValidationCode::GapHasId,
                        net_id,
                        node_id,
                        "gap record carries a chain id".to_string(),
                    );
                }

                for attr in COUNT_ATTRS {
                    if let Some(v) = node.attributes().int(attr)
                        && v < 0
                    {
                        report.push(
                            sev,
                            ValidationCode::NegativeCount,
                            net_id,
                            node_id,
                            format!("attribute {} is negative ({})", attr.as_str(), v),
                        );
                    }
                }
            }

            if node.has_children() {
                check_siblings(node.children(), mode, net_id, report);
            }
        }
    }

    /// Validate this section and return a report.
    pub fn validate(&self, mode: ValidationMode) -> ValidationReport {
        let mut report = ValidationReport::default();
        self.validate_into(mode, &mut report);
        report
    }
}

/// Count-like attributes that must not be negative.
const COUNT_ATTRS: [KnownAttr; 10] = [
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

/// Check a sibling chain for reference ordering and overlap.
fn check_siblings<'a, I>(
    siblings: I,
    mode: ValidationMode,
    net_id: NetId,
    report: &mut ValidationReport,
) where
    I: Iterator<Item = crate::io::NodeRef<'a>>,
{
    let Some(sev) = structural(mode) else {
        return;
    };
    let mut prev: Option<crate::io::NodeRef<'a>> = None;
    for child in siblings {
        if let Some(p) = prev {
            let pr = p.reference_range();
            let cr = child.reference_range();
            if cr.start < pr.start {
                report.push(
                    sev,
                    ValidationCode::SiblingOrder,
                    net_id,
                    Some(child.id()),
                    format!(
                        "sibling reference start {} precedes previous {}",
                        cr.start, pr.start
                    ),
                );
            }
            if !pr.is_empty() && !cr.is_empty() && pr.overlaps(cr) {
                report.push(
                    sev,
                    ValidationCode::SiblingOverlap,
                    net_id,
                    Some(child.id()),
                    format!(
                        "sibling reference {}..{} overlaps previous {}..{}",
                        cr.start, cr.end, pr.start, pr.end
                    ),
                );
            }
        }
        prev = Some(child);
    }
}

impl crate::io::Reader<crate::model::Net> {
    /// Validate every section and return an aggregate report.
    pub fn validate(&self, mode: ValidationMode) -> ValidationReport {
        let mut report = ValidationReport::default();
        for net in self.nets() {
            net.validate_into(mode, &mut report);
        }
        report
    }
}
