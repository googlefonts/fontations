//! Shared machinery for variable-font instancing.
//!
//! Format-specific consumers are added in follow-up changes. Keeping the
//! coordinate planning, tuple rebasing, store optimization, and serialization
//! primitives together gives those consumers one implementation of the hard
//! variation arithmetic.
#![allow(dead_code)]

mod avar2;
mod axes;
mod cull;
mod iup;
mod optimize;
mod rebase;
pub(crate) mod scalars;
mod serialize;
mod store;
mod table;
mod tuple;

pub(crate) use axes::AxisPlan;
pub use axes::{parse_axis_limits, parse_axis_limits_for_font, AxisLimits};
pub(crate) use store::StorePlan;
pub(crate) use table::{Instance, TableChange};

// HarfBuzz's roundf keeps ties toward positive infinity. Promote only at
// the rounding boundary: float addition can turn 0.5's predecessor into a
// tie, or change an already integral value above 2^23.
fn round_f32(value: f32) -> f64 {
    (value as f64 + 0.5).floor()
}
