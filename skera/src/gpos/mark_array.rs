//! impl subset() for MarkRecord subtable
use crate::fnv::FnvHashMap;
use crate::{
    layout::for_each_intersected_coverage_index,
    offset::SerializeSubset,
    serialize::{SerializeErrorFlags, SerializeResultEmpty, Serializer},
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
) -> FnvHashMap<u16, u16> {
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
    type ArgsForSubset = (&'a IntSet<u16>, &'a FnvHashMap<u16, u16>);
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
        // Every retained mark keeps its record, but as in harfbuzz the
        // MarkArray is only worth keeping if at least one of those records
        // still has an anchor. Otherwise report EMPTY: the Mark*Pos subtables
        // propagate it and get dropped, matching harfbuzz returning false
        // from MarkArray::subset.
        // ref: <https://github.com/harfbuzz/harfbuzz/blob/main/src/OT/Layout/GPOS/MarkArray.hh>
        let mut has_anchor = false;
        for i in mark_record_idxes.iter() {
            let Some(mark_record) = mark_records.get(i as usize) else {
                return Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER));
            };
            has_anchor |= !mark_record
                .subset(plan, s, (mark_class_map, font_data))
                .is_empty()?;
        }
        if !has_anchor {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY);
        }
        Ok(())
    }
}

impl<'a> SubsetTable<'a> for MarkRecord {
    type ArgsForSubset = (&'a FnvHashMap<u16, u16>, FontData<'a>);
    type Output = ();
    /// Serializes the record. Returns `SERIALIZE_ERROR_EMPTY` if the record
    /// was written but its anchor was not (null offset or empty anchor), in
    /// which case the anchor offset is left as 0.
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
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY);
        }
        let mark_anchor = self
            .mark_anchor(font_data)
            .map_err(|_| s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR))?;

        Offset16::serialize_subset(&mark_anchor, s, plan, (), anchor_offset_pos)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use write_fonts::read::FontRead;

    // MarkArray with two MarkRecords (class 0 and class 1). Record 0 has a
    // null anchor; record 1 points at `rec1_anchor_offset` (0 = null) where an
    // AnchorFormat1 (x=100, y=200) is stored.
    fn mark_array_bytes(rec1_anchor_offset: u8) -> [u8; 16] {
        #[rustfmt::skip]
        let raw: [u8; 16] = [
            // markCount=2
            0x00, 0x02,
            // record 0: class 0, null anchor
            0x00, 0x00, 0x00, 0x00,
            // record 1: class 1, anchor offset
            0x00, 0x01, 0x00, rec1_anchor_offset,
            // @10 AnchorFormat1 x=100 y=200
            0x00, 0x01, 0x00, 0x64, 0x00, 0xc8,
        ];
        raw
    }

    fn subset_mark_array(raw: &[u8]) -> (Result<(), SerializeErrorFlags>, Serializer) {
        let mark_array = MarkArray::read(FontData::new(raw)).unwrap();
        let plan = Plan::default();
        let mark_record_idxes: IntSet<u16> = [0, 1].into_iter().collect();
        let mark_class_map: FnvHashMap<u16, u16> = [(0, 0), (1, 1)].into_iter().collect();

        let mut s = Serializer::new(1024);
        assert_eq!(s.start_serialize(), Ok(()));
        let ret = mark_array.subset(&plan, &mut s, (&mark_record_idxes, &mark_class_map));
        (ret, s)
    }

    /// As in harfbuzz, a MarkArray whose retained records all have null
    /// anchors is EMPTY, so the Mark*Pos subtable that owns it is dropped.
    #[test]
    fn test_subset_mark_array_all_null_anchors_is_empty() {
        let (ret, s) = subset_mark_array(&mark_array_bytes(0));
        assert_eq!(ret, Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY));
        assert!(!s.in_error());
    }

    /// One retained anchor is enough to keep the MarkArray. The record with a
    /// null anchor is still written, with its offset left as 0.
    #[test]
    fn test_subset_mark_array_keeps_null_anchor_records() {
        let (ret, mut s) = subset_mark_array(&mark_array_bytes(10));
        assert_eq!(ret, Ok(()));
        assert!(!s.in_error());
        s.end_serialize();

        #[rustfmt::skip]
        let expected: [u8; 16] = [
            0x00, 0x02,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x01, 0x00, 0x0a,
            0x00, 0x01, 0x00, 0x64, 0x00, 0xc8,
        ];
        assert_eq!(s.copy_bytes(), expected);
    }
}
