// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the GNU General Public License v3.0.

//! Parallel, section-parallel NET parsing.
//!
//! Independent sections are parsed concurrently, then their local arenas are
//! rebased and concatenated in file order. The result is byte-for-byte
//! identical to the sequential parser, and a failure reports the error with the
//! smallest input byte offset rather than whichever worker happened to fail
//! first.

use rayon::prelude::*;

use crate::model::ids::{NO_ATTRIBUTES, NO_NODE};
use crate::parser::ParserConfig;
use crate::parser::error::{NetError, Result};
use crate::parser::section_scan::scan_sections;
use crate::parser::sequential::{Parsed, parse_document, parse_slice};

/// Parse a whole document, parallelising across sections.
///
/// Falls back to the sequential parser when there are fewer than two sections,
/// which also gives identical handling of empty input and stray records before
/// the first header.
pub(crate) fn parse_document_parallel(bytes: &[u8], config: ParserConfig) -> Result<Parsed> {
    let spans = scan_sections(bytes);
    if spans.len() < 2 {
        return parse_document(bytes, config);
    }

    // Content before the first header must not contain a stray record; parsing
    // the preamble reproduces the sequential error for that case.
    let first_start = spans[0].start;
    if first_start > 0 {
        parse_slice(&bytes[..first_start], 0, 1, config)?;
    }

    let results: Vec<Result<Parsed>> = spans
        .par_iter()
        .map(|span| {
            parse_slice(
                &bytes[span.start..span.end],
                span.start as u32,
                span.start_line,
                config,
            )
        })
        .collect();

    // Deterministic error selection: smallest byte offset, then earliest section.
    let mut best: Option<NetError> = None;
    for result in &results {
        if let Err(err) = result {
            let replace = match &best {
                None => true,
                Some(current) => err.byte_offset < current.byte_offset,
            };
            if replace {
                best = Some(err.clone());
            }
        }
    }
    if let Some(err) = best {
        return Err(err);
    }

    let parts: Vec<Parsed> = results
        .into_iter()
        .map(|r| r.expect("errors handled"))
        .collect();
    Ok(assemble(parts))
}

/// Concatenate section-local arenas into one global arena, rebasing all indices.
fn assemble(parts: Vec<Parsed>) -> Parsed {
    let total_nodes: usize = parts.iter().map(|p| p.nodes.len()).sum();
    let total_attrs: usize = parts.iter().map(|p| p.attrs.len()).sum();
    let total_extras: usize = parts.iter().map(|p| p.extras.len()).sum();
    let total_sections: usize = parts.iter().map(|p| p.sections.len()).sum();

    let mut nodes = Vec::with_capacity(total_nodes);
    let mut attrs = Vec::with_capacity(total_attrs);
    let mut extras = Vec::with_capacity(total_extras);
    let mut sections = Vec::with_capacity(total_sections);

    let mut node_off: u32 = 0;
    let mut attr_off: u32 = 0;
    let mut extra_off: u32 = 0;

    for part in parts {
        let Parsed {
            sections: p_sections,
            nodes: p_nodes,
            attrs: p_attrs,
            extras: p_extras,
        } = part;
        let node_count = p_nodes.len() as u32;
        let attr_count = p_attrs.len() as u32;
        let extra_count = p_extras.len() as u32;

        for mut node in p_nodes {
            if node.parent != NO_NODE {
                node.parent += node_off;
            }
            if node.first_child != NO_NODE {
                node.first_child += node_off;
            }
            if node.next_sibling != NO_NODE {
                node.next_sibling += node_off;
            }
            node.subtree_end += node_off;
            if node.attributes != NO_ATTRIBUTES {
                node.attributes += attr_off;
            }
            nodes.push(node);
        }

        for mut attr in p_attrs {
            attr.extra.start += extra_off;
            attrs.push(attr);
        }

        // Extra attributes carry global byte spans already; only their arena
        // position changes, which is handled by `attr.extra.start` above.
        extras.extend(p_extras);

        for mut section in p_sections {
            section.node_start += node_off;
            section.node_end += node_off;
            if section.first_root != NO_NODE {
                section.first_root += node_off;
            }
            sections.push(section);
        }

        node_off += node_count;
        attr_off += attr_count;
        extra_off += extra_count;
    }

    Parsed {
        sections,
        nodes,
        attrs,
        extras,
    }
}
