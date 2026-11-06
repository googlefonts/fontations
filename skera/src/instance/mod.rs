//! Shared axis planning, sparse variation transforms, and flat encodings.
#![allow(dead_code, unused_imports)]
mod avar2;
mod axes;
mod cull;
mod iup;
mod optimize;
mod rebase;
mod scalars;
mod serialize;
mod store;
mod table;
mod tuple;
pub(crate) use axes::AxisPlan;
pub use axes::{parse_axis_limits, parse_axis_limits_for_font, AxisLimits};
pub(crate) use store::StorePlan;
pub(crate) use table::{FontContext, Instance, TableChange};
// HarfBuzz's roundf keeps ties toward positive infinity. Promote only at
// the rounding boundary: float addition can turn 0.5's predecessor into a
// tie, or change an already integral value above 2^23.
fn round_f32(value: f32) -> f64 {
    (value as f64 + 0.5).floor()
}

fn iup_delta(v: f32, a: f32, b: f32, da: f32, db: f32) -> f32 {
    if a == b {
        return if da == db { da } else { 0. };
    }
    if v <= a.min(b) {
        if a < b {
            da
        } else {
            db
        }
    } else if v >= a.max(b) {
        if a > b {
            da
        } else {
            db
        }
    } else {
        let r = (v - a) / (b - a);
        da + r * (db - da)
    }
}
