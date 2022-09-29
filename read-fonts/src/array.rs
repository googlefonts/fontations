//! Custom array types

use font_types::{FixedSized, ReadScalar};

use crate::read::{ComputeSize, FontReadWithArgs, ReadArgs};
use crate::{FontData, FontRead, ReadError};

/// An array whose items size is not known at compile time.
///
/// At runtime, `Args` are provided which will be used to compute the size
/// of each item.
#[derive(Clone)]
pub struct ComputedArray<'a, T: ReadArgs> {
    // the length of each item
    item_len: usize,
    len: usize,
    data: FontData<'a>,
    args: T::Args,
}

impl<'a, T: ComputeSize> ComputedArray<'a, T> {
    pub fn new(data: FontData<'a>, args: T::Args) -> Self {
        let item_len = T::compute_size(&args);
        let len = data.len() / item_len;
        ComputedArray {
            item_len,
            len,
            data,
            args,
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl<'a, T: ReadArgs> ReadArgs for ComputedArray<'a, T> {
    type Args = T::Args;
}

impl<'a, T> FontReadWithArgs<'a> for ComputedArray<'a, T>
where
    T: ComputeSize + FontReadWithArgs<'a>,
    T::Args: Copy,
{
    fn read_with_args(data: FontData<'a>, args: &Self::Args) -> Result<Self, ReadError> {
        Ok(Self::new(data, *args))
    }
}

impl<'a, T> ComputedArray<'a, T>
where
    T: FontReadWithArgs<'a>,
    T::Args: Copy + 'static,
{
    pub fn iter(&self) -> impl Iterator<Item = Result<T, ReadError>> + 'a {
        let mut i = 0;
        let data = self.data;
        let args = self.args;
        let item_len = self.item_len;
        let len = self.len;

        std::iter::from_fn(move || {
            let args = args;
            if i == len {
                return None;
            }
            let item_start = item_len * i;
            i += 1;
            let data = data.split_off(item_start)?;
            Some(T::read_with_args(data, &args))
        })
    }

    pub fn get(&self, idx: usize) -> Result<T, ReadError> {
        let item_start = idx * self.item_len;
        self.data
            .split_off(item_start)
            .ok_or(ReadError::OutOfBounds)
            .and_then(|data| T::read_with_args(data, &self.args))
    }
}

impl<T: ReadArgs> std::fmt::Debug for ComputedArray<'_, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("DynSizedArray")
            .field("bytes", &self.data)
            .finish()
    }
}

/// A trait for types that have variable length.
///
/// As a rule, these types have an initial length field.
pub trait VarLen {
    /// The type of the first (length) field of the item.
    type Len: ReadScalar + FixedSized + Into<u32>;

    #[doc(hidden)]
    fn read_len_at(data: FontData, pos: usize) -> Option<usize> {
        let asu32 = data.read_at::<Self::Len>(pos).ok()?.into();
        Some(asu32 as usize + Self::Len::RAW_BYTE_LEN)
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

impl<'a, T: FontRead<'a> + VarLen> VarLenArray<'a, T> {
    /// Return the item at the provided index.
    ///
    /// This performs a linear search.
    pub fn get(&self, idx: usize) -> Option<Result<T, ReadError>> {
        let mut pos = 0;
        for _ in 0..idx {
            pos += T::read_len_at(self.data, pos)?;
        }
        self.data.split_off(pos).map(T::read)
    }

    /// Return an iterator over this array's items.
    pub fn iter(&self) -> impl Iterator<Item = Result<T, ReadError>> + 'a {
        let mut data = self.data;
        std::iter::from_fn(move || {
            if data.is_empty() {
                return None;
            }

            let item_len = T::read_len_at(data, 0)?;
            let next = T::read(data);
            data = data.split_off(item_len)?;
            Some(next)
        })
    }
}

impl<'a, T> FontRead<'a> for VarLenArray<'a, T> {
    fn read(data: FontData<'a>) -> Result<Self, ReadError> {
        Ok(VarLenArray {
            data,
            phantom: core::marker::PhantomData,
        })
    }
}
