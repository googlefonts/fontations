//! Custom array types

#![deny(clippy::arithmetic_side_effects)]

use bytemuck::AnyBitPattern;
use font_types::FixedSize;

use crate::read::{ComputeSize, FontRead, FontReadAt, ReadArgs, VarSize};
use crate::{FontData, ReadError};
use core::ops::Range;

/// An array whose items size is not known at compile time.
///
/// This requires the inner type to implement [`FontRead`] as well as
/// [`ComputeSize`].
///
/// At runtime, `Args` are provided which will be used to compute the size
/// of each item; this size is then used to compute the positions of the items
/// within the underlying data, from which they will be read lazily.
#[derive(Clone)]
pub struct ComputedArray<'a, T: ReadArgs> {
    // the length of each item
    item_len: usize,
    len: usize,
    data: FontData<'a>,
    args: T::Args,
}

impl<'a, T: ComputeSize> ComputedArray<'a, T> {
    pub fn new(data: FontData<'a>, args: T::Args) -> Result<Self, ReadError> {
        let item_len = T::compute_size(args)?;
        let len = data.len().checked_div(item_len).unwrap_or(0);
        Ok(ComputedArray {
            item_len,
            len,
            data,
            args,
        })
    }

    /// The number of items in the array
    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl<T: ReadArgs> ReadArgs for ComputedArray<'_, T> {
    type Args = T::Args;
}

impl<'a, T> FontRead<'a> for ComputedArray<'a, T>
where
    T: ComputeSize + FontRead<'a>,
    T::Args: Copy,
{
    fn read_with_args(data: FontData<'a>, args: Self::Args) -> Result<Self, ReadError> {
        Self::new(data, args)
    }
}

impl<T> Default for ComputedArray<'_, T>
where
    T: ReadArgs,
    T::Args: Default,
{
    fn default() -> Self {
        Self {
            item_len: 0,
            len: 0,
            data: Default::default(),
            args: Default::default(),
        }
    }
}

impl<'a, T> ComputedArray<'a, T>
where
    T: FontRead<'a>,
    T::Args: Copy + 'static,
{
    pub fn iter(&self) -> impl Iterator<Item = Result<T, ReadError>> + 'a {
        let mut i = 0;
        let data = self.data;
        let args = self.args;
        let item_len = self.item_len;
        let len = self.len;

        std::iter::from_fn(move || {
            if i == len {
                return None;
            }
            let item_start = item_len.checked_mul(i)?;
            i = i.checked_add(1)?;
            let data = data.split_off(item_start)?;
            Some(T::read_with_args(data, args))
        })
    }

    #[inline]
    pub fn get(&self, idx: usize) -> Result<T, ReadError> {
        let item_start = idx
            .checked_mul(self.item_len)
            .ok_or(ReadError::OutOfBounds)?;
        self.data
            .split_off(item_start)
            .ok_or(ReadError::OutOfBounds)
            .and_then(|data| T::read_with_args(data, self.args))
    }
}

impl<T: ReadArgs> std::fmt::Debug for ComputedArray<'_, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("DynSizedArray")
            .field("bytes", &self.data)
            .finish()
    }
}

/// An array of items located by position within enclosing data.
///
/// This is the [`FontReadAt`] counterpart to [`ComputedArray`]: item size is
/// computed at runtime from `Args`, but items are addressed by offset within
/// `data` rather than by slicing it, so each item keeps the enclosing table's
/// data and can resolve offsets relative to it.
#[derive(Clone)]
pub struct PositionedArray<'a, T: ReadArgs> {
    data: FontData<'a>,
    /// Position of the first item within `data`.
    start: usize,
    /// The length of each item.
    item_len: usize,
    len: usize,
    args: T::Args,
}

impl<'a, T: ComputeSize> PositionedArray<'a, T> {
    /// Creates an array of the items occupying `range` within `data`.
    ///
    /// `data` is retained in full; `range` only locates the items.
    pub fn new(data: FontData<'a>, range: Range<usize>, args: T::Args) -> Result<Self, ReadError> {
        let item_len = T::compute_size(args)?;
        // the whole range must be present, matching `ComputedArray`, which is
        // built from data already sliced to it
        let available = data
            .as_bytes()
            .get(range.clone())
            .ok_or(ReadError::OutOfBounds)?
            .len();
        let len = available.checked_div(item_len).unwrap_or(0);
        Ok(PositionedArray {
            data,
            start: range.start,
            item_len,
            len,
            args,
        })
    }

    /// The number of items in the array.
    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl<T: ReadArgs> ReadArgs for PositionedArray<'_, T> {
    type Args = T::Args;
}

impl<T> Default for PositionedArray<'_, T>
where
    T: ReadArgs,
    T::Args: Default,
{
    fn default() -> Self {
        Self {
            data: Default::default(),
            start: 0,
            item_len: 0,
            len: 0,
            args: Default::default(),
        }
    }
}

/// The number of items whose offsets are representable.
///
/// Item `i` sits at `start + i * item_len`, and walking the array computes one
/// offset past the last item, so this is the largest `len` for which
/// `start + len * item_len` does not overflow. Establishing it once up front
/// lets iteration be a plain add per step.
///
/// An array built by [`PositionedArray::new`] is already within this bound: its
/// last offset is `range.end`, which had to be a valid index into `data`.
fn representable_len(start: usize, item_len: usize, len: usize) -> usize {
    if item_len == 0 {
        return len;
    }
    // `start` is a usize, so this cannot underflow
    #[allow(clippy::arithmetic_side_effects)]
    let max_len = (usize::MAX - start) / item_len;
    len.min(max_len)
}

impl<'a, T> PositionedArray<'a, T>
where
    T: FontReadAt<'a>,
    T::Args: Copy + 'static,
{
    /// Returns the item at `idx`.
    #[inline]
    pub fn get(&self, idx: usize) -> Result<T, ReadError> {
        if idx >= self.len {
            return Err(ReadError::OutOfBounds);
        }
        let offset = idx
            .checked_mul(self.item_len)
            .and_then(|off| off.checked_add(self.start))
            .ok_or(ReadError::OutOfBounds)?;
        T::read_at(self.data, offset, self.args)
    }

    /// Returns an iterator over the items.
    pub fn iter(&self) -> impl Iterator<Item = Result<T, ReadError>> + 'a + Clone {
        let data = self.data;
        let args = self.args;
        let item_len = self.item_len;
        // Bounding the length here makes every offset below representable,
        // including the one computed past the last item, so each step is a
        // plain add. An array long enough to overflow simply stops early, where
        // per-item checked arithmetic would have started failing anyway.
        let len = representable_len(self.start, item_len, self.len);
        let mut offset = self.start;
        (0..len).map(move |_| {
            let item = T::read_at(data, offset, args);
            #[allow(clippy::arithmetic_side_effects)] // bounded by representable_len
            {
                offset += item_len;
            }
            item
        })
    }
}

impl<T: ReadArgs> std::fmt::Debug for PositionedArray<'_, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("PositionedArray")
            .field("start", &self.start)
            .field("item_len", &self.item_len)
            .field("len", &self.len)
            .finish()
    }
}

/// An array of items of non-uniform length.
///
/// Random access into this array cannot be especially efficient, since it requires
/// a linear scan.
pub struct VarLenArray<'a, T> {
    data: FontData<'a>,
    phantom: std::marker::PhantomData<*const T>,
}

impl<'a, T: FontRead<'a, Args = ()> + VarSize> VarLenArray<'a, T> {
    /// Return the item at the provided index.
    ///
    /// # Performance
    ///
    /// Determining the position of an item in this collection requires looking
    /// at all the preceding items; that is, it is `O(n)` instead of `O(1)` as
    /// it would be for a `Vec`.
    ///
    /// As a consequence, calling this method in a loop could potentially be
    /// very slow. If this is something you need to do, it will probably be
    /// much faster to first collect all the items into a `Vec` beforehand,
    /// and then fetch them from there.
    pub fn get(&self, idx: usize) -> Option<Result<T, ReadError>> {
        if self.data.is_empty() {
            return None;
        }
        let mut pos = 0usize;
        for _ in 0..idx {
            pos = pos.checked_add(T::read_len_at(self.data, pos)?)?;
        }
        let len = T::read_len_at(self.data, pos)?;
        let end = pos.checked_add(len)?;
        self.data.slice(pos..end).map(T::read)
    }

    /// Return an iterator over this array's items.
    pub fn iter(&self) -> impl Iterator<Item = Result<T, ReadError>> + 'a {
        let mut data = self.data;
        std::iter::from_fn(move || {
            if data.is_empty() {
                return None;
            }

            let item_len = T::read_len_at(data, 0)?;
            // If the length is 0 then then it's not useful to continue
            // iteration. The subsequent read will probably fail but if
            // the user is skipping malformed elements (which is common)
            // this this iterator will continue forever.
            if item_len == 0 {
                return None;
            }
            let item_data = data.slice(..item_len)?;
            let next = T::read(item_data);
            data = data.split_off(item_len)?;
            Some(next)
        })
    }
}

impl<T> ReadArgs for VarLenArray<'_, T> {
    type Args = ();
}

impl<'a, T> FontRead<'a> for VarLenArray<'a, T> {
    fn read_with_args(data: FontData<'a>, _: ()) -> Result<Self, ReadError> {
        Ok(VarLenArray {
            data,
            phantom: core::marker::PhantomData,
        })
    }
}

impl<T> Default for VarLenArray<'_, T> {
    fn default() -> Self {
        Self {
            data: Default::default(),
            phantom: std::marker::PhantomData,
        }
    }
}

impl<T: AnyBitPattern> ReadArgs for &[T] {
    type Args = u16;
}

impl<'a, T: AnyBitPattern + FixedSize> FontRead<'a> for &'a [T] {
    fn read_with_args(data: FontData<'a>, args: u16) -> Result<Self, ReadError> {
        let len = (args as usize)
            .checked_mul(T::RAW_BYTE_LEN)
            .ok_or(ReadError::OutOfBounds)?;
        data.read_array(0..len)
    }
}

/// Helper to retrieve a pair of items from a slice, returning an error
/// if the second index overflows or if either index is out of bounds.
pub(crate) fn get_pair<T>(slice: &[T], idx: usize) -> Result<[&T; 2], ReadError> {
    let second_idx = idx.checked_add(1).ok_or(ReadError::OutOfBounds)?;
    let first = slice.get(idx).ok_or(ReadError::OutOfBounds)?;
    let second = slice.get(second_idx).ok_or(ReadError::OutOfBounds)?;
    Ok([first, second])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_test::records::VarLenItem;
    use font_test_data::bebuffer::BeBuffer;

    /// The bound must admit every array that can actually be built, and must
    /// keep `start + len * item_len` representable in the cases that cannot.
    #[test]
    fn representable_len_bound() {
        // ordinary arrays are unaffected
        assert_eq!(representable_len(0, 4, 10), 10);
        assert_eq!(representable_len(100, 4, 10), 10);
        // a zero stride has no offsets to overflow
        assert_eq!(representable_len(usize::MAX, 0, 10), 10);

        // exactly at the limit
        assert_eq!(representable_len(0, 1, usize::MAX), usize::MAX);
        assert_eq!(representable_len(2, 1, usize::MAX), usize::MAX - 2);
        assert_eq!(representable_len(usize::MAX, 1, 5), 0);

        // and the bound it promises actually holds
        for (start, item_len, len) in [
            (0usize, 4usize, 10usize),
            (100, 4, 10),
            (usize::MAX, 1, 5),
            (usize::MAX - 8, 4, 100),
            (usize::MAX / 2, 3, usize::MAX),
        ] {
            let bounded = representable_len(start, item_len, len);
            assert!(bounded <= len);
            assert!(
                bounded
                    .checked_mul(item_len)
                    .and_then(|off| off.checked_add(start))
                    .is_some(),
                "start {start} item_len {item_len} len {len} -> {bounded}"
            );
        }
    }

    impl VarSize for VarLenItem<'_> {
        type Size = u32;

        fn read_len_at(data: FontData, pos: usize) -> Option<usize> {
            data.read_at::<u32>(pos).ok().map(|len| len as usize)
        }
    }

    /// HB/HarfRuzz test "shlana_9_006" has a morx table containing a chain
    /// with a length of 0. This caused the VarLenArray iterator to loop
    /// indefinitely.
    #[test]
    fn var_len_iter_with_zero_length_item() {
        // Create a buffer containing three elements where the last
        // has zero length
        let mut buf = BeBuffer::new();
        buf = buf.push(8u32).extend([0u8; 4]);
        buf = buf.push(18u32).extend([0u8; 14]);
        buf = buf.push(0u32);
        let arr: VarLenArray<VarLenItem> = VarLenArray::read(FontData::new(buf.data())).unwrap();
        // Ensure we don't iterate forever and only read two elements (the
        // take() exists so that the test fails rather than hanging if the
        // code regresses in the future)
        assert_eq!(arr.iter().take(10).count(), 2);
    }

    #[test]
    fn var_len_iter_same_as_get() {
        let mut buf = BeBuffer::new();
        buf = buf.push(4u32).extend([1u8, 2, 3, 4]);
        buf = buf.push(2u32).extend([5u8, 6]);
        buf = buf.push(3u32).extend([7u8, 8, 9]);
        let arr: VarLenArray<VarLenItem> = VarLenArray::read(FontData::new(buf.data())).unwrap();
        let iter_items: Vec<_> = arr.iter().map(|x| x.unwrap()).collect();
        let get_items: Vec<_> = (0..iter_items.len())
            .map(|i| arr.get(i).unwrap().unwrap())
            .collect();
        assert_eq!(iter_items.len(), get_items.len());
        for (a, b) in iter_items.iter().zip(get_items.iter()) {
            assert_eq!(a.data(), b.data());
        }
    }
}
