//! impl subset() for GDEF

use crate::{
    offset::{SerializeSerialize, SerializeSubset},
    offset_array::SubsetOffsetArray,
    serialize::{SerializeErrorFlags, Serializer},
    CollectVariationIndices, Plan, Subset, SubsetError, SubsetTable,
};
use write_fonts::{
    read::{
        collections::IntSet,
        tables::{
            gdef::{
                AttachList, AttachPoint, CaretValue, CaretValueFormat1, CaretValueFormat2,
                CaretValueFormat3, Gdef, LigCaretList, LigGlyph, MarkGlyphSets,
            },
            layout::CoverageTable,
        },
        types::GlyphId,
        FontRef,
    },
    types::Offset16,
    FontBuilder,
};

// reference: subset() for GDEF in harfbuzz
// <https://github.com/harfbuzz/harfbuzz/blob/59001aa9527c056ad08626cfec9a079b65d8aec8/src/OT/Layout/GDEF/GDEF.hh#L660>
impl Subset for Gdef<'_> {
    fn subset(
        &self,
        plan: &Plan,
        _font: &FontRef,
        s: &mut Serializer,
        _builder: &mut FontBuilder,
    ) -> Result<(), SubsetError> {
    }
}

impl SubsetTable<'_> for AttachList<'_> {
    type ArgsForSubset = ();
    type Output = ();

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        _args: Self::ArgsForSubset,
    ) -> Result<Self::Output, SerializeErrorFlags> {
        //coverage offset
        let coverage_offset_pos = s.embed(0_u16)?;
        //glyph_count
        let glyph_count_pos = s.embed(0_u16)?;

        let Ok(coverage) = self.coverage() else {
            return Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR));
        };

        let attach_points = self.attach_points();
        let mut count = 0_u16;
        let src_glyph_count = self.glyph_count() as usize;
        let mut retained_glyphs =
            Vec::with_capacity(plan.glyph_map_gsub.len().min(src_glyph_count));

        for (idx, glyph) in coverage
            .iter()
            .enumerate()
            .take(plan.font_num_glyphs.min(src_glyph_count))
        {
            let Some(new_gid) = plan.glyph_map_gsub.get(&GlyphId::from(glyph)) else {
                continue;
            };

            attach_points.subset_offset(idx, s, plan, ())?;
            count += 1;
            retained_glyphs.push(new_gid.to_u32());
        }

        if retained_glyphs.is_empty() {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY);
        }

        s.copy_assign(glyph_count_pos, count);
        Offset16::serialize_serialize::<CoverageTable>(s, &retained_glyphs, coverage_offset_pos)
    }
}

impl SubsetTable<'_> for AttachPoint<'_> {
    type ArgsForSubset = ();
    type Output = ();
    fn subset(
        &self,
        _plan: &Plan,
        s: &mut Serializer,
        _args: Self::ArgsForSubset,
    ) -> Result<Self::Output, SerializeErrorFlags> {
        s.embed_bytes(self.min_table_bytes()).map(|_| ())
    }
}

impl SubsetTable<'_> for LigCaretList<'_> {
    type ArgsForSubset = ();
    type Output = ();
    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        _args: Self::ArgsForSubset,
    ) -> Result<Self::Output, SerializeErrorFlags> {
        //coverage offset
        let coverage_offset_pos = s.embed(0_u16)?;
        //lig_glyph_count
        let lig_glyph_count_pos = s.embed(0_u16)?;

        let Ok(coverage) = self.coverage() else {
            return Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR));
        };

        let lig_glyphs = self.lig_glyphs();
        let mut count = 0_u16;
        let src_lig_glyph_count = self.lig_glyph_count() as usize;
        let mut retained_glyphs =
            Vec::with_capacity(plan.glyph_map_gsub.len().min(src_lig_glyph_count));

        for (idx, glyph) in coverage
            .iter()
            .enumerate()
            .take(plan.font_num_glyphs.min(src_lig_glyph_count))
        {
            let Some(new_gid) = plan.glyph_map_gsub.get(&GlyphId::from(glyph)) else {
                continue;
            };

            lig_glyphs.subset_offset(idx, s, plan, ())?;
            count += 1;
            retained_glyphs.push(new_gid.to_u32());
        }

        if retained_glyphs.is_empty() {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY);
        }

        s.copy_assign(lig_glyph_count_pos, count);
        Offset16::serialize_serialize::<CoverageTable>(s, &retained_glyphs, coverage_offset_pos)
    }
}

impl SubsetTable<'_> for LigGlyph<'_> {
    type ArgsForSubset = ();
    type Output = ();
    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        _args: Self::ArgsForSubset,
    ) -> Result<Self::Output, SerializeErrorFlags> {
        // caret count
        let caret_count_pos = s.embed(0_u16)?;

        let caret_values = self.caret_values();
        let mut count = 0_u16;
        for idx in 0..caret_values.len() {
            caret_values.subset_offset(idx, s, plan, ())?;
            count += 1;
        }

        if count == 0 {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY);
        }

        s.copy_assign(caret_count_pos, count);
        Ok(())
    }
}

impl SubsetTable<'_> for CaretValue<'_> {
    type ArgsForSubset = ();
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
            Self::Format3(item) => item.subset(plan, s, args),
        }
    }
}

impl SubsetTable<'_> for CaretValueFormat1<'_> {
    type ArgsForSubset = ();
    type Output = ();
    fn subset(
        &self,
        _plan: &Plan,
        s: &mut Serializer,
        _args: Self::ArgsForSubset,
    ) -> Result<Self::Output, SerializeErrorFlags> {
        s.embed_bytes(self.min_table_bytes()).map(|_| ())
    }
}

impl SubsetTable<'_> for CaretValueFormat2<'_> {
    type ArgsForSubset = ();
    type Output = ();
    fn subset(
        &self,
        _plan: &Plan,
        s: &mut Serializer,
        _args: Self::ArgsForSubset,
    ) -> Result<Self::Output, SerializeErrorFlags> {
        s.embed_bytes(self.min_table_bytes()).map(|_| ())
    }
}

impl SubsetTable<'_> for CaretValueFormat3<'_> {
    type ArgsForSubset = ();
    type Output = ();
    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        _args: Self::ArgsForSubset,
    ) -> Result<Self::Output, SerializeErrorFlags> {
        s.embed(self.caret_value_format())?;
        s.embed(self.coordinate())?;
        let device_offset_pos = s.embed(0_u16)?;

        let Ok(device) = self.device() else {
            return Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR));
        };
        Offset16::serialize_subset(
            &device,
            s,
            plan,
            &plan.layout_varidx_delta_map,
            device_offset_pos,
        )
    }
}

impl CollectVariationIndices for Gdef<'_> {
    fn collect_variation_indices(&self, plan: &Plan, varidx_set: &mut IntSet<u32>) {
        if let Some(Ok(lig_caret_list)) = self.lig_caret_list() {
            lig_caret_list.collect_variation_indices(plan, varidx_set);
        }
    }
}

impl CollectVariationIndices for LigCaretList<'_> {
    fn collect_variation_indices(&self, plan: &Plan, varidx_set: &mut IntSet<u32>) {
        let Ok(coverage) = self.coverage() else {
            return;
        };

        let lig_glyphs = self.lig_glyphs();
        for (gid, lig_glyph) in coverage.iter().zip(lig_glyphs.iter()) {
            let Ok(lig_glyph) = lig_glyph else {
                return;
            };
            if !plan.glyphset_gsub.contains(GlyphId::from(gid)) {
                continue;
            }
            lig_glyph.collect_variation_indices(plan, varidx_set);
        }
    }
}

impl CollectVariationIndices for LigGlyph<'_> {
    fn collect_variation_indices(&self, plan: &Plan, varidx_set: &mut IntSet<u32>) {
        for caret in self.caret_values().iter() {
            let Ok(caret) = caret else {
                return;
            };
            caret.collect_variation_indices(plan, varidx_set);
        }
    }
}

impl CollectVariationIndices for CaretValue<'_> {
    fn collect_variation_indices(&self, plan: &Plan, varidx_set: &mut IntSet<u32>) {
        if let Self::Format3(item) = self {
            if let Ok(device) = item.device() {
                device.collect_variation_indices(plan, varidx_set);
            }
        }
    }
}

pub(crate) trait CollectUsedMarkSets {
    fn collect_used_mark_sets(&self, plan: &Plan, used_mark_sets: &mut IntSet<u16>);
}

impl CollectUsedMarkSets for Gdef<'_> {
    fn collect_used_mark_sets(&self, plan: &Plan, used_mark_sets: &mut IntSet<u16>) {
        if let Some(Ok(mark_glyph_sets)) = self.mark_glyph_sets_def() {
            mark_glyph_sets.collect_used_mark_sets(plan, used_mark_sets);
        };
    }
}

impl CollectUsedMarkSets for MarkGlyphSets<'_> {
    fn collect_used_mark_sets(&self, plan: &Plan, used_mark_sets: &mut IntSet<u16>) {
        for (i, coverage) in self.coverages().iter().enumerate() {
            let Ok(coverage) = coverage else {
                return;
            };
            if coverage.intersects(&plan.glyphset_gsub) {
                used_mark_sets.insert(i as u16);
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use write_fonts::{
        read::{FontRef, TableProvider},
        types::GlyphId,
    };
    #[test]
    fn test_collect_var_indices() {
        let mut plan = Plan::default();
        plan.glyphset_gsub.insert(GlyphId::from(4_u32));
        plan.glyphset_gsub.insert(GlyphId::from(7_u32));

        let font =
            FontRef::new(include_bytes!("../test-data/fonts/AnekBangla-subset.ttf")).unwrap();
        let gdef = font.gdef().unwrap();

        let mut varidx_set = IntSet::empty();
        gdef.collect_variation_indices(&plan, &mut varidx_set);
        assert_eq!(varidx_set.len(), 3);
        assert!(varidx_set.contains(3));
        assert!(varidx_set.contains(5));
        assert!(varidx_set.contains(0));
    }

    #[test]
    fn test_collect_used_mark_sets() {
        let mut plan = Plan::default();
        plan.glyphset_gsub.insert(GlyphId::from(171_u32));

        let font = FontRef::new(include_bytes!("../test-data/fonts/Roboto-Regular.ttf")).unwrap();
        let gdef = font.gdef().unwrap();

        let mut used_mark_sets = IntSet::empty();
        gdef.collect_used_mark_sets(&plan, &mut used_mark_sets);
        assert_eq!(used_mark_sets.len(), 1);
        assert!(used_mark_sets.contains(0));
    }
}
