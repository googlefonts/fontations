//! Pre-validating font data for more efficient access.
//!
//! This module contains our version of the [`sanitize` machinery][hb_sanitize]
//! from HarfBuzz.
//!
//! The basic idea is straightforward: instead of doing bounds checks whenever a
//! field is accessed (or an offset is chased) we validate an entire font table
//! (recursively, including its subtables) once, and then elide subsequent
//! bounds checking, which shows significant speedups for certain important uses
//! cases, such as shaping.
//!
//! At a high level, sanitization follows the following process:
//! - a 'normal' (non-sanitized) table is read using the [`FontRead`] trait.
//!   this performs extremely basic validation.
//! - Once the table has been read, it is sanitized, by calling its implementation
//!   of the [`Sanitize`] trait. This implementation checks that all fields
//!   accessible via the table are in-bounds of the table's data, and then
//!   recursively ensures the same is true for any table reachable via an offset.
//! - If sanitization completes successfully, a special 'Sanitized' type is returned.
//!   This type is tied to precisely the data that was used during sanitization,
//!   and allows for access to fields and subtables without any further bounds
//!   checking.
//!
//! ```no_run
//! # use read_fonts::TableProvider;
//! use read_fonts::sanitize::TrySanitize;
//!
//! # fn get_font() -> read_fonts::FontRef<'static> { todo!() }
//!
//! let font = get_font();
//! let gpos = font.gpos().expect("read gpos failed");
//! let gpos_sanitized = gpos.try_sanitize().expect("sanitize failed");
//! let normal_script_list = gpos.script_list().unwrap(); // normally this is checked;
//! let sanitized_script_list = gpos_sanitized.script_list();
//! assert_eq!(normal_script_list.script_count(), sanitized_script_list.script_count());
//!
//! ```
//!
//! [hb_sanitize]: https://github.com/harfbuzz/harfbuzz/blob/90116a529/src/hb-sanitize.hh#L38

#![deny(clippy::arithmetic_side_effects)]

use bytemuck::AnyBitPattern;
use types::{BigEndian, FixedSize, Nullable, Offset16, Scalar};

use crate::{
    array::{ComputedArray, VarLenArray},
    font_data::FontData,
    read::{ComputeSize, FontRead, FontReadWithArgs, VarSize},
    Offset, ReadError,
};

// https://github.com/harfbuzz/harfbuzz/blob/aba63bb5f8cb6cfc77ee8cfc2700b3ed9c0838ef/src/hb-null.hh#L40
/// The number of bytes required to represent the largest table we have.
///
/// This is checked by an assert at compile time, and can be increased as needed
pub(crate) const NULL_POOL_SIZE: usize = 16;
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

/// Attempt to sanitize a table.
///
/// On success this returns a type that allows unchecked access to its fields.
pub trait TrySanitize: Sanitize {
    type Sanitized;
    fn try_sanitize(&self) -> Result<Self::Sanitized, ReadError>;
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

/// A trait for unchecked reading of a font table from bytes.
///
/// This trait is part of the [sanitize system], and is generally only expected
/// to be implemented through code generation.
///
/// [sanitize system]: crate::sanitize
pub trait ReadSanitized<'a> {
    /// Any arguments required by this table.
    type Args: Copy;
    /// Reinterpret the provided bytes as `Self`, without any bounds checking.
    ///
    /// # Safety
    ///
    /// This should only ever be called with data that has already been validated
    /// via the corresponding [`Sanitize`] implementation. See the [mod sanitize]
    /// for more information.
    ///
    /// [mod sanitize]: crate::sanitize
    unsafe fn read_sanitized(ptr: FontPtr<'a>, args: &Self::Args) -> Self;
}

/// A trait for resolving a sanitized table from an offset.
///
/// This trait is used in the typed getters of sanitized tables and records to
/// resolve offsets to other tables.
pub(crate) trait ResolveSanitizedOffset {
    /// Resolve a sanitized offset.
    ///
    /// Should return `None` only if the offset is null.
    ///
    /// # Safety
    ///
    /// This method is only intended to be called from generated code.
    ///
    /// In the small number of cases where a manual offset getter is required,
    /// the caller must ensure that the equivalent getter on the corresponding
    /// non-sanitize table is called as part of that table's [`Sanitize`]
    /// implementation, and that the provided `data` is derived from exactly
    /// the data that was used to resolve this table during [`Sanitize`].
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
                .map(|off| T::read_sanitized(data.split_off_unchecked(off), args))
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
    /// # Safety
    ///
    /// Should only ever be created from generated code, in a table that has
    /// already been [sanitized][crate::sanitize].
    ///
    /// Concretely, all offsets in the array must point to positions in the buffer
    /// that are valid for a table of type `T`.
    pub(crate) unsafe fn new(offsets: &'a [BigEndian<O>], ptr: FontPtr<'a>, args: T::Args) -> Self {
        Self { offsets, ptr, args }
    }
}

impl<'a, T, O> ArrayOfSanitizedNullableOffsets<'a, T, O>
where
    O: Scalar,
    T: ReadSanitized<'a>,
{
    /// # Safety
    ///
    /// Should only ever be created from generated code, in a table that has
    /// already been [sanitized][crate::sanitize].
    ///
    /// Concretely, all offsets in the array must point to positions in the buffer
    /// that are valid for a table of type `T`.
    pub(crate) unsafe fn new(
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
    /// # Safety
    ///
    /// Should only ever be created from generated code, in a table that has
    /// already been [sanitized][crate::sanitize].
    ///
    /// Concretely, the buffer must be large enough to contain all of the items.
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
    pub fn get(&self, idx: usize) -> Option<T> {
        if idx >= self.count {
            return None;
        }
        let offset = idx.saturating_mul(self.item_len);
        Some(unsafe { T::read_sanitized(self.ptr.split_off_unchecked(offset), &self.args) })
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
            // if i == u32::MAX, our offset is out of bounds, but we don't
            // want to loop forever on corrupt data..
            i = i.checked_add(1)?;
            Some(unsafe { T::read_sanitized(ptr.split_off_unchecked(offset), &args) })
        })
    }

    #[inline]
    pub fn binary_search_by<F>(&self, mut f: F) -> Option<T>
    where
        F: FnMut(T) -> std::cmp::Ordering,
    {
        let mut size = self.len();
        if size == 0 {
            return None;
        }
        let mut base = 0usize;

        // This loop intentionally doesn't have an early exit if the comparison
        // returns Equal. We want the number of loop iterations to depend *only*
        // on the size of the input slice so that the CPU can reliably predict
        // the loop count.
        while size > 1 {
            let half = size / 2;
            let mid = base + half;

            // SAFETY: the call is made safe by the following invariants:
            // - `mid >= 0`: by definition
            // - `mid < size`: `mid = size / 2 + size / 4 + size / 8 ...`
            let cmp = f(unsafe { self.get(mid).unwrap_unchecked() });

            // Binary search interacts poorly with branch prediction, so force
            // the compiler to use conditional moves if supported by the target
            // architecture.
            //base = std::hint::select_unpredictable(cmp == std::cmp::Ordering::Greater, base, mid);
            base = if cmp == std::cmp::Ordering::Greater {
                base
            } else {
                mid
            };

            // This is imprecise in the case where `size` is odd and the
            // comparison returns Greater: the mid element still gets included
            // by `size` even though it's known to be larger than the element
            // being searched for.
            //
            // This is fine though: we gain more performance by keeping the
            // loop iteration count invariant (and thus predictable) than we
            // lose from considering one additional element.
            size -= half;
        }

        // SAFETY: base is always in [0, size) because base <= mid.
        let cmp = f(unsafe { self.get(base).unwrap_unchecked() });
        if cmp == std::cmp::Ordering::Equal {
            // SAFETY: same as the `get_unchecked` above.
            unsafe { std::hint::assert_unchecked(base < self.len()) };
            Some(unsafe { self.get(base).unwrap_unchecked() })
        } else {
            let result = base + (cmp == std::cmp::Ordering::Less) as usize;
            // SAFETY: same as the `get_unchecked` above.
            // Note that this is `<=`, unlike the assume in the `Ok` path.
            unsafe { std::hint::assert_unchecked(result <= self.len()) };
            None
        }
    }
    pub fn binary_search_by_key<F, B>(&self, b: &B, mut f: F) -> Option<T>
    where
        F: FnMut(T) -> B,
        B: Ord,
    {
        self.binary_search_by(|item| f(item).cmp(b))
    }
}

/// A type providing unchecked access to font data.
// NOTE:
// this used to be more like a pointer, and is now just a wrapper around FontData.
// It's useful to have FontData because it means we can fallback to non-sanitize
// types from sanitize ones, and also because we do preserve the bounds in case
// we need them for some unknown future types? So maybe this could all just go
// away...
#[derive(Clone, Copy)]
pub struct FontPtr<'a>(FontData<'a>);

// default impl reuses a static slice of zeros
impl Default for FontPtr<'_> {
    fn default() -> Self {
        Self(EMPTY_TABLE_BYTES.as_slice().into())
    }
}

impl<'a> FontPtr<'a> {
    /// Construct a `FontPtr` from a byte slice.
    pub fn new(data: FontData<'a>) -> Self {
        if data.is_empty() {
            Default::default()
        } else {
            Self(data)
        }
    }

    fn raw(&self) -> *const u8 {
        self.0.as_bytes().as_ptr()
    }

    /// Read a scalar from the buffer without bounds checks.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the position in this buffer referenced by
    /// `offset` can represent the relevant scalar.
    pub(crate) unsafe fn read_at_unchecked<T: Scalar>(&self, offset: usize) -> T {
        let ptr = self.raw().add(offset);
        let temp: &BigEndian<T> = &*(ptr as *const BigEndian<T>);
        temp.get()
    }

    /// Read a slice from the buffer, without bounds checks.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the position in this buffer referenced by
    /// `offset` contains at least `T::RAW_BYTE_LEN * len` bytes.
    pub(crate) unsafe fn read_array_at_unchecked<T: AnyBitPattern + FixedSize>(
        &self,
        offset: usize,
        len: usize,
    ) -> &'a [T] {
        let ptr = self.raw().add(offset);
        std::slice::from_raw_parts(ptr as *const _, len)
    }

    /// Advance the pointer by `offset` bytes, without bounds checks.
    ///
    /// # Safety
    ///
    /// The caller must ensure that `offset` is in bounds.
    pub(crate) unsafe fn split_off_unchecked(&self, offset: usize) -> Self {
        let inner = self.0.as_bytes();
        let new = inner.get_unchecked(offset..);
        Self(FontData::new(new))
    }

    pub fn into_font_data(self) -> FontData<'a> {
        self.0
    }

    pub fn as_bytes(&self) -> &'a [u8] {
        self.0.as_bytes()
    }
}
