//! Custom array types

use crate::font_data::FontData;
use crate::parse_prelude::ReadError;
use crate::read::{ComputeSize, FontReadWithArgs};

/// An array whose items size is not known at compile time.
///
/// At runtime, `Args` are provided which will be used to compute the size
/// of each item.
pub struct ComputedArray<'a, Args, T> {
    // the length of each item
    item_len: usize,
    data: FontData<'a>,
    args: Args,
    phantom: std::marker::PhantomData<T>,
}

impl<'a, Args, T: ComputeSize<Args>> ComputedArray<'a, Args, T> {
    pub fn new(data: FontData<'a>, args: Args) -> Self {
        ComputedArray {
            item_len: T::compute_size(&args),
            data,
            args,
            phantom: std::marker::PhantomData,
        }
    }
}

impl<'a, Args, T> ComputedArray<'a, Args, T>
where
    T: FontReadWithArgs<'a, Args> + Default,
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

impl<Args, T> std::fmt::Debug for ComputedArray<'_, Args, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("DynSizedArray")
            .field("bytes", &self.data)
            .finish()
    }
}
