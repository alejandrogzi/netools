// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the GNU General Public License v3.0.

//! `netools filter` — predicate-based, tree-aware filtering.

use std::collections::HashSet;
use std::process::ExitCode;

use clap::ValueEnum;

use crate::cli::common::{CliResult, Ctx, Output, read_input};
use crate::{NetPredicate, NetRange, NodeKind, Strand, TreeFilterPolicy, Writer};

/// Tree-reshaping policy.
#[derive(Debug, Clone, Copy, ValueEnum, Default)]
pub enum TreePolicyArg {
    /// Remove failing records and their subtrees (UCSC-compatible).
    #[default]
    Prune,
    /// Keep matches and the ancestors on their paths.
    RetainAncestors,
    /// Reattach retained descendants to the nearest retained ancestor.
    Promote,
}

impl From<TreePolicyArg> for TreeFilterPolicy {
    fn from(value: TreePolicyArg) -> TreeFilterPolicy {
        match value {
            TreePolicyArg::Prune => TreeFilterPolicy::Prune,
            TreePolicyArg::RetainAncestors => TreeFilterPolicy::RetainAncestors,
            TreePolicyArg::Promote => TreeFilterPolicy::Promote,
        }
    }
}

/// Filter records by predicate.
#[derive(Debug, clap::Args)]
pub struct Args {
    /// Input file (`-` for stdin).
    pub input: String,
    /// Output file (`-` for stdout).
    #[arg(long, short, default_value = "-")]
    pub output: String,
    /// Keep only records of this kind (`fill` or `gap`).
    #[arg(long)]
    pub kind: Option<String>,
    /// Minimum score.
    #[arg(long)]
    pub min_score: Option<f64>,
    /// Maximum score.
    #[arg(long)]
    pub max_score: Option<f64>,
    /// Minimum aligned bases.
    #[arg(long)]
    pub min_ali: Option<u32>,
    /// Comma-separated list of accepted `type` values.
    #[arg(long, value_delimiter = ',')]
    pub r#type: Vec<String>,
    /// Comma-separated list of accepted chain ids.
    #[arg(long, value_delimiter = ',')]
    pub chain_id: Vec<u64>,
    /// Comma-separated list of accepted query names.
    #[arg(long, value_delimiter = ',')]
    pub query_name: Vec<String>,
    /// Accepted strand (`+` or `-`).
    #[arg(long)]
    pub strand: Option<char>,
    /// Minimum depth.
    #[arg(long)]
    pub min_depth: Option<u16>,
    /// Maximum depth.
    #[arg(long)]
    pub max_depth: Option<u16>,
    /// Restrict to records overlapping `START-END`.
    #[arg(long)]
    pub region: Option<String>,
    /// Tree-reshaping policy.
    #[arg(long, value_enum, default_value_t = TreePolicyArg::Prune)]
    pub tree_policy: TreePolicyArg,
}

pub fn run(args: Args, ctx: &Ctx) -> CliResult<ExitCode> {
    let reader = read_input(&args.input, ctx)?;
    let predicate = build_predicate(&args)?;
    let policy: TreeFilterPolicy = args.tree_policy.into();

    let mut out = Output::create(&args.output, ctx.gzip)?;
    {
        let mut writer = Writer::new(&mut out);
        for net in reader.nets() {
            let filtered = net.filter(&predicate, policy);
            writer.write_net(filtered.as_ref())?;
        }
    }
    out.finish()?;
    Ok(ExitCode::SUCCESS)
}

fn build_predicate(args: &Args) -> CliResult<NetPredicate> {
    let mut p = NetPredicate::new();

    if let Some(kind) = &args.kind {
        let k = match kind.as_str() {
            "fill" => NodeKind::Fill,
            "gap" => NodeKind::Gap,
            other => return Err(format!("unknown kind `{other}`").into()),
        };
        p.kinds = Some([k].into_iter().collect());
    }
    p.min_score = args.min_score;
    p.max_score = args.max_score;
    p.min_ali = args.min_ali;
    if !args.r#type.is_empty() {
        p.types = Some(args.r#type.iter().map(|t| t.as_bytes().to_vec()).collect());
    }
    if !args.chain_id.is_empty() {
        p.chain_ids = Some(args.chain_id.iter().copied().collect::<HashSet<_>>());
    }
    if !args.query_name.is_empty() {
        p.query_names = Some(
            args.query_name
                .iter()
                .map(|n| n.as_bytes().to_vec())
                .collect(),
        );
    }
    if let Some(strand) = args.strand {
        p.strand = Some(match strand {
            '+' => Strand::Forward,
            '-' => Strand::Reverse,
            other => return Err(format!("invalid strand `{other}`").into()),
        });
    }
    p.min_depth = args.min_depth;
    p.max_depth = args.max_depth;
    if let Some(region) = &args.region {
        let (start, end) = region.split_once('-').ok_or("region must be START-END")?;
        p.reference_overlap = Some(NetRange::new(start.trim().parse()?, end.trim().parse()?));
    }

    Ok(p)
}
