//! impl subset() for maxp
use crate::{Plan, Subset, SubsetError};
use write_fonts::tables::maxp::Maxp;

impl Subset for Maxp {
    fn subset(&mut self, plan: &Plan) -> Result<bool, SubsetError> {
        self.num_glyphs = plan.num_output_glyphs;
        Ok(true)
    }
}
