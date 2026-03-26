//! Pre-validating font data.

use bytemuck::AnyBitPattern;
use types::{BigEndian, FixedSize, Nullable, Offset16, Scalar};

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

pub struct ArrayOfSanitizedOffsets<'a, T: ReadSanitized<'a>, O: Scalar = Offset16> {
    offsets: &'a [BigEndian<O>],
    ptr: FontPtr<'a>,
    args: T::Args,
}

pub struct ArrayOfSanitizedNullableOffsets<'a, T: ReadSanitized<'a>, O: Scalar = Offset16> {
    offsets: &'a [BigEndian<Nullable<O>>],
    ptr: FontPtr<'a>,
    args: T::Args,
}

impl<'a, T, O> ArrayOfSanitizedOffsets<'a, T, O>
where
    O: Scalar,
    T: ReadSanitized<'a>,
{
    pub(crate) fn new(offsets: &'a [BigEndian<O>], ptr: FontPtr<'a>, args: T::Args) -> Self {
        Self { offsets, ptr, args }
    }
}

impl<'a, T, O> ArrayOfSanitizedNullableOffsets<'a, T, O>
where
    O: Scalar,
    T: ReadSanitized<'a>,
{
    pub(crate) fn new(
        offsets: &'a [BigEndian<Nullable<O>>],
        ptr: FontPtr<'a>,
        args: T::Args,
    ) -> Self {
        Self { offsets, ptr, args }
    }
}

impl<'a, T, O> ArrayOfSanitizedOffsets<'a, T, O>
where
    O: Scalar + Offset,
    T: ReadSanitized<'a> + Default,
    T::Args: Copy + 'static,
{
    /// The number of offsets in the array
    pub fn len(&self) -> usize {
        self.offsets.len()
    }

    /// `true` if the array is empty
    pub fn is_empty(&self) -> bool {
        self.offsets.is_empty()
    }

    /// Resolve the offset at the provided index.
    pub fn get(&self, idx: usize) -> Option<T> {
        self.offsets
            .get(idx)
            .map(|o| unsafe { o.get().resolve_sanitized(self.ptr, &self.args) }.unwrap_or_default())
    }

    /// Iterate over all of the offset targets.
    ///
    /// Each offset will be resolved as it is encountered.
    pub fn iter(&self) -> impl Iterator<Item = T> + 'a {
        let mut iter = self.offsets.iter();
        let args = self.args;
        let ptr = self.ptr;
        std::iter::from_fn(move || {
            iter.next()
                .map(|off| unsafe { off.get().resolve_sanitized(ptr, &args).unwrap_or_default() })
        })
    }
}

impl<'a, T, O> ArrayOfSanitizedNullableOffsets<'a, T, O>
where
    O: Scalar + Offset,
    T: ReadSanitized<'a> + Default,
    T::Args: Copy + 'static,
{
    /// The number of offsets in the array
    pub fn len(&self) -> usize {
        self.offsets.len()
    }

    /// `true` if the array is empty
    pub fn is_empty(&self) -> bool {
        self.offsets.is_empty()
    }

    /// Resolve the offset at the provided index.
    ///
    /// `None` means the index is out of bounds; `Some(None)` means the index
    /// is valid but the offset is null.
    pub fn get(&self, idx: usize) -> Option<Option<T>> {
        self.offsets
            .get(idx)
            .map(|o| unsafe { o.get().offset().resolve_sanitized(self.ptr, &self.args) })
    }

    /// Iterate over all of the offset targets.
    ///
    /// Each offset will be resolved as it is encountered. Null offsets are
    /// represented as `None`.
    pub fn iter(&self) -> impl Iterator<Item = Option<T>> + 'a {
        let mut iter = self.offsets.iter();
        let args = self.args;
        let ptr = self.ptr;
        std::iter::from_fn(move || {
            iter.next()
                .map(|off| unsafe { off.get().offset().resolve_sanitized(ptr, &args) })
        })
    }
}
/// A sanitize-friendly analog of [`ComputedArray`].
///
/// Stores a `FontPtr` advanced to the start of the array data, along with a
/// pre-computed per-item byte length and the args needed to read each item.
/// Items are read on demand via [`ReadSanitized`] without bounds checking.
pub struct ComputedArraySanitized<'a, T: ReadSanitized<'a>> {
    ptr: FontPtr<'a>,
    count: usize,
    item_len: usize,
    args: T::Args,
}

impl<'a, T: ReadSanitized<'a>> ComputedArraySanitized<'a, T> {
    pub(crate) fn new(ptr: FontPtr<'a>, count: usize, item_len: usize, args: T::Args) -> Self {
        Self {
            ptr,
            count,
            item_len,
            args,
        }
    }

    pub fn len(&self) -> usize {
        self.count
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
}

impl<'a, T> ComputedArraySanitized<'a, T>
where
    T: ReadSanitized<'a>,
    T::Args: Copy + 'static,
{
    /// Return the item at `idx`.
    pub fn get(&self, idx: usize) -> T {
        let offset = idx.saturating_mul(self.item_len);
        unsafe { T::read_sanitized(self.ptr.for_offset(offset), &self.args) }
    }

    /// Iterate over all items.
    pub fn iter(&self) -> impl Iterator<Item = T> + 'a {
        let mut i = 0;
        let count = self.count;
        let item_len = self.item_len;
        let args = self.args;
        let ptr = self.ptr;
        std::iter::from_fn(move || {
            if i >= count {
                return None;
            }
            let offset = i.saturating_mul(item_len);
            i += 1;
            Some(unsafe { T::read_sanitized(ptr.for_offset(offset), &args) })
        })
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

    pub(crate) unsafe fn for_offset(&self, offset: usize) -> Self {
        let raw = self.raw();
        let new = raw.add(offset);
        Self(&*new)
    }
}
