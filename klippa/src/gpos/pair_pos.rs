//! impl subset() for PairPos subtable

use crate::{
    gpos::value_record::compute_effective_format,
    layout::ClassDefSubsetStruct,
    offset::{SerializeSerialize, SerializeSubset},
    offset_array::SubsetOffsetArray,
    serialize::{SerializeErrorFlags, Serializer},
    Plan, SubsetFlags, SubsetState, SubsetTable,
};

use fnv::FnvHashMap;
use write_fonts::{
    read::{
        collections::IntSet,
        tables::{
            gpos::{
                PairPos, PairPosFormat1, PairPosFormat2, PairSet, PairValueRecord, ValueFormat,
            },
            layout::CoverageTable,
        },
        types::GlyphId,
        FontData, FontRef, ReadError, TableProvider,
    },
    types::Offset16,
};

impl<'a> SubsetTable<'a> for PairPos<'_> {
    type ArgsForSubset = (&'a SubsetState, &'a FontRef<'a>);
    type Output = ();
    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        args: Self::ArgsForSubset,
    ) -> Result<Self::Output, SerializeErrorFlags> {
        match self {
            Self::Format1(item) => item.subset(plan, s, args),
            Self::Format2(item) => item.subset(plan, s, args),
        }
    }
}

fn compute_effective_pair_formats_1(
    pair_pos: &PairPosFormat1,
    glyph_set: &IntSet<GlyphId>,
    strip_hints: bool,
    strip_empty: bool,
) -> Result<(ValueFormat, ValueFormat), ReadError> {
    let mut new_format1 = ValueFormat::empty();
    let mut new_format2 = ValueFormat::empty();

    let orig_format1 = pair_pos.value_format1();
    let orig_format2 = pair_pos.value_format2();

    let coverage = pair_pos.coverage()?;
    let pair_sets = pair_pos.pair_sets();
    for (_, pair_set) in coverage
        .iter()
        .zip(pair_sets.iter())
        .filter(|&(g, _)| glyph_set.contains(GlyphId::from(g)))
    {
        let pair_set = pair_set?;
        for pair_value_rec in pair_set.pair_value_records().iter() {
            let pair_value_rec = pair_value_rec?;
            let second_glyph = pair_value_rec.second_glyph();
            if !glyph_set.contains(GlyphId::from(second_glyph)) {
                continue;
            }

            new_format1 |=
                compute_effective_format(pair_value_rec.value_record1(), strip_hints, strip_empty);
            new_format2 |=
                compute_effective_format(pair_value_rec.value_record2(), strip_hints, strip_empty);
        }
        if new_format1 == orig_format1 && new_format2 == orig_format2 {
            break;
        }
    }

    Ok((new_format1, new_format2))
}

impl<'a> SubsetTable<'a> for PairSet<'_> {
    type ArgsForSubset = (ValueFormat, ValueFormat);
    type Output = ();
    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        args: Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        // pairvalue count
        let pairvalue_count_pos = s.embed(0_u16)?;
        let mut count = 0_u16;

        let glyph_map = &plan.glyph_map_gsub;
        let font_data = self.offset_data();
        let (new_format1, new_format2) = args;

        for pairvalue_rec in self.pair_value_records().iter() {
            let pairvalue_rec = pairvalue_rec
                .or_else(|_| Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)))?;
            let Some(gid) = glyph_map.get(&GlyphId::from(pairvalue_rec.second_glyph())) else {
                continue;
            };
            pairvalue_rec.subset(plan, s, (gid, new_format1, new_format2, font_data))?;
            count += 1;
        }

        if count == 0 {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY);
        }
        s.copy_assign(pairvalue_count_pos, count);
        Ok(())
    }
}

impl<'a> SubsetTable<'a> for PairValueRecord {
    type ArgsForSubset = (&'a GlyphId, ValueFormat, ValueFormat, FontData<'a>);
    type Output = ();
    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        args: Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        let (new_gid, new_format1, new_format2, font_data) = args;
        // second glyph
        s.embed(new_gid.to_u32() as u16)?;

        //value records
        self.value_record1()
            .subset(plan, s, (new_format1, font_data))?;
        self.value_record2()
            .subset(plan, s, (new_format2, font_data))
    }
}

impl<'a> SubsetTable<'a> for PairPosFormat1<'_> {
    type ArgsForSubset = (&'a SubsetState, &'a FontRef<'a>);
    type Output = ();
    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        args: Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        let (subset_state, font) = args;
        let glyph_map = &plan.glyph_map_gsub;
        let glyph_set = &plan.glyphset_gsub;
        // format
        s.embed(self.pos_format())?;

        // coverage offset
        let cov_offset_pos = s.embed(0_u16)?;

        // value_formats
        let (new_format1, new_format2) = if plan
            .subset_flags
            .contains(SubsetFlags::SUBSET_FLAGS_NO_HINTING)
        {
            // do not strip hints for VF unless it has no GDEF varstore after subsetting
            let strip_hints = if font.fvar().is_ok() {
                !subset_state.has_gdef_varstore
            } else {
                true
            };

            compute_effective_pair_formats_1(self, glyph_set, strip_hints, true)
                .map_err(|_| s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR))?
        } else {
            (self.value_format1(), self.value_format2())
        };
        s.embed(new_format1)?;
        s.embed(new_format2)?;

        // pairset count
        let pairset_count_pos = s.embed(0_u16)?;
        let mut pairset_count = 0_u16;

        let coverage = self
            .coverage()
            .map_err(|_| s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR))?;
        let pair_sets = self.pair_sets();

        let mut glyphs = Vec::new();
        for (i, g) in coverage
            .iter()
            .enumerate()
            .filter_map(|(i, g)| glyph_map.get(&GlyphId::from(g)).map(|new_g| (i, new_g)))
        {
            match pair_sets.subset_offset(i, s, plan, (new_format1, new_format2)) {
                Ok(()) => {
                    pairset_count += 1;
                    glyphs.push(*g);
                }
                Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY) => (),
                Err(e) => {
                    return Err(e);
                }
            }
        }

        if glyphs.is_empty() {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY);
        }

        s.copy_assign(pairset_count_pos, pairset_count);
        Offset16::serialize_serialize::<CoverageTable>(s, &glyphs, cov_offset_pos)
    }
}

fn compute_effective_pair_formats_2(
    pair_pos: &PairPosFormat2,
    class1_idxes: &[u16],
    class2_idxes: &[u16],
    strip_hints: bool,
    strip_empty: bool,
) -> Result<(ValueFormat, ValueFormat), ReadError> {
    let mut new_format1 = ValueFormat::empty();
    let mut new_format2 = ValueFormat::empty();

    let orig_format1 = pair_pos.value_format1();
    let orig_format2 = pair_pos.value_format2();

    let class1_records = pair_pos.class1_records();
    for i in class1_idxes {
        let class1_rec = class1_records.get(*i as usize)?;
        let class2_records = class1_rec.class2_records();

        for j in class2_idxes {
            let class2_rec = class2_records.get(*j as usize)?;

            new_format1 |=
                compute_effective_format(class2_rec.value_record1(), strip_hints, strip_empty);
            new_format2 |=
                compute_effective_format(class2_rec.value_record2(), strip_hints, strip_empty);
        }
        if new_format1 == orig_format1 && new_format2 == orig_format2 {
            break;
        }
    }
    Ok((new_format1, new_format2))
}

impl<'a> SubsetTable<'a> for PairPosFormat2<'_> {
    type ArgsForSubset = (&'a SubsetState, &'a FontRef<'a>);
    type Output = ();
    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        args: Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        let Ok(coverage) = self.coverage() else {
            return Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR));
        };

        let glyphs: Vec<GlyphId> = coverage
            .intersect_set(&plan.glyphset_gsub)
            .iter()
            .filter_map(|g| plan.glyph_map_gsub.get(&g))
            .copied()
            .collect();
        if glyphs.is_empty() {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY);
        }

        // format
        s.embed(self.pos_format())?;

        // coverage offset
        let cov_offset_pos = s.embed(0_u16)?;

        // value format
        let value_format1_pos = s.embed(0_u16)?;
        let value_format2_pos = s.embed(0_u16)?;

        // classdef1 offset
        let classdef1_offset_pos = s.embed(0_u16)?;
        let class_def1 = self
            .class_def1()
            .map_err(|_| s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR))?;
        let class1_map = match Offset16::serialize_subset(
            &class_def1,
            s,
            plan,
            &ClassDefSubsetStruct {
                remap_class: true,
                keep_empty_table: true,
                use_class_zero: true,
                glyph_filter: Some(&coverage),
            },
            classdef1_offset_pos,
        ) {
            Ok(Some(out)) => out,
            _ => FnvHashMap::default(),
        };

        if class1_map.is_empty() {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY);
        }
        let class1_count = class1_map.len() as u16;

        // classdef2 offset
        let classdef2_offset_pos = s.embed(0_u16)?;
        let class_def2 = self
            .class_def2()
            .map_err(|_| s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR))?;

        let class2_map = match Offset16::serialize_subset(
            &class_def2,
            s,
            plan,
            &ClassDefSubsetStruct {
                remap_class: true,
                keep_empty_table: true,
                use_class_zero: false,
                glyph_filter: None,
            },
            classdef2_offset_pos,
        ) {
            Ok(Some(out)) => out,
            _ => FnvHashMap::default(),
        };

        // If only Class2 0 left, no need to keep anything.
        if class2_map.len() <= 1 {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY);
        }
        let class2_count = class2_map.len() as u16;

        // class1_count
        s.embed(class1_count)?;
        // class2_count
        s.embed(class2_count)?;

        // value formats
        let (subset_state, font) = args;
        let class1_idxes: Vec<u16> = (0..self.class1_count())
            .filter(|i| class1_map.contains_key(i))
            .collect();
        let class2_idxes: Vec<u16> = (0..self.class2_count())
            .filter(|i| class2_map.contains_key(i))
            .collect();

        let (new_format1, new_format2) = if plan
            .subset_flags
            .contains(SubsetFlags::SUBSET_FLAGS_NO_HINTING)
        {
            // do not strip hints for VF unless it has no GDEF varstore after subsetting
            let strip_hints = if font.fvar().is_ok() {
                !subset_state.has_gdef_varstore
            } else {
                true
            };

            compute_effective_pair_formats_2(self, &class1_idxes, &class2_idxes, strip_hints, true)
                .map_err(|_| s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR))?
        } else {
            (self.value_format1(), self.value_format2())
        };

        s.copy_assign(value_format1_pos, new_format1);
        s.copy_assign(value_format2_pos, new_format2);

        // serialize value records
        let font_data = self.offset_data();
        let class1_records = self.class1_records();
        for i in class1_idxes {
            let Ok(class1_record) = class1_records.get(i as usize) else {
                return Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR));
            };
            let class2_records = class1_record.class2_records();
            for j in &class2_idxes {
                let Ok(class2_rec) = class2_records.get(*j as usize) else {
                    return Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR));
                };

                class2_rec
                    .value_record1()
                    .subset(plan, s, (new_format1, font_data))?;
                class2_rec
                    .value_record2()
                    .subset(plan, s, (new_format2, font_data))?;
            }
        }

        // this can be moved, put it at last so we have the same binary data with Harfbuzz subsetter
        Offset16::serialize_serialize::<CoverageTable>(s, &glyphs, cov_offset_pos)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use write_fonts::read::{FontRef, TableProvider};

    #[test]
    fn test_subset_pairpos_format1() {
        use write_fonts::read::tables::gpos::PositionSubtables;

        let font = FontRef::new(include_bytes!("../../test-data/fonts/Amiri-Regular.ttf")).unwrap();
        let gpos_lookups = font.gpos().unwrap().lookup_list().unwrap();
        let lookup = gpos_lookups.lookups().get(56).unwrap();

        let PositionSubtables::Pair(sub_tables) = lookup.subtables().unwrap() else {
            panic!("Wrong type of lookup table!");
        };
        let pairpos_table = sub_tables.get(0).unwrap();

        let subset_state = SubsetState::default();
        let mut plan = Plan::default();

        plan.glyph_map_gsub
            .insert(GlyphId::from(6292_u32), GlyphId::from(3_u32));
        plan.glyph_map_gsub
            .insert(GlyphId::from(6298_u32), GlyphId::from(4_u32));

        plan.glyphset_gsub.insert(GlyphId::from(6292_u32));
        plan.glyphset_gsub.insert(GlyphId::from(6298_u32));

        let mut s = Serializer::new(1024);
        assert_eq!(s.start_serialize(), Ok(()));

        pairpos_table
            .subset(&plan, &mut s, (&subset_state, &font))
            .unwrap();
        assert!(!s.in_error());
        s.end_serialize();

        let subsetted_data = s.copy_bytes();
        let expected_data: [u8; 34] = [
            0x00, 0x01, 0x00, 0x0e, 0x00, 0x04, 0x00, 0x00, 0x00, 0x02, 0x00, 0x1c, 0x00, 0x16,
            0x00, 0x01, 0x00, 0x02, 0x00, 0x03, 0x00, 0x04, 0x00, 0x01, 0x00, 0x03, 0xff, 0xcf,
            0x00, 0x01, 0x00, 0x04, 0xff, 0xe8,
        ];

        assert_eq!(subsetted_data, expected_data);
    }

    #[test]
    fn test_subset_pairpos_format2() {
        use write_fonts::read::tables::gpos::PositionSubtables;

        let font = FontRef::new(include_bytes!("../../test-data/fonts/Amiri-Regular.ttf")).unwrap();
        let gpos_lookups = font.gpos().unwrap().lookup_list().unwrap();
        let lookup = gpos_lookups.lookups().get(82).unwrap();

        let PositionSubtables::Pair(sub_tables) = lookup.subtables().unwrap() else {
            panic!("Wrong type of lookup table!");
        };
        let pairpos_table = sub_tables.get(0).unwrap();

        let subset_state = SubsetState::default();
        let mut plan = Plan::default();

        //test case 1: ValueFormat remains the same
        plan.glyph_map_gsub
            .insert(GlyphId::from(40_u32), GlyphId::from(1_u32));
        plan.glyph_map_gsub
            .insert(GlyphId::from(72_u32), GlyphId::from(2_u32));
        plan.glyph_map_gsub
            .insert(GlyphId::from(168_u32), GlyphId::from(3_u32));
        plan.glyph_map_gsub
            .insert(GlyphId::from(6736_u32), GlyphId::from(4_u32));

        plan.glyphset_gsub.insert(GlyphId::from(40_u32));
        plan.glyphset_gsub.insert(GlyphId::from(72_u32));
        plan.glyphset_gsub.insert(GlyphId::from(168_u32));
        plan.glyphset_gsub.insert(GlyphId::from(6736_u32));

        let mut s = Serializer::new(1024);
        assert_eq!(s.start_serialize(), Ok(()));

        pairpos_table
            .subset(&plan, &mut s, (&subset_state, &font))
            .unwrap();
        assert!(!s.in_error());
        s.end_serialize();

        let subsetted_data = s.copy_bytes();
        let expected_data: [u8; 82] = [
            0x00, 0x02, 0x00, 0x2e, 0x00, 0x04, 0x00, 0x00, 0x00, 0x46, 0x00, 0x38, 0x00, 0x03,
            0x00, 0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0xff, 0xf2, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x01, 0x00, 0x01, 0x00, 0x04, 0x00, 0x00,
            0x00, 0x01, 0x00, 0x01, 0x00, 0x04, 0x00, 0x01, 0x00, 0x02, 0x00, 0x04, 0x00, 0x03,
            0x00, 0x01, 0x00, 0x01, 0x00, 0x03, 0x00, 0x02, 0x00, 0x01, 0x00, 0x01,
        ];

        assert_eq!(subsetted_data, expected_data);

        // test case 2: strip hints is enabled
        plan.subset_flags = SubsetFlags::SUBSET_FLAGS_NO_HINTING;
        plan.glyph_map_gsub.clear();
        plan.glyph_map_gsub
            .insert(GlyphId::from(72_u32), GlyphId::from(1_u32));
        plan.glyph_map_gsub
            .insert(GlyphId::from(168_u32), GlyphId::from(2_u32));
        plan.glyph_map_gsub
            .insert(GlyphId::from(6736_u32), GlyphId::from(3_u32));

        plan.glyphset_gsub.clear();
        plan.glyphset_gsub.insert(GlyphId::from(72_u32));
        plan.glyphset_gsub.insert(GlyphId::from(168_u32));
        plan.glyphset_gsub.insert(GlyphId::from(6736_u32));

        let mut s = Serializer::new(1024);
        assert_eq!(s.start_serialize(), Ok(()));

        pairpos_table
            .subset(&plan, &mut s, (&subset_state, &font))
            .unwrap();
        assert!(!s.in_error());
        s.end_serialize();

        let subsetted_data = s.copy_bytes();
        let expected_data: [u8; 48] = [
            0x00, 0x02, 0x00, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x26, 0x00, 0x1a, 0x00, 0x02,
            0x00, 0x04, 0x00, 0x01, 0x00, 0x03, 0x00, 0x01, 0x00, 0x02, 0x00, 0x03, 0x00, 0x01,
            0x00, 0x01, 0x00, 0x03, 0x00, 0x01, 0x00, 0x03, 0x00, 0x02, 0x00, 0x01, 0x00, 0x01,
            0x00, 0x02, 0x00, 0x01, 0x00, 0x01,
        ];

        assert_eq!(subsetted_data, expected_data);
    }
}
