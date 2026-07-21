// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the GNU General Public License v3.0.

//! Generative round-trip testing. Requires the `write` feature.
//!
//! Random-but-valid NET documents are generated deterministically, then checked
//! for the parse -> write -> parse fixed point and full structural equivalence.
#![cfg(feature = "write")]

use std::fmt::Write as _;

use netools::{NetRef, Reader, Writer};

/// A tiny deterministic PRNG (SplitMix-style), so failures reproduce exactly.
struct Rng(u64);
impl Rng {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next_u64() % n.max(1)
    }
}

fn gen_node(
    out: &mut String,
    rng: &mut Rng,
    depth: usize,
    ref_start: u32,
    ref_end: u32,
    max_depth: usize,
    chain: &mut u64,
) {
    let is_fill = depth % 2 == 0;
    let indent = depth + 1;
    let ref_size = ref_end - ref_start;
    let qname = format!("q{}", rng.below(5));
    let strand = if rng.below(2) == 0 { '+' } else { '-' };
    let qstart = rng.below(1_000_000) as u32;
    let qsize = 1 + rng.below(5000) as u32;

    for _ in 0..indent {
        out.push(' ');
    }
    let kind = if is_fill { "fill" } else { "gap" };
    let _ = write!(
        out,
        "{kind} {ref_start} {ref_size} {qname} {strand} {qstart} {qsize}"
    );
    if is_fill {
        *chain += 1;
        // Canonical order: id, score, ali, type, then any unknown attribute.
        let _ = write!(out, " id {}", *chain);
        if rng.below(2) == 0 {
            let _ = write!(out, " score {}", rng.below(500_000));
        }
        if rng.below(2) == 0 {
            let _ = write!(out, " ali {}", rng.below(ref_size as u64 + 1));
        }
        if rng.below(3) == 0 {
            let t = ["top", "syn", "inv", "nonSyn"][rng.below(4) as usize];
            let _ = write!(out, " type {t}");
        }
        if rng.below(4) == 0 {
            let _ = write!(out, " custom{} val{}", rng.below(3), rng.below(3));
        }
    }
    out.push('\n');

    if depth >= max_depth {
        return;
    }
    let k = rng.below(4) as u32; // 0..3 children
    if k == 0 || ref_size < k * 2 + 2 {
        return;
    }
    let chunk = ref_size / (k + 1);
    if chunk < 2 {
        return;
    }
    for i in 0..k {
        let cs = ref_start + i * chunk + 1;
        let ce = ref_start + (i + 1) * chunk - 1;
        if ce > cs {
            gen_node(out, rng, depth + 1, cs, ce, max_depth, chain);
        }
    }
}

fn generate(seed: u64) -> Vec<u8> {
    let mut rng = Rng(seed);
    let mut out = String::new();
    let sections = 1 + rng.below(4);
    for s in 0..sections {
        let size = 1_000_000 + rng.below(9_000_000) as u32;
        let _ = writeln!(out, "net chr{s} {size}");
        let mut chain = s * 1000;
        let roots = 1 + rng.below(3) as u32;
        let chunk = size / (roots + 1);
        for r in 0..roots {
            let cs = r * chunk + 1;
            let ce = (r + 1) * chunk - 1;
            if ce > cs {
                gen_node(&mut out, &mut rng, 0, cs, ce, 6, &mut chain);
            }
        }
    }
    out.into_bytes()
}

fn serialize(reader: &Reader) -> Vec<u8> {
    let mut writer = Writer::new(Vec::new());
    for net in reader.nets() {
        writer.write_net(net).unwrap();
    }
    writer.into_inner()
}

fn assert_structural_eq(a: NetRef<'_>, b: NetRef<'_>) {
    assert_eq!(a.reference_name_bytes(), b.reference_name_bytes());
    assert_eq!(a.reference_size(), b.reference_size());
    let an: Vec<_> = a.preorder().collect();
    let bn: Vec<_> = b.preorder().collect();
    assert_eq!(an.len(), bn.len());
    for (x, y) in an.iter().zip(&bn) {
        assert_eq!(x.kind(), y.kind());
        assert_eq!(x.depth(), y.depth());
        assert_eq!(x.reference_range(), y.reference_range());
        assert_eq!(x.query_range(), y.query_range());
        assert_eq!(x.query_strand(), y.query_strand());
        assert_eq!(x.query_name_bytes(), y.query_name_bytes());
        let (xa, ya) = (x.attributes(), y.attributes());
        assert_eq!(xa.mask(), ya.mask());
        assert_eq!(xa.chain_id(), ya.chain_id());
        assert_eq!(xa.alignment_score(), ya.alignment_score());
        assert_eq!(xa.aligned_bases(), ya.aligned_bases());
        assert_eq!(xa.net_type(), ya.net_type());
        let xe: Vec<_> = xa.extras().collect();
        let ye: Vec<_> = ya.extras().collect();
        assert_eq!(xe, ye);
    }
}

#[test]
fn generated_documents_roundtrip() {
    for seed in 0..200u64 {
        let doc = generate(seed.wrapping_mul(0x1234_5678).wrapping_add(1));
        let first = match Reader::from_owned_bytes(doc.clone()) {
            Ok(r) => r,
            Err(e) => panic!("seed {seed}: generated doc failed to parse: {e}"),
        };
        let written = serialize(&first);
        let second = Reader::from_owned_bytes(written.clone())
            .unwrap_or_else(|e| panic!("seed {seed}: re-parse failed: {e}"));

        assert_eq!(first.len(), second.len(), "seed {seed}");
        for (a, b) in first.nets().zip(second.nets()) {
            assert_structural_eq(a, b);
        }
        // Writer is a fixed point.
        assert_eq!(
            written,
            serialize(&second),
            "seed {seed}: not a fixed point"
        );
    }
}

#[cfg(feature = "parallel")]
#[test]
fn generated_documents_parallel_parity() {
    for seed in 0..100u64 {
        let doc = generate(seed.wrapping_mul(0x9E37).wrapping_add(7));
        let seq = Reader::options()
            .parallel(false)
            .from_owned_bytes(doc.clone())
            .unwrap();
        let par = Reader::options()
            .parallel(true)
            .from_owned_bytes(doc)
            .unwrap();
        assert_eq!(serialize(&seq), serialize(&par), "seed {seed}");
    }
}
