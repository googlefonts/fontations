//! impl subset() for MarkRecord subtable
use crate::fnv::FnvHashMap;
use crate::{
    offset::SerializeSubset,
    serialize::{SerializeErrorFlags, Serializer},
    CollectVariationIndices, Plan, SubsetTable,
};
use write_fonts::{
    read::{
        collections::IntSet,
        tables::{
            gpos::{AnchorTable, MarkArray, MarkArray2, MarkRecord, MarkRecord2},
            layout::CoverageTable,
        },
        types::GlyphId,
        FontData, MinByteRange, ReadError,
    },
    types::{FixedSize, Offset16, Offset24, Uint24},
};

pub(crate) trait MarkRecordData {
    fn mark_class(&self) -> u16;
    fn mark_anchor<'a>(&self, data: FontData<'a>) -> Result<AnchorTable<'a>, ReadError>;
}

macro_rules! mark_record_data {
    ($record:ident) => {
        impl MarkRecordData for $record {
            fn mark_class(&self) -> u16 {
                $record::mark_class(self)
            }
            fn mark_anchor<'a>(&self, data: FontData<'a>) -> Result<AnchorTable<'a>, ReadError> {
                $record::mark_anchor(self, data)
            }
        }
    };
}
mark_record_data!(MarkRecord);
mark_record_data!(MarkRecord2);

pub(crate) fn collect_mark_record_varidx(
    mark_record: &impl MarkRecordData,
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
    mark_records: &[impl MarkRecordData],
    glyph_set: &IntSet<GlyphId>,
) -> FnvHashMap<u16, u16> {
    let count = match coverage {
        CoverageTable::Format1(t) => u32::from(t.glyph_count()),
        CoverageTable::Format2(t) => u32::from(t.range_count()),
        CoverageTable::Format3(t) => t.glyph_count().to_u32(),
        CoverageTable::Format4(t) => t.range_count().to_u32(),
    };
    let num_bits = 32 - count.leading_zeros();
    let coverage_population = coverage.population();

    let retained_classes: IntSet<u16> =
        if coverage_population as u32 > (glyph_set.len() as u32) * num_bits {
            glyph_set
                .iter()
                .filter_map(|g| {
                    coverage.get(g).and_then(|idx| {
                        mark_records
                            .get(idx as usize)
                            .map(|mark_record| mark_record.mark_class())
                    })
                })
                .collect()
        } else {
            coverage
                .iter()
                .enumerate()
                .filter(|&(_, g)| glyph_set.contains(g))
                .filter_map(|(idx, _)| {
                    mark_records
                        .get(idx)
                        .map(|mark_record| mark_record.mark_class())
                })
                .collect()
        };
    retained_classes
        .iter()
        .enumerate()
        .map(|(new_class, class)| (class, new_class as u16))
        .collect()
}

macro_rules! subset_mark_array {
    ($table:ident, $count:ident) => {
        impl<'a> SubsetTable<'a> for $table<'_> {
            type ArgsForSubset = (&'a IntSet<u32>, &'a FnvHashMap<u16, u16>);
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
                if self.min_table_bytes().is_empty() {
                    return Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR));
                }
                let mark_len = usize::try_from(mark_record_idxes.len())
                    .map_err(|_| s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_INT_OVERFLOW))?;
                let mark_count = $count::try_from(mark_len)
                    .map_err(|_| s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_INT_OVERFLOW))?;
                s.embed(mark_count)?;

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
    };
}
subset_mark_array!(MarkArray, u16);
subset_mark_array!(MarkArray2, Uint24);

macro_rules! subset_mark_record {
    ($record:ident, $offset:ident) => {
        impl<'a> SubsetTable<'a> for $record {
            type ArgsForSubset = (&'a FnvHashMap<u16, u16>, FontData<'a>);
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

                let anchor_offset_pos = s.allocate_size($offset::RAW_BYTE_LEN, true)?;
                if self.mark_anchor_offset().is_null() {
                    return Ok(());
                }
                let mark_anchor = self
                    .mark_anchor(font_data)
                    .map_err(|_| s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR))?;

                $offset::serialize_subset(&mark_anchor, s, plan, (), anchor_offset_pos)
            }
        }
    };
}
subset_mark_record!(MarkRecord, Offset16);
subset_mark_record!(MarkRecord2, Offset24);
