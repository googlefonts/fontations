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
    Args: Copy + 'static,
    T: FontReadWithArgs<'a, Args>,
{
    pub fn iter(&self) -> impl Iterator<Item = T> + 'a {
        let args = self.args;
        let mut bytes = self.bytes;
        std::iter::from_fn(move || {
            let (next, remaining_bytes) = T::read_with_args(bytes, &args)?;
            bytes = remaining_bytes;
            Some(next)
        })
    }
}

impl<Args, T> std::fmt::Debug for DynSizedArray<'_, Args, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("DynSizedArray")
            .field("bytes", &self.bytes)
            .finish()
    }
}
