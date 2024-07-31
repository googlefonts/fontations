//! Latin writing system.

mod blues;
mod metrics;
mod segments;
mod widths;

/// All constants are defined based on a UPEM of 2048.
///
/// See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/autofit/aflatin.h#L34>
fn derived_constant(units_per_em: i32, value: i32) -> i32 {
    value * units_per_em / 2048
}
