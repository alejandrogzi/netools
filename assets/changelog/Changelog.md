# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [0.0.2] — 2026-07-26

This release adds the first chain-to-NET construction path: a library
builder and CLI subcommand that generate UCSC alignment NETs directly from
score-sorted chain records, replacing the Perl `chainNet` + `NetFilterNonNested`
pipeline. It also tightens the CLI interface for `merge` and `split`, and
fixes several Docker build issues.

### Added

- **`chainnet` feature: chain-driven NET construction** — A new
  `ChainNetBuilder` constructs reference and query NETs from score-descending
  chains without serialising or reparsing an intermediate raw NET. The builder
  is order-sensitive within a chromosome (matching Kent's expectations) and
  parallelises only across independent chromosomes. Query-minus chains are
  projected into forward query coordinates without mutating the parsed chain
  records. A `chaincleaner_compatible()` preset fuses raw construction with the
  exact non-nested `minScore1=3000` transformation that the Perl pipeline
  applies as a separate step.

- **`NonNestedFilter` and `filter_non_nested`** — A public library function
  that applies the exact descendant-promotion semantics of
  `NetFilterNonNested.perl -minScore1` to any finalised `OwnedNet`. When a fill
  is rejected, its direct gap and that gap's child fills are promoted to the
  rejected fill's output parent, reproducing the Kent behaviour without an
  external Perl invocation.

- **`SizeSource::TwoBit`** — Sequence sizes can now be read from a `.2bit`
  genome index via the `twobit` crate, removing the need for a separate
  `chrom.sizes` file when a 2bit index is already available alongside the
  chain input.

- **`netools net` CLI subcommand** — A new `net` subcommand reads
  score-sorted chains from a file or stdin and writes reference and/or query
  NETs. It accepts `--chain`, `--reference-sizes` / `--reference-2bit`, and
  `--query-sizes` / `--query-2bit`. The `--preset ccr` flag selects exact
  `chainCleaner`-compatible generation (reference-only, `minScore=0` during
  construction, then `minScore1=3000` post-filter). Diagnostic
  `--raw-net-out` and `--raw-query-net-out` flags emit the pre-post-filter
  arena for inspection.

- **Merge input via `--nets` and `--file`** — `netools merge` now accepts
  `--nets` for comma-separated paths and `--file` for a plain-text list of
  NET paths (one per line). Positional arguments are rejected to prevent
  accidental argument-order mistakes.

- **Split input via `--net`** — `netools split` now requires `--net` for the
  input path. When `--net` is omitted, input is read from stdin. Positional
  arguments are rejected for interface uniformity across subcommands.

- **Thread count passthrough for `net` construction** — The CLI context
  struct carries an explicit `threads: Option<usize>` field. Chain-to-NET
  construction now respects the global `--threads` flag for chromosome-level
  parallelism, and serial mode (`--threads 1`) is honoured throughout.

- **Benchmarks against Kent `chainNet`** — The README now includes a
  benchmark comparison of Rust `chainNet`-style NET construction against the
  Kent `chainNet` binary and the `chainNet + NetFilterNonNested.perl` pipeline
  on hg38↔rhiRex1 whole-genome chains (3,049,608 chains, 608 reference
  sequences, 91 query sequences). The Rust reference-only 16-thread path
  achieves 2.54× speedup over Kent `chainNet` alone. The `--preset ccr` path
  achieves 3.71× over the full Perl pipeline.

- **Chainnet tests** — A new `tests/chainnet.rs` module covers both-side
  construction, query-minus projection, internal-gap coordinate correctness,
  non-nested filter descendant promotion, public `filter_non_nested` on
  standalone `OwnedNet`s, score-order validation, Kent ties-to-even score
  rounding, metadata preservation and omission after filtering, and parallel
  determinism across thread counts.

- **Expanded CLI integration tests** — `tests/cli.rs` now tests `merge`
  input via `--nets` and `--file`, split input via `--net` and stdin, split
  positional-argument rejection, `net` subcommand chain input via `--chain`
  and stdin, positional chain input rejection, and old `chain-net` alias
  removal.

### Changed

- **`cli` feature now implies `chainnet`** — Building with `--features cli`
  pulls in `chaintools` and `twobit` transitively, so the `net` subcommand is
  always available in the CLI binary.

- **`gzip` and `parallel` features forward to `chaintools`** — Enabling
  `gzip` or `parallel` now also enables the corresponding `chaintools`
  features, ensuring chain parsing and parallel chain reading use consistent
  compression and thread-pool settings across both crates.

### Dependencies

- Added `chaintools` 0.0.9 (local path dependency) — chain parsing and
  sequential/parallel reader infrastructure required by the `chainnet` feature.
- Added `twobit` 0.2.2 — optional 2bit genome index reader for extracting
  sequence sizes without a separate `chrom.sizes` file.

### Fixed

- **Dockerfile** — Pinned the build stage to `rust:1.93.0-slim-bookworm`
  instead of the rolling `rust:1-slim-bookworm` tag. Added `ca-certificates`
  and `procps` to the runtime image. Explicitly set executable permissions on
  the binary with `chmod +x`. Fixed the `RUN 'netools --version'` quoting
  issue that caused a shell error by using `RUN netools --version` without
  quotes. Build now compiles with `--all-features` and `--locked`, and the
  binary is stripped to reduce image size.

---

## [0.0.1] — 2026-07-21

### Added

- **Core data model** — Contiguous preorder arena representation for NET
  records. Nodes store topology links (`parent`, `first_child`, `next_sibling`,
  `subtree_end`) instead of recursive child vectors, providing O(1) parent
  lookup and cache-efficient traversal. Stable `NodeId` and `NetId` types
  backed by `u32`.

- **Sequential parser** — Single-pass byte parser built on `memchr` for
  line-boundary detection. Uses an explicit indentation stack rather than
  recursion. Handles the seven fixed record fields (`class`, `tStart`, `tSize`,
  `qName`, `qStrand`, `qStart`, `qSize`) plus optional key/value attributes.
  Checked arithmetic on all numeric fields; overflow produces structured
  errors. Three parsing modes: `Permissive`, `Compatible` (default), and
  `Strict`.

- **Reader backends** — `Reader<Net>::from_path` auto-detects gzip via magic
  bytes and transparently decompresses. Falls back to owned-buffer I/O when
  mmap is unavailable. `from_mmap` uses `memmap2` for zero-copy input.
  `from_reader` wraps any `Read` source. Gzip detection is by content, not
  solely by filename extension.

- **Byte storage layer** — `SharedBytes` enum (`Owned` / `Mapped`) and
  `ByteSlice` handle textual data without requiring UTF-8. All query names,
  reference names, and unknown attribute values remain byte-oriented.
  `to_string_lossy()` is provided for display purposes.

- **Parallel parser** — Section-level parallelism with Rayon. Scans input for
  `net` headers, dispatches independent chromosome sections to worker threads,
  then rebases local node and attribute arenas into global IDs. Deterministic
  output: `from_path_parallel` produces byte-identical canonical output to
  `from_path`.

- **Streaming reader** — `StreamingReader` yields one `OwnedNet` at a time
  without retaining the full file in memory. Suitable for pipelines over stdin
  and large files. Also provides an event-based API (`NetEvent`) for the
  lowest-memory workflows, at the cost of losing hierarchical handles.

- **Traversal API** — Non-recursive iterators over net roots, preorder,
  children, siblings, ancestors, descendants, subtrees, fills, gaps, and
  nodes at a given depth. A `NetVisitor` trait enables allocation-free
  tree-walking for writing, statistics, and filtering passes.

- **Canonical writer** — `Writer<W>` serialises borrowed and owned nets to
  any `Write` sink. Writes known optional fields in UCSC canonical order,
  followed by preserved unknown fields in input order. Supports gzip output
  and configurable indentation. Implements `Display` for `NetRef` and
  `OwnedNet`.

- **Parallel serialisation** — Chunked parallel writing that partitions nets
  by ordinal, serialises each chunk in parallel into separate byte buffers,
  and flushes buffers sequentially. A configurable byte threshold prevents
  unbounded memory use on files with very large chromosome sections.

- **Validation** — Three validation modes (`Syntax`, `Compatible`, `Strict`)
  that check structural invariants: reference intervals within chromosome
  boundaries, child intervals inside parent intervals, fill/gap alternation
  by depth, sorted sibling intervals, presence of chain IDs on fills, and
  absence of unknown attributes in strict mode. Issues are returned as a typed
  `ValidationReport` rather than written to stderr.

- **Indexing** — Three index types:
  - **Section index**: maps reference sequence names to byte spans, enabling
    header-only scanning without full parse.
  - **Chain-ID index**: maps chain IDs to all fill records that carry them,
    ordered by reference net and preorder position.
  - **Reference-interval index**: sorted interval vectors per chromosome for
    overlap, containment, and endpoint queries.

- **Chain-cleaning context API** — Primitives designed for a future
  `chainCleaner` port: `FillContext`, `NestedFillGapContext`,
  `AlignmentSpan`, and `AlignmentSpanIndex`. `uninterrupted_reference_spans`
  partitions a fill's reference range around child gaps that themselves contain
  fills. Adjacent chain occurrences are provided in deterministic order.

- **Filtering** — `NetPredicate` supports conditions on reference name, query
  name, record kind, depth, coordinates, strand, chain ID, score, aligned
  bases, `type`, and attribute presence. Three tree policies:
  - `Prune`: remove failing nodes and their subtrees (default).
  - `RetainAncestors`: keep matching nodes and the path from root.
  - `Promote`: discard failing nodes but graft their children onto the
    nearest retained ancestor.

- **Sorting** — Reference nets can be sorted lexicographically, naturally
  (human-aware numeric ordering), or left in input order. Sibling records
  can be recursively stable-sorted by reference coordinates, record kind,
  and query name. Sorting is parallelised across independent NET sections.

- **Merging** — Combines complete NET files with explicit duplicate-reference
  policies: `error`, `keep-first`, `keep-last`, `keep-all`. Reference sizes
  are preserved. Input order is maintained unless `--sort` is specified.

- **Splitting** — Writes each NET section to an independent file with
  filename-safe reference names. Detects sanitised-name collisions. Generates
  a manifest mapping reference names to output files. Uses atomic
  temporary-file-plus-rename. Bounds concurrent output file descriptors.

- **Statistics** — Aggregates counts of reference nets, total records, fills,
  gaps, maximum depth, records per depth, distinct chain IDs, distinct query
  names, reference bases, query bases, counts by `type`, missing-field counts,
  and unknown attribute names.

- **View** — Human-readable and TSV output with filtering by reference net,
  chain ID, reference interval, and maximum depth.

- **CLI** — Seven subcommands (`validate`, `view`, `stats`, `split`, `sort`,
  `filter`, `merge`) behind the `cli` feature. Global flags for thread count,
  log level, parse mode, and gzip control. Supports `-` for stdin/stdout
  where appropriate.

- **Error model** — `NetError` with typed `NetErrorKind`, absolute byte
  offset, line and column numbers, and optional reference-net context.
  No malformed input causes a panic. Parallel parsing reports the error
  with the smallest input byte offset.

- **Feature gating** — Optional features mirror `chaintools`: `mmap` (default),
  `gzip`, `parallel`, `index`, `write`, `cli`, and `serde`. Streaming depends
  only on `std::io::BufRead` and is always available.

- **Property-based tests** — Random valid NET trees are generated, written,
  reparsed, and compared for structural identity. Filtering invariants and
  sibling sorting are also tested under random generation.

- **Differential test baseline** — Parsing, round-trip, sequential/parallel
  parity, streaming parity, traverse correctness, validation, malformed-input
  rejection, chain-context semantics, and CLI integration are covered across
  twelve test modules.

### License

- Project licensed under GPL-3.0. See `LICENSE` for the full text.
