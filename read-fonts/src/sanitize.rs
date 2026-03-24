//! Pre-validating font data.

use bytemuck::AnyBitPattern;
use types::{BigEndian, FixedSize, Scalar};

use crate::{
    array::{ComputedArray, VarLenArray},
    font_data::FontData,
    read::{ComputeSize, FontRead, FontReadWithArgs, VarSize},
    Offset, ReadError,
};

// https://github.com/harfbuzz/harfbuzz/blob/aba63bb5f8cb6cfc77ee8cfc2700b3ed9c0838ef/src/hb-null.hh#L40
// the number of bytes required to represent the largest table we have.
// note: this is too big for our needs, but is at least large _enough_.
const NULL_POOL_SIZE: usize = 640;
static EMPTY_TABLE_BYTES: [u8; NULL_POOL_SIZE] = [0; NULL_POOL_SIZE];

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

/// A trait for reading a sanitized table from raw bytes
pub unsafe trait ReadSanitized<'a> {
    type Args: Copy;
    unsafe fn read_sanitized(ptr: FontPtr<'a>, args: &Self::Args) -> Self;
}

/// A trait for resolving a sanitized table from an offset
pub trait ResolveSanitizedOffset {
    unsafe fn resolve_sanitized<'a, T: ReadSanitized<'a>>(
        &self,
        data: FontPtr<'a>,
        args: &T::Args,
    ) -> Option<T>;
}

impl<O: Offset> ResolveSanitizedOffset for O {
    unsafe fn resolve_sanitized<'a, T: ReadSanitized<'a>>(
        &self,
        data: FontPtr<'a>,
        args: &T::Args,
    ) -> Option<T> {
        unsafe {
            self.non_null()
                .map(|off| T::read_sanitized(data.for_offset(off), args))
        }
    }
}

// a utility type for reading fields from a pointer.
//
// This stores a `&'a u8` instead of a raw pointer in order to maintain... a lifetime..
// does this even make sense probably not
#[derive(Clone, Copy)]
pub struct FontPtr<'a>(&'a u8);

// default impl reuses a static slice of zeros
impl<'a> Default for FontPtr<'a> {
    fn default() -> Self {
        Self(&EMPTY_TABLE_BYTES[0])
    }
}

impl<'a> FontPtr<'a> {
    fn raw(&self) -> *const u8 {
        self.0 as *const u8
    }

    pub(crate) unsafe fn read_at<T: Scalar>(&self, offset: usize) -> T {
        let ptr = self.raw().add(offset);
        let temp: &BigEndian<T> = &*(ptr as *const BigEndian<T>);
        temp.get()
    }

    pub(crate) unsafe fn read_array_at<T: AnyBitPattern + FixedSize>(
        &self,
        offset: usize,
        len: usize,
    ) -> &'a [T] {
        let ptr = self.raw().add(offset);
        std::slice::from_raw_parts(ptr as *const _, len)
    }

    unsafe fn for_offset(&self, offset: usize) -> Self {
        let raw = self.raw();
        let new = raw.add(offset);
        Self(&*new)
    }
}

