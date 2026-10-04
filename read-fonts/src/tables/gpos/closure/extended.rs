//! Closure for the extended positioning formats.

use super::super::{
    CursivePosFormat2, MarkBasePosFormat2, MarkLigPosFormat2, MarkMarkPosFormat2, PairPosFormat3,
    PairPosFormat4, PairSet2, SinglePosFormat3, SinglePosFormat4,
};
use super::*;
use crate::{tables::layout::CoverageTable, ArrayOfOffsets, Offset};
use font_types::Scalar;

#[cfg(test)]
mod tests;

macro_rules! intersect_coverage {
    ($table:ident) => {
        impl Intersect for $table<'_> {
            fn intersects(&self, glyphs: &IntSet<GlyphId>) -> Result<bool, ReadError> {
                if self.coverage_offset().is_null() {
                    return Ok(false);
                }
                Ok(self.coverage()?.intersects(glyphs))
            }
        }
    };
}
intersect_coverage!(SinglePosFormat3);
intersect_coverage!(SinglePosFormat4);
intersect_coverage!(CursivePosFormat2);

macro_rules! intersect_mark_coverages {
    ($table:ident, $first_offset:ident, $second_offset:ident, $first:ident, $second:ident) => {
        impl Intersect for $table<'_> {
            fn intersects(&self, glyphs: &IntSet<GlyphId>) -> Result<bool, ReadError> {
                if self.$first_offset().is_null() || self.$second_offset().is_null() {
                    return Ok(false);
                }
                Ok(self.$first()?.intersects(glyphs) && self.$second()?.intersects(glyphs))
            }
        }
    };
}
intersect_mark_coverages!(
    MarkBasePosFormat2,
    mark_coverage_offset,
    base_coverage_offset,
    mark_coverage,
    base_coverage
);
intersect_mark_coverages!(
    MarkLigPosFormat2,
    mark_coverage_offset,
    ligature_coverage_offset,
    mark_coverage,
    ligature_coverage
);
intersect_mark_coverages!(
    MarkMarkPosFormat2,
    mark1_coverage_offset,
    mark2_coverage_offset,
    mark1_coverage,
    mark2_coverage
);

pub(super) fn pair_pos_intersects<'a, T, O>(
    coverage: &CoverageTable,
    pair_sets: ArrayOfOffsets<'a, T, O>,
    count: u32,
    glyphs: &IntSet<GlyphId>,
) -> Result<bool, ReadError>
where
    T: FontRead<'a> + Intersect,
    T::Args: Copy + 'static,
    O: Scalar + Offset,
{
    let num_bits = 32 - count.leading_zeros();
    if count as u64 > glyphs.len() * num_bits as u64 {
        for gid in glyphs.iter() {
            let Some(index) = coverage.get(gid) else {
                continue;
            };
            let set = match pair_sets.get(index as usize) {
                Err(ReadError::NullOffset) => continue,
                other => other,
            }?;
            if set.intersects(glyphs)? {
                return Ok(true);
            }
        }
    } else {
        for (gid, set) in coverage.iter().zip(pair_sets.iter_as_nullable()) {
            if !glyphs.contains(gid) {
                continue;
            }
            let Some(set) = set.transpose()? else {
                continue;
            };
            if set.intersects(glyphs)? {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

impl Intersect for PairPosFormat3<'_> {
    fn intersects(&self, glyphs: &IntSet<GlyphId>) -> Result<bool, ReadError> {
        if self.coverage_offset().is_null() {
            return Ok(false);
        }
        pair_pos_intersects(
            &self.coverage()?,
            self.pair_sets(),
            self.pair_set_count().to_u32(),
            glyphs,
        )
    }
}

impl Intersect for PairSet2<'_> {
    fn intersects(&self, glyphs: &IntSet<GlyphId>) -> Result<bool, ReadError> {
        for record in self.pair_value_records().iter() {
            if glyphs.contains(GlyphId::from(record?.second_glyph())) {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

impl Intersect for PairPosFormat4<'_> {
    fn intersects(&self, glyphs: &IntSet<GlyphId>) -> Result<bool, ReadError> {
        if self.coverage_offset().is_null()
            || self.class_def1_offset().is_null()
            || self.class_def2_offset().is_null()
        {
            return Ok(false);
        }
        Ok(self.coverage()?.intersects(glyphs) && self.class_def2()?.intersects(glyphs)?)
    }
}
