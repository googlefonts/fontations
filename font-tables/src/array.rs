//! Custom array types

use crate::font_data::FontData;
use crate::parse_prelude::ReadError;
use crate::read::{ComputeSize, FontReadWithArgs, ReadArgs};

/// An array whose items size is not known at compile time.
///
/// At runtime, `Args` are provided which will be used to compute the size
/// of each item.
#[derive(Clone)]
pub struct ComputedArray<'a, T: ReadArgs> {
    // the length of each item
    item_len: usize,
    data: FontData<'a>,
    args: T::Args,
}

impl<'a, T: ComputeSize> ComputedArray<'a, T> {
    pub fn new(data: FontData<'a>, args: T::Args) -> Self {
        ComputedArray {
            item_len: T::compute_size(&args),
            data,
            args,
        }
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
    T: FontReadWithArgs<'a> + Default,
{
    pub fn iter<'b: 'a>(&'b self) -> impl Iterator<Item = Result<T, ReadError>> + 'b {
        let mut i = 0;
        std::iter::from_fn(move || {
            let item_start = self.item_len * i;
            i += 1;
            let data = self.data.split_off(item_start)?;
            Some(T::read_with_args(data, &self.args))
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
