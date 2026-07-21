# netools sort

Order sections and, optionally, sibling records — without flattening the
hierarchy.

## Synopsis

```text
netools sort <INPUT> [-o <OUTPUT>] [--nets <ORDER>] [--records <ORDER>]
             [--validate]
```

`INPUT`/`OUTPUT` accept `-` (default output is stdout).

## Options

| Option | Values | Default | Meaning |
|---|---|---|---|
| `-o`, `--output` | path / `-` | `-` | Where to write. |
| `--nets` | `preserve`, `lexicographic`, `natural` | `natural` | Section ordering by reference name. |
| `--records` | `preserve`, `reference` | `preserve` | Sibling ordering within each section. |
| `--validate` | flag | off | Validate each rebuilt section before writing. |

`natural` orders names so `chr2` precedes `chr10`. `--records reference`
stable-sorts every sibling list by `(reference_start, reference_size, kind,
query_name, query_start)`, recursively; parent/child relationships are never
changed and records are never globally flattened.

## Examples

```sh
netools sort genome.net -o sorted.net
netools sort --nets natural --records reference genome.net -o sorted.net
netools sort --records reference --validate genome.net -o sorted.net
```

## Global flags

`--threads`, `--log-level`, `--parse-mode`, `--gzip`/`--no-gzip`.
