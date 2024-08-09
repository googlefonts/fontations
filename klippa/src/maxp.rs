//! impl subset() for maxp
use crate::{
    Plan,
    SubsetError::{self, SubsetTableError},
    SubsetFlags,
};
use write_fonts::{
    read::{tables::maxp::Maxp, FontRef, TableProvider, TopLevelTable},
    types::Version16Dot16,
    FontBuilder,
};

pub fn subset_maxp(
    font: &FontRef,
    plan: &Plan,
    builder: &mut FontBuilder,
) -> Result<(), SubsetError> {
    let maxp = font.maxp().or(Err(SubsetTableError(Maxp::TAG)))?;

    let num_glyphs = plan.num_output_glyphs.min(0xFFFF) as u16;
    let mut out = maxp.offset_data().as_bytes().to_owned();
    out.get_mut(4..6)
        .unwrap()
        .copy_from_slice(&num_glyphs.to_be_bytes());

    //drop hints
    if maxp.version() == Version16Dot16::VERSION_1_0
        && plan
            .subset_flags
            .contains(SubsetFlags::SUBSET_FLAGS_NO_HINTING)
    {
        //maxZones
        out.get_mut(14..16).unwrap().copy_from_slice(&[0, 1]);
        //maxTwilightPoints..maxSizeOfInstructions
        out.get_mut(16..28).unwrap().fill(0);
    }
    builder.add_raw(Maxp::TAG, out);
    Ok(())
}
