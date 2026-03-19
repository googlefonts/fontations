//! Pre-validating font data.

use crate::ReadError;

/// A trait for pre-validating a font table.
///
/// This is based on the [`sanitize` machinery][hb_sanitize] in HarfBuzz. The
/// basic idea is simple: when the `sanitize` method on a table is called, we
/// will navigate the entire graph of subtables reachable from that table, and
/// ensure that they are well-formed. Concretely, this means that all fields of
/// all tables in the subgraph are in-bounds of the font's underlying data.
///
/// [hb_sanitize]: https://github.com/harfbuzz/harfbuzz/blob/90116a529/src/hb-sanitize.hh#L38
pub trait Sanitize {
    /// Recursively check the validity of this table and its subgraph.
    ///
    /// The object's 'subgraph' is the graph of tables reachable from this table
    /// via an offset.
    fn sanitize(&self) -> Result<(), ReadError>;
}
