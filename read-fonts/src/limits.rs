//! Limits applied when interpreting font data.
//!
//! Font files can describe structures that recurse, refer to themselves, or are
//! simply enormous. Nothing here comes from the OpenType specification: these
//! are the bounds this crate places on how much work one operation will do
//! before giving up, so that a malformed or hostile font cannot turn a parse
//! into a hang.

/// Maximum depth for structures that nest.
///
/// Applies to composite glyph trees, nested variation data, layout lookup
/// nesting, and colour graph traversal.
///
/// Matches HarfBuzz's [`HB_MAX_NESTING_LEVEL`].
///
/// [`HB_MAX_NESTING_LEVEL`]: https://github.com/harfbuzz/harfbuzz/blob/724ef405f3a0a0b2792c7a575d1f3341b41ca950/src/hb-limits.hh#L53
pub const MAX_RECURSION_DEPTH: usize = 64;

/// Maximum number of points in a single assembled outline.
///
/// A composite glyph accumulates the points of every component in its tree, so
/// unlike the point count of one simple glyph this is not bounded by the
/// format. A contour end point is a `u16`, so the last point an outline can
/// name is `u16::MAX`, and it holds one more than that.
pub const MAX_OUTLINE_POINTS: usize = u16::MAX as usize + 1;

/// Maximum number of references followed while assembling one composite
/// glyph, whether a [`glyf`] component tree or a [`VARC`] graph.
///
/// Bounds the total work for a graph that fans out at every level, which the
/// depth limit alone does not.
///
/// [`glyf`]: crate::tables::glyf
/// [`VARC`]: crate::tables::varc
pub const MAX_COMPOSITE_EDGES: usize = 2048;

/// Maximum number of scripts visited in a layout table's script list.
///
/// Matches HarfBuzz's `HB_MAX_SCRIPTS` in [`hb-limits.hh`].
///
/// [`hb-limits.hh`]: https://github.com/harfbuzz/harfbuzz/blob/main/src/hb-limits.hh
#[cfg(feature = "std")]
pub const MAX_SCRIPTS: u16 = 500;

/// Maximum number of language systems visited in a layout table's script list.
///
/// Matches HarfBuzz's `HB_MAX_LANGSYS` in [`hb-limits.hh`].
///
/// [`hb-limits.hh`]: https://github.com/harfbuzz/harfbuzz/blob/main/src/hb-limits.hh
#[cfg(feature = "std")]
pub const MAX_LANGSYS: u16 = 2000;

/// Maximum total number of feature indices referenced by the language systems
/// visited when pruning a layout table's language systems.
///
/// Matches HarfBuzz's `HB_MAX_LANGSYS_FEATURE_COUNT` in [`hb-limits.hh`].
///
/// [`hb-limits.hh`]: https://github.com/harfbuzz/harfbuzz/blob/main/src/hb-limits.hh
#[cfg(feature = "std")]
pub const MAX_LANGSYS_FEATURE_COUNT: u32 = 50000;

/// Maximum total number of feature indices visited across the language
/// systems of a layout table.
///
/// Matches HarfBuzz's `HB_MAX_FEATURE_INDICES` in [`hb-limits.hh`].
///
/// [`hb-limits.hh`]: https://github.com/harfbuzz/harfbuzz/blob/main/src/hb-limits.hh
#[cfg(feature = "std")]
pub const MAX_FEATURE_INDICES: u16 = 8000;

/// Maximum number of lookups visited by one layout closure operation.
///
/// Matches HarfBuzz's `HB_MAX_LOOKUP_VISIT_COUNT` in [`hb-limits.hh`].
///
/// [`hb-limits.hh`]: https://github.com/harfbuzz/harfbuzz/blob/main/src/hb-limits.hh
#[cfg(feature = "std")]
pub const MAX_LOOKUP_VISIT_COUNT: u16 = 35000;

/// Number of bytes one walk of a layout table's offset graph may read for
/// each byte of the table.
///
/// Offsets may share or overlap their targets, so the bytes reachable from a
/// table are not bounded by its size: a target counts once for every offset
/// that leads to it.
///
/// Matches HarfBuzz's `HB_SANITIZE_MAX_OPS_FACTOR` in [`hb-sanitize.hh`].
///
/// [`hb-sanitize.hh`]: https://github.com/harfbuzz/harfbuzz/blob/main/src/hb-sanitize.hh
#[cfg(feature = "std")]
pub const MAX_OPS_FACTOR: usize = 64;

/// Number of bytes one walk of a layout table's offset graph may always read,
/// however small the table.
///
/// Matches HarfBuzz's `HB_SANITIZE_MAX_OPS_MIN` in [`hb-sanitize.hh`].
///
/// [`hb-sanitize.hh`]: https://github.com/harfbuzz/harfbuzz/blob/main/src/hb-sanitize.hh
#[cfg(feature = "std")]
pub const MAX_OPS_MIN: usize = 16384;

/// Number of bytes one walk of a layout table's offset graph may read at most,
/// however large the table.
///
/// Matches HarfBuzz's `HB_SANITIZE_MAX_OPS_MAX` in [`hb-sanitize.hh`].
///
/// [`hb-sanitize.hh`]: https://github.com/harfbuzz/harfbuzz/blob/main/src/hb-sanitize.hh
#[cfg(feature = "std")]
pub const MAX_OPS_MAX: usize = 0x3FFFFFFF;

/// The number of bytes one walk of a table's offset graph may read, for a
/// table of `table_len` bytes.
///
/// HarfBuzz sizes this from the whole table it sanitizes, such as GSUB or
/// GPOS, so `table_len` should be the length of that table rather than of one
/// of its subtables.
///
/// See [`MAX_OPS_FACTOR`], [`MAX_OPS_MIN`] and [`MAX_OPS_MAX`].
#[cfg(feature = "std")]
pub fn max_ops(table_len: usize) -> usize {
    table_len
        .saturating_mul(MAX_OPS_FACTOR)
        .clamp(MAX_OPS_MIN, MAX_OPS_MAX)
}

/// The error of a walk of a layout table's offset graph that would read more
/// than [`max_ops`] allows.
///
/// HarfBuzz's sanitizer rejects such a table, so a caller may want to tell
/// this apart from the errors of a table it can partly use.
#[cfg(feature = "std")]
pub const MAX_OPS_EXCEEDED: crate::ReadError =
    crate::ReadError::MalformedData("layout tables share too much data");
