use crate::FontReadWithArgs;

/// An array whose items size is not known at compile time.
///
/// At runtime, `Args` are provided which will be used to compute the size
/// of each item.
pub struct DynSizedArray<'a, Args, T> {
    bytes: &'a [u8],
    args: Args,
    phantom: std::marker::PhantomData<T>,
}

impl<'a, Args: Copy, T> FontReadWithArgs<'a, Args> for DynSizedArray<'a, Args, T> {
    fn read_with_args(bytes: &'a [u8], args: &Args) -> Option<(Self, &'a [u8])> {
        Some((
            DynSizedArray {
                bytes,
                args: *args,
                phantom: std::marker::PhantomData,
            },
            &[],
        ))
    }
}

impl<'a, Args, T> DynSizedArray<'a, Args, T>
where
    T: FontReadWithArgs<'a, Args>,
{
    pub fn iter<'b: 'a>(&'b self) -> impl Iterator<Item = T> + 'b {
        let args = &self.args;
        let mut bytes = self.bytes;
        std::iter::from_fn(move || {
            let (next, remaining_bytes) = T::read_with_args(bytes, args)?;
            bytes = remaining_bytes;
            Some(next)
        })
    }
}
