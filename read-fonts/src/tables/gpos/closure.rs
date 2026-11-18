//! support closure for GPOS
mod extended;

use super::{
    CursivePosFormat1, ExtensionPosFormat1, ExtensionSubtable, Gpos, MarkBasePosFormat1,
    MarkLigPosFormat1, MarkMarkPosFormat1, PairPos, PairPosFormat1, PairPosFormat2, PairSet,
    PositionLookup, PositionLookupList, PositionSubtables, SinglePos, SinglePosFormat1,
    SinglePosFormat2,
};
use crate::{
    collections::IntSet, tables::layout::CoverageTable, ArrayOfOffsets, FontRead, GlyphId, Offset,
    ReadError, Tag,
};
use font_types::Scalar;

#[cfg(feature = "std")]
use crate::tables::layout::{
    charge_records, Intersect, LayoutLookupList, LookupClosure, LookupClosureCtx,
};

impl Gpos<'_> {
    /// Return a set of all feature indices underneath the specified scripts, languages and features
    pub fn collect_features(
        &self,
        scripts: &IntSet<Tag>,
        languages: &IntSet<Tag>,
        features: &IntSet<Tag>,
    ) -> Result<IntSet<u16>, ReadError> {
        if self.selected_script_list_offset()?.is_null()
            || self.selected_feature_list_offset()?.is_null()
        {
            return Ok(IntSet::empty());
        }
        let feature_list = self.feature_list()?;
        let script_list = self.script_list()?;
        let head_ptr = self.offset_data().as_bytes().as_ptr() as usize;
        script_list.collect_features(head_ptr, &feature_list, scripts, languages, features)
    }

    /// Return a set of lookups referenced by the specified features
    pub fn collect_lookups(&self, feature_indices: &IntSet<u16>) -> Result<IntSet<u16>, ReadError> {
        if self.selected_feature_list_offset()?.is_null() {
            return Ok(IntSet::empty());
        }
        // One budget for the whole table, as when HarfBuzz sanitizes it.
        let mut ops_left = crate::limits::max_ops(self.offset_data().len());
        let feature_list = self.feature_list()?;
        let mut lookup_indices = feature_list.collect_lookups(feature_indices, &mut ops_left)?;

        if let Some(feature_variations) = self.feature_variations().transpose()? {
            let subs_lookup_indices =
                feature_variations.collect_lookups(feature_indices, &mut ops_left)?;
            lookup_indices.union(&subs_lookup_indices);
        }
        Ok(lookup_indices)
    }

    /// Update the set of lookup indices with all lookups reachable from specified glyph set and lookup_indices.
    pub fn closure_lookups(
        &self,
        glyphs: &IntSet<GlyphId>,
        lookup_indices: &mut IntSet<u16>,
    ) -> Result<(), ReadError> {
        if self.selected_lookup_list_offset()?.is_null() {
            return Ok(());
        }
        let lookup_list = self.lookup_list()?;
        // One budget for the whole table, as when HarfBuzz sanitizes it.
        let max_ops = crate::limits::max_ops(self.offset_data().len());
        lookup_list.closure_lookups(glyphs, lookup_indices, max_ops)
    }
}

impl PositionLookupList<'_> {
    /// Update the set of lookup indices with all lookups reachable from the
    /// specified glyph set and lookup indices.
    ///
    /// The closure may read `max_ops` records of the lookups, which should be
    /// [`max_ops`](crate::limits::max_ops) of the table the lookups are in.
    /// Beyond that it returns
    /// [`MAX_OPS_EXCEEDED`](crate::limits::MAX_OPS_EXCEEDED).
    pub fn closure_lookups(
        &self,
        glyph_set: &IntSet<GlyphId>,
        lookup_indices: &mut IntSet<u16>,
        max_ops: usize,
    ) -> Result<(), ReadError> {
        lookup_indices.remove_range(self.lookup_count()..=u16::MAX);
        if lookup_indices.is_empty() {
            return Ok(());
        }
        let lookup_list = LayoutLookupList::Gpos(self);
        let mut c = LookupClosureCtx::new(glyph_set, &lookup_list, max_ops);

        let lookups = self.lookups();
        for idx in lookup_indices.iter() {
            let lookup = match lookups.get(idx as usize) {
                Err(ReadError::NullOffset) => {
                    c.set_lookup_inactive(idx);
                    continue;
                }
                other => other,
            }?;
            lookup.closure_lookups(&mut c, idx)?;
        }

        lookup_indices.union(c.visited_lookups());
        lookup_indices.subtract(c.inactive_lookups());
        Ok(())
    }
}

impl LookupClosure for PositionLookup<'_> {
    fn closure_lookups(
        &self,
        c: &mut LookupClosureCtx,
        lookup_index: u16,
    ) -> Result<(), ReadError> {
        if !c.should_visit_lookup(lookup_index) {
            return Ok(());
        }
        c.visit_subtables(usize::from(self.of_unit_type().sub_table_count()))?;

        let subtables = self.subtables()?;
        // Contextual subtables are checked for intersection as they are
        // visited, and mark the lookup inactive if none of them intersect.
        if matches!(
            subtables,
            PositionSubtables::Contextual(_) | PositionSubtables::ChainContextual(_)
        ) {
            return subtables.closure_lookups(c, lookup_index);
        }

        if !c.intersects(self)? {
            c.set_lookup_inactive(lookup_index);
        }
        Ok(())
    }
}

impl LookupClosure for PositionSubtables<'_> {
    fn closure_lookups(&self, c: &mut LookupClosureCtx, arg: u16) -> Result<(), ReadError> {
        match self {
            PositionSubtables::Contextual(subtables) => subtables.closure_lookups(c, arg),
            PositionSubtables::ChainContextual(subtables) => subtables.closure_lookups(c, arg),
            _ => Ok(()),
        }
    }
}

impl Intersect for PositionLookup<'_> {
    fn intersects(
        &self,
        glyph_set: &IntSet<GlyphId>,
        ops_left: &mut usize,
    ) -> Result<bool, ReadError> {
        match self {
            PositionLookup::Single(inner) => inner.subtables().intersects(glyph_set, ops_left),
            PositionLookup::Pair(inner) => inner.subtables().intersects(glyph_set, ops_left),
            PositionLookup::Cursive(inner) => inner.subtables().intersects(glyph_set, ops_left),
            PositionLookup::MarkToBase(inner) => inner.subtables().intersects(glyph_set, ops_left),
            PositionLookup::MarkToLig(inner) => inner.subtables().intersects(glyph_set, ops_left),
            PositionLookup::MarkToMark(inner) => inner.subtables().intersects(glyph_set, ops_left),
            PositionLookup::Contextual(inner) => inner.subtables().intersects(glyph_set, ops_left),
            PositionLookup::ChainContextual(inner) => {
                inner.subtables().intersects(glyph_set, ops_left)
            }
            PositionLookup::Extension(inner) => inner.subtables().intersects(glyph_set, ops_left),
        }
    }
}

impl Intersect for ExtensionSubtable<'_> {
    fn intersects(
        &self,
        glyph_set: &IntSet<GlyphId>,
        ops_left: &mut usize,
    ) -> Result<bool, ReadError> {
        match self {
            ExtensionSubtable::Single(inner) => inner.intersects(glyph_set, ops_left),
            ExtensionSubtable::Pair(inner) => inner.intersects(glyph_set, ops_left),
            ExtensionSubtable::Cursive(inner) => inner.intersects(glyph_set, ops_left),
            ExtensionSubtable::MarkToBase(inner) => inner.intersects(glyph_set, ops_left),
            ExtensionSubtable::MarkToLig(inner) => inner.intersects(glyph_set, ops_left),
            ExtensionSubtable::MarkToMark(inner) => inner.intersects(glyph_set, ops_left),
            ExtensionSubtable::Contextual(inner) => inner.intersects(glyph_set, ops_left),
            ExtensionSubtable::ChainContextual(inner) => inner.intersects(glyph_set, ops_left),
        }
    }
}

impl<'a, T> Intersect for ExtensionPosFormat1<'a, T>
where
    T: Intersect + FontRead<'a, Args = ()>,
{
    fn intersects(
        &self,
        glyph_set: &IntSet<GlyphId>,
        ops_left: &mut usize,
    ) -> Result<bool, ReadError> {
        if self.extension_offset().is_null() {
            return Ok(false);
        }
        self.extension()?.intersects(glyph_set, ops_left)
    }
}

impl Intersect for SinglePos<'_> {
    fn intersects(
        &self,
        glyph_set: &IntSet<GlyphId>,
        ops_left: &mut usize,
    ) -> Result<bool, ReadError> {
        match self {
            Self::Format1(item) => item.intersects(glyph_set, ops_left),
            Self::Format2(item) => item.intersects(glyph_set, ops_left),
            Self::Format3(item) => item.intersects(glyph_set, ops_left),
            Self::Format4(item) => item.intersects(glyph_set, ops_left),
        }
    }
}

impl Intersect for SinglePosFormat1<'_> {
    fn intersects(
        &self,
        glyph_set: &IntSet<GlyphId>,
        _ops_left: &mut usize,
    ) -> Result<bool, ReadError> {
        if self.coverage_offset().is_null() {
            return Ok(false);
        }
        Ok(self.coverage()?.intersects(glyph_set))
    }
}

impl Intersect for SinglePosFormat2<'_> {
    fn intersects(
        &self,
        glyph_set: &IntSet<GlyphId>,
        _ops_left: &mut usize,
    ) -> Result<bool, ReadError> {
        if self.coverage_offset().is_null() {
            return Ok(false);
        }
        Ok(self.coverage()?.intersects(glyph_set))
    }
}

impl Intersect for PairPos<'_> {
    fn intersects(
        &self,
        glyph_set: &IntSet<GlyphId>,
        ops_left: &mut usize,
    ) -> Result<bool, ReadError> {
        match self {
            Self::Format1(item) => item.intersects(glyph_set, ops_left),
            Self::Format2(item) => item.intersects(glyph_set, ops_left),
            Self::Format3(item) => item.intersects(glyph_set, ops_left),
            Self::Format4(item) => item.intersects(glyph_set, ops_left),
        }
    }
}

/// Returns whether a pair set of a covered glyph in `glyphs` intersects them,
/// for either pair set width.
fn pair_pos_intersects<'a, T, O>(
    coverage: &CoverageTable,
    pair_sets: ArrayOfOffsets<'a, T, O>,
    count: u32,
    glyphs: &IntSet<GlyphId>,
    ops_left: &mut usize,
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
            if set.intersects(glyphs, ops_left)? {
                return Ok(true);
            }
        }
    } else {
        charge_records(ops_left, count as usize)?;
        for (gid, set) in coverage.iter().zip(pair_sets.iter_as_nullable()) {
            if !glyphs.contains(gid) {
                continue;
            }
            let Some(set) = set.transpose()? else {
                continue;
            };
            if set.intersects(glyphs, ops_left)? {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

impl Intersect for PairPosFormat1<'_> {
    fn intersects(
        &self,
        glyph_set: &IntSet<GlyphId>,
        ops_left: &mut usize,
    ) -> Result<bool, ReadError> {
        if self.coverage_offset().is_null() {
            return Ok(false);
        }
        pair_pos_intersects(
            &self.coverage()?,
            self.pair_sets(),
            u32::from(self.pair_set_count()),
            glyph_set,
            ops_left,
        )
    }
}

impl Intersect for PairSet<'_> {
    fn intersects(
        &self,
        glyph_set: &IntSet<GlyphId>,
        ops_left: &mut usize,
    ) -> Result<bool, ReadError> {
        charge_records(ops_left, 1 + usize::from(self.pair_value_count()))?;
        for record in self.pair_value_records().iter() {
            let second_glyph = record?.second_glyph();
            if glyph_set.contains(GlyphId::from(second_glyph)) {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

impl Intersect for PairPosFormat2<'_> {
    fn intersects(
        &self,
        glyph_set: &IntSet<GlyphId>,
        ops_left: &mut usize,
    ) -> Result<bool, ReadError> {
        if self.coverage_offset().is_null()
            || self.class_def1_offset().is_null()
            || self.class_def2_offset().is_null()
        {
            return Ok(false);
        }
        Ok(self.coverage()?.intersects(glyph_set)
            && self.class_def2()?.intersects(glyph_set, ops_left)?)
    }
}

impl Intersect for CursivePosFormat1<'_> {
    fn intersects(
        &self,
        glyph_set: &IntSet<GlyphId>,
        _ops_left: &mut usize,
    ) -> Result<bool, ReadError> {
        if self.coverage_offset().is_null() {
            return Ok(false);
        }
        Ok(self.coverage()?.intersects(glyph_set))
    }
}

impl Intersect for MarkBasePosFormat1<'_> {
    fn intersects(
        &self,
        glyph_set: &IntSet<GlyphId>,
        _ops_left: &mut usize,
    ) -> Result<bool, ReadError> {
        if self.mark_coverage_offset().is_null() || self.base_coverage_offset().is_null() {
            return Ok(false);
        }
        Ok(self.mark_coverage()?.intersects(glyph_set)
            && self.base_coverage()?.intersects(glyph_set))
    }
}

impl Intersect for MarkLigPosFormat1<'_> {
    fn intersects(
        &self,
        glyph_set: &IntSet<GlyphId>,
        _ops_left: &mut usize,
    ) -> Result<bool, ReadError> {
        if self.mark_coverage_offset().is_null() || self.ligature_coverage_offset().is_null() {
            return Ok(false);
        }
        Ok(self.mark_coverage()?.intersects(glyph_set)
            && self.ligature_coverage()?.intersects(glyph_set))
    }
}

impl Intersect for MarkMarkPosFormat1<'_> {
    fn intersects(
        &self,
        glyph_set: &IntSet<GlyphId>,
        _ops_left: &mut usize,
    ) -> Result<bool, ReadError> {
        if self.mark1_coverage_offset().is_null() || self.mark2_coverage_offset().is_null() {
            return Ok(false);
        }
        Ok(self.mark1_coverage()?.intersects(glyph_set)
            && self.mark2_coverage()?.intersects(glyph_set))
    }
}

impl Intersect for super::CursivePos<'_> {
    fn intersects(
        &self,
        glyphs: &IntSet<GlyphId>,
        ops_left: &mut usize,
    ) -> Result<bool, ReadError> {
        match self {
            Self::Format1(table) => table.intersects(glyphs, ops_left),
            Self::Format2(table) => table.intersects(glyphs, ops_left),
        }
    }
}

impl Intersect for super::MarkBasePos<'_> {
    fn intersects(
        &self,
        glyphs: &IntSet<GlyphId>,
        ops_left: &mut usize,
    ) -> Result<bool, ReadError> {
        match self {
            Self::Format1(table) => table.intersects(glyphs, ops_left),
            Self::Format2(table) => table.intersects(glyphs, ops_left),
        }
    }
}

impl Intersect for super::MarkLigPos<'_> {
    fn intersects(
        &self,
        glyphs: &IntSet<GlyphId>,
        ops_left: &mut usize,
    ) -> Result<bool, ReadError> {
        match self {
            Self::Format1(table) => table.intersects(glyphs, ops_left),
            Self::Format2(table) => table.intersects(glyphs, ops_left),
        }
    }
}

impl Intersect for super::MarkMarkPos<'_> {
    fn intersects(
        &self,
        glyphs: &IntSet<GlyphId>,
        ops_left: &mut usize,
    ) -> Result<bool, ReadError> {
        match self {
            Self::Format1(table) => table.intersects(glyphs, ops_left),
            Self::Format2(table) => table.intersects(glyphs, ops_left),
        }
    }
}
