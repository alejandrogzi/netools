# netools

A Rust library and CLI suite for reading, writing, validating, indexing,
transforming, and analysing UCSC `.net` files.

`netools` prioritises correct, panic-free parsing; low memory use; zero-copy
access to textual fields; efficient hierarchical traversal; parallel and
streaming processing; and deterministic output.

## NET format in one paragraph

A NET file describes, for one *reference* genome, how a *query* genome aligns to
it, as a hierarchy of alignment `fill` records and the `gap` records between
them. Each reference sequence is a **section** beginning with a header:

```text
net chr1 248956422
 fill 10000 50000 chr2 + 30000 48000 id 1 score 300000 ali 45000 type top
  gap 20000 5000 chr2 + 40000 4000
   fill 21000 3000 chr3 - 100 2900 id 2 score 5000 ali 2800 type syn
```

Each record has seven fixed fields — `class tStart tSize qName qStrand qStart
qSize` — followed by optional `key value` pairs. Leading-space indentation is
semantic: each child is indented one space deeper than its parent.

> **Nomenclature.** `netools` uses descriptive names rather than UCSC's terse
> keys, and "reference" in place of UCSC's "target". The on-disk keys are
> unchanged; every attribute documents its UCSC equivalent (e.g. `AlignmentScore`
> ↔ `score`, `AlignedBases` ↔ `ali`, `ReferenceUnsequenced` ↔ `tN`). See the
> `KnownAttr` docs for the full table.

## Installation

This crate is not published. Add it by path or git dependency:

```toml
[dependencies]
netools = { path = "path/to/netools" }
```

Build the CLI:

```sh
cargo build --release --features cli
```

## Features

| feature    | default | enables                                              |
|------------|:-------:|------------------------------------------------------|
| `mmap`     |   yes   | memory-mapped file reading (`memmap2`)               |
| `gzip`     |         | gzip input detection/decompression and output (`flate2`) |
| `parallel` |         | section-parallel parsing/writing (`rayon`)           |
| `index`    |         | section / chain-id / reference-interval indexes      |
| `write`    |         | canonical writer, `Display`, `to_net_string`         |
| `chainnet` |         | chain-driven NET construction (`chaintools`, `twobit`) |
| `serde`    |         | `serde` derives on small model types                 |
| `cli`      |         | the `netools` binary (implies `chainnet parallel write index gzip`) |

Core sequential parsing and streaming depend only on `std`.

## Basic reading

```rust,no_run
use netools::{Net, Reader};

fn main() -> netools::Result<()> {
    let reader = Reader::<Net>::from_path("example.net")?;
    println!("reference nets: {}", reader.len());

    for net in reader.nets() {
        println!(
            "Net {}: reference size {}, records {}",
            net.reference_name().to_string_lossy(),
            net.reference_size(),
            net.len(),
        );
    }
    Ok(())
}
```

## Traversal

The hierarchy is a contiguous preorder arena (no boxed nodes). Views are
lightweight handles:

```rust
# use netools::{Net, Reader};
# fn f(reader: &Reader<Net>) {
for net in reader.nets() {
    for node in net.preorder() {
        println!(
            "{}{} {}:{}-{} -> {}:{}-{}",
            " ".repeat(node.depth() as usize),
            node.kind(),
            net.reference_name().to_string_lossy(),
            node.reference_start(),
            node.reference_end(),
            node.query_name().to_string_lossy(),
            node.query_start(),
            node.query_end(),
        );
    }
}
# }
```

`NodeRef` also exposes `parent`, `first_child`, `next_sibling`,
`previous_sibling`, `children`, `ancestors`, `descendants`, and `subtree`;
`NetRef` adds `roots`, `fills`, `gaps`, `nodes_at_depth`, and `max_depth`. A
non-recursive `NetVisitor` / `events()` API walks enter/leave events without
risking stack overflow on deep input.

## Parallel parsing

```rust,ignore
use netools::{Net, Reader};
use rayon::prelude::*;

let reader = Reader::<Net>::from_path_parallel("example.net")?;
reader.par_nets().for_each(|net| {
    println!("{} contains {} records", net.reference_name().to_string_lossy(), net.len());
});
```

Sections are parsed independently and reassembled deterministically:
`from_path` and `from_path_parallel` yield identical structures and
byte-identical canonical output.

## Streaming

Streaming owns one section at a time, so memory stays proportional to the
largest section rather than the whole file:

```rust,ignore
use netools::StreamingReader;

let mut reader = StreamingReader::from_path("large.net")?;
while let Some(net) = reader.next_net()? {
    println!("{}", net.reference_name());
}
```

A lower-level `next_event()` yields flat `NetStart` / `Record` / `NetEnd`
events.

## Writing

```rust,ignore
use netools::Writer;

let mut writer = Writer::from_path("output.net")?;
for net in reader.nets() {
    writer.write_net(net)?;
}
writer.finish()?;
```

The writer emits one leading space for roots (one more per depth), documented
attributes in canonical order, then preserved unknown attributes. Output is
deterministic and re-parses to the same tree.

## Indexing (`index` feature)

* `NetIndex` — section headers and byte ranges, scanned without parsing records.
* `ChainIdIndex` — fill occurrences by chain id, in reference/preorder order.
* `ReferenceIntervalIndex` — per-section sorted intervals with overlap queries.

## Validation

`reader.validate(mode)` / `net.validate(mode)` return a `ValidationReport`
(never printing to stderr). Modes: `Syntax`, `Compatible` (structural anomalies
as warnings), `Strict` (as errors). Checks include containment, sibling
ordering/overlap, fill/gap alternation, id rules, and reference bounds.

## chainCleaner-oriented API

`netools` exposes the primitives a future `chainCleaner` reimplementation needs,
without any chain-scoring baked in:

```rust,ignore
for net in reader.nets() {
    for ctx in net.nested_fill_gap_contexts() {
        println!(
            "chain {} fills {}-{} inside chain {} gap {}-{}",
            ctx.fill_chain_id, ctx.fill_range.start, ctx.fill_range.end,
            ctx.parent_chain_id, ctx.parent_gap_range.start, ctx.parent_gap_range.end,
        );
    }
}
```

Also: `fill_contexts`, `chain_occurrences`, `adjacent_chain_occurrences`,
`used_chain_ids`, `uninterrupted_reference_spans`, and an
`alignment_span_index()` whose `any_overlap_where(range, rank_fn)` keeps chain
ranking in the caller's hands.

## Constructing NETs from chains (`chainnet` feature)

`ChainNetBuilder` constructs reference and query NETs directly from
score-descending chains. It preserves equal-score input order, handles
query-minus projection in forward query coordinates, and parallelises only
across independent chromosomes. The cleaner-compatible preset fuses raw NET
construction with the exact non-nested `minScore1=3000` transformation:

```rust,ignore
use chaintools::{Chain, Reader};
use netools::chainnet::{ChainNetBuilder, ChainNetOptions, SizeSource};

let chains = Reader::<Chain>::from_path("input.chain")?;
let generated = ChainNetBuilder::new(ChainNetOptions::chaincleaner_compatible())
    .reference_sizes(SizeSource::TwoBit("reference.2bit".into()))
    .query_sizes(SizeSource::TwoBit("query.2bit".into()))
    .build(&chains)?;

for net in generated.reference_nets() {
    consume(net.as_ref());
}
```

This path constructs only the reference side, performs score filtering after
the complete hierarchy exists, promotes valid descendants of rejected fills,
and does not serialize or reparse an intermediate raw NET.


## CLI

```text
netools <net|validate|view|stats|split|sort|filter|merge> [OPTIONS]
```

Global flags: `--threads`, `--log-level`, `--parse-mode`, `--gzip`/`--no-gzip`.
`-` denotes stdin/stdout where streaming applies. Examples:

```sh
netools stats input.net
netools validate --mode strict input.net
netools view --reference chr1 --max-depth 3 input.net
netools filter --kind fill --min-score 3000 --type syn,inv --tree-policy prune in.net -o out.net
netools sort --nets natural --records reference in.net -o out.net
netools split --net input.net --out-dir split --manifest split/manifest.tsv
netools merge --nets a.net,b.net --duplicates error -o merged.net
netools merge --file nets.list --duplicates error -o merged.net
netools net --chain input.chain \
  --reference-sizes reference.chrom.sizes \
  --query-sizes query.chrom.sizes \
  --reference-net reference.net \
  --query-net query.net
netools net --chain input.chain \
  --reference-2bit reference.2bit \
  --query-2bit query.2bit \
  --reference-net cleaner.net \
  --preset ccr
```

Tree-aware filter policies are explicit: `prune` (drop failing subtrees,
UCSC-compatible default), `retain-ancestors` (keep matching paths), and
`promote` (reparent survivors).

## Performance & gzip limitations

Parsing is `O(bytes + nodes + attributes)` with a handful of arena allocations
and no per-token `String`s. Coordinates are 32-bit (`u32`), so a single input is
limited to 4 GiB; chain ids are 64-bit. With ordinary gzip, decompression is not
random-access and section-level parallelism only applies after decompression;
`mmap` maps the *compressed* bytes, which must still be inflated before parsing.

## License

GPL-3.0. See [LICENSE](LICENSE).

## Contributing & tests

```sh
cargo test --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --check
```
