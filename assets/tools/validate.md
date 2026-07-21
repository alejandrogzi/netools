# netools validate

Report structural issues in a NET file. Parsing (syntax) and structural
validity are checked separately.

## Synopsis

```text
netools validate <INPUT> [--mode <MODE>] [--tsv] [--error-on-warning]
```

`INPUT` is a path or `-` for stdin. Gzip input is detected automatically.

## Options

| Option | Values | Default | Meaning |
|---|---|---|---|
| `--mode` | `syntax`, `compatible`, `strict` | `strict` | How strict the structural checks are. |
| `--tsv` | flag | off | Emit tab-separated findings instead of human-readable text. |
| `--error-on-warning` | flag | off | Exit non-zero when warnings (not just errors) are present. |

Modes: `syntax` reports only anomalies implied by a successful parse;
`compatible` reports structural problems as warnings; `strict` reports them as
errors. Checks include reference-bounds, child-inside-parent containment,
sibling ordering/overlap, fill/gap alternation, and chain-id rules
(fills carry an id, gaps do not).

## Output

Human-readable (default), one finding per line, plus an error/warning summary on
stderr. With `--tsv`: `severity  code  net  node  message` (node is `-1` when
not applicable).

## Exit status

`0` when clean; `1` when errors exist, or when warnings exist and
`--error-on-warning` is set.

## Examples

```sh
netools validate genome.net
netools validate --mode compatible --tsv genome.net > report.tsv
netools validate --mode strict --error-on-warning genome.net
```

## Global flags

`--threads`, `--log-level`, `--parse-mode`, `--gzip`/`--no-gzip` apply to every
command. `--parse-mode` controls what the parser accepts before validation runs.
