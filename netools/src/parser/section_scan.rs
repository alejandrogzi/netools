// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the GNU General Public License v3.0.

//! Section boundary detection for parallel parsing.
//!
//! A NET file separates into independent sections at every `net` header that
//! begins at column zero. Scanning for these boundaries is a cheap newline walk
//! that does not tokenise record lines.

use crate::parser::common::strip_cr;

/// A section's byte span and starting line, in file order.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SectionSpan {
    /// Inclusive byte offset of the header line.
    pub(crate) start: usize,
    /// Exclusive byte offset where the next section (or EOF) begins.
    pub(crate) end: usize,
    /// 1-based line number of the header line.
    pub(crate) start_line: u64,
}

/// Whether a raw line (already stripped of a trailing CR) is a section header.
#[inline]
fn is_header_line(line: &[u8]) -> bool {
    line.len() >= 3 && &line[0..3] == b"net" && (line.len() == 3 || matches!(line[3], b' ' | b'\t'))
}

/// Locate every section span in `bytes`, in file order.
pub(crate) fn scan_sections(bytes: &[u8]) -> Vec<SectionSpan> {
    let mut headers: Vec<(usize, u64)> = Vec::new();
    let mut pos = 0usize;
    let mut line_no = 1u64;
    while pos < bytes.len() {
        let line_end = match memchr::memchr(b'\n', &bytes[pos..]) {
            Some(i) => pos + i,
            None => bytes.len(),
        };
        let line = strip_cr(&bytes[pos..line_end]);
        if is_header_line(line) {
            headers.push((pos, line_no));
        }
        pos = line_end + 1;
        line_no += 1;
    }

    let mut spans = Vec::with_capacity(headers.len());
    for (i, &(start, start_line)) in headers.iter().enumerate() {
        let end = headers.get(i + 1).map_or(bytes.len(), |&(next, _)| next);
        spans.push(SectionSpan {
            start,
            end,
            start_line,
        });
    }
    spans
}
