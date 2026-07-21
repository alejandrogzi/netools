// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the GNU General Public License v3.0.

//! `netools stats` — summary statistics.

use std::fmt::Write as _;
use std::process::ExitCode;

use crate::algorithms::Stats;
use crate::cli::common::{CliResult, Ctx, read_input, write_stdout};

/// Print summary statistics.
#[derive(Debug, clap::Args)]
pub struct Args {
    /// Input file (`-` for stdin).
    pub input: String,
}

pub fn run(args: Args, ctx: &Ctx) -> CliResult<ExitCode> {
    let reader = read_input(&args.input, ctx)?;
    let stats = Stats::compute(&reader);

    // Build the whole report first, then write it once (tolerating a closed pipe).
    let mut out = String::new();
    let _ = writeln!(out, "reference nets       {}", stats.nets);
    let _ = writeln!(out, "records              {}", stats.records);
    let _ = writeln!(out, "  fills              {}", stats.fills);
    let _ = writeln!(out, "  gaps               {}", stats.gaps);
    let _ = writeln!(out, "max depth            {}", stats.max_depth);
    for (depth, count) in stats.records_per_depth.iter().enumerate() {
        let _ = writeln!(out, "  depth {depth:<12} {count}");
    }
    let _ = writeln!(out, "distinct chain ids   {}", stats.distinct_chain_ids);
    let _ = writeln!(out, "distinct query names {}", stats.distinct_query_names);
    let _ = writeln!(out, "reference bases      {}", stats.reference_bases);
    let _ = writeln!(out, "query bases          {}", stats.query_bases);
    if !stats.type_counts.is_empty() {
        let _ = writeln!(out, "types:");
        for (ty, count) in &stats.type_counts {
            let _ = writeln!(out, "  {ty:<18} {count}");
        }
    }
    if !stats.unknown_attribute_names.is_empty() {
        let _ = writeln!(out, "unknown attributes:");
        for (name, count) in &stats.unknown_attribute_names {
            let _ = writeln!(out, "  {name:<18} {count}");
        }
    }

    write_stdout(&out)?;
    Ok(ExitCode::SUCCESS)
}
