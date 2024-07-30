//! impl subset() for maxp
use crate::{Plan, SubsetError, SubsetError::SubsetTableError};
use write_fonts::{
    read::{tables::maxp::Maxp, FontRef, TableProvider, TopLevelTable},
    FontBuilder,
};

pub fn subset_maxp(
    font: &FontRef,
    plan: &Plan,
    builder: &mut FontBuilder,
) -> Result<(), SubsetError> {
    let Ok(maxp) = font.maxp() else {
        return Err(SubsetTableError(Maxp::TAG));
    };

    let num_glyphs = plan.num_output_glyphs.min(0xFFFF) as u16;
    let mut out = Vec::with_capacity(maxp.offset_data().len());
    out.extend_from_slice(maxp.offset_data().as_bytes());

    let Some(index_num_glyphs) = out.get_mut(4..6) else {
        return Err(SubsetTableError(Maxp::TAG));
    };
    index_num_glyphs.clone_from_slice(&num_glyphs.to_be_bytes());
    builder.add_raw(Maxp::TAG, out);
    Ok(())
}
