// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the GNU General Public License v3.0.

//! `netools sort` — order sections and, optionally, sibling records.

use std::process::ExitCode;

use clap::ValueEnum;

use crate::Writer;
use crate::cli::common::{CliResult, Ctx, Output, natural_cmp, read_input};

/// Section ordering.
#[derive(Debug, Clone, Copy, ValueEnum, Default)]
pub enum NetsOrder {
    /// Keep input order.
    Preserve,
    /// Lexicographic by reference name.
    Lexicographic,
    /// Natural order (chr1 < chr2 < chr10).
    #[default]
    Natural,
}

/// Record ordering within a section.
#[derive(Debug, Clone, Copy, ValueEnum, Default)]
pub enum RecordsOrder {
    /// Keep input order.
    #[default]
    Preserve,
    /// Stable-sort each sibling list by the canonical record key.
    Reference,
}

/// Sort a NET file.
#[derive(Debug, clap::Args)]
pub struct Args {
    /// Input file (`-` for stdin).
    pub input: String,
    /// Output file (`-` for stdout).
    #[arg(long, short, default_value = "-")]
    pub output: String,
    /// Section ordering.
    #[arg(long, value_enum, default_value_t = NetsOrder::Natural)]
    pub nets: NetsOrder,
    /// Record ordering within each section.
    #[arg(long, value_enum, default_value_t = RecordsOrder::Preserve)]
    pub records: RecordsOrder,
    /// Validate the result before writing.
    #[arg(long)]
    pub validate: bool,
}

pub fn run(args: Args, ctx: &Ctx) -> CliResult<ExitCode> {
    let reader = read_input(&args.input, ctx)?;

    let mut order: Vec<usize> = (0..reader.len()).collect();
    match args.nets {
        NetsOrder::Preserve => {}
        NetsOrder::Lexicographic => order.sort_by(|&a, &b| {
            reader
                .net(a)
                .unwrap()
                .reference_name_bytes()
                .cmp(reader.net(b).unwrap().reference_name_bytes())
        }),
        NetsOrder::Natural => order.sort_by(|&a, &b| {
            natural_cmp(
                reader.net(a).unwrap().reference_name_bytes(),
                reader.net(b).unwrap().reference_name_bytes(),
            )
        }),
    }

    let mut out = Output::create(&args.output, ctx.gzip)?;
    {
        let mut writer = Writer::new(&mut out);
        for &i in &order {
            let net = reader.net(i).unwrap();
            match args.records {
                RecordsOrder::Preserve => writer.write_net(net)?,
                RecordsOrder::Reference => {
                    let sorted = net.sort_records();
                    if args.validate
                        && sorted
                            .validate(crate::ValidationMode::Compatible)
                            .has_errors()
                    {
                        return Err(format!(
                            "sorted section `{}` failed validation",
                            net.reference_name().to_string_lossy()
                        )
                        .into());
                    }
                    writer.write_net(sorted.as_ref())?;
                }
            }
        }
    }
    out.finish()?;
    Ok(ExitCode::SUCCESS)
}
