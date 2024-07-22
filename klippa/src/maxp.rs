//! impl subset() for maxp
use crate::{Plan, Subset, SubsetError, SubsetError::SubsetTableError};
use write_fonts::{read::TopLevelTable, tables::maxp::Maxp};

impl Subset for Maxp {
    fn subset(&mut self, plan: &Plan) -> Result<bool, SubsetError> {
        let Ok(num_glyphs) = plan.num_output_glyphs.try_into() else {
            return Err(SubsetTableError(Maxp::TAG));       
        };
        self.num_glyphs = num_glyphs;
        Ok(true)
    }
}
