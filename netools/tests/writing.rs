// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the GNU General Public License v3.0.

//! Canonical writer tests. Requires the `write` feature.
#![cfg(feature = "write")]

use netools::{Reader, Writer};

const BASIC: &[u8] = include_bytes!("fixtures/basic.net");

/// Serialise every section of a reader into a byte buffer.
fn serialize(reader: &Reader) -> Vec<u8> {
    let mut writer = Writer::new(Vec::new());
    for net in reader.nets() {
        writer.write_net(net).unwrap();
    }
    writer.into_inner()
}

#[test]
fn canonical_output_matches_canonical_fixture() {
    // basic.net is already in canonical form, so a round trip is byte-identical.
    let reader = Reader::from_owned_bytes(BASIC.to_vec()).unwrap();
    let output = serialize(&reader);
    assert_eq!(
        output,
        BASIC,
        "canonical output differs:\n--- got ---\n{}\n--- want ---\n{}",
        String::from_utf8_lossy(&output),
        String::from_utf8_lossy(BASIC),
    );
}

#[test]
fn reorders_attributes_into_canonical_order() {
    // Input has `score` before `id` and an unknown field between known ones.
    let input = "net chrA 1000\n fill 0 100 chrB + 0 100 score 50 id 7 weird val ali 90\n";
    let reader = Reader::from_owned_bytes(input.as_bytes().to_vec()).unwrap();
    let output = String::from_utf8(serialize(&reader)).unwrap();
    assert_eq!(
        output,
        "net chrA 1000\n fill 0 100 chrB + 0 100 id 7 score 50 ali 90 weird val\n"
    );
}

#[test]
fn display_and_to_net_string_agree() {
    let reader = Reader::from_owned_bytes(BASIC.to_vec()).unwrap();
    let net = reader.net(0).unwrap();
    let via_display = format!("{net}");
    let via_method = net.to_net_string();
    assert_eq!(via_display, via_method);
    assert!(via_display.starts_with("net chr1 248956422\n"));
}

#[test]
fn write_net_like_matches_fast_path() {
    use netools::NetLike;
    let reader = Reader::from_owned_bytes(BASIC.to_vec()).unwrap();
    let net = reader.net(0).unwrap();

    let mut fast = Writer::new(Vec::new());
    fast.write_net(net).unwrap();

    let mut generic = Writer::new(Vec::new());
    generic.write_net_like(&net).unwrap();

    // Suppress unused-trait-import warning while asserting equality.
    let _ = <netools::NetRef as NetLike>::reference_size(&net);
    assert_eq!(fast.into_inner(), generic.into_inner());
}
