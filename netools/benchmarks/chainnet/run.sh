#!/usr/bin/env bash
set -euo pipefail

usage() {
    echo "usage: $0 INPUT.chain CHAIN_NET NET_FILTER OUTPUT_DIR [THREADS] [RUNS] [INPUT.chain.gz]" >&2
    exit 2
}

if [[ $# -lt 4 || $# -gt 7 ]]; then
    usage
fi

input_chain=$(realpath "$1")
kent_chain_net=$(realpath "$2")
net_filter=$(realpath "$3")
output_dir=$4
threads=${5:-$(nproc)}
runs=${6:-15}
gzip_chain=
if [[ $# -eq 7 ]]; then
    gzip_chain=$(realpath "$7")
fi
warmup=${CHAINNET_BENCH_WARMUP:-3}
rss_runs=${CHAINNET_BENCH_RSS_RUNS:-3}

if [[ ! -f "$input_chain" || ! -x "$kent_chain_net" || ! -x "$net_filter" ]]; then
    echo "input must exist and reference programs must be executable" >&2
    exit 2
fi
if [[ -n "$gzip_chain" && ! -f "$gzip_chain" ]]; then
    echo "gzip input does not exist: $gzip_chain" >&2
    exit 2
fi
if [[ ! "$threads" =~ ^[1-9][0-9]*$ || ! "$runs" =~ ^[1-9][0-9]*$ ]]; then
    echo "THREADS and RUNS must be positive integers" >&2
    exit 2
fi
if [[ ! "$warmup" =~ ^[0-9]+$ || ! "$rss_runs" =~ ^[1-9][0-9]*$ ]]; then
    echo "CHAINNET_BENCH_WARMUP must be non-negative and CHAINNET_BENCH_RSS_RUNS positive" >&2
    exit 2
fi

for program in awk cmp cargo gzip hyperfine sha256sum /usr/bin/time; do
    if ! command -v "$program" >/dev/null 2>&1; then
        echo "required program not found: $program" >&2
        exit 2
    fi
done

if [[ -n "$gzip_chain" ]]; then
    gzip -t "$gzip_chain"
    if ! gzip -dc "$gzip_chain" | cmp "$input_chain" -; then
        echo "plain and gzip inputs do not contain the same chain stream" >&2
        exit 2
    fi
fi

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
crate_dir=$(cd "$script_dir/../.." && pwd)
mkdir -p "$output_dir"
output_dir=$(realpath "$output_dir")
work_dir="$output_dir/work"
results_dir="$output_dir/results"
mkdir -p "$work_dir" "$results_dir"

reference_sizes="$work_dir/reference.sizes"
query_sizes="$work_dir/query.sizes"

# Derive the minimal ordered size files represented by the chain headers.
# Abort if one sequence name is paired with conflicting sizes.
awk -v reference_out="$reference_sizes" -v query_out="$query_sizes" '
BEGIN {
    printf "%s", "" > reference_out
    printf "%s", "" > query_out
    close(reference_out)
    close(query_out)
}
$1 == "chain" {
    chains++
    if (($3 in reference_size) && reference_size[$3] != $4) {
        printf "conflicting reference size for %s: %s and %s\n", \
            $3, reference_size[$3], $4 > "/dev/stderr"
        invalid = 1
    } else if (!($3 in reference_size)) {
        print $3 "\t" $4 >> reference_out
        reference_size[$3] = $4
    }
    if (($8 in query_size) && query_size[$8] != $9) {
        printf "conflicting query size for %s: %s and %s\n", \
            $8, query_size[$8], $9 > "/dev/stderr"
        invalid = 1
    } else if (!($8 in query_size)) {
        print $8 "\t" $9 >> query_out
        query_size[$8] = $9
    }
}
END {
    if (chains == 0) {
        print "no chain headers found" > "/dev/stderr"
        invalid = 1
    }
    if (invalid) {
        exit 1
    }
}
' "$input_chain"

(
    cd "$crate_dir"
    cargo build --offline --release --all-features
)
rust_chain_net="$crate_dir/target/release/netools"

shell_command() {
    local rendered
    printf -v rendered '%q ' "$@"
    printf '%s' "${rendered% }"
}

kent_raw=$(shell_command \
    "$kent_chain_net" -minScore=0 "$input_chain" "$reference_sizes" "$query_sizes" \
    "$work_dir/bench-kent.raw.net" /dev/null)
rust_both_t1=$(shell_command \
    "$rust_chain_net" --threads 1 net --chain "$input_chain" \
    --reference-sizes "$reference_sizes" --query-sizes "$query_sizes" \
    --reference-net "$work_dir/bench-rust-both-t1.reference.net" \
    --query-net /dev/null --min-score 0)
rust_both_parallel=$(shell_command \
    "$rust_chain_net" --threads "$threads" net --chain "$input_chain" \
    --reference-sizes "$reference_sizes" --query-sizes "$query_sizes" \
    --reference-net "$work_dir/bench-rust-both-parallel.reference.net" \
    --query-net /dev/null --min-score 0)
rust_reference_t1=$(shell_command \
    "$rust_chain_net" --threads 1 net --chain "$input_chain" \
    --reference-sizes "$reference_sizes" --query-sizes "$query_sizes" \
    --reference-net "$work_dir/bench-rust-reference-t1.raw.net" \
    --reference-only --min-score 0)
rust_reference_parallel=$(shell_command \
    "$rust_chain_net" --threads "$threads" net --chain "$input_chain" \
    --reference-sizes "$reference_sizes" --query-sizes "$query_sizes" \
    --reference-net "$work_dir/bench-rust-reference-parallel.raw.net" \
    --reference-only --min-score 0)

kent_filter=$(shell_command \
    "$net_filter" "$work_dir/bench-kent.pipeline.raw.net" -minScore1 3000)
kent_pipeline="$(shell_command \
    "$kent_chain_net" -minScore=0 "$input_chain" "$reference_sizes" "$query_sizes" \
    "$work_dir/bench-kent.pipeline.raw.net" /dev/null) && $kent_filter > $(printf '%q' "$work_dir/bench-kent.pipeline.filtered.net")"
rust_fused_t1=$(shell_command \
    "$rust_chain_net" --threads 1 net --chain "$input_chain" \
    --reference-sizes "$reference_sizes" --query-sizes "$query_sizes" \
    --reference-net "$work_dir/bench-rust-t1.fused.net" --preset ccr)
rust_fused_parallel=$(shell_command \
    "$rust_chain_net" --threads "$threads" net --chain "$input_chain" \
    --reference-sizes "$reference_sizes" --query-sizes "$query_sizes" \
    --reference-net "$work_dir/bench-rust-parallel.fused.net" --preset ccr)

if [[ -n "$gzip_chain" ]]; then
    gzip_to_stdout=$(shell_command gzip -dc "$gzip_chain")
    kent_gzip_core=$(shell_command \
        "$kent_chain_net" -minScore=0 /dev/stdin "$reference_sizes" "$query_sizes" \
        "$work_dir/bench-kent-gzip.raw.net" /dev/null)
    kent_gzip_raw_inner="$gzip_to_stdout | $kent_gzip_core"
    kent_gzip_raw=$(shell_command bash -o pipefail -c "$kent_gzip_raw_inner")
    rust_gzip_both_t1=$(shell_command \
        "$rust_chain_net" --threads 1 net --chain "$gzip_chain" \
        --reference-sizes "$reference_sizes" --query-sizes "$query_sizes" \
        --reference-net "$work_dir/bench-rust-gzip-both-t1.reference.net" \
        --query-net /dev/null --min-score 0)
    rust_gzip_both_parallel=$(shell_command \
        "$rust_chain_net" --threads "$threads" net --chain "$gzip_chain" \
        --reference-sizes "$reference_sizes" --query-sizes "$query_sizes" \
        --reference-net "$work_dir/bench-rust-gzip-both-parallel.reference.net" \
        --query-net /dev/null --min-score 0)
    rust_gzip_reference_t1=$(shell_command \
        "$rust_chain_net" --threads 1 net --chain "$gzip_chain" \
        --reference-sizes "$reference_sizes" --query-sizes "$query_sizes" \
        --reference-net "$work_dir/bench-rust-gzip-reference-t1.raw.net" \
        --reference-only --min-score 0)
    rust_gzip_reference_parallel=$(shell_command \
        "$rust_chain_net" --threads "$threads" net --chain "$gzip_chain" \
        --reference-sizes "$reference_sizes" --query-sizes "$query_sizes" \
        --reference-net "$work_dir/bench-rust-gzip-reference-parallel.raw.net" \
        --reference-only --min-score 0)

    kent_gzip_pipeline_core=$(shell_command \
        "$kent_chain_net" -minScore=0 /dev/stdin "$reference_sizes" "$query_sizes" \
        "$work_dir/bench-kent-gzip.pipeline.raw.net" /dev/null)
    kent_gzip_filter=$(shell_command \
        "$net_filter" "$work_dir/bench-kent-gzip.pipeline.raw.net" -minScore1 3000)
    kent_gzip_pipeline_inner="$gzip_to_stdout | $kent_gzip_pipeline_core && $kent_gzip_filter > $(printf '%q' "$work_dir/bench-kent-gzip.pipeline.filtered.net")"
    kent_gzip_pipeline=$(shell_command bash -o pipefail -c "$kent_gzip_pipeline_inner")
    rust_gzip_fused_t1=$(shell_command \
        "$rust_chain_net" --threads 1 net --chain "$gzip_chain" \
        --reference-sizes "$reference_sizes" --query-sizes "$query_sizes" \
        --reference-net "$work_dir/bench-rust-gzip-t1.fused.net" --preset ccr)
    rust_gzip_fused_parallel=$(shell_command \
        "$rust_chain_net" --threads "$threads" net --chain "$gzip_chain" \
        --reference-sizes "$reference_sizes" --query-sizes "$query_sizes" \
        --reference-net "$work_dir/bench-rust-gzip-parallel.fused.net" --preset ccr)
fi

# Establish raw reference/query parity, including parallel determinism, before
# collecting timings.
"$kent_chain_net" -minScore=0 "$input_chain" "$reference_sizes" "$query_sizes" \
    "$work_dir/parity-kent.reference.net" "$work_dir/parity-kent.query.net"
"$rust_chain_net" --threads 1 net --chain "$input_chain" \
    --reference-sizes "$reference_sizes" --query-sizes "$query_sizes" \
    --reference-net "$work_dir/parity-rust-t1.reference.net" \
    --query-net "$work_dir/parity-rust-t1.query.net" --min-score 0
"$rust_chain_net" --threads "$threads" net --chain "$input_chain" \
    --reference-sizes "$reference_sizes" --query-sizes "$query_sizes" \
    --reference-net "$work_dir/parity-rust-parallel.reference.net" \
    --query-net "$work_dir/parity-rust-parallel.query.net" --min-score 0
cmp "$work_dir/parity-kent.reference.net" "$work_dir/parity-rust-t1.reference.net"
cmp "$work_dir/parity-kent.query.net" "$work_dir/parity-rust-t1.query.net"
cmp "$work_dir/parity-rust-t1.reference.net" "$work_dir/parity-rust-parallel.reference.net"
cmp "$work_dir/parity-rust-t1.query.net" "$work_dir/parity-rust-parallel.query.net"

"$net_filter" "$work_dir/parity-kent.reference.net" -minScore1 3000 \
    > "$work_dir/parity-kent.filtered.net"
"$rust_chain_net" --threads 1 net --chain "$input_chain" \
    --reference-sizes "$reference_sizes" --query-sizes "$query_sizes" \
    --reference-net "$work_dir/parity-rust-t1.filtered.net" --preset ccr
"$rust_chain_net" --threads "$threads" net --chain "$input_chain" \
    --reference-sizes "$reference_sizes" --query-sizes "$query_sizes" \
    --reference-net "$work_dir/parity-rust-parallel.filtered.net" --preset ccr
cmp "$work_dir/parity-kent.filtered.net" "$work_dir/parity-rust-t1.filtered.net"
cmp "$work_dir/parity-rust-t1.filtered.net" "$work_dir/parity-rust-parallel.filtered.net"

if [[ -n "$gzip_chain" ]]; then
    bash -c "$kent_gzip_raw"
    "$rust_chain_net" --threads 1 net --chain "$gzip_chain" \
        --reference-sizes "$reference_sizes" --query-sizes "$query_sizes" \
        --reference-net "$work_dir/parity-rust-gzip-t1.reference.net" \
        --query-net "$work_dir/parity-rust-gzip-t1.query.net" --min-score 0
    "$rust_chain_net" --threads "$threads" net --chain "$gzip_chain" \
        --reference-sizes "$reference_sizes" --query-sizes "$query_sizes" \
        --reference-net "$work_dir/parity-rust-gzip-parallel.reference.net" \
        --query-net "$work_dir/parity-rust-gzip-parallel.query.net" --min-score 0
    cmp "$work_dir/parity-kent.reference.net" "$work_dir/bench-kent-gzip.raw.net"
    cmp "$work_dir/parity-kent.reference.net" "$work_dir/parity-rust-gzip-t1.reference.net"
    cmp "$work_dir/parity-kent.query.net" "$work_dir/parity-rust-gzip-t1.query.net"
    cmp "$work_dir/parity-rust-gzip-t1.reference.net" \
        "$work_dir/parity-rust-gzip-parallel.reference.net"
    cmp "$work_dir/parity-rust-gzip-t1.query.net" \
        "$work_dir/parity-rust-gzip-parallel.query.net"

    "$net_filter" "$work_dir/bench-kent-gzip.raw.net" -minScore1 3000 \
        > "$work_dir/parity-kent-gzip.filtered.net"
    "$rust_chain_net" --threads 1 net --chain "$gzip_chain" \
        --reference-sizes "$reference_sizes" --query-sizes "$query_sizes" \
        --reference-net "$work_dir/parity-rust-gzip-t1.filtered.net" --preset ccr
    "$rust_chain_net" --threads "$threads" net --chain "$gzip_chain" \
        --reference-sizes "$reference_sizes" --query-sizes "$query_sizes" \
        --reference-net "$work_dir/parity-rust-gzip-parallel.filtered.net" --preset ccr
    cmp "$work_dir/parity-kent.filtered.net" "$work_dir/parity-kent-gzip.filtered.net"
    cmp "$work_dir/parity-kent.filtered.net" "$work_dir/parity-rust-gzip-t1.filtered.net"
    cmp "$work_dir/parity-rust-gzip-t1.filtered.net" \
        "$work_dir/parity-rust-gzip-parallel.filtered.net"
fi

hyperfine --warmup "$warmup" --runs "$runs" --style basic \
    --export-json "$results_dir/raw.json" \
    --export-markdown "$results_dir/raw.md" \
    --command-name "Kent chainNet (both; query discarded)" "$kent_raw" \
    --command-name "Rust both (1 thread)" "$rust_both_t1" \
    --command-name "Rust both ($threads threads)" "$rust_both_parallel" \
    --command-name "Rust reference-only (1 thread)" "$rust_reference_t1" \
    --command-name "Rust reference-only ($threads threads)" "$rust_reference_parallel"

hyperfine --warmup "$warmup" --runs "$runs" --style basic \
    --export-json "$results_dir/filtered.json" \
    --export-markdown "$results_dir/filtered.md" \
    --command-name "Kent chainNet + Perl" "$kent_pipeline" \
    --command-name "Rust fused reference-only (1 thread)" "$rust_fused_t1" \
    --command-name "Rust fused reference-only ($threads threads)" "$rust_fused_parallel"

if [[ -n "$gzip_chain" ]]; then
    hyperfine --warmup "$warmup" --runs "$runs" --style basic \
        --export-json "$results_dir/raw-gzip.json" \
        --export-markdown "$results_dir/raw-gzip.md" \
        --command-name "Kent chainNet, gzip pipeline (both; query discarded)" "$kent_gzip_raw" \
        --command-name "Rust both, gzip (1 thread)" "$rust_gzip_both_t1" \
        --command-name "Rust both, gzip ($threads threads)" "$rust_gzip_both_parallel" \
        --command-name "Rust reference-only, gzip (1 thread)" "$rust_gzip_reference_t1" \
        --command-name "Rust reference-only, gzip ($threads threads)" \
        "$rust_gzip_reference_parallel"

    hyperfine --warmup "$warmup" --runs "$runs" --style basic \
        --export-json "$results_dir/filtered-gzip.json" \
        --export-markdown "$results_dir/filtered-gzip.md" \
        --command-name "Kent chainNet + Perl, gzip pipeline" "$kent_gzip_pipeline" \
        --command-name "Rust fused reference-only, gzip (1 thread)" "$rust_gzip_fused_t1" \
        --command-name "Rust fused reference-only, gzip ($threads threads)" \
        "$rust_gzip_fused_parallel"
fi

rss_file="$results_dir/rss.tsv"
printf '' > "$rss_file"
measure_rss() {
    local label=$1
    local command=$2
    local run
    for ((run = 1; run <= rss_runs; run++)); do
        /usr/bin/time -q -a -o "$rss_file" -f "$label\t$run\t%M" \
            bash -c "$command" >/dev/null 2>&1
    done
}

measure_rss raw_kent "$kent_raw"
measure_rss raw_rust_both_t1 "$rust_both_t1"
measure_rss raw_rust_both_parallel "$rust_both_parallel"
measure_rss raw_rust_reference_t1 "$rust_reference_t1"
measure_rss raw_rust_reference_parallel "$rust_reference_parallel"
measure_rss filtered_kent_perl "$kent_pipeline"
measure_rss filtered_rust_t1 "$rust_fused_t1"
measure_rss filtered_rust_parallel "$rust_fused_parallel"
if [[ -n "$gzip_chain" ]]; then
    measure_rss raw_gzip_kent "$kent_gzip_raw"
    measure_rss raw_gzip_rust_both_t1 "$rust_gzip_both_t1"
    measure_rss raw_gzip_rust_both_parallel "$rust_gzip_both_parallel"
    measure_rss raw_gzip_rust_reference_t1 "$rust_gzip_reference_t1"
    measure_rss raw_gzip_rust_reference_parallel "$rust_gzip_reference_parallel"
    measure_rss filtered_gzip_kent_perl "$kent_gzip_pipeline"
    measure_rss filtered_gzip_rust_t1 "$rust_gzip_fused_t1"
    measure_rss filtered_gzip_rust_parallel "$rust_gzip_fused_parallel"
fi

# Recheck the exact artifacts produced by the final timed invocations.
cmp "$work_dir/bench-kent.raw.net" "$work_dir/bench-rust-both-t1.reference.net"
cmp "$work_dir/bench-kent.raw.net" "$work_dir/bench-rust-both-parallel.reference.net"
cmp "$work_dir/bench-kent.raw.net" "$work_dir/bench-rust-reference-t1.raw.net"
cmp "$work_dir/bench-kent.raw.net" "$work_dir/bench-rust-reference-parallel.raw.net"
cmp "$work_dir/bench-kent.pipeline.filtered.net" "$work_dir/bench-rust-t1.fused.net"
cmp "$work_dir/bench-kent.pipeline.filtered.net" "$work_dir/bench-rust-parallel.fused.net"
if [[ -n "$gzip_chain" ]]; then
    cmp "$work_dir/bench-kent.raw.net" "$work_dir/bench-kent-gzip.raw.net"
    cmp "$work_dir/bench-kent.raw.net" "$work_dir/bench-rust-gzip-both-t1.reference.net"
    cmp "$work_dir/bench-kent.raw.net" "$work_dir/bench-rust-gzip-both-parallel.reference.net"
    cmp "$work_dir/bench-kent.raw.net" "$work_dir/bench-rust-gzip-reference-t1.raw.net"
    cmp "$work_dir/bench-kent.raw.net" "$work_dir/bench-rust-gzip-reference-parallel.raw.net"
    cmp "$work_dir/bench-kent.pipeline.filtered.net" \
        "$work_dir/bench-kent-gzip.pipeline.filtered.net"
    cmp "$work_dir/bench-kent.pipeline.filtered.net" "$work_dir/bench-rust-gzip-t1.fused.net"
    cmp "$work_dir/bench-kent.pipeline.filtered.net" \
        "$work_dir/bench-rust-gzip-parallel.fused.net"
fi

sha256sum \
    "$kent_chain_net" \
    "$net_filter" \
    "$input_chain" \
    "$work_dir/bench-kent.raw.net" \
    "$work_dir/bench-kent.pipeline.filtered.net" \
    > "$results_dir/sha256.txt"
if [[ -n "$gzip_chain" ]]; then
    sha256sum "$gzip_chain" >> "$results_dir/sha256.txt"
fi

echo "parity checks and benchmarks completed: $results_dir"
