# netools stats

Print summary statistics for a NET file.

## Synopsis

```text
netools stats <INPUT>
```

`INPUT` is a path or `-` for stdin.

## Reported values

| Field | Meaning |
|---|---|
| reference nets | Number of reference-sequence sections. |
| records / fills / gaps | Total records and their split by class. |
| max depth | Deepest nesting level (roots are depth 0). |
| depth N | Record count at each depth. |
| distinct chain ids | Unique fill chain ids. |
| distinct query names | Unique query sequence names. |
| reference bases | Sum of reference sizes over fills. |
| query bases | Sum of query sizes over fills. |
| types | Record counts by `type` value. |
| unknown attributes | Counts of undocumented attribute keys encountered. |

## Examples

```sh
netools stats genome.net
netools stats genome.net.gz            # gzip auto-detected
cat genome.net | netools stats -
```

## Global flags

`--threads`, `--log-level`, `--parse-mode`, `--gzip`/`--no-gzip`.
