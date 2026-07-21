// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the GNU General Public License v3.0.

//! Shared CLI helpers: argument enums, input reading, and output sinks.

use std::fs::File;
use std::io::{self, BufWriter, Read, Write};

use clap::ValueEnum;

use crate::{Net, ParseMode, Reader, ValidationMode};

/// Error type used across CLI commands.
pub type CliResult<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// Parse-mode choice.
#[derive(Debug, Clone, Copy, ValueEnum, Default)]
pub enum ParseModeArg {
    /// Widest acceptance.
    Permissive,
    /// The default.
    #[default]
    Compatible,
    /// Strict syntax.
    Strict,
}

impl From<ParseModeArg> for ParseMode {
    fn from(value: ParseModeArg) -> ParseMode {
        match value {
            ParseModeArg::Permissive => ParseMode::Permissive,
            ParseModeArg::Compatible => ParseMode::Compatible,
            ParseModeArg::Strict => ParseMode::Strict,
        }
    }
}

/// Validation-mode choice.
#[derive(Debug, Clone, Copy, ValueEnum, Default)]
pub enum ValidationModeArg {
    /// Only parse-implied checks.
    Syntax,
    /// Structural anomalies as warnings.
    #[default]
    Compatible,
    /// Structural anomalies as errors.
    Strict,
}

impl From<ValidationModeArg> for ValidationMode {
    fn from(value: ValidationModeArg) -> ValidationMode {
        match value {
            ValidationModeArg::Syntax => ValidationMode::Syntax,
            ValidationModeArg::Compatible => ValidationMode::Compatible,
            ValidationModeArg::Strict => ValidationMode::Strict,
        }
    }
}

/// Log-level choice.
#[derive(Debug, Clone, Copy, ValueEnum, Default)]
pub enum LogLevel {
    /// Errors only.
    Error,
    /// Warnings and errors.
    Warn,
    /// Informational (default).
    #[default]
    Info,
    /// Debug output.
    Debug,
    /// Trace output.
    Trace,
}

impl From<LogLevel> for log::LevelFilter {
    fn from(value: LogLevel) -> log::LevelFilter {
        match value {
            LogLevel::Error => log::LevelFilter::Error,
            LogLevel::Warn => log::LevelFilter::Warn,
            LogLevel::Info => log::LevelFilter::Info,
            LogLevel::Debug => log::LevelFilter::Debug,
            LogLevel::Trace => log::LevelFilter::Trace,
        }
    }
}

/// Resolved global options passed to each command.
#[derive(Debug, Clone, Copy)]
pub struct Ctx {
    /// Parsing strictness.
    pub parse_mode: ParseMode,
    /// Output gzip preference: `Some(true)`/`Some(false)` force, `None` infers.
    pub gzip: Option<bool>,
    /// Whether to parse in parallel.
    pub parallel: bool,
}

/// Read a reader from a path or stdin (`-`).
pub fn read_input(path: &str, ctx: &Ctx) -> crate::Result<Reader<Net>> {
    let builder = Reader::options()
        .parse_mode(ctx.parse_mode)
        .parallel(ctx.parallel);
    if path == "-" {
        let mut data = Vec::new();
        io::stdin()
            .read_to_end(&mut data)
            .map_err(crate::NetError::from)?;
        builder.from_owned_bytes(data)
    } else {
        builder.from_path(path)
    }
}

/// An output sink over stdout or a file, optionally gzip-compressed.
pub enum Output {
    /// Plain stdout.
    Stdout(BufWriter<io::Stdout>),
    /// Plain file.
    File(BufWriter<File>),
    /// Gzip-compressed file.
    GzFile(flate2::write::GzEncoder<BufWriter<File>>),
    /// Gzip-compressed stdout.
    GzStdout(flate2::write::GzEncoder<BufWriter<io::Stdout>>),
}

impl Output {
    /// Create an output sink for `path` (`-` for stdout).
    pub fn create(path: &str, gzip: Option<bool>) -> io::Result<Output> {
        if path == "-" {
            let out = BufWriter::new(io::stdout());
            Ok(match gzip {
                Some(true) => Output::GzStdout(flate2::write::GzEncoder::new(
                    out,
                    flate2::Compression::default(),
                )),
                _ => Output::Stdout(out),
            })
        } else {
            let gz = gzip.unwrap_or_else(|| path.ends_with(".gz"));
            let out = BufWriter::new(File::create(path)?);
            Ok(if gz {
                Output::GzFile(flate2::write::GzEncoder::new(
                    out,
                    flate2::Compression::default(),
                ))
            } else {
                Output::File(out)
            })
        }
    }

    /// Flush and finalise (writes the gzip footer where applicable).
    pub fn finish(self) -> io::Result<()> {
        match self {
            Output::Stdout(mut w) => w.flush(),
            Output::File(mut w) => w.flush(),
            Output::GzFile(w) => w.finish().map(|_| ()),
            Output::GzStdout(w) => w.finish().map(|_| ()),
        }
    }
}

impl Write for Output {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Output::Stdout(w) => w.write(buf),
            Output::File(w) => w.write(buf),
            Output::GzFile(w) => w.write(buf),
            Output::GzStdout(w) => w.write(buf),
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        match self {
            Output::Stdout(w) => w.flush(),
            Output::File(w) => w.flush(),
            Output::GzFile(w) => w.flush(),
            Output::GzStdout(w) => w.flush(),
        }
    }
}

/// Write `text` to stdout, treating a closed pipe (e.g. `… | head`) as success
/// rather than panicking or erroring.
pub fn write_stdout(text: &str) -> CliResult<()> {
    let mut out = io::stdout().lock();
    match out.write_all(text.as_bytes()).and_then(|_| out.flush()) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        Err(e) => Err(e.into()),
    }
}

/// Natural-order comparison of two byte strings (numeric runs compared by value).
pub fn natural_cmp(a: &[u8], b: &[u8]) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        let (ca, cb) = (a[i], b[j]);
        if ca.is_ascii_digit() && cb.is_ascii_digit() {
            let start_a = i;
            let start_b = j;
            while i < a.len() && a[i].is_ascii_digit() {
                i += 1;
            }
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            // Compare numeric runs, ignoring leading zeros.
            let na = trim_zeros(&a[start_a..i]);
            let nb = trim_zeros(&b[start_b..j]);
            match na.len().cmp(&nb.len()).then_with(|| na.cmp(nb)) {
                Ordering::Equal => {}
                other => return other,
            }
        } else {
            match ca.cmp(&cb) {
                Ordering::Equal => {
                    i += 1;
                    j += 1;
                }
                other => return other,
            }
        }
    }
    a.len().cmp(&b.len())
}

fn trim_zeros(digits: &[u8]) -> &[u8] {
    let first = digits
        .iter()
        .position(|&d| d != b'0')
        .unwrap_or(digits.len());
    &digits[first..]
}

/// Sanitise a reference name into a safe filename component.
pub fn sanitize_filename(name: &[u8]) -> String {
    let mut out = String::with_capacity(name.len());
    for &b in name {
        let c = b as char;
        if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
            out.push(c);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        out.push_str("unnamed");
    }
    out
}
