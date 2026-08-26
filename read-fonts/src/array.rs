//! Custom array types

#![deny(clippy::arithmetic_side_effects)]

use bytemuck::AnyBitPattern;
use font_types::FixedSize;

use crate::read::{ComputeSize, FontRead, ReadArgs, VarSize};
use crate::{FontData, ReadError};

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
pub(crate) fn get_pair<T>(slice: &[T], idx: usize) -> Result<&[T; 2], ReadError> {
    slice
        .get(idx..)
        .ok_or(ReadError::OutOfBounds)?
        .first_chunk()
        .ok_or(ReadError::OutOfBounds)
}

/// An array of records paired with the data of the enclosing table.
///
/// Records may contain offsets, and these offsets are resolved against the
/// data of the table that contains the record; a bare record has no way of
/// getting at that data, which is why the record's generated offset getters
/// require it to be passed in. Bundling the data up with the records lets
/// the items of this array (each an [`OffsetResolving`]) resolve their own
/// offsets, unburdening the user from needing to determine the appropriate
/// input data (and making it impossible to pass the *wrong* data).
///
/// This is the analog, for arrays of records containing offsets, of
/// [`ArrayOfOffsets`][crate::offset_array::ArrayOfOffsets].
pub struct ArrayOfRecordsWithOffsetData<'a, T> {
    data: FontData<'a>,
    records: &'a [T],
}

impl<'a, T> ArrayOfRecordsWithOffsetData<'a, T> {
    pub(crate) fn new(records: &'a [T], data: FontData<'a>) -> Self {
        Self { data, records }
    }

    /// The number of records in the array.
    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Return the record at `idx`, or `None` if it is out of bounds.
    pub fn get(&self, idx: usize) -> Option<OffsetResolving<'a, T>> {
        self.records.get(idx).map(|record| OffsetResolving {
            data: self.data,
            record,
        })
    }

    /// Return an iterator over the records in this array.
    pub fn iter(&self) -> ArrayOfRecordsIter<'a, T> {
        ArrayOfRecordsIter {
            data: self.data,
            inner: self.records.iter(),
        }
    }

    /// The records, as a plain slice.
    ///
    /// This is an escape hatch for slice-only APIs like `binary_search_by`;
    /// pair the resulting index with [`get`][Self::get] to recover an
    /// offset-resolving record.
    pub fn as_slice(&self) -> &'a [T] {
        self.records
    }

    /// The data of the enclosing table, against which record offsets resolve.
    pub fn offset_data(&self) -> FontData<'a> {
        self.data
    }
}

impl<T> Clone for ArrayOfRecordsWithOffsetData<'_, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for ArrayOfRecordsWithOffsetData<'_, T> {}

impl<T> Default for ArrayOfRecordsWithOffsetData<'_, T> {
    fn default() -> Self {
        Self {
            data: FontData::default(),
            records: &[],
        }
    }
}

impl<T: std::fmt::Debug> std::fmt::Debug for ArrayOfRecordsWithOffsetData<'_, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_list().entries(self.records).finish()
    }
}

impl<'a, T> IntoIterator for ArrayOfRecordsWithOffsetData<'a, T> {
    type Item = OffsetResolving<'a, T>;
    type IntoIter = ArrayOfRecordsIter<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, T> IntoIterator for &ArrayOfRecordsWithOffsetData<'a, T> {
    type Item = OffsetResolving<'a, T>;
    type IntoIter = ArrayOfRecordsIter<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// An iterator over an [`ArrayOfRecordsWithOffsetData`].
pub struct ArrayOfRecordsIter<'a, T> {
    data: FontData<'a>,
    inner: std::slice::Iter<'a, T>,
}

impl<'a, T> Iterator for ArrayOfRecordsIter<'a, T> {
    type Item = OffsetResolving<'a, T>;

    fn next(&mut self) -> Option<Self::Item> {
        let record = self.inner.next()?;
        Some(OffsetResolving {
            data: self.data,
            record,
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<T> DoubleEndedIterator for ArrayOfRecordsIter<'_, T> {
    fn next_back(&mut self) -> Option<Self::Item> {
        let record = self.inner.next_back()?;
        Some(OffsetResolving {
            data: self.data,
            record,
        })
    }
}

impl<T> ExactSizeIterator for ArrayOfRecordsIter<'_, T> {}

impl<T> Clone for ArrayOfRecordsIter<'_, T> {
    fn clone(&self) -> Self {
        Self {
            data: self.data,
            inner: self.inner.clone(),
        }
    }
}

/// A record paired with the data of the enclosing table.
///
/// This derefs to the record itself, so all of the record's methods are
/// available; in addition, for each offset in the record, codegen provides
/// a getter *on this type* that resolves the offset without requiring the
/// caller to pass in the enclosing table's data.
pub struct OffsetResolving<'a, T> {
    data: FontData<'a>,
    record: &'a T,
}

impl<'a, T> OffsetResolving<'a, T> {
    /// The underlying record.
    pub fn record(&self) -> &'a T {
        self.record
    }

    /// The data of the enclosing table, against which offsets resolve.
    pub fn offset_data(&self) -> FontData<'a> {
        self.data
    }
}

impl<T> std::ops::Deref for OffsetResolving<'_, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        self.record
    }
}

impl<T> Clone for OffsetResolving<'_, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for OffsetResolving<'_, T> {}

impl<T: std::fmt::Debug> std::fmt::Debug for OffsetResolving<'_, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.record.fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_test::records::VarLenItem;
    use font_test_data::bebuffer::BeBuffer;

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
