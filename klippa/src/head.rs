//! impl subset() for head
use crate::{Plan, Subset, SubsetError};
use write_fonts::{
    read::{tables::head::Head, FontRef, TopLevelTable},
    FontBuilder,
};

// reference: subset() for head in harfbuzz
// https://github.com/harfbuzz/harfbuzz/blob/a070f9ebbe88dc71b248af9731dd49ec93f4e6e6/src/hb-ot-head-table.hh#L63
impl<'a> Subset for Head<'a> {
    fn subset(
        &self,
        _plan: &Plan,
        _font: &FontRef,
        builder: &mut FontBuilder,
    ) -> Result<(), SubsetError> {
        let out = self.offset_data().as_bytes().to_owned();
        builder.add_raw(Head::TAG, out);
        Ok(())
    }
}
