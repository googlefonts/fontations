//! impl subset() for head
use crate::{Plan, SubsetError, SubsetError::SubsetTableError};
use write_fonts::{
    read::{tables::head::Head, FontRef, TableProvider, TopLevelTable},
    FontBuilder,
};

pub fn subset_head(
    font: &FontRef,
    _plan: &Plan,
    builder: &mut FontBuilder,
) -> Result<(), SubsetError> {
    let Ok(head) = font.head() else {
        return Err(SubsetTableError(Head::TAG));
    };

    let mut out = Vec::with_capacity(head.offset_data().len());
    out.extend_from_slice(head.offset_data().as_bytes());

    builder.add_raw(Head::TAG, out);
    Ok(())
}
