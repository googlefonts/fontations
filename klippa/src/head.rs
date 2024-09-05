//! impl subset() for head
use crate::{Plan, SubsetError, SubsetError::SubsetTableError};
use write_fonts::{
    read::{tables::head::Head, FontRef, TableProvider, TopLevelTable},
    FontBuilder,
};

// reference: subset() for head in harfbuzz
// https://github.com/harfbuzz/harfbuzz/blob/a070f9ebbe88dc71b248af9731dd49ec93f4e6e6/src/hb-ot-head-table.hh#L63
pub(crate) fn subset_head<'a>(
    font: &FontRef<'a>,
    _plan: &Plan,
    builder: &mut FontBuilder<'a>,
) -> Result<(), SubsetError> {
    let head = font.head().or(Err(SubsetTableError(Head::TAG)))?;
    builder.add_raw(Head::TAG, head.offset_data());
    Ok(())
}
