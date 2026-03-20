//! Pre-validating font data.

use crate::{
    array::{ComputedArray, VarLenArray},
    font_data::FontData,
    read::{ComputeSize, FontRead, FontReadWithArgs, VarSize},
    ReadError,
};

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

/// A trait for pre-validating a record type that requires external offset data.
///
/// Unlike [`Sanitize`], which is for self-contained tables, this trait is for
/// record types whose offset fields are resolved relative to some parent table's
/// data. The `data` parameter provides that context.
pub trait SanitizeRecord {
    fn sanitize_record(&self, data: FontData) -> Result<(), ReadError>;
}

impl<'a, T> SanitizeRecord for ComputedArray<'a, T>
where
    T: SanitizeRecord + FontReadWithArgs<'a> + ComputeSize,
    T::Args: Copy + 'static,
{
    fn sanitize_record(&self, data: FontData) -> Result<(), ReadError> {
        for item in self.iter() {
            item?.sanitize_record(data)?;
        }
        Ok(())
    }
}

impl<'a, T> SanitizeRecord for VarLenArray<'a, T>
where
    T: SanitizeRecord + FontRead<'a> + VarSize,
{
    fn sanitize_record(&self, data: FontData) -> Result<(), ReadError> {
        for item in self.iter() {
            item?.sanitize_record(data)?;
        }
        Ok(())
    }
}

/// Sanitize an offset target, treating a null offset as acceptable.
///
/// Real-world fonts sometimes have non-nullable offset fields set to zero.
/// Rather than failing sanitize for these, we skip them.
pub fn sanitize_ignoring_null<T: Sanitize>(result: Result<T, ReadError>) -> Result<(), ReadError> {
    match result {
        Ok(x) => x.sanitize(),
        Err(ReadError::NullOffset) => Ok(()),
        Err(other) => Err(other),
    }
}
