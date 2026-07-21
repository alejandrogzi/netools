// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the GNU General Public License v3.0.

//! Depth-first visitor and event traversal.
//!
//! Both are non-recursive: they use the contiguous preorder arena plus each
//! node's `subtree_end` to emit `enter`/`leave` in the correct nesting order,
//! so pathologically deep trees cannot overflow the stack.

use std::ops::ControlFlow;

use crate::io::reader::{NetRef, NetStore, NodeRef};

/// A depth-first visitor over a record tree.
pub trait NetVisitor {
    /// Called on entering a record. Return [`ControlFlow::Break`] to stop.
    fn enter(&mut self, node: NodeRef<'_>) -> ControlFlow<()>;

    /// Called on leaving a record, after all its descendants.
    fn leave(&mut self, node: NodeRef<'_>) {
        let _ = node;
    }
}

/// A traversal event produced by [`NetRef::events`] / [`NodeRef::events`].
#[derive(Debug, Clone, Copy)]
pub enum TraversalEvent<'a> {
    /// Entering a record (preorder).
    Enter(NodeRef<'a>),
    /// Leaving a record, after its descendants (postorder).
    Leave(NodeRef<'a>),
}

impl<'a> TraversalEvent<'a> {
    /// The record this event concerns.
    #[inline]
    pub fn node(&self) -> NodeRef<'a> {
        match self {
            TraversalEvent::Enter(n) | TraversalEvent::Leave(n) => *n,
        }
    }

    /// Whether this is an enter event.
    #[inline]
    pub fn is_enter(&self) -> bool {
        matches!(self, TraversalEvent::Enter(_))
    }
}

/// Drive a visitor over the arena range `[start, end)`.
fn walk_range<V: NetVisitor>(
    store: &NetStore,
    start: u32,
    end: u32,
    visitor: &mut V,
) -> ControlFlow<()> {
    let mut stack: Vec<u32> = Vec::new();
    let mut i = start;
    while i < end {
        while let Some(&top) = stack.last() {
            if store.nodes[top as usize].subtree_end <= i {
                stack.pop();
                visitor.leave(NodeRef::from_raw(store, top));
            } else {
                break;
            }
        }
        visitor.enter(NodeRef::from_raw(store, i))?;
        stack.push(i);
        i += 1;
    }
    while let Some(top) = stack.pop() {
        visitor.leave(NodeRef::from_raw(store, top));
    }
    ControlFlow::Continue(())
}

/// Lazy iterator over enter/leave events for an arena range.
#[derive(Clone)]
pub struct EventIter<'a> {
    store: &'a NetStore,
    pos: u32,
    end: u32,
    stack: Vec<u32>,
}

impl<'a> Iterator for EventIter<'a> {
    type Item = TraversalEvent<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(&top) = self.stack.last() {
            if self.pos >= self.end || self.store.nodes[top as usize].subtree_end <= self.pos {
                self.stack.pop();
                return Some(TraversalEvent::Leave(NodeRef::from_raw(self.store, top)));
            }
        }
        if self.pos >= self.end {
            return None;
        }
        let i = self.pos;
        self.pos += 1;
        self.stack.push(i);
        Some(TraversalEvent::Enter(NodeRef::from_raw(self.store, i)))
    }
}

impl<'a> NetRef<'a> {
    /// Drive `visitor` over every record of this section, depth first.
    pub fn walk<V: NetVisitor>(&self, visitor: &mut V) -> ControlFlow<()> {
        let range = self.node_range();
        walk_range(self.store, range.start, range.end, visitor)
    }

    /// Iterate enter/leave events over this section, depth first.
    pub fn events(&self) -> EventIter<'a> {
        let range = self.node_range();
        EventIter {
            store: self.store,
            pos: range.start,
            end: range.end,
            stack: Vec::new(),
        }
    }
}

impl<'a> NodeRef<'a> {
    /// Drive `visitor` over this record's subtree, depth first.
    pub fn walk<V: NetVisitor>(&self, visitor: &mut V) -> ControlFlow<()> {
        walk_range(self.store, self.id.get(), self.subtree_end(), visitor)
    }

    /// Iterate enter/leave events over this record's subtree.
    pub fn events(&self) -> EventIter<'a> {
        EventIter {
            store: self.store,
            pos: self.id.get(),
            end: self.subtree_end(),
            stack: Vec::new(),
        }
    }
}
