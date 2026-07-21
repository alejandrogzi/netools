# netools merge

Combine several NET files into one, with an explicit duplicate-reference policy.

## Synopsis

```text
netools merge <INPUT...> [-o <OUTPUT>] [--duplicates <POLICY>] [--sort]
```

One or more `INPUT`s (each a path or `-`); default output is stdout.

## Options

| Option | Values | Default | Meaning |
|---|---|---|---|
| `-o`, `--output` | path / `-` | `-` | Where to write. |
| `--duplicates` | `error`, `keep-first`, `keep-last`, `keep-all` | `error` | How to handle repeated reference names. |
| `--sort` | flag | off | Emit sections in natural reference-name order. |

Duplicate policies act on whole sections keyed by reference name: `error` fails
on any repeat; `keep-first`/`keep-last` keep one; `keep-all` keeps every section
verbatim (allowing repeated names). Root lists from two sections with the same
reference name are **not** silently coalesced — two independently generated nets
may not be compatible.

Section input order is preserved unless `--sort` is given. Reference sizes and
preserved attributes pass through unchanged.

## Examples

```sh
netools merge chr1.net chr2.net chr3.net -o genome.net
netools merge a.net b.net --duplicates keep-first --sort -o merged.net
netools merge *.net --duplicates keep-all -o all.net
```

## Global flags

`--threads`, `--log-level`, `--parse-mode`, `--gzip`/`--no-gzip`.
