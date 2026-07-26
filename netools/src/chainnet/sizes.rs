// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the GNU General Public License v3.0.

//! Ordered sequence-size sources used by chain NET construction.

use std::collections::HashMap;
use std::io::BufRead;
use std::path::{Path, PathBuf};

use super::error::{ChainNetError, Result};

/// One named sequence and its output ordinal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SequenceSize {
    /// Sequence name as uninterpreted bytes.
    pub name: Vec<u8>,
    /// Sequence length.
    pub size: u32,
    /// Position in the source; generated NET sections follow this order.
    pub ordinal: u32,
}

/// Ordered sequence sizes plus a name lookup.
#[derive(Debug, Clone)]
pub struct SequenceSizes {
    pub(crate) entries: Vec<SequenceSize>,
    pub(crate) by_name: HashMap<Vec<u8>, usize>,
}

impl SequenceSizes {
    /// Construct sizes in the supplied order.
    pub fn new<I, N>(entries: I) -> Result<Self>
    where
        I: IntoIterator<Item = (N, u32)>,
        N: Into<Vec<u8>>,
    {
        let mut ordered = Vec::new();
        let mut by_name = HashMap::new();
        for (index, (name, size)) in entries.into_iter().enumerate() {
            let name = name.into();
            if by_name.contains_key(name.as_slice()) {
                return Err(ChainNetError::DuplicateSequence(name));
            }
            let ordinal =
                u32::try_from(index).map_err(|_| ChainNetError::SequenceTooLarge(name.clone()))?;
            by_name.insert(name.clone(), index);
            ordered.push(SequenceSize {
                name,
                size,
                ordinal,
            });
        }
        Ok(Self {
            entries: ordered,
            by_name,
        })
    }

    /// Read a UCSC two-column `.chrom.sizes` file.
    pub fn from_chrom_sizes<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file = std::fs::File::open(path)?;
        let mut entries = Vec::new();
        for (line_index, line) in std::io::BufReader::new(file).split(b'\n').enumerate() {
            let line = line?;
            let trimmed = trim_ascii(&line);
            if trimmed.is_empty() || trimmed.starts_with(b"#") {
                continue;
            }
            let fields: Vec<&[u8]> = trimmed
                .split(|byte| byte.is_ascii_whitespace())
                .filter(|field| !field.is_empty())
                .collect();
            if fields.len() != 2 {
                return Err(ChainNetError::InvalidSizeRecord {
                    line: line_index as u64 + 1,
                    reason: "expected exactly two fields",
                });
            }
            let number = std::str::from_utf8(fields[1])
                .ok()
                .and_then(|text| text.parse::<u32>().ok())
                .ok_or(ChainNetError::InvalidSizeRecord {
                    line: line_index as u64 + 1,
                    reason: "size is not an unsigned 32-bit integer",
                })?;
            entries.push((fields[0].to_vec(), number));
        }
        Self::new(entries)
    }

    /// Read names and lengths from a 2bit index without decoding sequence.
    pub fn from_twobit<P: AsRef<Path>>(path: P) -> Result<Self> {
        let twobit = twobit::TwoBitFile::open(path)?;
        let mut entries = Vec::new();
        for info in twobit.sequence_info() {
            let size = u32::try_from(info.length)
                .map_err(|_| ChainNetError::SequenceTooLarge(info.chr.as_bytes().to_vec()))?;
            entries.push((info.chr.into_bytes(), size));
        }
        Self::new(entries)
    }

    /// Sequence entries in source order.
    #[inline]
    pub fn entries(&self) -> &[SequenceSize] {
        &self.entries
    }

    /// Number of sequences.
    #[inline]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether no sizes were supplied.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Look up a sequence by its raw name.
    #[inline]
    pub fn get(&self, name: &[u8]) -> Option<&SequenceSize> {
        self.by_name.get(name).map(|&index| &self.entries[index])
    }
}

/// Source from which ordered sequence sizes are loaded.
#[derive(Debug, Clone)]
pub enum SizeSource {
    /// A two-column UCSC `.chrom.sizes` path.
    ChromSizes(PathBuf),
    /// A UCSC 2bit path.
    TwoBit(PathBuf),
    /// Already loaded sizes.
    Provided(SequenceSizes),
}

impl SizeSource {
    pub(crate) fn load(&self) -> Result<SequenceSizes> {
        match self {
            Self::ChromSizes(path) => SequenceSizes::from_chrom_sizes(path),
            Self::TwoBit(path) => SequenceSizes::from_twobit(path),
            Self::Provided(sizes) => Ok(sizes.clone()),
        }
    }
}

fn trim_ascii(mut bytes: &[u8]) -> &[u8] {
    while bytes.first().is_some_and(u8::is_ascii_whitespace) {
        bytes = &bytes[1..];
    }
    while bytes.last().is_some_and(u8::is_ascii_whitespace) {
        bytes = &bytes[..bytes.len() - 1];
    }
    bytes
}
