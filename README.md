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
    <a href="https://img.shields.io/badge/version-0.0.1-green" target="_blank">
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
        <a href="https://docs.rs/netools/0.0.1/netools/">docs</a> .
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
