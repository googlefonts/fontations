//! impl subset() for CursivePos subtable

use crate::{
    offset::{SerializeSerialize, SerializeSubset},
    serialize::{SerializeErrorFlags, Serializer},
    Plan, SubsetState, SubsetTable,
};
use write_fonts::{
    read::{
        tables::{
            gpos::{CursivePosFormat1, EntryExitRecord},
            layout::CoverageTable,
        },
        types::GlyphId,
        FontData, FontRef,
    },
    types::Offset16,
};

impl<'a> SubsetTable<'a> for CursivePosFormat1<'_> {
    type ArgsForSubset = (&'a SubsetState, &'a FontRef<'a>);
    type Output = ();
    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        _args: Self::ArgsForSubset,
    ) -> Result<Self::Output, SerializeErrorFlags> {
        s.embed(self.pos_format())?;

        //cov offset
        let cov_offset_pos = s.embed(0_u16)?;

        //entry exit count
        let entryexit_count_pos = s.embed(0_u16)?;
        let mut entry_exit_count = 0_u16;

        let coverage = self
            .coverage()
            .map_err(|_| SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)?;
        let exit_records = self.entry_exit_record();
        let mut glyphs =
            Vec::with_capacity(exit_records.len().min(plan.glyphset_gsub.len() as usize));

        let font_data = self.offset_data();
        for (i, g) in coverage.iter().enumerate().filter_map(|(i, g)| {
            plan.glyph_map_gsub
                .get(&GlyphId::from(g))
                .map(|new_g| (i, *new_g))
        }) {
            let Some(exit_record) = exit_records.get(i) else {
                return Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR));
            };
            match exit_record.subset(plan, s, font_data) {
                Ok(()) => {
                    entry_exit_count += 1;
                    glyphs.push(g);
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
        s.copy_assign(entryexit_count_pos, entry_exit_count);
        Offset16::serialize_serialize::<CoverageTable>(s, &glyphs, cov_offset_pos)
    }
}

impl<'a> SubsetTable<'a> for EntryExitRecord {
    type ArgsForSubset = FontData<'a>;
    type Output = ();
    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        font_data: FontData,
    ) -> Result<Self::Output, SerializeErrorFlags> {
        if self.entry_anchor_offset().is_null() && self.exit_anchor_offset().is_null() {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY);
        }
        let entry_offset_pos = s.embed(0_u16)?;
        if let Some(entry_anchor) = self
            .entry_anchor(font_data)
            .transpose()
            .map_err(|_| SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)?
        {
            Offset16::serialize_subset(&entry_anchor, s, plan, (), entry_offset_pos)?;
        }

        let exit_offset_pos = s.embed(0_u16)?;
        if let Some(exit_anchor) = self
            .exit_anchor(font_data)
            .transpose()
            .map_err(|_| SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)?
        {
            Offset16::serialize_subset(&exit_anchor, s, plan, (), exit_offset_pos)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use write_fonts::read::{FontRef, TableProvider};

    #[test]
    fn test_subset_cursive_pos() {
        use write_fonts::read::tables::gpos::PositionSubtables;

        let font = FontRef::new(include_bytes!("../../test-data/fonts/Amiri-Regular.ttf")).unwrap();
        let gpos_lookups = font.gpos().unwrap().lookup_list().unwrap();
        let lookup = gpos_lookups.lookups().get(57).unwrap();

        let PositionSubtables::Cursive(sub_tables) = lookup.subtables().unwrap() else {
            panic!("Wrong type of lookup table!");
        };
        let cursivepos_table = sub_tables.get(0).unwrap();

        let subset_state = SubsetState::default();
        let mut plan = Plan::default();

        plan.glyph_map_gsub
            .insert(GlyphId::from(1803_u32), GlyphId::from(2_u32));
        plan.glyph_map_gsub
            .insert(GlyphId::from(3098_u32), GlyphId::from(4_u32));
        plan.glyphset_gsub.insert(GlyphId::from(1803_u32));
        plan.glyphset_gsub.insert(GlyphId::from(3098_u32));

        let mut s = Serializer::new(1024);
        assert_eq!(s.start_serialize(), Ok(()));

        cursivepos_table
            .subset(&plan, &mut s, (&subset_state, &font))
            .unwrap();
        assert!(!s.in_error());
        s.end_serialize();

        let subsetted_data = s.copy_bytes();
        let expected_data: [u8; 34] = [
            0x00, 0x01, 0x00, 0x0e, 0x00, 0x02, 0x00, 0x00, 0x00, 0x1c, 0x00, 0x00, 0x00, 0x16,
            0x00, 0x01, 0x00, 0x02, 0x00, 0x02, 0x00, 0x04, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x01, 0x00, 0x00, 0x00, 0xfe,
        ];

        assert_eq!(subsetted_data, expected_data);
    }
}
