// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the GNU General Public License v3.0.

//! Command-line interface.
//!
//! Enabled by the `cli` feature. Each subcommand lives in its own module and
//! shares helpers from [`common`].

pub mod common;

pub mod chain_net;
pub mod filter;
pub mod merge;
pub mod sort;
pub mod split;
pub mod stats;
pub mod validate;
pub mod view;

use std::process::ExitCode;

use clap::{Parser, Subcommand};

use common::{Ctx, LogLevel, ParseModeArg};

/// `netools` — read, validate, transform, and analyse UCSC `.net` files.
#[derive(Debug, Parser)]
#[command(name = "netools", version, about, long_about = None)]
pub struct Cli {
    /// Worker threads for parallel parsing (unset or 0 = automatic).
    #[arg(long, global = true)]
    threads: Option<usize>,

    /// Logging verbosity.
    #[arg(long, global = true, value_enum, default_value_t = LogLevel::Info)]
    log_level: LogLevel,

    /// Parsing strictness.
    #[arg(long, global = true, value_enum, default_value_t = ParseModeArg::Compatible)]
    parse_mode: ParseModeArg,

    /// Force gzip compression for outputs.
    #[arg(long, global = true)]
    gzip: bool,

    /// Never gzip outputs (overrides `--gzip` and `.gz` inference).
    #[arg(long = "no-gzip", global = true)]
    no_gzip: bool,

    #[command(subcommand)]
    command: Command,
}

/// Top-level subcommands.
#[derive(Debug, Subcommand)]
enum Command {
    /// Construct reference/query NETs from score-sorted chains.
    Net(chain_net::Args),
    /// Report structural issues.
    Validate(validate::Args),
    /// Inspect selected records.
    View(view::Args),
    /// Summary statistics.
    Stats(stats::Args),
    /// Split into one file per section.
    Split(split::Args),
    /// Order sections and sibling records.
    Sort(sort::Args),
    /// Filter records by predicate.
    Filter(filter::Args),
    /// Merge several files.
    Merge(merge::Args),
}

impl Cli {
    /// Run the parsed command.
    pub fn run(self) -> common::CliResult<ExitCode> {
        let _ = simple_logger::SimpleLogger::new()
            .with_level(self.log_level.into())
            .init();

        if let Some(threads) = self.threads
            && threads > 0
        {
            let _ = rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .build_global();
        }

        let gzip = if self.no_gzip {
            Some(false)
        } else if self.gzip {
            Some(true)
        } else {
            None
        };
        let ctx = Ctx {
            parse_mode: self.parse_mode.into(),
            gzip,
            parallel: self.threads != Some(1),
            threads: self.threads,
        };

        match self.command {
            Command::Net(args) => chain_net::run(args, &ctx),
            Command::Validate(args) => validate::run(args, &ctx),
            Command::View(args) => view::run(args, &ctx),
            Command::Stats(args) => stats::run(args, &ctx),
            Command::Split(args) => split::run(args, &ctx),
            Command::Sort(args) => sort::run(args, &ctx),
            Command::Filter(args) => filter::run(args, &ctx),
            Command::Merge(args) => merge::run(args, &ctx),
        }
    }
}

/// Parse arguments and run, mapping errors to an exit code.
pub fn main() -> ExitCode {
    match Cli::parse().run() {
        Ok(code) => code,
        Err(err) => {
            // A downstream reader that closed the pipe (e.g. `… | head`) is not
            // a failure of this tool.
            if err.to_string().to_ascii_lowercase().contains("broken pipe") {
                return ExitCode::SUCCESS;
            }
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}
