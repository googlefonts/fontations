//! A directory of all the font tables.

pub mod head;
pub mod hhea;
pub mod hmtx;
pub mod maxp;
pub mod name;
pub use crate::layout::{gdef, gpos, gsub};
