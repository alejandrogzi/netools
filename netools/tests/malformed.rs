// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the GNU General Public License v3.0.

//! Malformed-input sweep: nothing panics, and the right cases are rejected.

use netools::{NetErrorKind, ParseMode, Reader};

fn parse(text: &[u8]) -> netools::Result<Reader> {
    Reader::from_owned_bytes(text.to_vec())
}

fn parse_mode(text: &[u8], mode: ParseMode) -> netools::Result<Reader> {
    Reader::options()
        .parse_mode(mode)
        .from_owned_bytes(text.to_vec())
}

#[test]
fn rejected_inputs_error_without_panicking() {
    let cases: &[(&[u8], NetErrorKind)] = &[
        (b"", NetErrorKind::EmptyInput),
        (b"   \n\n", NetErrorKind::EmptyInput),
        (
            b" fill 0 10 q + 0 10 id 1\n",
            NetErrorKind::MissingNetHeader,
        ),
        (b"net\n", NetErrorKind::InvalidNetHeader),
        (b"net chrA\n", NetErrorKind::InvalidNetHeader),
        (b"net chrA notanumber\n", NetErrorKind::InvalidNetHeader),
        (
            b"net chrA 100\n fill 0 10 q +\n",
            NetErrorKind::TooFewFields,
        ),
        (
            b"net chrA 100\n fill 0 10 q + 0 10 id\n",
            NetErrorKind::OddAttributeCount,
        ),
        (
            b"net chrA 100\n fill 0 10 q + 0 10 id abc\n",
            NetErrorKind::InvalidInteger,
        ),
        (
            b"net chrA 100\n fill 0 10 q + 0 10 id 1 score notafloat\n",
            NetErrorKind::InvalidFloat,
        ),
        (
            b"net chrA 100\n fill 0 10 q + 0 10 id 1 score nan\n",
            NetErrorKind::InvalidFloat,
        ),
        (
            b"net chrA 100\n fill 0 10 q ? 0 10 id 1\n",
            NetErrorKind::InvalidStrand,
        ),
        (
            b"net chrA 100\n fill 99999999999 1 q + 0 10 id 1\n",
            NetErrorKind::NumericOverflow,
        ),
        (
            b"net chrA 100\n fill 4294967295 1 q + 0 10 id 1\n",
            NetErrorKind::CoordinateOverflow,
        ),
        (
            b"net chrA 100\n\tfill 0 10 q + 0 10 id 1\n",
            NetErrorKind::TabIndentation,
        ),
        (b"gap 0 10 q + 0 10\n", NetErrorKind::MissingNetHeader),
    ];

    for (input, expected) in cases {
        let err = parse(input)
            .err()
            .unwrap_or_else(|| panic!("expected error for {:?}", String::from_utf8_lossy(input)));
        assert_eq!(
            err.kind(),
            *expected,
            "input {:?} gave {:?}",
            String::from_utf8_lossy(input),
            err.kind()
        );
    }
}

#[test]
fn indentation_jump_rejected_in_compatible() {
    // A dedent to an indentation width never used by this parent's children.
    let text = b"net chrA 100000\n fill 0 100 q + 0 100 id 1\n   gap 10 10 q + 10 10\n  fill 12 3 r + 0 3 id 2\n";
    let err = parse_mode(text, ParseMode::Compatible).unwrap_err();
    assert!(matches!(
        err.kind(),
        NetErrorKind::IndentationJump | NetErrorKind::InvalidIndentation
    ));
    // Permissive maps it to the next logical depth instead.
    assert!(parse_mode(text, ParseMode::Permissive).is_ok());
}

#[test]
fn tolerated_inputs_parse() {
    // CRLF, missing final newline, comments, blank lines, embedded NUL in a name.
    assert!(parse(b"net chrA 100\r\n fill 0 10 q + 0 10 id 1\r\n").is_ok());
    assert!(parse(b"net chrA 100\n fill 0 10 q + 0 10 id 1").is_ok());
    assert!(parse(b"# comment\nnet chrA 100\n\n fill 0 10 q + 0 10 id 1\n").is_ok());

    let with_nul = b"net chrA 100\n fill 0 10 q\x00x + 0 10 id 1\n";
    let reader = parse(with_nul).unwrap();
    assert_eq!(
        reader
            .net(0)
            .unwrap()
            .preorder()
            .next()
            .unwrap()
            .query_name_bytes(),
        b"q\x00x"
    );
}

#[test]
fn deep_nesting_does_not_overflow_stack() {
    // 5000 levels of alternating fill/gap; must parse and traverse iteratively.
    let mut text = String::from("net chrA 100000000\n");
    for depth in 0..5000 {
        let indent = " ".repeat(depth + 1);
        let kind = if depth % 2 == 0 { "fill" } else { "gap" };
        let id = if depth % 2 == 0 {
            format!(" id {}", depth + 1)
        } else {
            String::new()
        };
        text.push_str(&format!(
            "{indent}{kind} {depth} 5000 q + {depth} 5000{id}\n"
        ));
    }
    let reader = Reader::options()
        .parse_mode(ParseMode::Permissive)
        .max_depth(8192)
        .from_owned_bytes(text.into_bytes())
        .unwrap();
    let net = reader.net(0).unwrap();
    assert_eq!(net.len(), 5000);
    // Traversal (Drop of the deep arena included) must not recurse.
    assert_eq!(net.preorder().count(), 5000);
    assert_eq!(net.max_depth(), 4999);
}

#[cfg(feature = "gzip")]
#[test]
fn truncated_gzip_errors() {
    // Valid gzip magic then garbage: decompression must fail, not panic.
    let truncated = b"\x1f\x8b\x08\x00\x00\x00\x00\x00\x00\x03garbage";
    assert!(Reader::from_owned_bytes(truncated.to_vec()).is_err());
}
