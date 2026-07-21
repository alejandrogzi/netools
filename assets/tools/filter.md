# netools filter

Filter records by a predicate, with explicit tree-reshaping semantics.

## Synopsis

```text
netools filter <INPUT> [-o <OUTPUT>] [PREDICATES...] [--tree-policy <POLICY>]
```

`INPUT`/`OUTPUT` accept `-` (default output is stdout). All predicates are
combined with logical AND; an unset predicate always passes.

## Predicates

| Option | Values | Meaning |
|---|---|---|
| `--kind` | `fill`, `gap` | Keep only this record class. |
| `--min-score` / `--max-score` | float | Bound the alignment score (`score`). |
| `--min-ali` | integer | Minimum aligned bases (`ali`). |
| `--type` | comma list | Accepted `type` values (e.g. `syn,inv`). |
| `--chain-id` | comma list | Accepted chain ids. |
| `--query-name` | comma list | Accepted query sequence names. |
| `--strand` | `+`, `-` | Accepted strand. |
| `--min-depth` / `--max-depth` | integer | Bound nesting depth. |
| `--region` | `START-END` | Record must overlap this reference interval. |

## Tree policy

| `--tree-policy` | Meaning |
|---|---|
| `prune` (default) | Drop a failing record and its whole subtree. UCSC-compatible. |
| `retain-ancestors` | Keep matches and every ancestor needed to preserve their paths. |
| `promote` | Drop a failing record but reattach retained descendants to the nearest retained ancestor. May produce non-canonical structure. |

## Examples

```sh
netools filter --kind fill --min-score 3000 genome.net -o hi.net
netools filter --type syn,inv --tree-policy retain-ancestors genome.net -o syn.net
netools filter --chain-id 12,34,56 --region 1000000-2000000 chr1.net -o sub.net
```

## Global flags

`--threads`, `--log-level`, `--parse-mode`, `--gzip`/`--no-gzip`.
