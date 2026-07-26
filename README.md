<p align="center">
  <p align="center">
    <img width=200 align="center" src="./assets/figures/logo.png" >
  </p>

  <span>
    <h1 align="center">
        netools
    </h1>
  </span>

  <p align="center">
    <a href="https://img.shields.io/badge/version-0.0.2-green" target="_blank">
      <img alt="Version Badge" src="https://img.shields.io/badge/version-0.0.1-green">
    </a>
    <a href="https://crates.io/crates/netools" target="_blank">
      <img alt="Crates.io Version" src="https://img.shields.io/crates/v/netools">
    </a>
    <a href="https://github.com/alejandrogzi/netools" target="_blank">
      <img alt="GitHub License" src="https://img.shields.io/github/license/alejandrogzi/netools?color=blue">
    </a>
    <a href="https://crates.io/crates/netools" target="_blank">
      <img alt="Crates.io Total Downloads" src="https://img.shields.io/crates/d/netools">
    </a>
  </p>

  <p align="center">
    <samp>
        <span>work with .net files in Rust</span>
        <br>
        <br>
        <a href="https://docs.rs/netools/0.0.2/netools/">docs</a> .
        <a href="https://github.com/alejandrogzi/netools/tree/master/assets/usage/usage.md">usage</a> .
        <a href="https://github.com/alejandrogzi/netools/tree/master/assets/tools">tools</a> .
        <a href="https://genome.ucsc.edu/goldenpath/help/net.html">nets</a> 
    </samp>
  </p>

</p>


## Installation
### Binary
```bash
cargo install --all-features netools
```

### Docker
```bash
docker pull ghcr.io/alejandrogzi/netools:latest
```

### Conda
```bash
conda install -c bioconda netools
```

### Library
Add this to your `Cargo.toml`:

```toml
[dependencies]
netools = { version = "0.0.1", features = ["mmap", "gzip", "parallel"] }
```
## Benchmarks

### `netools` commands

<div align="center">

| Command | Mean [s] | Min [s] | Max [s] | Relative |
|:---|---:|---:|---:|---:|
| `netools filter` | 3.255 ± 0.029 | 3.209 | 3.305 | 1.00 |
||
| `netools sort` | 14.014 ± 0.131 | 13.868 | 14.327 | 1.00 |
||
| `netools merge` | 31.587 ± 9.757 | 22.248 | 50.022 | 1.00 |

</div>

Commands: `filter --kind fill --min-score 100000 IN -o /dev/null`, `sort --records reference IN -o /dev/null`, `merge --duplicates keep-all IN IN -o /dev/null` (10–12 runs each, 2 warmups; `merge` is memory-bound — it parses two full copies — hence the wider spread).

*ran using hyperfine 1.18.0 on an AMD Ryzen 7 5700X with 128 GB of RAM and 16 cores with a 1.4 GB uncompressed `hg38.mm39.net` (hg38↔mm39 whole-genome net, 20.3M records across 349 sequences) file as input

---

### Raw `chainNet` conversion

<div align="center">

| Command | Plain mean ± SD | Gzip mean ± SD | Plain speedup | Gzip speedup | Peak RSS plain / gzip |
|:--|--:|--:|--:|--:|--:|
| Kent `chainNet` (query discarded) | 18.019 ± 0.248 s | 18.920 ± 0.402 s | 1.00× | 1.00× | 3.63 / 3.63 GiB |
| Rust both, 1 thread | 10.896 ± 0.064 s | 12.130 ± 0.021 s | 1.65× | 1.56× | 3.54 / 3.54 GiB |
| Rust both, 16 threads | 7.830 ± 0.063 s | 9.074 ± 0.056 s | 2.30× | 2.09× | 4.13 / 4.11 GiB |
| Rust reference-only, 1 thread | 8.619 ± 0.107 s | 9.767 ± 0.043 s | 2.09× | 1.94× | 3.45 / 3.45 GiB |
| Rust reference-only, 16 threads | 7.094 ± 0.052 s | 8.217 ± 0.029 s | 2.54× | 2.30× | 4.09 / 4.09 GiB |

</div>

*ran using hyperfine 1.18.0 on an AMD Ryzen 7 5700X with 128 GB of RAM and 16 cores with a 360 MB compressed `hg38.rhiRex1.net` (hg38↔rhiRex1 whole-genome chain; 3,049,608 chains; 608 reference sequences; 91 query sequences) file as input

---

### `chainNet` + Kent `NetFilterNonNested.perl`

<div align="center">


| Command | Plain mean ± SD | Gzip mean ± SD | Plain speedup | Gzip speedup | Peak RSS plain / gzip |
|:--|--:|--:|--:|--:|--:|
| Kent `chainNet` + Perl | 24.611 ± 0.339 s | 25.685 ± 1.153 s | 1.00× | 1.00× | 3.63 / 3.63 GiB |
| Rust `--preset ccr`, 1 thread | 8.060 ± 0.029 s | 9.216 ± 0.111 s | 3.05× | 2.79× | 3.43 / 3.43 GiB |
| Rust `--preset ccr`, 16 threads | 6.634 ± 0.044 s | 7.668 ± 0.051 s | 3.71× | 3.35× | 4.05 / 4.05 GiB |

</div>

*ran using hyperfine 1.18.0 on an AMD Ryzen 7 5700X with 128 GB of RAM and 16 cores with a 360 MB compressed `hg38.rhiRex1.net` (hg38↔rhiRex1 whole-genome chain; 3,049,608 chains; 608 reference sequences; 91 query sequences) file as input
