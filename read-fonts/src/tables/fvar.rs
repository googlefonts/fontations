//! The [CPAL](https://docs.microsoft.com/en-us/typography/opentype/spec/fvar) table

use core::marker::PhantomData;

use font_types::Tag;

/// 'fvar'
pub const TAG: Tag = Tag::new(b"fvar");

include!("../../generated/generated_fvar.rs");

/// The awkwardly arranged axes and instances arrays, found
/// starting at the offset indicated by axesArrayOffset
/// https://learn.microsoft.com/en-us/typography/opentype/spec/fvar#fvar-header
#[derive(Clone, Default, PartialEq, Eq)]
pub struct FvarData<'a> {
    phantom: PhantomData<&'a u16>,
}

#[derive(Copy, Clone)]
struct FvarDataArgs {
    axis_count: u16,
    instance_count: u16,
    instance_size: u16,
}

impl<'a> ReadArgs for FvarData<'a> {
    type Args = FvarDataArgs;
}

impl<'a> FontReadWithArgs<'a> for FvarData<'a> {
    fn read_with_args(data: FontData<'a>, args: &Self::Args) -> Result<Self, ReadError> {
        FvarData::read(data, *args)
    }
}

#[cfg(test)]
mod tests {
    use crate::test_data;

    #[test]
    fn read_sample() {
        // TODO
    }
}
