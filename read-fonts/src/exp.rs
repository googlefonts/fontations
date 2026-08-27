//! A sketch of a reworked parsing framework.
//!
//! This module is a scratch space: nothing here is wired into codegen or used
//! by the rest of the crate. It exists to check that the pieces described in
//! `docs/parsing-rework.md` actually fit together, by hand-writing the code
//! that codegen would emit for the cases that are hard today.
//!
//! The four ideas being tested:
//!
//! - [`Table`] covers tables only, and there is no matching trait for
//!   records. A table is read from data that begins at the table and runs to
//!   the end of its parent; a record is read at a position *within* data it
//!   does not own, so it keeps the base its offsets are measured against. What
//!   a record needs is already covered by how its size is known
//!   ([`ComputedSize`] and friends) plus [`ArrayElement`].
//! - Accessors return [`Option`], never `Result`, and never nest. A field that
//!   is absent, null, or unreadable is `None`.
//! - [`WithParent`] pairs a zerocopy record with the data its offsets resolve
//!   against, so a record's offset accessors take no arguments.
//! - One [`Array`] type, parameterised over where its elements live.
//!
//! Validation stays minimal: a table checks that its fixed header is present
//! and nothing else; an array checks its whole extent once when it is built.
//! Everything downstream of those two checks is total.

pub mod parse;
pub mod prelude;
pub mod shapes;
pub mod tables;

pub use parse::{
    Array, Bytes, ComputedSize, Discriminant, Resolve, Table, VariableSize, WithParent,
};
