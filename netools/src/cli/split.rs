// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the GNU General Public License v3.0.

//! `netools split` — one file per section.

use std::collections::HashSet;
use std::io::Write;
use std::path::Path;
use std::process::ExitCode;

use crate::Writer;
use crate::cli::common::{CliResult, Ctx, Output, read_input, sanitize_filename};

/// Split a NET file into one file per section.
#[derive(Debug, clap::Args)]
pub struct Args {
    /// Input file (`-` for stdin).
    pub input: String,
    /// Output directory.
    #[arg(long, default_value = "split")]
    pub out_dir: String,
    /// Filename suffix for each section.
    #[arg(long, default_value = ".net")]
    pub suffix: String,
    /// Gzip-compress each output file.
    #[arg(long)]
    pub gzip: bool,
    /// Write a manifest TSV to this path.
    #[arg(long)]
    pub manifest: Option<String>,
}

pub fn run(args: Args, ctx: &Ctx) -> CliResult<ExitCode> {
    let reader = read_input(&args.input, ctx)?;
    std::fs::create_dir_all(&args.out_dir)?;

    let suffix = if args.gzip && !args.suffix.ends_with(".gz") {
        format!("{}.gz", args.suffix)
    } else {
        args.suffix.clone()
    };

    let mut used: HashSet<String> = HashSet::new();
    let mut manifest_lines: Vec<String> = Vec::new();

    for net in reader.nets() {
        let base = sanitize_filename(net.reference_name_bytes());
        let mut name = format!("{base}{suffix}");
        let mut counter = 1;
        while used.contains(&name) {
            name = format!("{base}.{counter}{suffix}");
            counter += 1;
        }
        used.insert(name.clone());

        let path = Path::new(&args.out_dir).join(&name);
        let path_str = path.to_string_lossy().into_owned();

        // Atomic-ish write: write to a temporary then rename.
        let tmp = format!("{path_str}.tmp");
        {
            let gzip = if args.gzip { Some(true) } else { Some(false) };
            let mut out = Output::create(&tmp, gzip)?;
            {
                let mut writer = Writer::new(&mut out);
                writer.write_net(net)?;
            }
            out.finish()?;
        }
        std::fs::rename(&tmp, &path)?;

        manifest_lines.push(format!(
            "{}\t{}\t{}\t{}",
            net.reference_name().to_string_lossy(),
            net.reference_size(),
            path_str,
            net.len(),
        ));
    }

    if let Some(manifest_path) = &args.manifest {
        let mut file = std::fs::File::create(manifest_path)?;
        writeln!(file, "reference_name\treference_size\toutput_file\trecords")?;
        for line in &manifest_lines {
            writeln!(file, "{line}")?;
        }
    }

    eprintln!("wrote {} section(s) to {}", used.len(), args.out_dir);
    Ok(ExitCode::SUCCESS)
}
