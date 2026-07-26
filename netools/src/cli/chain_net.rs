// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the GNU General Public License v3.0.

//! `netools net`.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use chaintools::{Chain, Reader};
use clap::{Args as ClapArgs, ValueEnum};

use super::common::{CliResult, Ctx, Output};
use crate::chainnet::{ChainNetBuilder, ChainNetOptions, NetSide, NonNestedFilter, SizeSource};

/// Standalone chain-to-NET construction arguments.
#[derive(Debug, ClapArgs)]
pub struct Args {
    /// Score-sorted chain input. Omit to read from stdin.
    #[arg(long, value_name = "INPUT-CHAIN")]
    chain: Option<PathBuf>,

    /// Ordered reference sizes in UCSC two-column form.
    #[arg(
        long,
        value_name = "PATH",
        required_unless_present = "reference_2bit",
        conflicts_with = "reference_2bit"
    )]
    reference_sizes: Option<PathBuf>,

    /// Read ordered reference sizes from a 2bit index.
    #[arg(long, value_name = "PATH")]
    reference_2bit: Option<PathBuf>,

    /// Ordered query sizes in UCSC two-column form.
    #[arg(
        long,
        value_name = "PATH",
        required_unless_present = "query_2bit",
        conflicts_with = "query_2bit"
    )]
    query_sizes: Option<PathBuf>,

    /// Read ordered query sizes from a 2bit index.
    #[arg(long, value_name = "PATH")]
    query_2bit: Option<PathBuf>,

    /// Reference-side NET output (`-` for stdout).
    #[arg(long, value_name = "PATH")]
    reference_net: Option<String>,

    /// Query-side NET output (`-` for stdout).
    #[arg(long, value_name = "PATH")]
    query_net: Option<String>,

    /// Minimum available space retained for lower-ranking chains.
    #[arg(long, default_value_t = 25)]
    min_space: u32,

    /// Minimum fill span/aligned bases (default: half of `--min-space`).
    #[arg(long)]
    min_fill: Option<u32>,

    /// Minimum chain/proportional fill score.
    #[arg(long)]
    min_score: Option<f64>,

    /// Include query names containing `_hap` or `_alt`.
    #[arg(long)]
    include_haplotypes: bool,

    /// Construct only the reference side.
    #[arg(long, conflicts_with = "query_only")]
    reference_only: bool,

    /// Construct only the query side.
    #[arg(long)]
    query_only: bool,

    /// Apply exact non-nested filtering at this displayed-score threshold.
    #[arg(long, value_name = "INT")]
    post_filter_min_score: Option<i64>,

    /// Compatibility preset.
    #[arg(long, value_enum)]
    preset: Option<Preset>,

    /// Write the raw pre-post-filter reference NET.
    #[arg(long, visible_alias = "raw-generated-net-out", value_name = "PATH")]
    raw_net_out: Option<String>,

    /// Write the raw pre-post-filter query NET.
    #[arg(long, value_name = "PATH")]
    raw_query_net_out: Option<String>,
}

/// Named option presets.
#[derive(Debug, Clone, Copy, ValueEnum)]
enum Preset {
    /// Exact chainCleaner generation: reference-only, minScore 0, then minScore1 3000.
    Ccr,
}

pub fn run(args: Args, ctx: &Ctx) -> CliResult<ExitCode> {
    let chain_path = args.chain.as_deref().unwrap_or_else(|| Path::new("-"));

    let mut options = match args.preset {
        Some(Preset::Ccr) => ChainNetOptions::chaincleaner_compatible(),
        None => ChainNetOptions::default(),
    };
    options.min_space = args.min_space;
    options.min_fill = args.min_fill.unwrap_or(args.min_space / 2);
    if let Some(score) = args.min_score {
        options.min_score = score;
    }
    if args.include_haplotypes {
        options.include_haplotypes = true;
    }
    if args.reference_only {
        options.side = NetSide::ReferenceOnly;
    } else if args.query_only {
        options.side = NetSide::QueryOnly;
    }
    if let Some(score) = args.post_filter_min_score {
        options.post_filter = Some(NonNestedFilter::MinScore { set1: score });
    }
    // The global CLI has already installed the requested pool. Preserve the
    // explicit sequential mode; otherwise use that global pool.
    options.threads = (ctx.threads == Some(1)).then_some(1);
    options.capture_raw = args.raw_net_out.is_some() || args.raw_query_net_out.is_some();

    require_outputs(&args, options.side)?;
    let reference_source = args
        .reference_sizes
        .clone()
        .map(SizeSource::ChromSizes)
        .or_else(|| args.reference_2bit.clone().map(SizeSource::TwoBit))
        .expect("clap requires reference sizes");
    let query_source = args
        .query_sizes
        .clone()
        .map(SizeSource::ChromSizes)
        .or_else(|| args.query_2bit.clone().map(SizeSource::TwoBit))
        .expect("clap requires query sizes");

    let preserve_metadata = options.post_filter.is_none() || options.capture_raw;
    let (reader, metadata) = read_chains(chain_path, ctx, preserve_metadata)?;
    let generated = ChainNetBuilder::new(options)
        .reference_sizes(reference_source)
        .query_sizes(query_source)
        .metadata_lines(metadata)
        .build(&reader)?;

    if let Some(path) = &args.reference_net {
        write_output(path, ctx, |output| generated.write_reference_to(output))?;
    }
    if let Some(path) = &args.query_net {
        write_output(path, ctx, |output| generated.write_query_to(output))?;
    }
    if let Some(path) = &args.raw_net_out {
        write_output(path, ctx, |output| generated.write_raw_reference_to(output))?;
    }
    if let Some(path) = &args.raw_query_net_out {
        write_output(path, ctx, |output| generated.write_raw_query_to(output))?;
    }
    Ok(ExitCode::SUCCESS)
}

fn require_outputs(args: &Args, side: NetSide) -> CliResult<()> {
    if matches!(side, NetSide::ReferenceOnly | NetSide::Both) && args.reference_net.is_none() {
        return Err("--reference-net is required for the selected side".into());
    }
    if matches!(side, NetSide::QueryOnly | NetSide::Both) && args.query_net.is_none() {
        return Err("--query-net is required for the selected side".into());
    }
    if args.raw_query_net_out.is_some() && side == NetSide::ReferenceOnly {
        return Err("--raw-query-net-out requires query-side construction".into());
    }
    Ok(())
}

fn read_chains(
    path: &Path,
    ctx: &Ctx,
    preserve_metadata: bool,
) -> CliResult<(Reader<Chain>, Vec<Vec<u8>>)> {
    if path == Path::new("-") {
        let mut data = Vec::new();
        std::io::stdin().read_to_end(&mut data)?;
        let reader = if ctx.parallel {
            Reader::<Chain>::from_owned_bytes_parallel(data)?
        } else {
            Reader::<Chain>::from_owned_bytes(data)?
        };
        let metadata = reader_metadata(&reader, preserve_metadata);
        return Ok((reader, metadata));
    }

    let reader = if ctx.parallel {
        Reader::<Chain>::from_path_parallel(path)?
    } else {
        Reader::<Chain>::from_path(path)?
    };
    let metadata = reader_metadata(&reader, preserve_metadata);
    Ok((reader, metadata))
}

fn reader_metadata(reader: &Reader<Chain>, preserve: bool) -> Vec<Vec<u8>> {
    if preserve {
        reader.metadata_lines().map(<[u8]>::to_vec).collect()
    } else {
        Vec::new()
    }
}

fn write_output<F>(path: &str, ctx: &Ctx, write: F) -> CliResult<()>
where
    F: FnOnce(&mut Output) -> crate::chainnet::Result<()>,
{
    let mut output = Output::create(path, ctx.gzip)?;
    write(&mut output)?;
    output.finish()?;
    Ok(())
}
