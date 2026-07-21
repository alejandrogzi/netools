# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
