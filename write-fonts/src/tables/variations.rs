//! OpenType variations common table formats

include!("../../generated/generated_variations.rs");

pub use read_fonts::tables::variations::TupleIndex;

impl VariationRegionList {
    fn compute_axis_count(&self) -> usize {
        let count = self
            .variation_regions
            .first()
            .map(|reg| reg.region_axes.len())
            .unwrap_or(0);
        //TODO: check this at validation time
        debug_assert!(self
            .variation_regions
            .iter()
            .map(|reg| reg.region_axes.len())
            .all(|n| n == count));
        count
    }
}

impl FontWrite for TupleIndex {
    fn write_into(&self, writer: &mut TableWriter) {
        self.bits().write_into(writer)
    }
}
