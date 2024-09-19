//! impl subset() for cmap table
use crate::{Plan, Subset, SubsetError};

use write_fonts::{
    read::{tables::cmap::Cmap, FontRef, TopLevelTable},
    FontBuilder,
};

// reference: subset() for gvar table in harfbuzz
// https://github.com/harfbuzz/harfbuzz/blob/63d09dbefcf7ad9f794ca96445d37b6d8c3c9124/src/hb-ot-var-gvar-table.hh#L411
impl<'a> Subset for Cmap<'a> {
    fn subset(
        &self,
        plan: &Plan,
        _font: &FontRef,
        builder: &mut FontBuilder,
    ) -> Result<(), SubsetError> {
        let char_map = plan
            .unicode_to_new_gid_list
            .iter()
            .filter_map(|x| char::from_u32(x.0).map(|c| (c, x.1)));
        let cmap = write_fonts::tables::cmap::Cmap::from_mappings(char_map)
            .map_err(|_| SubsetError::SubsetTableError(Cmap::TAG))?;
        let cmap_bytes =
            write_fonts::dump_table(&cmap).map_err(|_| SubsetError::SubsetTableError(Cmap::TAG))?;
        builder.add_raw(Cmap::TAG, cmap_bytes);
        Ok(())
    }
}
