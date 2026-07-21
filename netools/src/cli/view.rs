// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the GNU General Public License v3.0.

//! `netools view` — inspect selected records.

use std::io::Write;
use std::process::ExitCode;

use crate::cli::common::{CliResult, Ctx, Output, read_input};
use crate::{NetPredicate, NetRange, NetRef, TreeFilterPolicy, Writer};

/// View selected sections/records.
#[derive(Debug, clap::Args)]
pub struct Args {
    /// Input file (`-` for stdin).
    pub input: String,
    /// Output file (`-` for stdout).
    #[arg(long, short, default_value = "-")]
    pub output: String,
    /// Restrict to one reference sequence.
    #[arg(long)]
    pub reference: Option<String>,
    /// Restrict to records of one chain id (keeps their paths).
    #[arg(long)]
    pub chain_id: Option<u64>,
    /// Restrict to records overlapping `START-END` on the reference.
    #[arg(long)]
    pub region: Option<String>,
    /// Maximum depth to show.
    #[arg(long)]
    pub max_depth: Option<u16>,
    /// Emit a flat TSV with explicit depth and parent ids.
    #[arg(long)]
    pub flat: bool,
}

pub fn run(args: Args, ctx: &Ctx) -> CliResult<ExitCode> {
    let reader = read_input(&args.input, ctx)?;

    let region = args.region.as_deref().map(parse_region).transpose()?;

    let mut predicate = NetPredicate::new();
    let mut needs_filter = false;
    if let Some(chain) = args.chain_id {
        predicate.chain_ids = Some([chain].into_iter().collect());
        needs_filter = true;
    }
    if let Some(r) = region {
        predicate.reference_overlap = Some(r);
        needs_filter = true;
    }
    let mut depth_only = false;
    if let Some(max) = args.max_depth {
        predicate.max_depth = Some(max);
        needs_filter = true;
        depth_only = args.chain_id.is_none() && region.is_none();
    }

    let policy = if depth_only {
        TreeFilterPolicy::Prune
    } else {
        TreeFilterPolicy::RetainAncestors
    };

    let mut out = Output::create(&args.output, ctx.gzip)?;

    for net in reader.nets() {
        if let Some(name) = &args.reference
            && net.reference_name_bytes() != name.as_bytes()
        {
            continue;
        }
        if needs_filter {
            let owned = net.filter(&predicate, policy);
            emit(owned.as_ref(), args.flat, &mut out)?;
        } else {
            emit(net, args.flat, &mut out)?;
        }
    }

    out.finish()?;
    Ok(ExitCode::SUCCESS)
}

fn emit(net: NetRef<'_>, flat: bool, out: &mut Output) -> CliResult<()> {
    if flat {
        let name = net.reference_name().to_string_lossy().into_owned();
        for node in net.preorder() {
            let parent = node.parent().map_or(-1i64, |p| p.id().get() as i64);
            writeln!(
                out,
                "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                name,
                node.depth(),
                node.kind(),
                node.reference_start(),
                node.reference_end(),
                node.query_name().to_string_lossy(),
                node.query_strand(),
                node.query_start(),
                node.query_end(),
                node.chain_id().map_or(-1i64, |c| c as i64),
                parent,
            )?;
        }
    } else {
        let mut writer = Writer::new(&mut *out);
        writer.write_net(net)?;
    }
    Ok(())
}

fn parse_region(text: &str) -> CliResult<NetRange> {
    let (start, end) = text.split_once('-').ok_or("region must be START-END")?;
    let start: u32 = start.trim().parse()?;
    let end: u32 = end.trim().parse()?;
    if end < start {
        return Err("region end precedes start".into());
    }
    Ok(NetRange::new(start, end))
}
