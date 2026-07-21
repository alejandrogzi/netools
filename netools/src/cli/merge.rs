// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the GNU General Public License v3.0.

//! `netools merge` — combine complete NET files.

use std::collections::HashMap;
use std::process::ExitCode;

use clap::ValueEnum;

use crate::cli::common::{CliResult, Ctx, Output, natural_cmp, read_input};
use crate::{NetRef, Reader, Writer};

/// How to handle sections with duplicate reference names.
#[derive(Debug, Clone, Copy, ValueEnum, Default)]
pub enum DuplicatePolicy {
    /// Fail on any duplicate reference name (default).
    #[default]
    Error,
    /// Keep the first section seen for a name.
    KeepFirst,
    /// Keep the last section seen for a name.
    KeepLast,
    /// Keep every section, even with duplicate names.
    KeepAll,
}

/// Merge several NET files.
#[derive(Debug, clap::Args)]
pub struct Args {
    /// Input files (`-` for stdin).
    #[arg(required = true)]
    pub inputs: Vec<String>,
    /// Output file (`-` for stdout).
    #[arg(long, short, default_value = "-")]
    pub output: String,
    /// Duplicate-reference policy.
    #[arg(long, value_enum, default_value_t = DuplicatePolicy::Error)]
    pub duplicates: DuplicatePolicy,
    /// Sort sections by reference name (natural order) in the output.
    #[arg(long)]
    pub sort: bool,
}

pub fn run(args: Args, ctx: &Ctx) -> CliResult<ExitCode> {
    // Keep every reader alive so borrowed sections remain valid.
    let readers: Vec<Reader> = args
        .inputs
        .iter()
        .map(|path| read_input(path, ctx))
        .collect::<crate::Result<Vec<_>>>()?;

    // Collect sections in input order, applying the duplicate policy.
    let mut selected: Vec<NetRef<'_>> = Vec::new();
    let mut index_by_name: HashMap<Vec<u8>, usize> = HashMap::new();

    use std::collections::hash_map::Entry;
    for reader in &readers {
        for net in reader.nets() {
            let name = net.reference_name_bytes().to_vec();
            match args.duplicates {
                DuplicatePolicy::KeepAll => selected.push(net),
                DuplicatePolicy::Error => match index_by_name.entry(name) {
                    Entry::Vacant(slot) => {
                        slot.insert(selected.len());
                        selected.push(net);
                    }
                    Entry::Occupied(_) => {
                        return Err(format!(
                            "duplicate reference `{}`; use --duplicates to resolve",
                            net.reference_name().to_string_lossy()
                        )
                        .into());
                    }
                },
                DuplicatePolicy::KeepFirst => {
                    if let Entry::Vacant(slot) = index_by_name.entry(name) {
                        slot.insert(selected.len());
                        selected.push(net);
                    }
                }
                DuplicatePolicy::KeepLast => match index_by_name.entry(name) {
                    Entry::Occupied(slot) => selected[*slot.get()] = net,
                    Entry::Vacant(slot) => {
                        slot.insert(selected.len());
                        selected.push(net);
                    }
                },
            }
        }
    }

    if args.sort {
        selected.sort_by(|a, b| natural_cmp(a.reference_name_bytes(), b.reference_name_bytes()));
    }

    let mut out = Output::create(&args.output, ctx.gzip)?;
    {
        let mut writer = Writer::new(&mut out);
        for net in &selected {
            writer.write_net(*net)?;
        }
    }
    out.finish()?;
    Ok(ExitCode::SUCCESS)
}
