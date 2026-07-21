// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the GNU General Public License v3.0.

//! Hierarchical traversal of the record arena.
//!
//! Every traversal is non-recursive and allocation-light. Iterators borrow the
//! shared store and yield [`NodeRef`](crate::io::NodeRef) handles; the visitor
//! and event APIs use each node's `subtree_end` boundary rather than Rust
//! recursion, so deeply nested inputs cannot overflow the stack.

pub mod ancestors;
pub mod children;
pub mod preorder;
pub mod visitors;

pub use ancestors::AncestorIter;
pub use children::{ChildrenIter, RootIter, SiblingIter};
pub use preorder::{DepthIter, DescendantIter, FillIter, GapIter, SubtreeIter};
pub use visitors::{EventIter, NetVisitor, TraversalEvent};
