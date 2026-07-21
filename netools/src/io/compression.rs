// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the GNU General Public License v3.0.

//! Compression detection and gzip decompression.
//!
//! Gzip is detected by magic bytes rather than filename so that piped and
//! mis-named inputs are handled correctly. With an ordinary gzip stream true
//! byte-range random access is not available; the stream is fully decompressed
//! into an owned buffer before independent NET sections are parsed.

use crate::parser::error::{NetError, Result};

/// Whether `bytes` begins with the gzip magic number.
#[inline]
pub(crate) fn is_gzip(bytes: &[u8]) -> bool {
    bytes.len() >= 2 && bytes[0] == 0x1f && bytes[1] == 0x8b
}

/// Decompress a (possibly multi-member) gzip stream into an owned buffer.
#[cfg(feature = "gzip")]
pub(crate) fn decompress_gzip(data: &[u8]) -> Result<Vec<u8>> {
    use std::io::Read;

    let mut decoder = flate2::read::MultiGzDecoder::new(data);
    let mut out = Vec::with_capacity(data.len().saturating_mul(4));
    decoder.read_to_end(&mut out).map_err(NetError::from)?;
    Ok(out)
}

/// Report gzip input when the feature is disabled.
#[cfg(not(feature = "gzip"))]
pub(crate) fn decompress_gzip(_data: &[u8]) -> Result<Vec<u8>> {
    use crate::parser::error::NetErrorKind;
    Err(NetError::new(NetErrorKind::UnsupportedCompression, 0, 0, 0)
        .with_context(b"input is gzip-compressed but the `gzip` feature is disabled"))
}
