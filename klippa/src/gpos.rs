//! impl subset() for GPOS table

use crate::{
    collect_features_with_retained_subs, find_duplicate_features,
    offset::{SerializeSerialize, SerializeSubset},
    prune_features, remap_indices,
    serialize::{SerializeErrorFlags, Serializer},
    LayoutClosure, NameIdClosure, Plan, PruneLangSysContext, Serialize, Subset, SubsetError,
    SubsetFlags, SubsetLayoutContext, SubsetState, SubsetTable,
};
use fnv::FnvHashMap;
use write_fonts::{
    read::{
        collections::IntSet,
        tables::{
            gpos::{Gpos, SinglePos, SinglePosFormat1, SinglePosFormat2, ValueFormat, ValueRecord},
            layout::CoverageTable,
        },
        types::{GlyphId, Tag},
        FontData, FontRef, TableProvider, TopLevelTable,
    },
    types::Offset16,
    FontBuilder,
};

impl NameIdClosure for Gpos<'_> {
    //TODO: support instancing: collect from feature substitutes if exist
    fn collect_name_ids(&self, plan: &mut Plan) {
        let Ok(feature_list) = self.feature_list() else {
            return;
        };
        for (i, feature_record) in feature_list.feature_records().iter().enumerate() {
            if !plan.gpos_features.contains_key(&(i as u16)) {
                continue;
            }
            let Ok(feature) = feature_record.feature(feature_list.offset_data()) else {
                continue;
            };
            feature.collect_name_ids(plan);
        }
    }
}

impl LayoutClosure for Gpos<'_> {
    fn prune_features(
        &self,
        lookup_indices: &IntSet<u16>,
        feature_indices: IntSet<u16>,
    ) -> IntSet<u16> {
        let alternate_features = if let Some(Ok(feature_variations)) = self.feature_variations() {
            collect_features_with_retained_subs(&feature_variations, lookup_indices)
        } else {
            IntSet::empty()
        };

        let Ok(feature_list) = self.feature_list() else {
            return IntSet::empty();
        };
        prune_features(
            &feature_list,
            &alternate_features,
            lookup_indices,
            feature_indices,
        )
    }

    fn find_duplicate_features(
        &self,
        lookup_indices: &IntSet<u16>,
        feature_indices: IntSet<u16>,
    ) -> FnvHashMap<u16, u16> {
        let Ok(feature_list) = self.feature_list() else {
            return FnvHashMap::default();
        };
        find_duplicate_features(&feature_list, lookup_indices, feature_indices)
    }

    fn prune_langsys(
        &self,
        duplicate_feature_index_map: &FnvHashMap<u16, u16>,
        layout_scripts: &IntSet<Tag>,
    ) -> (FnvHashMap<u16, IntSet<u16>>, IntSet<u16>) {
        let mut c = PruneLangSysContext::new(duplicate_feature_index_map);
        let Ok(script_list) = self.script_list() else {
            return (c.script_langsys_map(), c.feature_indices());
        };
        c.prune_langsys(&script_list, layout_scripts)
    }

    fn closure_glyphs_lookups_features(&self, plan: &mut Plan) {
        let Ok(feature_indices) =
            self.collect_features(&plan.layout_scripts, &IntSet::all(), &plan.layout_features)
        else {
            return;
        };

        let Ok(mut lookup_indices) = self.collect_lookups(&feature_indices) else {
            return;
        };
        let Ok(_) = self.closure_lookups(&plan.glyphset_gsub, &mut lookup_indices) else {
            return;
        };

        let feature_indices = self.prune_features(&lookup_indices, feature_indices);
        let duplicate_feature_index_map =
            self.find_duplicate_features(&lookup_indices, feature_indices);

        let (script_langsys_map, feature_indices) =
            self.prune_langsys(&duplicate_feature_index_map, &plan.layout_scripts);

        plan.gpos_lookups = remap_indices(lookup_indices);
        plan.gpos_features = remap_indices(feature_indices);
        plan.gpos_script_langsys = script_langsys_map;
    }
}

impl Subset for Gpos<'_> {
    fn subset(
        &self,
        plan: &Plan,
        _font: &FontRef,
        s: &mut Serializer,
        _builder: &mut FontBuilder,
    ) -> Result<(), SubsetError> {
        subset_gpos(self, plan, s).map_err(|_| SubsetError::SubsetTableError(Gpos::TAG))
    }
}

fn subset_gpos(gpos: &Gpos, plan: &Plan, s: &mut Serializer) -> Result<(), SerializeErrorFlags> {
    // TODO: version update
    let _version = s.embed(gpos.version())?;

    // script_list
    let script_list_offset_pos = s.embed(0_u16)?;

    let script_list = gpos
        .script_list()
        .map_err(|_| s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR))?;

    let mut c = SubsetLayoutContext::new(Gpos::TAG);
    Offset16::serialize_subset(&script_list, s, plan, &mut c, script_list_offset_pos)?;

    // feature list
    let feature_list_offset_pos = s.embed(0_u16)?;
    let feature_list = gpos
        .feature_list()
        .map_err(|_| s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR))?;
    Offset16::serialize_subset(&feature_list, s, plan, &mut c, feature_list_offset_pos)?;

    // TODO: lookup_list
    //let lookup_list_pos = s.embed(0_u16)?;

    // TODO: feature variations
    //if let Some(feature_variations) = gpos
    //    .feature_variations()
    //    .transpose()
    //    .map_err(|_| SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)?
    //{
    //    let feature_vars_offset_pos = s.embed(0_u32)?;
    //}
    Ok(())
}

fn compute_effective_format(
    value_record: &ValueRecord,
    strip_hints: bool,
    strip_empty: bool,
) -> ValueFormat {
    let mut value_format = ValueFormat::empty();

    if let Some(x_placement) = value_record.x_placement {
        if !strip_empty || x_placement.get() != 0 {
            value_format |= ValueFormat::X_PLACEMENT;
        }
    }

    if let Some(y_placement) = value_record.y_placement {
        if !strip_empty || y_placement.get() != 0 {
            value_format |= ValueFormat::Y_PLACEMENT;
        }
    }

    if let Some(x_advance) = value_record.x_advance {
        if !strip_empty || x_advance.get() != 0 {
            value_format |= ValueFormat::X_ADVANCE;
        }
    }

    if let Some(y_advance) = value_record.y_advance {
        if !strip_empty || y_advance.get() != 0 {
            value_format |= ValueFormat::Y_ADVANCE;
        }
    }

    if !value_record.x_placement_device.get().is_null() && !strip_hints {
        value_format |= ValueFormat::X_PLACEMENT_DEVICE;
    }

    if !value_record.y_placement_device.get().is_null() && !strip_hints {
        value_format |= ValueFormat::Y_PLACEMENT_DEVICE;
    }

    if !value_record.x_advance_device.get().is_null() && !strip_hints {
        value_format |= ValueFormat::X_ADVANCE_DEVICE;
    }

    if !value_record.y_advance_device.get().is_null() && !strip_hints {
        value_format |= ValueFormat::Y_ADVANCE_DEVICE;
    }
    value_format
}

impl<'a> SubsetTable<'a> for ValueRecord {
    type ArgsForSubset = (ValueFormat, FontData<'a>);
    type Output = ();

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        args: Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        let (new_format, font_data) = args;
        if new_format.is_empty() {
            return Ok(());
        }

        if new_format.contains(ValueFormat::X_PLACEMENT) {
            s.embed(self.x_placement().unwrap_or(0))?;
        }

        if new_format.contains(ValueFormat::Y_PLACEMENT) {
            s.embed(self.y_placement().unwrap_or(0))?;
        }

        if new_format.contains(ValueFormat::X_ADVANCE) {
            s.embed(self.x_advance().unwrap_or(0))?;
        }

        if new_format.contains(ValueFormat::Y_ADVANCE) {
            s.embed(self.y_advance().unwrap_or(0))?;
        }

        if !new_format.intersects(ValueFormat::ANY_DEVICE_OR_VARIDX) {
            return Ok(());
        }

        if new_format.contains(ValueFormat::X_PLACEMENT_DEVICE) {
            let offset_pos = s.embed(0_u16)?;
            if let Some(device) = self
                .x_placement_device(font_data)
                .transpose()
                .map_err(|_| s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR))?
            {
                Offset16::serialize_subset(
                    &device,
                    s,
                    plan,
                    &plan.layout_varidx_delta_map,
                    offset_pos,
                )?;
            }
        }

        if new_format.contains(ValueFormat::Y_PLACEMENT_DEVICE) {
            let offset_pos = s.embed(0_u16)?;
            if let Some(device) = self
                .y_placement_device(font_data)
                .transpose()
                .map_err(|_| s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR))?
            {
                Offset16::serialize_subset(
                    &device,
                    s,
                    plan,
                    &plan.layout_varidx_delta_map,
                    offset_pos,
                )?;
            }
        }

        if new_format.contains(ValueFormat::X_ADVANCE_DEVICE) {
            let offset_pos = s.embed(0_u16)?;
            if let Some(device) = self
                .x_advance_device(font_data)
                .transpose()
                .map_err(|_| s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR))?
            {
                Offset16::serialize_subset(
                    &device,
                    s,
                    plan,
                    &plan.layout_varidx_delta_map,
                    offset_pos,
                )?;
            }
        }

        if new_format.contains(ValueFormat::Y_ADVANCE_DEVICE) {
            let offset_pos = s.embed(0_u16)?;
            if let Some(device) = self
                .y_advance_device(font_data)
                .transpose()
                .map_err(|_| s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR))?
            {
                Offset16::serialize_subset(
                    &device,
                    s,
                    plan,
                    &plan.layout_varidx_delta_map,
                    offset_pos,
                )?;
            }
        }
        Ok(())
    }
}

impl<'a> SubsetTable<'a> for SinglePos<'_> {
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

impl<'a> SubsetTable<'a> for SinglePosFormat1<'_> {
    type ArgsForSubset = (&'a SubsetState, &'a FontRef<'a>);
    type Output = ();
    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        args: Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        let coverage = self
            .coverage()
            .map_err(|_| s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR))?;
        let retained_glyphs: Vec<GlyphId> = coverage
            .intersect_set(&plan.glyphset_gsub)
            .iter()
            .filter_map(|g| plan.glyph_map_gsub.get(&g))
            .copied()
            .collect();
        if retained_glyphs.is_empty() {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY);
        }

        let value_record = self.value_record();
        let new_format = if plan
            .subset_flags
            .contains(SubsetFlags::SUBSET_FLAGS_NO_HINTING)
        {
            let (state, font) = args;
            // do not strip hints for VF unless it has no GDEF varstore after subsetting
            let strip_hints = if font.fvar().is_ok() {
                !state.has_gdef_varstore
            } else {
                true
            };
            compute_effective_format(&value_record, strip_hints, true)
        } else {
            self.value_format()
        };

        SinglePosFormat1::serialize(
            s,
            (
                &retained_glyphs,
                &value_record,
                new_format,
                plan,
                self.offset_data(),
            ),
        )
    }
}

impl<'a> Serialize<'a> for SinglePosFormat1<'_> {
    type Args = (
        &'a [GlyphId],
        &'a ValueRecord,
        ValueFormat,
        &'a Plan,
        FontData<'a>,
    );
    fn serialize(s: &mut Serializer, args: Self::Args) -> Result<(), SerializeErrorFlags> {
        // format
        s.embed(1_u16)?;

        // coverage offset
        let cov_offset_pos = s.embed(0_u16)?;

        let (glyphs, value_record, value_format, plan, font_data) = args;
        //value format
        s.embed(value_format)?;
        //value record
        value_record.subset(plan, s, (value_format, font_data))?;

        Offset16::serialize_serialize::<CoverageTable>(s, glyphs, cov_offset_pos)
    }
}

fn compute_new_value_format(
    plan: &Plan,
    has_gdef_varstore: bool,
    font: &FontRef,
    value_records: impl IntoIterator<Item = ValueRecord>,
) -> ValueFormat {
    // TODO: support instancing
    let mut new_format = ValueFormat::empty();
    if plan
        .subset_flags
        .contains(SubsetFlags::SUBSET_FLAGS_NO_HINTING)
    {
        // do not strip hints for VF unless it has no GDEF varstore after subsetting
        let strip_hints = if font.fvar().is_ok() {
            !has_gdef_varstore
        } else {
            true
        };

        for record in value_records {
            new_format |= compute_effective_format(&record, strip_hints, true);
        }
    } else if let Some(rec) = value_records.into_iter().next() {
        new_format = rec.format;
    }

    new_format
}

impl<'a> SubsetTable<'a> for SinglePosFormat2<'_> {
    type ArgsForSubset = (&'a SubsetState, &'a FontRef<'a>);
    type Output = ();
    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        args: Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        let coverage = self
            .coverage()
            .map_err(|_| s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR))?;

        let glyph_map = &plan.glyph_map_gsub;
        let cap = glyph_map.len().min(self.value_count() as usize);
        let mut retained_glyphs = Vec::with_capacity(cap);
        let mut retained_rec_idxes = IntSet::empty();

        for (idx, g) in coverage.iter().enumerate() {
            let Some(new_g) = glyph_map.get(&GlyphId::from(g)) else {
                continue;
            };
            retained_glyphs.push(*new_g);
            retained_rec_idxes.insert(idx as u16);
        }

        if retained_glyphs.is_empty() {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY);
        }

        let (state, font) = args;
        let value_records = self.value_records();
        let it = value_records
            .iter()
            .enumerate()
            .filter(|&(i, ref _rec)| retained_rec_idxes.contains(i as u16))
            .filter_map(|(_i, rec)| rec.ok());
        let new_format = compute_new_value_format(plan, state.has_gdef_varstore, font, it);

        let Ok(first_retained_rec) =
            value_records.get(retained_rec_idxes.first().unwrap() as usize)
        else {
            return Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR));
        };

        let mut table_format = 1;
        for i in retained_rec_idxes.iter().skip(1) {
            let Ok(rec) = value_records.get(i as usize) else {
                return Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR));
            };

            if rec != first_retained_rec {
                table_format = 2;
                break;
            }
        }

        if table_format == 1 {
            SinglePosFormat1::serialize(
                s,
                (
                    &retained_glyphs,
                    &first_retained_rec,
                    new_format,
                    plan,
                    self.offset_data(),
                ),
            )
        } else {
            SinglePosFormat2::serialize(
                s,
                (
                    &retained_glyphs,
                    new_format,
                    self,
                    &retained_rec_idxes,
                    plan,
                ),
            )
        }
    }
}

impl<'a> Serialize<'a> for SinglePosFormat2<'_> {
    type Args = (
        &'a [GlyphId],
        ValueFormat,
        &'a SinglePosFormat2<'a>,
        &'a IntSet<u16>,
        &'a Plan,
    );
    fn serialize(s: &mut Serializer, args: Self::Args) -> Result<(), SerializeErrorFlags> {
        // format
        s.embed(2_u16)?;

        // coverage offset
        let cov_offset_pos = s.embed(0_u16)?;

        let (glyphs, value_format, table, retained_rec_idxes, plan) = args;
        //value format
        s.embed(value_format)?;

        //value count
        let value_count = glyphs.len();
        s.embed(value_count as u16)?;

        let value_records = table.value_records();
        let font_data = table.offset_data();
        for i in retained_rec_idxes.iter() {
            let value_record = value_records
                .get(i as usize)
                .map_err(|_| s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR))?;
            value_record.subset(plan, s, (value_format, font_data))?;
        }

        Offset16::serialize_serialize::<CoverageTable>(s, glyphs, cov_offset_pos)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use write_fonts::read::{FontRef, TableProvider};

    #[test]
    fn test_prune_langsys() {
        let font = FontRef::new(include_bytes!("../test-data/fonts/Amiri-Regular.ttf")).unwrap();
        let gpos = font.gpos().unwrap();

        let mut layout_scripts = IntSet::all();
        let mut duplicate_feature_index_map = FnvHashMap::default();
        duplicate_feature_index_map.insert(0, 0);
        duplicate_feature_index_map.insert(2, 2);
        duplicate_feature_index_map.insert(4, 2);

        let (script_langsys_map, features) =
            gpos.prune_langsys(&duplicate_feature_index_map, &layout_scripts);
        // script langsys map is empty cause all langsys duplicate with default langsys
        assert!(script_langsys_map.is_empty());
        assert_eq!(features.len(), 2);
        assert!(features.contains(0));
        assert!(features.contains(2));

        // test script filter
        layout_scripts.clear();
        layout_scripts.insert(Tag::new(b"arab"));
        let (script_langsys_map, features) =
            gpos.prune_langsys(&duplicate_feature_index_map, &layout_scripts);
        // script langsys map is still empty cause all langsys duplicate with default langsys
        assert!(script_langsys_map.is_empty());
        assert_eq!(features.len(), 1);
        assert!(features.contains(0));
    }

    #[test]
    fn test_find_duplicate_features() {
        let font = FontRef::new(include_bytes!("../test-data/fonts/Amiri-Regular.ttf")).unwrap();
        let gpos = font.gpos().unwrap();

        let mut lookups = IntSet::empty();
        lookups.insert(0_u16);

        let mut feature_indices = IntSet::empty();
        // 1 and 2 diffs: 2 has one more lookup that's indexed at 82
        feature_indices.insert(1_u16);
        feature_indices.insert(2_u16);
        // 3 and 4 diffs:
        // feature indexed at 4 has only 2 lookups: index 2 and 58
        // feature indexed at 3 has 13 more lookups
        feature_indices.insert(3_u16);
        feature_indices.insert(4_u16);

        let feature_index_map = gpos.find_duplicate_features(&lookups, feature_indices);
        // with only lookup index=0
        // feature=1 and feature=2 are duplicates
        // feature=3 and feature=4 are duplicates
        assert_eq!(feature_index_map.len(), 4);
        assert_eq!(feature_index_map.get(&1), Some(&1));
        assert_eq!(feature_index_map.get(&2), Some(&1));
        assert_eq!(feature_index_map.get(&3), Some(&3));
        assert_eq!(feature_index_map.get(&4), Some(&3));

        // lookup=82 only referenced by feature=2
        lookups.insert(82_u16);
        // lookup=81 only referenced by feature=3
        lookups.insert(81_u16);
        let mut feature_indices = IntSet::empty();
        // 1 and 2 diffs: 2 has one more lookup that's indexed at 82
        feature_indices.insert(1_u16);
        feature_indices.insert(2_u16);
        feature_indices.insert(3_u16);
        feature_indices.insert(4_u16);
        let feature_index_map = gpos.find_duplicate_features(&lookups, feature_indices);
        // with only lookup index=0
        // feature=1 and feature=2 are duplicates
        // feature=3 and feature=4 are duplicates
        assert_eq!(feature_index_map.len(), 4);
        assert_eq!(feature_index_map.get(&1), Some(&1));
        assert_eq!(feature_index_map.get(&2), Some(&2));
        assert_eq!(feature_index_map.get(&3), Some(&3));
        assert_eq!(feature_index_map.get(&4), Some(&4));
    }

    #[test]
    fn test_subset_gpos_format1() {
        use write_fonts::read::tables::gpos::PositionSubtables;

        let font = FontRef::new(include_bytes!("../test-data/fonts/Amiri-Regular.ttf")).unwrap();
        let gpos_lookups = font.gpos().unwrap().lookup_list().unwrap();
        let lookup = gpos_lookups.lookups().get(6).unwrap();

        let PositionSubtables::Single(sub_tables) = lookup.subtables().unwrap() else {
            panic!("Wrong type of lookup table!");
        };
        let singlepos_table = sub_tables.get(0).unwrap();

        let subset_state = SubsetState::default();
        let mut plan = Plan::default();

        plan.glyph_map_gsub
            .insert(GlyphId::from(5987_u32), GlyphId::from(3_u32));
        plan.glyphset_gsub.insert(GlyphId::from(5987_u32));

        let mut s = Serializer::new(1024);
        assert_eq!(s.start_serialize(), Ok(()));

        singlepos_table
            .subset(&plan, &mut s, (&subset_state, &font))
            .unwrap();
        assert!(!s.in_error());
        s.end_serialize();

        let subsetted_data = s.copy_bytes();
        let expected_data: [u8; 16] = [
            0x00, 0x01, 0x00, 0x0a, 0x00, 0x05, 0xfb, 0xc9, 0xfe, 0xdc, 0x00, 0x01, 0x00, 0x01,
            0x00, 0x03,
        ];

        assert_eq!(subsetted_data, expected_data);
    }

    #[test]
    fn test_subset_gpos_format2() {
        use write_fonts::read::tables::gpos::PositionSubtables;

        let font = FontRef::new(include_bytes!("../test-data/fonts/Amiri-Regular.ttf")).unwrap();
        let gpos_lookups = font.gpos().unwrap().lookup_list().unwrap();
        let lookup = gpos_lookups.lookups().get(36).unwrap();

        let PositionSubtables::Single(sub_tables) = lookup.subtables().unwrap() else {
            panic!("Wrong type of lookup table!");
        };
        let singlepos_table = sub_tables.get(4).unwrap();

        let subset_state = SubsetState::default();
        let mut plan = Plan::default();

        // test case 1: subsetted output is still format 2
        plan.glyph_map_gsub
            .insert(GlyphId::from(2270_u32), GlyphId::from(3_u32));
        plan.glyph_map_gsub
            .insert(GlyphId::from(2349_u32), GlyphId::from(4_u32));
        plan.glyphset_gsub.insert(GlyphId::from(2270_u32));
        plan.glyphset_gsub.insert(GlyphId::from(2349_u32));

        let mut s = Serializer::new(1024);
        assert_eq!(s.start_serialize(), Ok(()));

        singlepos_table
            .subset(&plan, &mut s, (&subset_state, &font))
            .unwrap();
        assert!(!s.in_error());
        s.end_serialize();

        let subsetted_data = s.copy_bytes();
        let expected_data: [u8; 20] = [
            0x00, 0x02, 0x00, 0x0c, 0x00, 0x04, 0x00, 0x02, 0x00, 0xc3, 0x00, 0xfe, 0x00, 0x01,
            0x00, 0x02, 0x00, 0x03, 0x00, 0x04,
        ];

        assert_eq!(subsetted_data, expected_data);

        // test case 2: subsetted output is optimized to format 1
        plan.glyph_map_gsub.clear();
        plan.glyph_map_gsub
            .insert(GlyphId::from(2270_u32), GlyphId::from(3_u32));
        plan.glyph_map_gsub
            .insert(GlyphId::from(6179_u32), GlyphId::from(4_u32));

        plan.glyphset_gsub.clear();
        plan.glyphset_gsub.insert(GlyphId::from(2270_u32));
        plan.glyphset_gsub.insert(GlyphId::from(6179_u32));

        let mut s = Serializer::new(1024);
        assert_eq!(s.start_serialize(), Ok(()));

        singlepos_table
            .subset(&plan, &mut s, (&subset_state, &font))
            .unwrap();
        assert!(!s.in_error());
        s.end_serialize();

        let subsetted_data = s.copy_bytes();
        let expected_data: [u8; 16] = [
            0x00, 0x01, 0x00, 0x08, 0x00, 0x04, 0x00, 0xc3, 0x00, 0x01, 0x00, 0x02, 0x00, 0x03,
            0x00, 0x04,
        ];

        assert_eq!(subsetted_data, expected_data);
    }
}
