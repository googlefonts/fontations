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
    let maxp = font.maxp().or(Err(SubsetTableError(Maxp::TAG)))?;

    let num_glyphs = plan.num_output_glyphs.min(0xFFFF) as u16;
    let mut out = maxp.offset_data().as_bytes().to_owned();
    out.get_mut(4..6)
        .unwrap()
        .clone_from_slice(&num_glyphs.to_be_bytes());
    builder.add_raw(Maxp::TAG, out);
    Ok(())
}
