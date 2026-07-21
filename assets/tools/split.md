# netools split

Write each reference-sequence section to its own file.

## Synopsis

```text
netools split <INPUT> [--out-dir <DIR>] [--suffix <SUFFIX>] [--gzip]
              [--manifest <PATH>]
```

`INPUT` is a path or `-` for stdin.

## Options

| Option | Default | Meaning |
|---|---|---|
| `--out-dir` | `split` | Output directory (created if missing). |
| `--suffix` | `.net` | Filename suffix per section (`.gz` appended when `--gzip`). |
| `--gzip` | off | Gzip-compress each output file. |
| `--manifest` | none | Write a TSV manifest to this path. |

## Behaviour

- One file per `net` section; the filename is the sanitised reference name plus
  the suffix (non-alphanumeric characters become `_`).
- Sanitised-name collisions are disambiguated with a numeric suffix
  (`name.1.net`, `name.2.net`, …).
- Each file is written to a temporary path and atomically renamed into place.
- Hierarchy is preserved; output is canonical NET.

## Manifest columns

```text
reference_name  reference_size  output_file  records
```

## Examples

```sh
netools split genome.net --out-dir chroms
netools split genome.net --out-dir chroms --gzip --manifest chroms/manifest.tsv
```

## Global flags

`--threads`, `--log-level`, `--parse-mode`, `--gzip`/`--no-gzip` (the global
`--gzip` also biases output; the per-command `--gzip` here is explicit).
