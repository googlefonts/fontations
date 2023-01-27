//! The [gvar (Glyph Variations)](https://learn.microsoft.com/en-us/typography/opentype/spec/gvar)
//! table

include!("../../generated/generated_gvar.rs");

use super::variations::{Tuple, TupleVariationCount, TupleVariationHeader};

pub struct U16Or32(u32);

impl ReadArgs for U16Or32 {
    type Args = GvarFlags;
}

impl ComputeSize for U16Or32 {
    fn compute_size(args: &GvarFlags) -> usize {
        args.contains(GvarFlags::LONG_OFFSETS)
            .then_some(4)
            .unwrap_or(2)
    }
}

impl FontReadWithArgs<'_> for U16Or32 {
    fn read_with_args(data: FontData<'_>, args: &Self::Args) -> Result<Self, ReadError> {
        if args.contains(GvarFlags::LONG_OFFSETS) {
            data.read_at::<u32>(0).map(Self)
        } else {
            data.read_at::<u16>(0).map(|v| Self(v as u32 * 2))
        }
    }
}
