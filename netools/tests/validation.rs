// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the GNU General Public License v3.0.

//! Structural validation tests.

use netools::{Reader, ValidationCode, ValidationMode};

const BASIC: &[u8] = include_bytes!("fixtures/basic.net");

#[test]
fn well_formed_fixture_has_no_errors() {
    let r = Reader::from_owned_bytes(BASIC.to_vec()).unwrap();
    let report = r.validate(ValidationMode::Strict);
    assert!(
        !report.has_errors(),
        "unexpected errors: {:?}",
        report.issues()
    );
}

#[test]
fn child_outside_parent_is_flagged() {
    // The nested fill 5000..6000 is not contained in its parent gap 100..200.
    let text = "\
net chrA 1000000
 fill 0 100000 chrB + 0 100000 id 1 score 1000
  gap 100 100 chrB + 100 100
   fill 5000 1000 chrC + 0 1000 id 2 score 10
";
    let r = Reader::from_owned_bytes(text.as_bytes().to_vec()).unwrap();
    let report = r.validate(ValidationMode::Strict);
    assert!(
        report
            .issues()
            .iter()
            .any(|i| i.code == ValidationCode::ChildOutsideParent),
        "issues: {:?}",
        report.issues()
    );
}

#[test]
fn reference_out_of_bounds_is_flagged() {
    let text = "net chrA 100\n fill 50 100 chrB + 0 100 id 1\n";
    let r = Reader::from_owned_bytes(text.as_bytes().to_vec()).unwrap();
    let report = r.validate(ValidationMode::Compatible);
    assert!(
        report
            .issues()
            .iter()
            .any(|i| i.code == ValidationCode::ReferenceOutOfBounds)
    );
}

#[test]
fn syntax_mode_ignores_structural_issues() {
    let text = "\
net chrA 1000000
 fill 0 100000 chrB + 0 100000 id 1
  gap 100 100 chrB + 100 100
   fill 5000 1000 chrC + 0 1000 id 2
";
    let r = Reader::from_owned_bytes(text.as_bytes().to_vec()).unwrap();
    let report = r.validate(ValidationMode::Syntax);
    assert!(
        !report
            .issues()
            .iter()
            .any(|i| i.code == ValidationCode::ChildOutsideParent)
    );
}

#[test]
fn gap_with_id_flagged_in_strict() {
    let text = "net chrA 1000\n fill 0 100 chrB + 0 100 id 1\n  gap 10 10 chrB + 10 10 id 9\n";
    let r = Reader::from_owned_bytes(text.as_bytes().to_vec()).unwrap();
    let report = r.validate(ValidationMode::Strict);
    assert!(
        report
            .issues()
            .iter()
            .any(|i| i.code == ValidationCode::GapHasId)
    );
}
