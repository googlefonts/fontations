//! impl subset() for MarkRecord subtable
use crate::FastHashMap;
use crate::{
    layout::for_each_intersected_coverage_index,
    offset::SerializeSubset,
    serialize::{SerializeErrorFlags, Serializer},
    CollectVariationIndices, Plan, SubsetTable,
};
use write_fonts::{
    read::{
        collections::IntSet,
        tables::{
            gpos::{MarkArray, MarkRecord},
            layout::CoverageTable,
        },
        types::GlyphId,
        FontData,
    },
    types::Offset16,
};

#[inline]
pub(crate) fn collect_mark_record_varidx(
    mark_record: &MarkRecord,
    plan: &Plan,
    varidx_set: &mut IntSet<u32>,
    font_data: FontData,
) {
    if let Ok(mark_anchor) = mark_record.mark_anchor(font_data) {
        mark_anchor.collect_variation_indices(plan, varidx_set);
    };
}

pub(crate) fn get_mark_class_map(
    coverage: &CoverageTable,
    mark_array: &MarkArray,
    glyph_set: &IntSet<GlyphId>,
) -> FastHashMap<u16, u16> {
    let mark_records = mark_array.mark_records();

    let mut retained_classes = IntSet::<u16>::empty();
    let _ = for_each_intersected_coverage_index::<()>(
        coverage,
        glyph_set,
        mark_array.mark_count(),
        |idx| {
            if let Some(mark_record) = mark_records.get(idx as usize) {
                retained_classes.insert(mark_record.mark_class());
            }
            Ok(())
        },
    );
    retained_classes
        .iter()
        .enumerate()
        .map(|(new_class, class)| (class, new_class as u16))
        .collect()
}

/// Collect variation indices from the anchors of the retained MarkRecords
/// and return the set of retained mark classes.
pub(crate) fn collect_retained_mark_varidx_and_classes(
    coverage: &CoverageTable,
    mark_array: &MarkArray,
    plan: &Plan,
    varidx_set: &mut IntSet<u32>,
) -> IntSet<u16> {
    let mark_array_data = mark_array.offset_data();
    let mark_records = mark_array.mark_records();

    let mut retained_mark_classes = IntSet::empty();
    let _ = for_each_intersected_coverage_index::<()>(
        coverage,
        &plan.glyphset_gsub,
        mark_array.mark_count(),
        |idx| {
            let mark_record = mark_records.get(idx as usize).ok_or(())?;
            collect_mark_record_varidx(mark_record, plan, varidx_set, mark_array_data);
            retained_mark_classes.insert(mark_record.mark_class());
            Ok(())
        },
    );
    retained_mark_classes
}

impl<'a> SubsetTable<'a> for MarkArray<'_> {
    type ArgsForSubset = (&'a IntSet<u16>, &'a FastHashMap<u16, u16>);
    type Output = ();
    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        args: Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        let (mark_record_idxes, mark_class_map) = args;
        if mark_record_idxes.is_empty() {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY);
        }
        // mark count
        s.embed(mark_record_idxes.len() as u16)?;

        let font_data = self.offset_data();
        let mark_records = self.mark_records();
        for i in mark_record_idxes.iter() {
            let Some(mark_record) = mark_records.get(i as usize) else {
                return Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER));
            };
            mark_record.subset(plan, s, (mark_class_map, font_data))?;
        }
        Ok(())
    }
}

impl<'a> SubsetTable<'a> for MarkRecord {
    type ArgsForSubset = (&'a FastHashMap<u16, u16>, FontData<'a>);
    type Output = ();
    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        args: Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        let (class_map, font_data) = args;
        let Some(new_mark_class) = class_map.get(&self.mark_class()) else {
            return Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER));
        };

        s.embed(*new_mark_class)?;

        let anchor_offset_pos = s.embed(0_u16)?;
        if self.mark_anchor_offset().is_null() {
            return Ok(());
        }
        let mark_anchor = self
            .mark_anchor(font_data)
            .map_err(|_| s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR))?;

        Offset16::serialize_subset(&mark_anchor, s, plan, (), anchor_offset_pos)
    }
}
