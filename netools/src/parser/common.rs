// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the GNU General Public License v3.0.

//! Low-level byte helpers shared by the parsers: checked numeric parsing,
//! indentation measurement, and whitespace tokenisation.
//!
//! None of these helpers allocate or convert whole lines to UTF-8.

/// Reason a numeric field could not be parsed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NumErr {
    /// The bytes were not a well-formed number.
    Invalid,
    /// The value overflowed the target type.
    Overflow,
}

/// Parse an unsigned 32-bit decimal integer with overflow checking.
pub(crate) fn parse_u32(bytes: &[u8]) -> Result<u32, NumErr> {
    if bytes.is_empty() {
        return Err(NumErr::Invalid);
    }
    let mut acc: u32 = 0;
    for &b in bytes {
        if !b.is_ascii_digit() {
            return Err(NumErr::Invalid);
        }
        acc = acc
            .checked_mul(10)
            .and_then(|v| v.checked_add((b - b'0') as u32))
            .ok_or(NumErr::Overflow)?;
    }
    Ok(acc)
}

/// Parse an unsigned 64-bit decimal integer with overflow checking.
pub(crate) fn parse_u64(bytes: &[u8]) -> Result<u64, NumErr> {
    if bytes.is_empty() {
        return Err(NumErr::Invalid);
    }
    let mut acc: u64 = 0;
    for &b in bytes {
        if !b.is_ascii_digit() {
            return Err(NumErr::Invalid);
        }
        acc = acc
            .checked_mul(10)
            .and_then(|v| v.checked_add((b - b'0') as u64))
            .ok_or(NumErr::Overflow)?;
    }
    Ok(acc)
}

/// Parse a signed 32-bit decimal integer with overflow checking.
///
/// A leading `+` or `-` is accepted.
pub(crate) fn parse_i32(bytes: &[u8]) -> Result<i32, NumErr> {
    if bytes.is_empty() {
        return Err(NumErr::Invalid);
    }
    let (negative, digits) = match bytes[0] {
        b'-' => (true, &bytes[1..]),
        b'+' => (false, &bytes[1..]),
        _ => (false, bytes),
    };
    if digits.is_empty() {
        return Err(NumErr::Invalid);
    }
    // Accumulate as i64 so the negative extreme (-2^31) is representable.
    let mut acc: i64 = 0;
    for &b in digits {
        if !b.is_ascii_digit() {
            return Err(NumErr::Invalid);
        }
        acc = acc * 10 + (b - b'0') as i64;
        if acc > (i32::MAX as i64) + 1 {
            return Err(NumErr::Overflow);
        }
    }
    let value = if negative { -acc } else { acc };
    if value < i32::MIN as i64 || value > i32::MAX as i64 {
        return Err(NumErr::Overflow);
    }
    Ok(value as i32)
}

/// Parse a finite floating-point value.
///
/// Non-finite results (`inf`, `nan`, overflow to infinity) are rejected as
/// invalid; a nonfinite score is treated as malformed input rather than a
/// silently accepted value.
pub(crate) fn parse_f64(bytes: &[u8]) -> Result<f64, NumErr> {
    if bytes.is_empty() {
        return Err(NumErr::Invalid);
    }
    let text = std::str::from_utf8(bytes).map_err(|_| NumErr::Invalid)?;
    match text.parse::<f64>() {
        Ok(value) if value.is_finite() => Ok(value),
        Ok(_) => Err(NumErr::Invalid),
        Err(_) => Err(NumErr::Invalid),
    }
}

/// Result of measuring a line's leading whitespace.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Indent {
    /// Number of leading whitespace characters (the content column).
    pub(crate) width: usize,
    /// Whether any leading whitespace character was a tab.
    pub(crate) has_tab: bool,
    /// Index of the first non-whitespace byte (equals `width`).
    pub(crate) content_start: usize,
}

/// Measure the leading whitespace of a line.
pub(crate) fn measure_indent(line: &[u8]) -> Indent {
    let mut i = 0;
    let mut has_tab = false;
    while i < line.len() {
        match line[i] {
            b' ' => {}
            b'\t' => has_tab = true,
            _ => break,
        }
        i += 1;
    }
    Indent {
        width: i,
        has_tab,
        content_start: i,
    }
}

/// Strip a single trailing carriage return (for CRLF line endings).
#[inline]
pub(crate) fn strip_cr(line: &[u8]) -> &[u8] {
    match line.split_last() {
        Some((b'\r', rest)) => rest,
        _ => line,
    }
}

/// Iterator over whitespace-separated tokens of a line, yielding each token's
/// half-open byte range relative to the start of `line`.
pub(crate) struct Tokens<'a> {
    line: &'a [u8],
    pos: usize,
}

impl<'a> Tokens<'a> {
    /// Start tokenising `line` from byte offset `start`.
    #[inline]
    pub(crate) fn new(line: &'a [u8], start: usize) -> Tokens<'a> {
        Tokens { line, pos: start }
    }
}

impl Iterator for Tokens<'_> {
    /// `(start, end)` byte range relative to the line.
    type Item = (usize, usize);

    fn next(&mut self) -> Option<Self::Item> {
        let bytes = self.line;
        // Skip separators.
        while self.pos < bytes.len() && matches!(bytes[self.pos], b' ' | b'\t') {
            self.pos += 1;
        }
        if self.pos >= bytes.len() {
            return None;
        }
        let start = self.pos;
        while self.pos < bytes.len() && !matches!(bytes[self.pos], b' ' | b'\t') {
            self.pos += 1;
        }
        Some((start, self.pos))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn u32_ok_and_overflow() {
        assert_eq!(parse_u32(b"0"), Ok(0));
        assert_eq!(parse_u32(b"4294967295"), Ok(u32::MAX));
        assert_eq!(parse_u32(b"4294967296"), Err(NumErr::Overflow));
        assert_eq!(parse_u32(b""), Err(NumErr::Invalid));
        assert_eq!(parse_u32(b"12a"), Err(NumErr::Invalid));
    }

    #[test]
    fn i32_signs_and_range() {
        assert_eq!(parse_i32(b"-2147483648"), Ok(i32::MIN));
        assert_eq!(parse_i32(b"2147483647"), Ok(i32::MAX));
        assert_eq!(parse_i32(b"+5"), Ok(5));
        assert_eq!(parse_i32(b"2147483648"), Err(NumErr::Overflow));
        assert_eq!(parse_i32(b"-"), Err(NumErr::Invalid));
    }

    #[test]
    fn f64_rejects_nonfinite() {
        assert_eq!(parse_f64(b"3.5"), Ok(3.5));
        assert_eq!(parse_f64(b"300000"), Ok(300000.0));
        assert!(parse_f64(b"nan").is_err());
        assert!(parse_f64(b"inf").is_err());
    }

    #[test]
    fn tokenises_with_offsets() {
        let line = b"  fill 10 20";
        let indent = measure_indent(line);
        assert_eq!(indent.width, 2);
        assert!(!indent.has_tab);
        let tokens: Vec<_> = Tokens::new(line, indent.content_start).collect();
        assert_eq!(tokens, vec![(2, 6), (7, 9), (10, 12)]);
        assert_eq!(&line[tokens[0].0..tokens[0].1], b"fill");
    }
}
