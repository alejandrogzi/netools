// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the GNU General Public License v3.0.

//! `netools validate` — report structural issues.

use std::fmt::Write as _;
use std::process::ExitCode;

use crate::cli::common::{CliResult, Ctx, ValidationModeArg, read_input, write_stdout};

/// Validate a NET file.
#[derive(Debug, clap::Args)]
pub struct Args {
    /// Input file (`-` for stdin).
    pub input: String,
    /// Validation strictness.
    #[arg(long, value_enum, default_value_t = ValidationModeArg::Strict)]
    pub mode: ValidationModeArg,
    /// Emit tab-separated output instead of human-readable text.
    #[arg(long)]
    pub tsv: bool,
    /// Exit non-zero when warnings (not just errors) are present.
    #[arg(long)]
    pub error_on_warning: bool,
}

pub fn run(args: Args, ctx: &Ctx) -> CliResult<ExitCode> {
    let reader = read_input(&args.input, ctx)?;
    let report = reader.validate(args.mode.into());

    let mut out = String::new();
    if args.tsv {
        for issue in report.issues() {
            let _ = writeln!(
                out,
                "{}\t{}\t{}\t{}\t{}",
                match issue.severity {
                    crate::Severity::Error => "error",
                    crate::Severity::Warning => "warning",
                    crate::Severity::Info => "info",
                },
                issue.code.as_str(),
                issue.net.get(),
                issue.node.map_or(-1i64, |n| n.get() as i64),
                issue.message,
            );
        }
    } else {
        for issue in report.issues() {
            let node = issue
                .node
                .map_or_else(|| "-".to_string(), |n| n.get().to_string());
            let _ = writeln!(
                out,
                "[{:?}] {} (net {}, node {}): {}",
                issue.severity,
                issue.code.as_str(),
                issue.net.get(),
                node,
                issue.message,
            );
        }
        eprintln!(
            "{} error(s), {} warning(s)",
            report.error_count(),
            report.warning_count()
        );
    }
    write_stdout(&out)?;

    let fail = report.has_errors() || (args.error_on_warning && report.warning_count() > 0);
    Ok(if fail {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    })
}
