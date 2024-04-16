//! impl subset() for hhea
use crate::{Plan, Subset, SubsetError};
use write_fonts::tables::hhea::Hhea;

impl Subset for Hhea {
    fn subset(&mut self, plan: &Plan) -> Result<bool, SubsetError> {
        self.number_of_long_metrics = plan.num_h_metrics;
        Ok(true)
    }
}
