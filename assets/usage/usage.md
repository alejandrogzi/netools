# netools — library usage

A guide to the `netools` Rust library API. For the command-line tools see
[`../tools/`](../tools/).

## Contents

1. [The NET format](#the-net-format)
2. [Reading](#reading)
3. [Zero-copy text](#zero-copy-text)
4. [Traversal](#traversal)
5. [Attributes](#attributes)
6. [Validation](#validation)
7. [Writing](#writing)
8. [Owned sections & transformations](#owned-sections--transformations)
9. [Streaming](#streaming)
10. [Parallelism](#parallelism)
11. [Indexes](#indexes)
12. [Chain-analysis API](#chain-analysis-api)
13. [Errors](#errors)
14. [Feature flags](#feature-flags)

---

## The NET format

A `.net` file describes, for one **reference** genome, how a **query** genome
aligns to it. It is a hierarchy of alignment `fill` records and the `gap`
records between them, grouped into one **section** per reference sequence.

A section starts with a header `net <name> <size>`. Each record has seven fixed
fields followed by optional `key value` pairs, and leading-space indentation is
semantic — each child is indented one space deeper than its parent:

```text
net chr1 248956422
 fill 10000 50000 chr2 + 30000 48000 id 1 score 300000 ali 45000 type top
  gap 20000 5000 chr2 + 40000 4000
   fill 21000 3000 chr3 - 100 2900 id 2 score 5000 ali 2800 type syn
  gap 30000 2000 chr2 + 50000 1500
 fill 90000 10000 chr4 + 5000 9500 id 3 score 12000 ali 9000 type nonSyn
```

Fixed fields, in order:

```text
class  refStart  refSize  queryName  queryStrand  queryStart  querySize
```

* `class` — `fill` (aligned) or `gap` (unaligned).
* `refStart`, `refSize` — reference interval `[refStart, refStart+refSize)`.
* `queryName`, `queryStrand` (`+`/`-`), `queryStart`, `querySize` — the query side.

Reading the example: chromosome `chr1` (248,956,422 bp) has a top-level fill
covering reference `10000–60000` aligned to `chr2:30000–78000`; inside it, a gap
at `20000–25000` contains a deeper, inverted-strand fill to `chr3`; a second gap
follows; and a second independent top-level fill covers `90000–100000`.

> **Naming.** `netools` uses descriptive names rather than UCSC's terse keys, and
> "reference" in place of UCSC "target". The on-disk keys are unchanged — see the
> [attribute table](#attributes).

---

## Reading

One `Reader<Net>` owns a whole file: a single shared byte buffer and a
contiguous preorder arena of records shared by all sections.

```rust
use netools::{Net, Reader};

let reader = Reader::<Net>::from_path("genome.net")?;   // mmap or gzip-decompress
println!("{} sections, {} records", reader.len(), reader.node_count());
for net in reader.nets() {
    println!("{}\t{}", net.reference_name().to_string_lossy(), net.reference_size());
}
```

Constructors:

| Constructor | Notes |
|---|---|
| `Reader::from_path(p)` | mmap plain files (feature `mmap`); gzip auto-detected and decompressed. |
| `Reader::from_mmap(p)` | Explicit mmap (feature `mmap`). |
| `Reader::from_path_parallel(p)` | Section-parallel parsing (feature `parallel`). |
| `Reader::from_owned_bytes(Vec<u8>)` | Parse an owned buffer. |
| `Reader::from_bytes(Arc<[u8]>)` | Parse a shared buffer without copying. |
| `Reader::from_reader(r)` | Read all of `r` into memory, then parse. |

Options via a builder:

```rust
use netools::{Net, ParseMode, Reader};

let reader = Reader::<Net>::options()
    .parse_mode(ParseMode::Compatible)   // Permissive | Compatible | Strict
    .parallel(true)
    .max_depth(4096)
    .from_path("genome.net.gz")?;
```

`ParseMode::Permissive` accepts the widest range (mapping indentation jumps to
the next logical depth); `Compatible` (default) enforces consistent indentation
and rejects tabs; `Strict` additionally enforces one-space-per-level, fill/gap
alternation, and id rules while parsing.

---

## Zero-copy text

Textual fields (names, unknown attribute keys/values, unrecognised `type`
strings) are returned as `ByteSlice`, a cheap view that keeps the backing buffer
alive. Parsing never requires valid UTF-8.

```rust
# use netools::NodeRef;
# fn f(node: NodeRef<'_>) {
let name = node.query_name();          // ByteSlice
let bytes: &[u8] = name.as_bytes();
let s: Option<&str> = name.as_str();   // None if not UTF-8
let lossy = name.to_string_lossy();
# }
```

`NetRef::reference_name_bytes()` and `NodeRef::query_name_bytes()` return the
borrowed `&[u8]` directly when you don't need an owned handle.

---

## Traversal

The arena is stored in preorder, so many traversals are contiguous scans. All
traversals are non-recursive and allocation-light.

```rust
# use netools::{Net, Reader};
# fn f(reader: &Reader<Net>) {
let net = reader.net(0).unwrap();

for node in net.preorder() {           // whole section, preorder
    println!("{}{} {}..{}", " ".repeat(node.depth() as usize),
             node.kind(), node.reference_start(), node.reference_end());
}

for root in net.roots() { /* top-level fills */ }
for fill in net.fills() { /* only fills */ }
for gap  in net.gaps()  { /* only gaps  */ }
for n in net.nodes_at_depth(2) { /* records at depth 2 */ }
let deepest = net.max_depth();
# }
```

From a `NodeRef`:

```rust
# use netools::NodeRef;
# fn f(node: NodeRef<'_>) {
node.parent(); node.first_child(); node.next_sibling(); node.previous_sibling();
for child in node.children() {}
for anc in node.ancestors() {}         // nearest-first, up to the root
for d in node.descendants() {}         // subtree excluding self
for s in node.subtree() {}             // subtree including self
node.root();                            // top of this record's path
# }
```

A non-recursive visitor and a lazy event iterator are available for
enter/leave processing (safe on pathologically deep trees):

```rust
use std::ops::ControlFlow;
use netools::{NetVisitor, NodeRef, TraversalEvent};

struct Depths(u16);
impl NetVisitor for Depths {
    fn enter(&mut self, n: NodeRef<'_>) -> ControlFlow<()> {
        self.0 = self.0.max(n.depth());
        ControlFlow::Continue(())
    }
}
# fn f(net: netools::NetRef<'_>) {
let mut v = Depths(0);
net.walk(&mut v);

for event in net.events() {
    match event {
        TraversalEvent::Enter(n) => { /* ... */ }
        TraversalEvent::Leave(n) => { /* ... */ }
    }
}
# }
```

---

## Attributes

Optional fields are exposed through `AttributeView` (from `node.attributes()`).
Presence is tracked with a bitmask, so a genuine zero is distinct from an absent
field. Names are descriptive; the on-disk UCSC key is preserved on read/write.

| `KnownAttr` variant | UCSC key | accessor | type |
|---|---|---|---|
| `ChainId` | `id` | `chain_id()` | `u64` |
| `AlignmentScore` | `score` | `alignment_score()` | `f64` |
| `AlignedBases` | `ali` | `aligned_bases()` | `u32` |
| `QueryFar` | `qFar` | `query_far()` | `i32` |
| `QueryOver` | `qOver` | `query_over()` | `i32` |
| `QueryDup` | `qDup` | `query_dup()` | `i32` |
| `Type` | `type` | `net_type()` | `NetType` |
| `ReferenceUnsequenced` | `tN` | `int(..)` | `i32` |
| `QueryUnsequenced` | `qN` | `int(..)` | `i32` |
| `ReferenceMasked` | `tR` | `int(..)` | `i32` |
| `QueryMasked` | `qR` | `int(..)` | `i32` |
| `ReferenceNewMasked` | `tNewR` | `int(..)` | `i32` |
| `QueryNewMasked` | `qNewR` | `int(..)` | `i32` |
| `ReferenceOldMasked` | `tOldR` | `int(..)` | `i32` |
| `QueryOldMasked` | `qOldR` | `int(..)` | `i32` |
| `ReferenceTandem` | `tTrf` | `int(..)` | `i32` |
| `QueryTandem` | `qTrf` | `int(..)` | `i32` |

```rust
use netools::{KnownAttr, NetType, NodeRef};

# fn f(node: NodeRef<'_>) {
let a = node.attributes();
if let Some(id) = a.chain_id() { /* ... */ }
if let Some(score) = a.alignment_score() { /* ... */ }
match a.net_type() {
    Some(NetType::Top) => {}
    Some(NetType::Other(bytes)) => { /* unrecognised type, preserved */ }
    _ => {}
}
let masked = a.int(KnownAttr::ReferenceMasked);        // Option<i32>
let present = a.has(KnownAttr::AlignmentScore);
for (key, value) in a.extras() { /* preserved unknown key/value pairs */ }
# }
```

Strand values are `Strand::Forward` (`+`) and `Strand::Reverse` (`-`).

---

## Validation

Structural validity is separate from parse success. `validate` returns a report
rather than printing anything.

```rust
use netools::ValidationMode;

# fn f(reader: &netools::Reader) {
let report = reader.validate(ValidationMode::Strict);
if report.has_errors() {
    for issue in report.issues() {
        eprintln!("{:?} {}: {}", issue.severity, issue.code.as_str(), issue.message);
    }
}
println!("{} errors, {} warnings", report.error_count(), report.warning_count());
# }
```

Modes: `Syntax` (anomalies only), `Compatible` (structural problems as
warnings), `Strict` (as errors). A single section can be validated with
`net.validate(mode)`.

---

## Writing

Requires feature `write`. Output is canonical and deterministic; documented
attributes are written in canonical order, then preserved unknowns.

```rust
use netools::Writer;

# fn f(reader: &netools::Reader) -> netools::Result<()> {
let mut writer = Writer::from_path("out.net")?;    // `.gz` -> gzip (feature gzip)
for net in reader.nets() {
    writer.write_net(net)?;
}
writer.finish()?;
# Ok(()) }
```

`Writer::new(sink)` wraps any `std::io::Write` (e.g. `Vec<u8>`, stdout).
`NetRef` also has `to_net_string()` and implements `Display`; `NodeRef` has
`write_record_to(out, depth)`.

---

## Owned sections & transformations

`OwnedNet` is a self-contained single section; every borrowed API works on it
via `as_ref()`. It backs streaming and transformations.

```rust
use netools::{TreeFilterPolicy, NetPredicate, NodeKind};

# fn f(net: netools::NetRef<'_>) {
let owned = net.to_owned();            // shares bytes via Arc
// owned.compact();                    // detach and shrink the byte buffer

// Sort sibling lists (hierarchy preserved):
let sorted = net.sort_records();

// Filter with an explicit tree policy:
let pred = NetPredicate { kinds: Some([NodeKind::Fill].into_iter().collect()),
                          min_score: Some(3000.0), ..NetPredicate::new() };
let filtered = net.filter(&pred, TreeFilterPolicy::Prune);

// Extract one record's subtree as its own section:
let sub = net.roots().next().unwrap().clone_subtree();
# }
```

Tree policies: `Prune` (drop failing subtrees), `RetainAncestors` (keep matching
paths), `Promote` (reparent survivors to the nearest kept ancestor).

---

## Streaming

Streaming owns one section at a time — memory stays proportional to the largest
section, not the whole file. It depends only on `std::io::BufRead`.

```rust
use netools::{NetEvent, StreamingReader};

# fn f() -> netools::Result<()> {
let mut reader = StreamingReader::from_path("large.net")?;
while let Some(net) = reader.next_net()? {          // OwnedNet
    println!("{} ({} records)", net.reference_name().len(), net.len());
}

// Or flat events, without building a tree:
let mut ev = StreamingReader::from_path("large.net")?;
while let Some(event) = ev.next_event()? {
    match event {
        NetEvent::NetStart { reference_name, reference_size } => {}
        NetEvent::Record { depth, record } => {}     // OwnedNodeRecord
        NetEvent::NetEnd => {}
    }
}
# Ok(()) }
```

With feature `gzip`, `StreamingReader::from_path_auto` transparently inflates
gzip input.

---

## Parallelism

Requires feature `parallel`. Sections are parsed independently and reassembled
deterministically — parallel and sequential results are byte-identical.

```rust
use netools::{Net, Reader};
use rayon::prelude::*;

# fn f() -> netools::Result<()> {
let reader = Reader::<Net>::from_path_parallel("genome.net")?;
let total_records: usize = reader.par_nets().map(|net| net.len()).sum();
# Ok(()) }
```

Deterministic parallel writing is available via
`Writer::write_all_parallel(&[NetRef], chunk_size)` (bounded memory).

---

## Indexes

Requires feature `index`.

```rust
use netools::{NetIndex, NetRange};

# fn f(reader: &netools::Reader) -> netools::Result<()> {
// Section index (scans headers only):
let sections = NetIndex::from_path("genome.net")?;
if let Some(span) = sections.get(b"chr1") {
    println!("chr1 bytes {}..{}", span.byte_start, span.byte_end);
}

// Chain-id index over all fills:
let chains = reader.chain_id_index();
for loc in chains.occurrences(12345) { /* net_id, node_id */ }

// Per-section reference-interval index:
let net = reader.net(0).unwrap();
let intervals = net.reference_interval_index();
for hit in intervals.overlapping(NetRange::new(1_000_000, 2_000_000)) {}
# Ok(()) }
```

---

## Chain-analysis API

Primitives for chain-breaking analysis (e.g. a future `chainCleaner`), with no
chain scoring baked in — ranking is always supplied by the caller.

```rust
use netools::NetRange;

# fn f(reader: &netools::Reader) {
for net in reader.nets() {
    // Nested fill / parent-gap / enclosing-fill contexts:
    for c in net.nested_fill_gap_contexts() {
        println!("chain {} in chain {}'s gap {}..{}",
                 c.fill_chain_id, c.parent_chain_id,
                 c.parent_gap_range.start, c.parent_gap_range.end);
    }

    // Uninterrupted aligned spans of a fill:
    let fill = net.fills().next().unwrap();
    for span in fill.uninterrupted_reference_spans() { /* AlignmentSpan */ }

    // Overlap queries with caller-supplied ranking:
    let index = net.alignment_span_index();
    let interrupted = index.any_overlap_where(
        NetRange::new(1000, 2000),
        |span| span.chain_id < 42,          // caller decides "higher-ranking"
    );
}

// Across the whole file:
let ids = reader.used_chain_ids();                 // sorted, distinct
for occ in reader.chain_occurrences(12345) {}      // deterministic order
for (a, b) in reader.adjacent_chain_occurrences(12345) {}
# }
```

Also on `NodeRef`: `enclosing_gap()`, `enclosing_fill()`,
`enclosing_fill_with_chain_id()`, `child_gaps()`, `child_gaps_with_fills()`.

---

## Errors

Parsing never panics. Failures are a structured `NetError`:

```rust
use netools::{NetErrorKind, Reader};

match Reader::from_owned_bytes(b" fill 0 10 q + 0 10\n".to_vec()) {
    Ok(reader) => { /* ... */ }
    Err(e) => {
        assert_eq!(e.kind(), NetErrorKind::MissingNetHeader);
        eprintln!("at line {}, column {} (byte {}): {}",
                  e.line, e.column, e.byte_offset, e);
    }
}
```

`NetError` carries `kind`, `byte_offset`, `line`, `column`, the reference net
name once known, and a bounded copy of the offending bytes. In parallel mode the
error with the smallest byte offset is reported.

---

## Feature flags

| feature | default | enables |
|---|:---:|---|
| `mmap` | yes | memory-mapped reading |
| `gzip` | | gzip input/output |
| `parallel` | | section-parallel parse/write, `par_nets` |
| `index` | | `NetIndex`, `ChainIdIndex`, `ReferenceIntervalIndex` |
| `write` | | `Writer`, `Display`, `to_net_string` |
| `serde` | | `serde` derives on small model types |
| `cli` | | the `netools` binary (implies `parallel write index gzip`) |

Coordinates are 32-bit (`u32`); a single input is limited to 4 GiB. Chain ids
are 64-bit.
