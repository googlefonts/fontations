//! impl subset() for Sequence Context/Chained Sequence Context tables
use crate::{
    layout::intersected_glyphs_and_indices,
    offset::SerializeSerialize,
    offset_array::SubsetOffsetArray,
    serialize::{SerializeErrorFlags, Serializer},
    Plan, SubsetTable,
};
use fnv::FnvHashMap;
use write_fonts::{
    read::tables::layout::{
        ChainedSequenceContext, ChainedSequenceContextFormat1, ChainedSequenceRule,
        ChainedSequenceRuleSet, CoverageTable, SequenceContext, SequenceContextFormat1,
        SequenceLookupRecord, SequenceRule, SequenceRuleSet,
    },
    types::{BigEndian, GlyphId, GlyphId16, Offset16},
};

impl<'a> SubsetTable<'a> for SequenceContext<'_> {
    type ArgsForSubset = &'a FnvHashMap<u16, u16>;
    type Output = ();
    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        lookup_map: Self::ArgsForSubset,
    ) -> Result<Self::Output, SerializeErrorFlags> {
        match self {
            Self::Format1(item) => item.subset(plan, s, lookup_map),
            // TODO: support format 2 and 3
            Self::Format2(_item) => Ok(()),
            Self::Format3(_item) => Ok(()),
        }
    }
}

impl<'a> SubsetTable<'a> for SequenceContextFormat1<'_> {
    type ArgsForSubset = &'a FnvHashMap<u16, u16>;
    type Output = ();
    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        lookup_map: Self::ArgsForSubset,
    ) -> Result<Self::Output, SerializeErrorFlags> {
        let coverage = self
            .coverage()
            .map_err(|_| s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR))?;

        let (cov_glyphs, rule_sets_idxes) =
            intersected_glyphs_and_indices(&coverage, &plan.glyphset_gsub, &plan.glyph_map_gsub);
        if rule_sets_idxes.is_empty() {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY);
        }

        // format
        s.embed(self.format())?;

        // coverage offset
        let cov_offset_pos = s.embed(0_u16)?;
        // seq ruleset count
        let seq_ruleset_count_pos = s.embed(0_u16)?;
        let mut rule_set_count = 0_u16;

        let mut new_cov_glyphs = Vec::with_capacity(cov_glyphs.len());
        // seq rulesets offsets
        let rule_sets = self.seq_rule_sets();
        for (g, idx) in cov_glyphs.iter().zip(rule_sets_idxes.iter()) {
            match rule_sets.subset_offset(idx as usize, s, plan, lookup_map) {
                Ok(()) => {
                    new_cov_glyphs.push(*g);
                    rule_set_count += 1;
                }
                Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY) => (),
                Err(e) => return Err(e),
            }
        }

        if rule_set_count == 0 {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY);
        }

        s.copy_assign(seq_ruleset_count_pos, rule_set_count);
        Offset16::serialize_serialize::<CoverageTable>(s, &new_cov_glyphs, cov_offset_pos)
    }
}

impl<'a> SubsetTable<'a> for SequenceRuleSet<'_> {
    type ArgsForSubset = &'a FnvHashMap<u16, u16>;
    type Output = ();
    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        lookup_map: Self::ArgsForSubset,
    ) -> Result<Self::Output, SerializeErrorFlags> {
        // seq rule count
        let seq_rule_count_pos = s.embed(0_u16)?;
        let mut seq_rule_count = 0_u16;

        let seq_rules = self.seq_rules();
        let org_rule_count = self.seq_rule_count();
        for i in 0..org_rule_count {
            match seq_rules.subset_offset(i as usize, s, plan, lookup_map) {
                Ok(()) => seq_rule_count += 1,
                Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY) => (),
                Err(e) => return Err(e),
            }
        }

        if seq_rule_count == 0 {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY);
        }
        s.copy_assign(seq_rule_count_pos, seq_rule_count);
        Ok(())
    }
}

fn serialize_glyph_sequence(
    sequence: &[BigEndian<GlyphId16>],
    glyph_map: &FnvHashMap<GlyphId, GlyphId>,
    s: &mut Serializer,
) -> Result<(), SerializeErrorFlags> {
    for g in sequence {
        if let Some(new_g) = glyph_map.get(&GlyphId::from(g.get())) {
            s.embed(new_g.to_u32() as u16)?;
        } else {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY);
        }
    }
    Ok(())
}

fn serialize_lookup_records(
    lookup_records: &[SequenceLookupRecord],
    plan: &Plan,
    lookup_map: &FnvHashMap<u16, u16>,
    s: &mut Serializer,
) -> Result<u16, SerializeErrorFlags> {
    let mut seq_lookup_count = 0_u16;
    for lookup in lookup_records {
        match lookup.subset(plan, s, lookup_map) {
            Ok(()) => seq_lookup_count += 1,
            Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY) => (),
            Err(e) => return Err(e),
        }
    }
    Ok(seq_lookup_count)
}

impl<'a> SubsetTable<'a> for SequenceRule<'_> {
    type ArgsForSubset = &'a FnvHashMap<u16, u16>;
    type Output = ();
    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        lookup_map: Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        let glyph_count = self.glyph_count();
        if glyph_count == 0 {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY);
        }

        s.embed(glyph_count)?;
        let glyph_map = &plan.glyph_map_gsub;
        // seq lookup count
        let seq_lookup_count_pos = s.embed(0_u16)?;

        // input sequence
        serialize_glyph_sequence(self.input_sequence(), glyph_map, s)?;

        // seq lookup records
        let seq_lookup_count =
            serialize_lookup_records(self.seq_lookup_records(), plan, lookup_map, s)?;
        if seq_lookup_count == 0 {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY);
        }
        s.copy_assign(seq_lookup_count_pos, seq_lookup_count);
        Ok(())
    }
}

impl<'a> SubsetTable<'a> for SequenceLookupRecord {
    type ArgsForSubset = &'a FnvHashMap<u16, u16>;
    type Output = ();
    fn subset(
        &self,
        _plan: &Plan,
        s: &mut Serializer,
        lookup_map: Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        let lookup_index = self.lookup_list_index();
        let Some(new_index) = lookup_map.get(&lookup_index) else {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY);
        };

        s.embed(self.sequence_index())?;
        s.embed(*new_index).map(|_| ())
    }
}

impl<'a> SubsetTable<'a> for ChainedSequenceContext<'_> {
    type ArgsForSubset = &'a FnvHashMap<u16, u16>;
    type Output = ();
    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        lookup_map: Self::ArgsForSubset,
    ) -> Result<Self::Output, SerializeErrorFlags> {
        match self {
            Self::Format1(item) => item.subset(plan, s, lookup_map),
            // TODO: support format 2 and 3
            Self::Format2(_item) => Ok(()),
            Self::Format3(_item) => Ok(()),
        }
    }
}

impl<'a> SubsetTable<'a> for ChainedSequenceContextFormat1<'_> {
    type ArgsForSubset = &'a FnvHashMap<u16, u16>;
    type Output = ();
    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        lookup_map: Self::ArgsForSubset,
    ) -> Result<Self::Output, SerializeErrorFlags> {
        let coverage = self
            .coverage()
            .map_err(|_| s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR))?;

        let (cov_glyphs, rule_sets_idxes) =
            intersected_glyphs_and_indices(&coverage, &plan.glyphset_gsub, &plan.glyph_map_gsub);
        if rule_sets_idxes.is_empty() {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY);
        }

        // format
        s.embed(self.format())?;

        // coverage offset
        let cov_offset_pos = s.embed(0_u16)?;
        // chained seq ruleset count
        let seq_ruleset_count_pos = s.embed(0_u16)?;
        let mut rule_set_count = 0_u16;

        let mut new_cov_glyphs = Vec::with_capacity(cov_glyphs.len());
        // chained seq rulesets offsets
        let rule_sets = self.chained_seq_rule_sets();
        for (g, idx) in cov_glyphs.iter().zip(rule_sets_idxes.iter()) {
            match rule_sets.subset_offset(idx as usize, s, plan, lookup_map) {
                Ok(()) => {
                    new_cov_glyphs.push(*g);
                    rule_set_count += 1;
                }
                Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY) => (),
                Err(e) => return Err(e),
            }
        }

        if rule_set_count == 0 {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY);
        }

        s.copy_assign(seq_ruleset_count_pos, rule_set_count);
        Offset16::serialize_serialize::<CoverageTable>(s, &new_cov_glyphs, cov_offset_pos)
    }
}

impl<'a> SubsetTable<'a> for ChainedSequenceRuleSet<'_> {
    type ArgsForSubset = &'a FnvHashMap<u16, u16>;
    type Output = ();
    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        lookup_map: Self::ArgsForSubset,
    ) -> Result<Self::Output, SerializeErrorFlags> {
        // chained seq rule count
        let seq_rule_count_pos = s.embed(0_u16)?;
        let mut seq_rule_count = 0_u16;

        let seq_rules = self.chained_seq_rules();
        let org_rule_count = self.chained_seq_rule_count();
        for i in 0..org_rule_count {
            match seq_rules.subset_offset(i as usize, s, plan, lookup_map) {
                Ok(()) => seq_rule_count += 1,
                Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY) => (),
                Err(e) => return Err(e),
            }
        }

        if seq_rule_count == 0 {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY);
        }
        s.copy_assign(seq_rule_count_pos, seq_rule_count);
        Ok(())
    }
}

impl<'a> SubsetTable<'a> for ChainedSequenceRule<'_> {
    type ArgsForSubset = &'a FnvHashMap<u16, u16>;
    type Output = ();
    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        lookup_map: Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        let glyph_map = &plan.glyph_map_gsub;
        // backtrack glyph count
        s.embed(self.backtrack_glyph_count())?;
        // backtrack sequence
        serialize_glyph_sequence(self.backtrack_sequence(), glyph_map, s)?;

        // input glyph count
        s.embed(self.input_glyph_count())?;
        // input sequence
        serialize_glyph_sequence(self.input_sequence(), glyph_map, s)?;

        // lookahead glyph count
        s.embed(self.lookahead_glyph_count())?;
        // lookahead sequence
        serialize_glyph_sequence(self.lookahead_sequence(), glyph_map, s)?;

        // seq lookup count
        let seq_lookup_count_pos = s.embed(0_u16)?;
        // seq lookup records
        let seq_lookup_count =
            serialize_lookup_records(self.seq_lookup_records(), plan, lookup_map, s)?;
        if seq_lookup_count == 0 {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY);
        }
        s.copy_assign(seq_lookup_count_pos, seq_lookup_count);
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use write_fonts::read::{types::GlyphId, FontRef, TableProvider};

    #[test]
    fn test_subset_context_format1() {
        use write_fonts::read::tables::gsub::SubstitutionSubtables;

        let font = FontRef::new(include_bytes!(
            "../test-data/fonts/NotoNastaliqUrdu-Regular.ttf"
        ))
        .unwrap();
        let gsub_lookups = font.gsub().unwrap().lookup_list().unwrap();
        let lookup = gsub_lookups.lookups().get(105).unwrap();

        let SubstitutionSubtables::Contextual(sub_tables) = lookup.subtables().unwrap() else {
            panic!("Wrong type of lookup table!");
        };
        let contextsubst_table = sub_tables.get(0).unwrap();
        let mut plan = Plan::default();

        plan.glyph_map_gsub
            .insert(GlyphId::from(229_u32), GlyphId::from(1_u32));
        plan.glyph_map_gsub
            .insert(GlyphId::from(235_u32), GlyphId::from(2_u32));
        plan.glyph_map_gsub
            .insert(GlyphId::from(559_u32), GlyphId::from(3_u32));

        plan.glyphset_gsub.insert(GlyphId::from(229_u32));
        plan.glyphset_gsub.insert(GlyphId::from(235_u32));
        plan.glyphset_gsub.insert(GlyphId::from(559_u32));

        let mut lookup_map = FnvHashMap::default();
        lookup_map.insert(58_u16, 0_u16);

        let mut s = Serializer::new(1024);
        assert_eq!(s.start_serialize(), Ok(()));

        contextsubst_table
            .subset(&plan, &mut s, &lookup_map)
            .unwrap();
        assert!(!s.in_error());
        s.end_serialize();

        let subsetted_data = s.copy_bytes();
        let expected_data: [u8; 40] = [
            0x00, 0x01, 0x00, 0x08, 0x00, 0x01, 0x00, 0x0e, 0x00, 0x01, 0x00, 0x01, 0x00, 0x03,
            0x00, 0x02, 0x00, 0x10, 0x00, 0x06, 0x00, 0x02, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x02, 0x00, 0x01, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00,
        ];

        assert_eq!(subsetted_data, expected_data);
    }

    #[test]
    fn test_subset_chained_context_format1() {
        use write_fonts::read::tables::gpos::PositionSubtables;

        let font = FontRef::new(include_bytes!(
            "../test-data/fonts/gpos_chaining1_multiple_subrules_f1.otf"
        ))
        .unwrap();
        let gpos_lookups = font.gpos().unwrap().lookup_list().unwrap();
        let lookup = gpos_lookups.lookups().get(4).unwrap();

        let PositionSubtables::ChainContextual(sub_tables) = lookup.subtables().unwrap() else {
            panic!("Wrong type of lookup table!");
        };
        let chainedcontextpos_table = sub_tables.get(0).unwrap();
        let mut plan = Plan::default();

        plan.glyph_map_gsub
            .insert(GlyphId::from(48_u32), GlyphId::from(1_u32));
        plan.glyph_map_gsub
            .insert(GlyphId::from(49_u32), GlyphId::from(2_u32));
        plan.glyph_map_gsub
            .insert(GlyphId::from(50_u32), GlyphId::from(3_u32));
        plan.glyph_map_gsub
            .insert(GlyphId::from(51_u32), GlyphId::from(4_u32));

        plan.glyphset_gsub.insert(GlyphId::from(48_u32));
        plan.glyphset_gsub.insert(GlyphId::from(49_u32));
        plan.glyphset_gsub.insert(GlyphId::from(50_u32));
        plan.glyphset_gsub.insert(GlyphId::from(51_u32));

        let mut lookup_map = FnvHashMap::default();
        lookup_map.insert(1_u16, 0_u16);

        let mut s = Serializer::new(1024);
        assert_eq!(s.start_serialize(), Ok(()));

        chainedcontextpos_table
            .subset(&plan, &mut s, &lookup_map)
            .unwrap();
        assert!(!s.in_error());
        s.end_serialize();

        let subsetted_data = s.copy_bytes();
        let expected_data: [u8; 36] = [
            0x00, 0x01, 0x00, 0x08, 0x00, 0x01, 0x00, 0x0e, 0x00, 0x01, 0x00, 0x01, 0x00, 0x02,
            0x00, 0x01, 0x00, 0x04, 0x00, 0x01, 0x00, 0x01, 0x00, 0x02, 0x00, 0x03, 0x00, 0x01,
            0x00, 0x04, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00,
        ];

        assert_eq!(subsetted_data, expected_data);
    }
}
