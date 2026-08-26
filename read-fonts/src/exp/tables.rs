//! Real tables, generated into the reworked framework.
//!
//! GPOS plus what it depends on: enough to exercise every shape the design has
//! (value records, records nested two deep, offset arrays with args, records
//! holding offsets, format groups) on tables that actually exist.

pub mod cff;
pub mod fvar;
pub mod gpos;
pub mod layout;
pub mod variations;

#[cfg(test)]
mod tests;
