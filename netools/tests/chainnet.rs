// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the GNU General Public License v3.0.

#![cfg(feature = "chainnet")]

use chaintools::{Chain, Reader};
use netools::chainnet::{
    ChainNetBuilder, ChainNetError, ChainNetOptions, NetSide, NonNestedFilter,
    NonNestedFilterOptions, SequenceSizes, SizeSource, filter_non_nested, kent_display_score,
};
use netools::{NodeKind, OwnedNet, Strand};

fn sizes(entries: &[(&str, u32)]) -> SizeSource {
    SizeSource::Provided(
        SequenceSizes::new(
            entries
                .iter()
                .map(|&(name, size)| (name.as_bytes().to_vec(), size)),
        )
        .unwrap(),
    )
}

fn reader(text: &str) -> Reader<Chain> {
    Reader::<Chain>::from_owned_bytes(text.as_bytes().to_vec()).unwrap()
}

#[test]
fn constructs_both_sides_and_preserves_size_order() {
    let chains = reader(
        "chain 5000 chr1 1000 + 100 300 q1 1000 + 200 400 1\n\
         200\n",
    );
    let generated = ChainNetBuilder::new(ChainNetOptions::default())
        .reference_sizes(sizes(&[("empty", 50), ("chr1", 1000)]))
        .query_sizes(sizes(&[("q1", 1000)]))
        .build(&chains)
        .unwrap();

    assert_eq!(generated.reference_nets().len(), 1);
    assert_eq!(generated.query_nets().len(), 1);
    let reference = generated.reference_nets()[0].as_ref();
    assert_eq!(reference.reference_name_bytes(), b"chr1");
    let fill = reference.preorder().next().unwrap();
    assert_eq!(fill.kind(), NodeKind::Fill);
    assert_eq!((fill.reference_start(), fill.reference_size()), (100, 200));
    assert_eq!(fill.query_name_bytes(), b"q1");
    assert_eq!((fill.query_start(), fill.query_size()), (200, 200));
    assert_eq!(fill.attributes().alignment_score(), Some(5000.0));
    assert_eq!(fill.attributes().aligned_bases(), Some(200));

    let query = generated.query_nets()[0].as_ref();
    let fill = query.preorder().next().unwrap();
    assert_eq!(query.reference_name_bytes(), b"q1");
    assert_eq!((fill.reference_start(), fill.reference_size()), (200, 200));
    assert_eq!(fill.query_name_bytes(), b"chr1");
    assert_eq!((fill.query_start(), fill.query_size()), (100, 200));
}

#[test]
fn query_minus_projection_uses_forward_query_coordinates() {
    let chains = reader(
        "chain 5000 chr1 1000 + 100 300 q1 1000 - 200 400 7\n\
         200\n",
    );
    let generated = ChainNetBuilder::new(ChainNetOptions::default())
        .reference_sizes(sizes(&[("chr1", 1000)]))
        .query_sizes(sizes(&[("q1", 1000)]))
        .build(&chains)
        .unwrap();

    let reference_fill = generated.reference_nets()[0]
        .as_ref()
        .preorder()
        .next()
        .unwrap();
    assert_eq!(reference_fill.query_strand(), Strand::Reverse);
    assert_eq!(
        (reference_fill.query_start(), reference_fill.query_size()),
        (600, 200)
    );

    let query_fill = generated.query_nets()[0]
        .as_ref()
        .preorder()
        .next()
        .unwrap();
    assert_eq!(
        (query_fill.reference_start(), query_fill.reference_size()),
        (600, 200)
    );
    assert_eq!(
        (query_fill.query_start(), query_fill.query_size()),
        (100, 200)
    );
}

#[test]
fn query_minus_internal_gap_coordinates_match_each_net_side() {
    let chains = reader(
        "chain 5000 chr1 1000 + 100 400 q1 1000 - 200 500 7\n\
         100 100 100\n\
         100\n",
    );
    let generated = ChainNetBuilder::new(ChainNetOptions::default())
        .reference_sizes(sizes(&[("chr1", 1000)]))
        .query_sizes(sizes(&[("q1", 1000)]))
        .build(&chains)
        .unwrap();

    let reference: Vec<_> = generated.reference_nets()[0].as_ref().preorder().collect();
    assert_eq!(reference.len(), 2);
    assert_eq!(
        (reference[0].query_start(), reference[0].query_size()),
        (500, 300)
    );
    assert_eq!(
        (
            reference[1].reference_start(),
            reference[1].reference_size(),
            reference[1].query_start(),
            reference[1].query_size(),
        ),
        (200, 100, 600, 100)
    );

    let query: Vec<_> = generated.query_nets()[0].as_ref().preorder().collect();
    assert_eq!(query.len(), 2);
    assert_eq!(
        (
            query[1].reference_start(),
            query[1].reference_size(),
            query[1].query_start(),
            query[1].query_size(),
        ),
        (600, 100, 100, 300)
    );
}

#[test]
fn non_nested_filter_promotes_grandchild_and_drops_direct_gap() {
    let chains = reader(
        "chain 10000 chr1 1000 + 0 1000 qa 1000 + 0 200 1\n\
         100 800 0\n\
         100\n\
         \n\
         chain 5000 chr1 1000 + 0 1000 qb 1000 + 0 400 2\n\
         100 0 0\n\
         100 600 0\n\
         100 0 0\n\
         100\n\
         \n\
         chain 4000 chr1 1000 + 200 800 qc 1000 + 0 600 3\n\
         600\n",
    );
    let mut options = ChainNetOptions {
        min_score: 0.0,
        side: NetSide::ReferenceOnly,
        post_filter: Some(NonNestedFilter::MinScore { set1: 3000 }),
        capture_raw: true,
        ..ChainNetOptions::default()
    };
    options.threads = Some(1);
    let generated = ChainNetBuilder::new(options)
        .reference_sizes(sizes(&[("chr1", 1000)]))
        .query_sizes(sizes(&[("qa", 1000), ("qb", 1000), ("qc", 1000)]))
        .build(&chains)
        .unwrap();

    let raw: Vec<_> = generated.raw_reference_nets().unwrap()[0]
        .as_ref()
        .preorder()
        .map(|node| {
            (
                node.kind(),
                node.depth(),
                node.attributes().chain_id(),
                node.attributes().alignment_score(),
            )
        })
        .collect();
    assert_eq!(raw.len(), 5);
    assert_eq!(raw[2].2, Some(2));
    assert_eq!(raw[2].3, Some(2500.0));

    let filtered: Vec<_> = generated.reference_nets()[0]
        .as_ref()
        .preorder()
        .map(|node| (node.kind(), node.depth(), node.attributes().chain_id()))
        .collect();
    assert_eq!(
        filtered,
        vec![
            (NodeKind::Fill, 0, Some(1)),
            (NodeKind::Gap, 1, None),
            (NodeKind::Fill, 2, Some(3)),
        ]
    );
}

#[test]
fn public_non_nested_filter_uses_special_gap_semantics() {
    let raw = OwnedNet::parse_bytes(
        concat!(
            "net chr1 1000\n",
            " fill 0 1000 q + 0 1000 id 1 score 2500 ali 100\n",
            "  gap 100 800 q + 100 800\n",
            "   fill 200 600 q + 200 600 id 2 score 4000 ali 600\n",
            "    gap 300 10 q + 300 10\n",
        )
        .as_bytes()
        .to_vec(),
    )
    .unwrap();
    let filtered = filter_non_nested(&raw, &NonNestedFilterOptions { min_score1: 3000 })
        .unwrap()
        .unwrap();
    let records: Vec<_> = filtered
        .as_ref()
        .preorder()
        .map(|node| (node.kind(), node.depth(), node.attributes().chain_id()))
        .collect();
    assert_eq!(
        records,
        vec![(NodeKind::Fill, 0, Some(2)), (NodeKind::Gap, 1, None)]
    );
}

#[test]
fn rejects_score_order_increase_with_chain_context() {
    let chains = reader(
        "chain 100 chr1 1000 + 0 100 q1 1000 + 0 100 1\n\
         100\n\
         \n\
         chain 101 chr1 1000 + 100 200 q1 1000 + 100 200 2\n\
         100\n",
    );
    let error = ChainNetBuilder::new(ChainNetOptions {
        min_score: 0.0,
        side: NetSide::ReferenceOnly,
        ..ChainNetOptions::default()
    })
    .reference_sizes(sizes(&[("chr1", 1000)]))
    .query_sizes(sizes(&[("q1", 1000)]))
    .build(&chains)
    .unwrap_err();
    assert!(matches!(
        error,
        ChainNetError::UnsortedScores {
            chain_id: 2,
            score: 101,
            previous_score: 100,
        }
    ));
}

#[test]
fn kent_score_rounding_is_ties_to_even() {
    assert_eq!(kent_display_score(2999.6), 3000);
    assert_eq!(kent_display_score(2998.5), 2998);
    assert_eq!(kent_display_score(2999.5), 3000);
}

#[cfg(feature = "write")]
#[test]
fn mmap_metadata_is_preserved_raw_and_omitted_after_filtering() {
    let chains = reader(
        "#chain source\n\
         chain 5000 chr1 1000 + 100 300 q1 1000 + 200 400 1\n\
         200\n\
         \n\
         #trailing metadata\n",
    );
    let metadata: Vec<&[u8]> = chains.metadata_lines().collect();
    let common = ChainNetOptions {
        min_score: 0.0,
        side: NetSide::ReferenceOnly,
        ..ChainNetOptions::default()
    };

    let raw = ChainNetBuilder::new(common.clone())
        .reference_sizes(sizes(&[("chr1", 1000)]))
        .query_sizes(sizes(&[("q1", 1000)]))
        .metadata_lines(metadata.iter().copied())
        .build(&chains)
        .unwrap();
    let mut raw_bytes = Vec::new();
    raw.write_reference_to(&mut raw_bytes).unwrap();
    assert!(raw_bytes.starts_with(b"#chain source\n#trailing metadata\nnet chr1 1000\n"));

    let filtered = ChainNetBuilder::new(ChainNetOptions {
        post_filter: Some(NonNestedFilter::MinScore { set1: 3000 }),
        ..common
    })
    .reference_sizes(sizes(&[("chr1", 1000)]))
    .query_sizes(sizes(&[("q1", 1000)]))
    .metadata_lines(metadata)
    .build(&chains)
    .unwrap();
    let mut filtered_bytes = Vec::new();
    filtered.write_reference_to(&mut filtered_bytes).unwrap();
    assert!(filtered_bytes.starts_with(b"net chr1 1000\n"));
    assert!(!filtered_bytes.contains(&b'#'));
}

#[cfg(all(feature = "parallel", feature = "write"))]
#[test]
fn reference_only_and_parallel_thread_counts_are_byte_identical() {
    let chains = reader(
        "chain 5000 chr2 1000 + 10 110 q2 1000 + 20 120 1\n\
         100\n\
         \n\
         chain 4000 chr1 1000 + 30 130 q1 1000 + 40 140 2\n\
         100\n",
    );
    let reference_sizes = sizes(&[("chr1", 1000), ("chr2", 1000)]);
    let query_sizes = sizes(&[("q2", 1000), ("q1", 1000)]);

    let both = ChainNetBuilder::new(ChainNetOptions {
        side: NetSide::Both,
        threads: Some(1),
        ..ChainNetOptions::default()
    })
    .reference_sizes(reference_sizes.clone())
    .query_sizes(query_sizes.clone())
    .build(&chains)
    .unwrap();
    let reference_only = ChainNetBuilder::new(ChainNetOptions {
        side: NetSide::ReferenceOnly,
        threads: Some(2),
        ..ChainNetOptions::default()
    })
    .reference_sizes(reference_sizes)
    .query_sizes(query_sizes)
    .build(&chains)
    .unwrap();

    let mut both_bytes = Vec::new();
    both.write_reference_to(&mut both_bytes).unwrap();
    let mut reference_only_bytes = Vec::new();
    reference_only
        .write_reference_to(&mut reference_only_bytes)
        .unwrap();
    assert_eq!(reference_only_bytes, both_bytes);
    assert!(reference_only.query_nets().is_empty());
}
