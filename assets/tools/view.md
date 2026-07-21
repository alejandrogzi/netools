# netools view

Inspect selected sections and records, as canonical NET or a flat TSV.

## Synopsis

```text
netools view <INPUT> [-o <OUTPUT>] [--reference <NAME>] [--chain-id <ID>]
             [--region <START-END>] [--max-depth <N>] [--flat]
```

`INPUT`/`OUTPUT` accept `-` for stdin/stdout (default output is stdout).

## Options

| Option | Values | Default | Meaning |
|---|---|---|---|
| `-o`, `--output` | path / `-` | `-` | Where to write. |
| `--reference` | name | all | Restrict to one reference sequence. |
| `--chain-id` | integer | all | Keep records of this chain and the ancestors on their paths. |
| `--region` | `START-END` | all | Keep records overlapping this reference interval (paths kept). |
| `--max-depth` | integer | all | Drop records deeper than this. |
| `--flat` | flag | off | Emit a flat TSV instead of canonical NET. |

Selection semantics: `--chain-id` and `--region` retain the ancestor path to each
match; `--max-depth` alone prunes deeper subtrees. With no filters, `view` is an
identity canonicalisation (useful to normalise formatting).

## Flat TSV columns

```text
reference  depth  kind  ref_start  ref_end  query_name  strand  query_start  query_end  chain_id  parent_node_id
```

`chain_id` and `parent_node_id` are `-1` when absent / for roots.

## Examples

```sh
netools view --reference chr1 --max-depth 3 genome.net
netools view --chain-id 12345 genome.net
netools view --region 1000000-2000000 --flat chr1.net
cat genome.net | netools view -                     # canonicalise via stdin
```

## Global flags

`--threads`, `--log-level`, `--parse-mode`, `--gzip`/`--no-gzip`.
