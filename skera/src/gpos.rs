//! impl subset() for GPOS table

mod anchor;
mod cursive_pos;
mod mark_array;
mod mark_base_pos;
mod mark_lig_pos;
mod mark_mark_pos;
mod pair_pos;
mod single_pos;
mod value_record;

use crate::FastHashMap;
use crate::{
    collect_feature_variations_name_ids, collect_features_with_retained_subs,
    collect_variable_feature_indices, find_duplicate_features,
    layout::{CountOps, OpsBudget, OpsWalk},
    offset::SerializeSubset,
    prune_features, remap_feature_indices, remap_indices,
    serialize::{SerializeErrorFlags, SerializeResultEmpty, Serializer},
    CollectVariationIndices, LayoutClosure, NameIdClosure, Plan, PruneLangSysContext, Subset,
    SubsetError, SubsetLayoutContext, SubsetState, SubsetTable,
};
use read_fonts::{
    collections::IntSet,
    limits::MAX_OPS_EXCEEDED,
    tables::{
        gpos::{ExtensionPosFormat1, ExtensionSubtable, Gpos, PositionLookup, PositionSubtables},
        layout::{ExtensionLookup, FeatureRecord, Intersect, LookupFlag, Subtables},
    },
    types::Tag,
    types::{Compatible, MajorMinor, Offset16, Offset32},
    FontRead, FontRef, ReadError, TopLevelTable,
};
use write_fonts::FontBuilder;

impl NameIdClosure for Gpos<'_> {
    //TODO: support instancing: collect from feature substitutes if exist
    fn collect_name_ids(&self, plan: &mut Plan) {
        let Ok(feature_list) = self.feature_list() else {
            return;
        };
        let feature_records = feature_list.feature_records();
        let feature_count = feature_list.feature_count();

        for (feature_record, i) in feature_records.iter().zip(0..feature_count) {
            if !plan.gpos_features.contains_key(&i) {
                continue;
            }
            let Ok(feature) = feature_record.feature(feature_list.offset_data()) else {
                continue;
            };
            feature.collect_name_ids(plan);
        }

        if let Some(Ok(feature_variations)) = self.feature_variations() {
            let mut budget = OpsBudget::new(self.offset_data());
            collect_feature_variations_name_ids(
                &feature_variations,
                feature_records,
                Gpos::TAG,
                plan,
                &mut budget,
            );
            budget.record(Gpos::TAG, plan);
        }
    }
}

impl LayoutClosure for Gpos<'_> {
    fn prune_features(
        &self,
        lookup_indices: &IntSet<u16>,
        feature_indices: IntSet<u16>,
        budget: &mut OpsBudget,
    ) -> IntSet<u16> {
        let alternate_features = if let Some(Ok(feature_variations)) = self.feature_variations() {
            collect_features_with_retained_subs(&feature_variations, lookup_indices, budget)
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
            budget,
        )
    }

    fn find_duplicate_features(
        &self,
        lookup_indices: &IntSet<u16>,
        feature_indices: IntSet<u16>,
    ) -> FastHashMap<u16, u16> {
        let Ok(feature_list) = self.feature_list() else {
            return FastHashMap::default();
        };
        // This reads more than HarfBuzz's sanitizer does only if feature hashes collide,
        // so running out of this budget is not recorded as a failure. It only stops the
        // deduplication, which is always valid.
        let mut budget = OpsBudget::new(self.offset_data());
        let variable_features = match self.feature_variations() {
            Some(Ok(variations)) => collect_variable_feature_indices(&variations, &mut budget),
            _ => IntSet::empty(),
        };
        find_duplicate_features(
            &feature_list,
            lookup_indices,
            feature_indices,
            &variable_features,
            &mut budget,
        )
    }

    fn prune_langsys(
        &self,
        duplicate_feature_index_map: &FastHashMap<u16, u16>,
        layout_scripts: &IntSet<Tag>,
    ) -> (FastHashMap<u16, IntSet<u16>>, IntSet<u16>) {
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

        let mut lookup_indices = match self.collect_lookups(&feature_indices) {
            Ok(lookup_indices) => lookup_indices,
            Err(err) => {
                if err == MAX_OPS_EXCEEDED {
                    plan.layout_ops_exceeded.insert(Gpos::TAG);
                }
                return;
            }
        };
        if let Err(err) = self.closure_lookups(&plan.glyphset_gsub, &mut lookup_indices) {
            if err == MAX_OPS_EXCEEDED {
                plan.layout_ops_exceeded.insert(Gpos::TAG);
            }
            return;
        }

        let mut budget = OpsBudget::new(self.offset_data());
        let feature_indices = self.prune_features(&lookup_indices, feature_indices, &mut budget);
        budget.record(Gpos::TAG, plan);
        let duplicate_feature_index_map =
            self.find_duplicate_features(&lookup_indices, feature_indices);

        let (script_langsys_map, feature_indices) =
            self.prune_langsys(&duplicate_feature_index_map, &plan.layout_scripts);

        plan.gpos_lookups = remap_indices(lookup_indices);

        (plan.gpos_features, plan.gpos_features_w_duplicates) =
            remap_feature_indices(&feature_indices, &duplicate_feature_index_map);
        plan.gpos_script_langsys = script_langsys_map;
    }
}

impl Subset for Gpos<'_> {
    fn subset_with_state(
        &self,
        plan: &Plan,
        font: &FontRef,
        state: &mut SubsetState,
        s: &mut Serializer,
        _builder: &mut FontBuilder,
    ) -> Result<(), SubsetError> {
        subset_gpos(self, plan, font, state, s)
            .map_err(|_| SubsetError::SubsetTableError(Gpos::TAG))
    }
}

fn subset_gpos(
    gpos: &Gpos,
    plan: &Plan,
    font: &FontRef,
    state: &SubsetState,
    s: &mut Serializer,
) -> Result<(), SerializeErrorFlags> {
    // HarfBuzz fails to subset a table that its sanitizer rejects.
    if plan.layout_ops_exceeded.contains(Gpos::TAG) {
        return Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR));
    }
    s.set_layout_walk(OpsWalk::new(gpos.offset_data()));
    if gpos.version() >= MajorMinor::new(1, 2) {
        return subset_extended_layout(gpos, plan, font, state, s);
    }
    let version_pos = s.embed(gpos.version())?;
    let mut c = SubsetLayoutContext::new(Gpos::TAG, gpos.offset_data());
    let script_list_offset_pos = s.embed(0_u16)?;
    let feature_list_offset_pos = s.embed(0_u16)?;
    let lookup_list_offset_pos = s.embed(0_u16)?;

    // Subset the lists in the same order as harfbuzz: lookup list, feature list, then
    // script list. Objects packed later are placed closer to the header, so this keeps
    // the small script and feature lists within reach of the header's 16-bit offsets
    // when the lookups are large.
    // ref: <https://github.com/harfbuzz/harfbuzz/blob/main/src/hb-ot-layout-gsubgpos.hh>
    if !gpos.lookup_list_offset().is_null() {
        let lookup_list = gpos
            .lookup_list()
            .map_err(|_| s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR))?;
        Offset16::serialize_subset(
            &lookup_list,
            s,
            plan,
            (state, font, &plan.gpos_lookups),
            lookup_list_offset_pos,
        )?;
    }

    let mut feature_records: &[FeatureRecord] = &[];
    if !gpos.feature_list_offset().is_null() {
        let feature_list = gpos
            .feature_list()
            .map_err(|_| s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR))?;
        feature_records = feature_list.feature_records();
        Offset16::serialize_subset(&feature_list, s, plan, &mut c, feature_list_offset_pos)?;
    }

    if !gpos.script_list_offset().is_null() {
        let script_list = gpos
            .script_list()
            .map_err(|_| s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR))?;

        Offset16::serialize_subset(&script_list, s, plan, &mut c, script_list_offset_pos)?;
    }

    if let Some(feature_variations) = gpos
        .feature_variations()
        .transpose()
        .map_err(|_| SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)?
    {
        let snap = s.snapshot();
        let feature_vars_offset_pos = s.embed(0_u32)?;
        if Offset32::serialize_subset(
            &feature_variations,
            s,
            plan,
            (feature_records, &mut c),
            feature_vars_offset_pos,
        )
        .is_empty()?
        {
            // downgrade table version if there are no FeatureVariations
            s.revert_snapshot(snap);
            s.copy_assign(version_pos, MajorMinor::VERSION_1_0);
        }
    } else if gpos.version().compatible((1u16, 1u16)) {
        // If version 1.1 was written but no FeatureVariations exist, downgrade to 1.0
        s.copy_assign(version_pos, MajorMinor::VERSION_1_0);
    }
    Ok(())
}

crate::layout::header::subset_extended_layout!(Gpos, gpos_lookups);

impl<'a> SubsetTable<'a> for PositionLookup<'_> {
    type ArgsForSubset = (&'a SubsetState, &'a FontRef<'a>, &'a FastHashMap<u16, u16>);
    type Output = ();
    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        args: Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        let lookup_type = self.lookup_type();
        s.embed(lookup_type)?;

        let lookup_flag = self.lookup_flag();
        let lookup_flag_pos = s.embed(lookup_flag)?;
        let lookup_count_pos = s.embed(0_u16)?;

        //subset subtables
        let lookup_count = match self {
            PositionLookup::Single(inner) => inner.subtables().subset(plan, s, args)?,
            PositionLookup::Pair(inner) => inner.subtables().subset(plan, s, args)?,
            PositionLookup::Cursive(inner) => inner.subtables().subset(plan, s, args)?,
            PositionLookup::MarkToBase(inner) => inner.subtables().subset(plan, s, args)?,
            PositionLookup::MarkToLig(inner) => inner.subtables().subset(plan, s, args)?,
            PositionLookup::MarkToMark(inner) => inner.subtables().subset(plan, s, args)?,
            PositionLookup::Contextual(inner) => inner.subtables().subset(plan, s, args)?,
            PositionLookup::ChainContextual(inner) => inner.subtables().subset(plan, s, args)?,
            PositionLookup::Extension(inner) => inner.subtables().subset(plan, s, args)?,
        };
        s.copy_assign(lookup_count_pos, lookup_count);

        // ref: <https://github.com/harfbuzz/harfbuzz/blob/a790c38b782f9d8e6f0299d2837229e5726fc669/src/hb-ot-layout-common.hh#L1385>
        if let Some(mark_filtering_set) = self.mark_filtering_set() {
            if let Some(new_idx) = plan.used_mark_sets_map.get(&mark_filtering_set) {
                s.embed(*new_idx)?;
            } else {
                let new_flag =
                    (lookup_flag - LookupFlag::USE_MARK_FILTERING_SET) | LookupFlag::IGNORE_MARKS;
                s.copy_assign(lookup_flag_pos, new_flag);
            }
        }
        Ok(())
    }
}

impl<'a> SubsetTable<'a> for ExtensionSubtable<'a> {
    type ArgsForSubset = (&'a SubsetState, &'a FontRef<'a>, &'a FastHashMap<u16, u16>);
    type Output = ();

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        args: Self::ArgsForSubset,
    ) -> Result<Self::Output, SerializeErrorFlags> {
        match self {
            ExtensionSubtable::Single(inner) => inner.subset(plan, s, args),
            ExtensionSubtable::Pair(inner) => inner.subset(plan, s, args),
            ExtensionSubtable::Cursive(inner) => inner.subset(plan, s, args),
            ExtensionSubtable::MarkToBase(inner) => inner.subset(plan, s, args),
            ExtensionSubtable::MarkToLig(inner) => inner.subset(plan, s, args),
            ExtensionSubtable::MarkToMark(inner) => inner.subset(plan, s, args),
            ExtensionSubtable::Contextual(inner) => inner.subset(plan, s, args),
            ExtensionSubtable::ChainContextual(inner) => inner.subset(plan, s, args),
        }
    }
}

impl<'a, T> SubsetTable<'a> for ExtensionPosFormat1<'a, T>
where
    T: SubsetTable<
            'a,
            ArgsForSubset = (&'a SubsetState, &'a FontRef<'a>, &'a FastHashMap<u16, u16>),
            Output = (),
        > + FontRead<'a, Args = ()>,
{
    type ArgsForSubset = (&'a SubsetState, &'a FontRef<'a>, &'a FastHashMap<u16, u16>);
    type Output = ();

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        args: Self::ArgsForSubset,
    ) -> Result<Self::Output, SerializeErrorFlags> {
        if self.extension_offset().is_null() {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY);
        }
        s.embed(self.pos_format())?;
        s.embed(self.extension_lookup_type())?;

        let offset_pos = s.embed(0_u32)?;
        let subtable = self
            .extension()
            .map_err(|_| s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR))?;

        Offset32::serialize_subset(&subtable, s, plan, args, offset_pos)
    }
}

/// Collects the variation indices of the lookups that are retained.
///
/// Like the walk that subsets them, this counts the subtables it visits, and
/// what a subtable reaches before it reads that, against `walk`.
pub(crate) fn collect_lookup_variation_indices(
    gpos: &Gpos,
    plan: &Plan,
    varidx_set: &mut IntSet<u32>,
    walk: &mut OpsWalk,
) {
    let Ok(lookups) = gpos.lookup_list() else {
        return;
    };

    let lookups = lookups.lookups();
    for i in plan.gpos_lookups.keys() {
        let Ok(lookup) = lookups.get(*i as usize) else {
            return;
        };

        //TODO: support all lookup types
        let within_budget = match lookup.subtables() {
            Ok(PositionSubtables::Single(subs)) => {
                collect_subtable_variation_indices(&subs, plan, varidx_set, walk)
            }
            Ok(PositionSubtables::Pair(subs)) => {
                collect_subtable_variation_indices(&subs, plan, varidx_set, walk)
            }
            Ok(PositionSubtables::Cursive(subs)) => {
                collect_subtable_variation_indices(&subs, plan, varidx_set, walk)
            }
            Ok(PositionSubtables::MarkToBase(subs)) => {
                collect_subtable_variation_indices(&subs, plan, varidx_set, walk)
            }
            Ok(PositionSubtables::MarkToLig(subs)) => {
                collect_subtable_variation_indices(&subs, plan, varidx_set, walk)
            }
            Ok(PositionSubtables::MarkToMark(subs)) => {
                collect_subtable_variation_indices(&subs, plan, varidx_set, walk)
            }
            Ok(_) | Err(ReadError::NullOffset) => continue,
            Err(_) => return,
        };
        if !within_budget {
            return;
        }
    }
}

/// Collects the variation indices of the subtables of a lookup that intersect
/// the glyphs of the subset. False once `walk` is out of budget.
fn collect_subtable_variation_indices<'a, T, Ext>(
    subtables: &Subtables<'a, T, Ext>,
    plan: &Plan,
    varidx_set: &mut IntSet<u32>,
    walk: &mut OpsWalk,
) -> bool
where
    T: CollectVariationIndices + CountOps + Intersect + FontRead<'a, Args = ()> + 'a,
    Ext: ExtensionLookup<'a, T> + 'a,
{
    if !walk.visit_subtables(subtables.len()) {
        return false;
    }
    for t in subtables.iter().filter_map(|table| match table {
        Err(ReadError::NullOffset) => None,
        other => Some(other),
    }) {
        let Ok(t) = t else {
            return true;
        };

        match walk.intersects(&t, &plan.glyphset_gsub) {
            Ok(true) => (),
            Ok(false) => continue,
            Err(err) => return err != MAX_OPS_EXCEEDED,
        }
        if !t.count_ops(walk, &plan.glyphset_gsub) {
            return false;
        }
        t.collect_variation_indices(plan, varidx_set);
    }
    true
}

#[cfg(test)]
mod test {
    use super::*;
    use read_fonts::{FontRef, TableProvider};

    #[test]
    fn test_prune_langsys() {
        let font = FontRef::new(include_bytes!("../test-data/fonts/Amiri-Regular.ttf")).unwrap();
        let gpos = font.gpos().unwrap();

        let mut layout_scripts = IntSet::all();
        let mut duplicate_feature_index_map = FastHashMap::default();
        duplicate_feature_index_map.insert(0, 0);
        duplicate_feature_index_map.insert(2, 2);
        duplicate_feature_index_map.insert(4, 2);

        let (script_langsys_map, features) =
            gpos.prune_langsys(&duplicate_feature_index_map, &layout_scripts);
        // script langsys map is empty cause all langsys duplicate with default langsys
        assert!(script_langsys_map.is_empty());
        assert_eq!(features.len(), 3);
        assert!(features.contains(0));
        assert!(features.contains(2));
        assert!(features.contains(4));

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

    // Any number of offsets can lead to one PairSet. Like HarfBuzz's sanitizer, the walk that
    // subsets the lookups reads at most 64 times the table's size.
    #[test]
    fn test_gpos_lookups_over_ops_budget_fail_to_subset() {
        use crate::subset_font;
        use font_test_data::bebuffer::BeBuffer;
        use read_fonts::types::GlyphId;

        // The `count` PairSet offsets of a PairPos lead to one PairSet of 50 pairs, in a
        // font with 1294 glyphs. The subset keeps the `count` glyphs with a PairSet.
        let subsets = |count: u16| {
            let mut buf = BeBuffer::new()
                .extend([1u16, 0, 10, 30, 44])
                .push(1u16)
                .push(Tag::new(b"DFLT"))
                .extend([8u16, 4, 0])
                .extend([0u16, 0xFFFF, 1, 0])
                .push(1u16)
                .push(Tag::new(b"kern"))
                .extend([8u16, 0, 1, 0])
                .extend([1u16, 4])
                .extend([2u16, 0, 1, 8])
                .extend([1u16, 10 + 2 * count, 4, 0, count])
                .extend(std::iter::repeat_n(20 + 2 * count, usize::from(count)))
                .extend([2u16, 1, 1, count, 0])
                .push(50u16);
            for glyph in 1..=50u16 {
                buf = buf.extend([glyph, 10]);
            }

            let roboto = include_bytes!("../test-data/fonts/Roboto-Regular.ttf");
            let mut builder = FontBuilder::new();
            builder.add_raw(Gpos::TAG, buf.to_vec());
            builder.copy_missing_tables(FontRef::new(roboto).unwrap());
            let data = builder.build();
            let font = FontRef::new(&data).unwrap();
            let plan = Plan::new(
                &(1..=u32::from(count)).map(GlyphId::new).collect(),
                &IntSet::empty(),
                &font,
                crate::SubsetFlags::default(),
                &IntSet::empty(),
                &IntSet::all(),
                &IntSet::all(),
                &IntSet::empty(),
                &IntSet::empty(),
            );
            subset_font(&font, &plan)
        };

        // 200 reads of the 202 byte PairSet are within 64 times the 678 bytes of the table
        assert!(subsets(200).is_ok());

        // but 400 reads are not within 64 times 1078 bytes
        assert!(matches!(
            subsets(400),
            Err(SubsetError::SubsetTableError(tag)) if tag == Gpos::TAG
        ));
    }
}
