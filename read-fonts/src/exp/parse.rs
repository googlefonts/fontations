//! Reading font data.
//!
//! Everything the framework needs to turn bytes into tables, in one place:
//!
//! | | |
//! | --- | --- |
//! | [`Bytes`] | the bytes, and the only reads performed on them |
//! | [`Table`] | what a table is, and how one is read |
//! | [`ComputedSize`], [`VariableSize`] | the two ways a record's length is not known until runtime |
//! | [`Resolve`] | following an offset |
//! | [`Array`] | one array type, over four stores |
//! | [`WithParent`] | a fixed-size record paired with the base its offsets are measured from |
//!
//! Checking that any of it is well formed is kept apart, in two passes that
//! answer different questions: [`fast_sanitize`] says whether a font is well
//! formed, and [`sanitize`] says what is wrong with it and where. Each is a
//! feature and an impl per table, so a caller links only what it asks for —
//! and only the second one carries the names, which is why they are separate
//! walks rather than one with a flag.
//!
//! # Tables
//!
//! A table is handed data beginning at its own first byte, so its offsets are
//! measured from byte zero of what it was given. That is the whole of what
//! [`Table`] says, and reading one checks one thing: that `MIN_SIZE` bytes are
//! present.
//!
//! # Records
//!
//! There is no `Record` trait. A record's two questions already have homes —
//! how big it is, and [`ArrayElement::read`] — so what a record *is* is decided
//! entirely by how its byte length is known:
//!
//! | byte length | trait | given | store |
//! | --- | --- | --- | --- |
//! | at compile time | [`FixedSize`][types::FixedSize] (+ `AnyBitPattern`) | `&'a Self`, borrowed | [`SliceStore`] |
//! | computed from read args | [`ComputedSize`] | parent + position + args | [`StridedStore`] |
//! | read from the data | [`VariableSize`] | a slice at itself | [`VariableSizeStore`] |
//!
//! Only the middle row needs its parent, and that is what lets such a record
//! hold offsets measured against the enclosing table rather than against
//! itself. A fixed-size record that holds an offset is paired with its base by
//! [`WithParent`]; one that does not stays a plain `&'a [R]`.
//!
//! The last row is handed a slice at itself, which is exactly what a [`Table`]
//! is handed — so such a record implements `Table` and reads through it. That
//! is not a borrowed mechanism: what makes a record different from a table is
//! needing its parent, and a self-describing element does not.
//!
//! Note which size varies. It is the length *in the font*, never the size of
//! the Rust type: every one of these is an ordinary `Sized` value, which is why
//! neither "unsized" nor "dyn" would describe the middle row.
//!
//! # Where the fallibility went
//!
//! [`Bytes`] returns `Option`, and nothing below it returns `Result`. Building
//! a record cannot fail — it reads nothing — and reading an element of an array
//! cannot fail, because the array checked its whole extent when it was built.
//! What is left is [`Option`] on the accessors, and
//! [`sanitize`][super::sanitize] for anyone who wants to know why.

pub mod array;
pub mod bytes;
#[cfg(feature = "fast_sanitize")]
pub mod decycler;
#[cfg(feature = "fast_sanitize")]
pub mod fast_sanitize;
#[cfg(feature = "sanitize")]
pub mod sanitize;
pub mod traits;
pub mod with_parent;

pub use array::{
    Array, ArrayElement, ArrayStore, OffsetStore, OffsetTo, SizedArrayStore, SliceStore,
    StridedStore, VariableSizeArray, VariableSizeOf, VariableSizeStore,
};
pub use bytes::Bytes;
pub use traits::{ComputedSize, Discriminant, RawOffset, Resolve, Table, VariableSize};
pub use with_parent::WithParent;
