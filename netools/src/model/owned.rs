// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the GNU General Public License v3.0.

//! Owned, self-contained sections.
//!
//! An [`OwnedNet`] wraps a single-section store that owns its own bytes. It is
//! produced by [`NetRef::to_owned`], by [`NodeRef::clone_subtree`], and by the
//! streaming reader. Because it holds a real (single-section) store, every
//! borrowed API — traversal, validation, writing — works on it through
//! [`OwnedNet::as_ref`] with no duplicated logic.

use std::collections::HashMap;

use crate::io::reader::{NetRef, NetStore, NodeRef};
use crate::io::storage::SharedBytes;
use crate::model::attributes::NetTypeRepr;
use crate::model::ids::{NO_ATTRIBUTES, NO_NODE};
use crate::model::range::Span;
use crate::model::validation::{ValidationMode, ValidationReport};
use crate::model::{Coord, NetId, Section};
use crate::parser::ParserConfig;
use crate::parser::error::Result;

/// One reference-sequence section, owning its own storage.
pub struct OwnedNet {
    store: NetStore,
}

impl OwnedNet {
    /// Wrap a store that contains exactly one section.
    pub(crate) fn from_store(store: NetStore) -> OwnedNet {
        debug_assert_eq!(store.sections.len(), 1);
        OwnedNet { store }
    }

    /// Parse a byte buffer that contains a single section.
    ///
    /// If the buffer contains more than one section, only the first is kept.
    pub fn parse_bytes(bytes: Vec<u8>) -> Result<OwnedNet> {
        Self::parse_bytes_with(bytes, ParserConfig::default())
    }

    pub(crate) fn parse_bytes_with(bytes: Vec<u8>, config: ParserConfig) -> Result<OwnedNet> {
        let store = NetStore::parse(SharedBytes::from_vec(bytes), config, false)?;
        let store = if store.sections.len() == 1 {
            store
        } else {
            store.extract_section(NetId::new(0))
        };
        Ok(OwnedNet { store })
    }

    /// A borrowed view over this section.
    #[inline]
    pub fn as_ref(&self) -> NetRef<'_> {
        NetRef {
            store: &self.store,
            id: NetId::new(0),
        }
    }

    /// The reference sequence name.
    #[inline]
    pub fn reference_name(&self) -> &[u8] {
        self.as_ref().reference_name_bytes()
    }

    /// The reference sequence length.
    #[inline]
    pub fn reference_size(&self) -> Coord {
        self.as_ref().reference_size()
    }

    /// Number of records.
    #[inline]
    pub fn len(&self) -> usize {
        self.store.nodes.len()
    }

    /// Whether the section has no records.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.store.nodes.is_empty()
    }

    /// Validate this section.
    #[inline]
    pub fn validate(&self, mode: ValidationMode) -> ValidationReport {
        self.as_ref().validate(mode)
    }

    /// Shrink the byte buffer to only the bytes actually referenced by this
    /// section, deduplicating identical strings.
    ///
    /// [`to_owned`](NetRef::to_owned) shares the source buffer through an `Arc`,
    /// so a section extracted from a large reader keeps the whole file alive
    /// until this is called.
    pub fn compact(&mut self) {
        let (storage, sections, nodes, attrs, extras) = {
            let old = self.store.storage.as_bytes();
            let mut buf: Vec<u8> = Vec::new();
            let mut cache: HashMap<(u32, u32), Span> = HashMap::new();

            let mut sections = self.store.sections.clone();
            for section in &mut sections {
                section.reference_name = intern(&mut buf, &mut cache, old, section.reference_name);
            }

            let mut nodes = self.store.nodes.clone();
            for node in &mut nodes {
                node.query_name = intern(&mut buf, &mut cache, old, node.query_name);
            }

            let mut attrs = self.store.attrs.clone();
            for block in &mut attrs {
                if let NetTypeRepr::Other(span) = block.net_type {
                    block.net_type = NetTypeRepr::Other(intern(&mut buf, &mut cache, old, span));
                }
            }

            let mut extras = self.store.extras.clone();
            for extra in &mut extras {
                extra.key = intern(&mut buf, &mut cache, old, extra.key);
                extra.value = intern(&mut buf, &mut cache, old, extra.value);
            }

            (SharedBytes::from_vec(buf), sections, nodes, attrs, extras)
        };

        self.store.storage = storage;
        self.store.sections = sections;
        self.store.nodes = nodes;
        self.store.attrs = attrs;
        self.store.extras = extras;
    }
}

/// Node arena, attribute arena, and extra-attribute arena for a rebuilt
/// section, together with per-node parent and depth arrays.
struct RebuildParts {
    nodes: Vec<crate::model::node::NetNode>,
    attrs: Vec<crate::model::attributes::NodeAttributes>,
    extras: Vec<crate::model::attributes::ExtraAttribute>,
    parents: Vec<u32>,
    depths: Vec<u16>,
}

/// Copy the chosen source nodes (by local index, in emission order) into fresh
/// arenas, carrying attributes and extras along.
fn copy_nodes(
    store: &NetStore,
    node_start: u32,
    order: &[u32],
) -> (
    Vec<crate::model::node::NetNode>,
    Vec<crate::model::attributes::NodeAttributes>,
    Vec<crate::model::attributes::ExtraAttribute>,
) {
    let mut nodes = Vec::with_capacity(order.len());
    let mut attrs = Vec::new();
    let mut extras = Vec::new();
    for &local in order {
        let mut node = store.nodes[(node_start + local) as usize];
        if node.attributes != NO_ATTRIBUTES {
            let mut block = store.attrs[node.attributes as usize];
            if block.extra.len > 0 {
                let es = block.extra.start as usize;
                let el = block.extra.len as usize;
                let new_start = extras.len() as u32;
                extras.extend_from_slice(&store.extras[es..es + el]);
                block.extra.start = new_start;
            }
            node.attributes = attrs.len() as u32;
            attrs.push(block);
        }
        nodes.push(node);
    }
    (nodes, attrs, extras)
}

/// Reconstruct `first_child`/`next_sibling`/`subtree_end` and the parent/depth
/// fields of a preorder node arena from the parent and depth arrays.
///
/// Returns `(first_root, root_count)`.
fn link_topology(
    nodes: &mut [crate::model::node::NetNode],
    parents: &[u32],
    depths: &[u16],
) -> (u32, u32) {
    let n = nodes.len();

    // subtree_end[i] = first j > i whose depth is not greater than depth[i].
    for i in (0..n).rev() {
        let mut e = i + 1;
        while e < n && depths[e] > depths[i] {
            e = nodes[e].subtree_end as usize;
        }
        nodes[i].subtree_end = e as u32;
    }

    for i in 0..n {
        nodes[i].parent = parents[i];
        nodes[i].depth = depths[i];
        nodes[i].first_child = NO_NODE;
        nodes[i].next_sibling = NO_NODE;
    }

    let mut last_child = vec![NO_NODE; n];
    let (mut first_root, mut last_root, mut root_count) = (NO_NODE, NO_NODE, 0u32);
    for (i, &p) in parents.iter().enumerate() {
        if p == NO_NODE {
            if first_root == NO_NODE {
                first_root = i as u32;
            } else {
                nodes[last_root as usize].next_sibling = i as u32;
            }
            last_root = i as u32;
            root_count += 1;
        } else {
            let lc = last_child[p as usize];
            if lc == NO_NODE {
                nodes[p as usize].first_child = i as u32;
            } else {
                nodes[lc as usize].next_sibling = i as u32;
            }
            last_child[p as usize] = i as u32;
        }
    }
    (first_root, root_count)
}

/// Assemble a single-section store from rebuilt parts, sharing `store`'s bytes.
fn assemble_section(store: &NetStore, section: &Section, mut parts: RebuildParts) -> NetStore {
    let (first_root, root_count) = link_topology(&mut parts.nodes, &parts.parents, &parts.depths);
    let new_section = Section {
        reference_name: section.reference_name,
        reference_size: section.reference_size,
        node_start: 0,
        node_end: parts.nodes.len() as u32,
        first_root,
        root_count,
    };
    NetStore {
        storage: store.storage.clone(),
        sections: vec![new_section],
        nodes: parts.nodes,
        attrs: parts.attrs,
        extras: parts.extras,
    }
}

/// Build a single-section store keeping only the source nodes flagged in `keep`
/// (indexed by local node index), reparenting each kept node to its nearest kept
/// ancestor. Callers supply the policy that produced `keep`.
pub(crate) fn build_selected(store: &NetStore, net_id: NetId, keep: &[bool]) -> NetStore {
    let section = store.sections[net_id.index()];
    let ns = section.node_start;
    let count = (section.node_end - ns) as usize;

    let mut new_index = vec![NO_NODE; count];
    let mut order: Vec<u32> = Vec::new();
    for (i, &kept) in keep.iter().enumerate().take(count) {
        if kept {
            new_index[i] = order.len() as u32;
            order.push(i as u32);
        }
    }
    let n = order.len();

    let mut parents = vec![NO_NODE; n];
    let mut depths = vec![0u16; n];
    for (new_i, &local) in order.iter().enumerate() {
        let mut p = store.nodes[(ns + local) as usize].parent;
        let mut pnew = NO_NODE;
        while p != NO_NODE {
            let plocal = (p - ns) as usize;
            if keep[plocal] {
                pnew = new_index[plocal];
                break;
            }
            p = store.nodes[p as usize].parent;
        }
        parents[new_i] = pnew;
        depths[new_i] = if pnew == NO_NODE {
            0
        } else {
            depths[pnew as usize] + 1
        };
    }

    let (nodes, attrs, extras) = copy_nodes(store, ns, &order);
    assemble_section(
        store,
        &section,
        RebuildParts {
            nodes,
            attrs,
            extras,
            parents,
            depths,
        },
    )
}

/// Build a single-section store with every sibling list stably sorted by the
/// canonical record key (reference start, size, kind, query name, query start).
pub(crate) fn build_sorted(store: &NetStore, net_id: NetId) -> NetStore {
    let section = store.sections[net_id.index()];
    let ns = section.node_start;
    let bytes = store.storage.as_bytes();

    let sort_key = |local: u32| {
        let node = &store.nodes[(ns + local) as usize];
        (
            node.reference_start,
            node.reference_size,
            node.kind as u8,
            node.query_name.resolve(bytes).to_vec(),
            node.query_start,
        )
    };
    let sorted_children = |global_first: u32| -> Vec<u32> {
        let mut children = Vec::new();
        let mut c = global_first;
        while c != NO_NODE {
            children.push(c - ns);
            c = store.nodes[c as usize].next_sibling;
        }
        children.sort_by_key(|&l| sort_key(l));
        children
    };

    let mut order: Vec<u32> = Vec::new();
    let mut parents: Vec<u32> = Vec::new();
    let mut depths: Vec<u16> = Vec::new();

    // Stack of (source local index, new-parent index, depth). Children are
    // pushed in reverse so they pop in sorted order.
    let mut stack: Vec<(u32, u32, u16)> = Vec::new();
    for &root in sorted_children(section.first_root).iter().rev() {
        stack.push((root, NO_NODE, 0));
    }
    while let Some((local, pnew, depth)) = stack.pop() {
        let new_i = order.len() as u32;
        order.push(local);
        parents.push(pnew);
        depths.push(depth);
        let first_child = store.nodes[(ns + local) as usize].first_child;
        for &child in sorted_children(first_child).iter().rev() {
            stack.push((child, new_i, depth + 1));
        }
    }

    let (nodes, attrs, extras) = copy_nodes(store, ns, &order);
    assemble_section(
        store,
        &section,
        RebuildParts {
            nodes,
            attrs,
            extras,
            parents,
            depths,
        },
    )
}

/// Copy a span into `buf`, reusing an earlier identical copy when possible.
fn intern(
    buf: &mut Vec<u8>,
    cache: &mut HashMap<(u32, u32), Span>,
    old: &[u8],
    span: Span,
) -> Span {
    if span.is_empty() {
        return Span::EMPTY;
    }
    *cache.entry((span.start, span.end)).or_insert_with(|| {
        let start = buf.len() as u32;
        buf.extend_from_slice(&old[span.start as usize..span.end as usize]);
        Span::new(start, buf.len() as u32)
    })
}

impl NetRef<'_> {
    /// Copy this section into an owned, self-contained [`OwnedNet`].
    ///
    /// The byte buffer is shared through an `Arc`; call
    /// [`OwnedNet::compact`] to detach and shrink it.
    pub fn to_owned(&self) -> OwnedNet {
        OwnedNet::from_store(self.store.extract_section(self.id))
    }

    /// Rebuild this section with every sibling list stably sorted by the
    /// canonical record key. Parent/child relationships are preserved; only the
    /// order of siblings changes.
    pub fn sort_records(&self) -> OwnedNet {
        OwnedNet::from_store(build_sorted(self.store, self.id))
    }
}

impl NodeRef<'_> {
    /// Copy this record's subtree into a new owned section whose single root is
    /// this record. Depths are rebased so the subtree root is depth 0.
    pub fn clone_subtree(&self) -> OwnedNet {
        let store = self.store;
        let start = self.id.get();
        let end = self.subtree_end();
        let base_depth = self.depth();

        let section_id = store.section_of(self.id).unwrap_or(NetId::new(0));
        let section = store.sections[section_id.index()];

        let rebase = |raw: u32| {
            if raw == NO_NODE || raw < start || raw >= end {
                NO_NODE
            } else {
                raw - start
            }
        };

        let mut nodes = Vec::with_capacity((end - start) as usize);
        let mut attrs = Vec::new();
        let mut extras = Vec::new();

        for gi in start..end {
            let mut node = store.nodes[gi as usize];
            node.parent = rebase(node.parent);
            node.first_child = rebase(node.first_child);
            node.next_sibling = rebase(node.next_sibling);
            node.subtree_end -= start;
            node.depth -= base_depth;

            if node.attributes != NO_ATTRIBUTES {
                let mut block = store.attrs[node.attributes as usize];
                if block.extra.len > 0 {
                    let es = block.extra.start as usize;
                    let el = block.extra.len as usize;
                    let new_start = extras.len() as u32;
                    extras.extend_from_slice(&store.extras[es..es + el]);
                    block.extra.start = new_start;
                }
                node.attributes = attrs.len() as u32;
                attrs.push(block);
            }
            nodes.push(node);
        }

        // The subtree root becomes the sole root.
        if let Some(root) = nodes.first_mut() {
            root.parent = NO_NODE;
            root.next_sibling = NO_NODE;
        }

        let new_section = Section {
            reference_name: section.reference_name,
            reference_size: section.reference_size,
            node_start: 0,
            node_end: nodes.len() as u32,
            first_root: if nodes.is_empty() { NO_NODE } else { 0 },
            root_count: if nodes.is_empty() { 0 } else { 1 },
        };

        OwnedNet::from_store(NetStore {
            storage: store.storage.clone(),
            sections: vec![new_section],
            nodes,
            attrs,
            extras,
        })
    }
}

#[cfg(feature = "write")]
impl OwnedNet {
    /// Serialise this section to `out`.
    pub fn write_to<W: std::io::Write>(&self, out: &mut W) -> Result<()> {
        self.as_ref().write_to(out)
    }

    /// Serialise this section to a `String`.
    pub fn to_net_string(&self) -> String {
        self.as_ref().to_net_string()
    }
}

#[cfg(feature = "write")]
impl std::fmt::Display for OwnedNet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.as_ref(), f)
    }
}

impl std::fmt::Debug for OwnedNet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OwnedNet")
            .field(
                "reference_name",
                &String::from_utf8_lossy(self.reference_name()),
            )
            .field("reference_size", &self.reference_size())
            .field("records", &self.len())
            .finish()
    }
}
