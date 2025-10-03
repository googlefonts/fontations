//! impl subset() for LigatureSubst subtable
use crate::{
    layout::intersected_glyphs_and_indices,
    offset::SerializeSerialize,
    offset_array::SubsetOffsetArray,
    serialize::{SerializeErrorFlags, Serializer},
    Plan, SubsetState, SubsetTable,
};
use fnv::FnvHashMap;
use skrifa::raw::tables::layout::Intersect;
use write_fonts::{
    read::{
        collections::IntSet,
        tables::{
            gsub::{Ligature, LigatureSet, LigatureSubstFormat1},
            layout::CoverageTable,
        },
        types::GlyphId,
        FontRef, ReadError,
    },
    types::Offset16,
};

impl<'a> SubsetTable<'a> for LigatureSubstFormat1<'_> {
    type ArgsForSubset = (&'a SubsetState, &'a FontRef<'a>, &'a FnvHashMap<u16, u16>);
    type Output = ();
    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        _args: Self::ArgsForSubset,
    ) -> Result<Self::Output, SerializeErrorFlags> {
        let coverage = self
            .coverage()
            .map_err(|_| s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR))?;

        let glyph_set = &plan.glyphset_gsub;
        let (cov_glyphs, lig_set_idxes) =
            intersected_glyphs_and_indices(&coverage, glyph_set, &plan.glyph_map_gsub);

        if cov_glyphs.is_empty() {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY);
        }

        let lig_sets = self.ligature_sets();

        // we need to serialize retained coverage glyphs first so it'll be packed after the LigatureSet and Ligature tables
        // ref: <https://github.com/harfbuzz/harfbuzz/blob/0a257b0188ce8b002b51d9955713cd7136ca4769/src/OT/Layout/GSUB/LigatureSubstFormat1.hh#L155>
        let cap = cov_glyphs.len();
        let mut retained_cov_glyphs = Vec::with_capacity(cap);
        let mut retained_lig_set_idxes = Vec::with_capacity(cap);
        for (g, idx) in cov_glyphs.iter().zip(lig_set_idxes.iter()) {
            let lig_set = lig_sets
                .get(idx as usize)
                .map_err(|_| s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR))?;

            if !intersects_lig_glyph(&lig_set, glyph_set)
                .map_err(|_| s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR))?
            {
                continue;
            }

            retained_cov_glyphs.push(*g);
            retained_lig_set_idxes.push(idx as usize);
        }

        let lig_set_count = retained_lig_set_idxes.len();
        if lig_set_count == 0 {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY);
        }

        s.embed(self.subst_format())?;

        // cov offset
        // TODO: ensure that the repacker always orders the coverage table after the LigatureSet and LigatureSubtable's
        let cov_offset_pos = s.embed(0_u16)?;
        Offset16::serialize_serialize::<CoverageTable>(s, &retained_cov_glyphs, cov_offset_pos)?;

        // ligature set count
        s.embed(lig_set_count as u16)?;

        for i in retained_lig_set_idxes {
            lig_sets.subset_offset(i, s, plan, ())?;
        }

        Ok(())
    }
}

impl<'a> SubsetTable<'a> for LigatureSet<'_> {
    type ArgsForSubset = ();
    type Output = ();
    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        _args: Self::ArgsForSubset,
    ) -> Result<Self::Output, SerializeErrorFlags> {
        // ligature count
        let lig_count_pos = s.embed(0_u16)?;
        let mut lig_count = 0_u16;

        let ligs = self.ligatures();
        let org_lig_count = self.ligature_count();
        for idx in 0..org_lig_count as usize {
            match ligs.subset_offset(idx, s, plan, ()) {
                Ok(()) => lig_count += 1,
                Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY) => (),
                Err(e) => return Err(e),
            }
        }

        s.copy_assign(lig_count_pos, lig_count);
        Ok(())
    }
}

impl<'a> SubsetTable<'a> for Ligature<'_> {
    type ArgsForSubset = ();
    type Output = ();
    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        _args: Self::ArgsForSubset,
    ) -> Result<Self::Output, SerializeErrorFlags> {
        let glyph_map = &plan.glyph_map_gsub;
        let lig_glyph = glyph_map
            .get(&GlyphId::from(self.ligature_glyph()))
            .ok_or(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY)?;
        s.embed(lig_glyph.to_u32() as u16)?;

        s.embed(self.component_count())?;

        for g in self.component_glyph_ids() {
            let new_g = glyph_map
                .get(&GlyphId::from(g.get()))
                .ok_or(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY)?;

            s.embed(new_g.to_u32() as u16)?;
        }
        Ok(())
    }
}

fn intersects_lig_glyph(
    lig_set: &LigatureSet,
    glyphs: &IntSet<GlyphId>,
) -> Result<bool, ReadError> {
    for lig in lig_set.ligatures().iter() {
        let lig = lig?;
        let lig_glyph = lig.ligature_glyph();
        if glyphs.contains(GlyphId::from(lig_glyph)) && lig.intersects(glyphs)? {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(test)]
mod test {
    use super::*;
    use write_fonts::read::{types::GlyphId, FontRef, TableProvider};

    #[test]
    fn test_subset_ligature_subst() {
        use write_fonts::read::tables::gsub::SubstitutionSubtables;

        let font =
            FontRef::new(include_bytes!("../../test-data/fonts/Roboto-Regular.ttf")).unwrap();
        let gsub_lookups = font.gsub().unwrap().lookup_list().unwrap();
        let lookup = gsub_lookups.lookups().get(4).unwrap();

        let SubstitutionSubtables::Ligature(sub_tables) = lookup.subtables().unwrap() else {
            panic!("Wrong type of lookup table!");
        };
        let ligsubst_table = sub_tables.get(0).unwrap();
        let mut plan = Plan::default();

        plan.glyph_map_gsub
            .insert(GlyphId::from(51_u32), GlyphId::from(1_u32));
        plan.glyph_map_gsub
            .insert(GlyphId::from(169_u32), GlyphId::from(4_u32));
        plan.glyph_map_gsub
            .insert(GlyphId::from(170_u32), GlyphId::from(5_u32));
        plan.glyph_map_gsub
            .insert(GlyphId::from(657_u32), GlyphId::from(9_u32));
        plan.glyph_map_gsub
            .insert(GlyphId::from(659_u32), GlyphId::from(10_u32));
        plan.glyph_map_gsub
            .insert(GlyphId::from(1208_u32), GlyphId::from(13_u32));

        plan.glyphset_gsub.insert(GlyphId::from(51_u32));
        plan.glyphset_gsub.insert(GlyphId::from(169_u32));
        plan.glyphset_gsub.insert(GlyphId::from(170_u32));
        plan.glyphset_gsub.insert(GlyphId::from(657_u32));
        plan.glyphset_gsub.insert(GlyphId::from(659_u32));
        plan.glyphset_gsub.insert(GlyphId::from(1208_u32));

        let mut s = Serializer::new(1024);
        assert_eq!(s.start_serialize(), Ok(()));

        let subset_state = SubsetState::default();
        ligsubst_table
            .subset(&plan, &mut s, (&subset_state, &font, &plan.gsub_lookups))
            .unwrap();
        assert!(!s.in_error());
        s.end_serialize();

        let subsetted_data = s.copy_bytes();
        let expected_data: [u8; 42] = [
            0x00, 0x01, 0x00, 0x24, 0x00, 0x01, 0x00, 0x08, 0x00, 0x03, 0x00, 0x14, 0x00, 0x0e,
            0x00, 0x08, 0x00, 0x0a, 0x00, 0x02, 0x00, 0x05, 0x00, 0x09, 0x00, 0x02, 0x00, 0x04,
            0x00, 0x0d, 0x00, 0x03, 0x00, 0x05, 0x00, 0x04, 0x00, 0x01, 0x00, 0x01, 0x00, 0x01,
        ];

        assert_eq!(subsetted_data, expected_data);
    }
}
