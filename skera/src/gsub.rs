//! impl subset() for GSUB table
mod alternate_subst;
mod ligature_subst;
mod multiple_subst;
mod reverse_chain_single_subst;
mod single_subst;

use crate::{
    collect_features_with_retained_subs, find_duplicate_features,
    offset::SerializeSubset,
    prune_features, remap_feature_indices, remap_indices,
    serialize::{SerializeErrorFlags, Serializer},
    LayoutClosure, NameIdClosure, Plan, PruneLangSysContext, Subset, SubsetError,
    SubsetLayoutContext, SubsetState, SubsetTable,
};
use fnv::FnvHashMap;
use write_fonts::{
    read::{
        collections::IntSet,
        tables::{
            gsub::{Gsub, SubstitutionLookup, SubstitutionSubtables},
            layout::LookupFlag,
        },
        types::{MajorMinor, Offset16, Offset32, Tag},
        FontRef, TopLevelTable,
    },
    FontBuilder,
};

impl NameIdClosure for Gsub<'_> {
    //TODO: support instancing: collect from feature substitutes if exist
    fn collect_name_ids(&self, plan: &mut Plan) {
        let Ok(feature_list) = self.feature_list() else {
            return;
        };
        for (i, feature_record) in feature_list.feature_records().iter().enumerate() {
            if !plan.gsub_features.contains_key(&(i as u16)) {
                continue;
            }
            let Ok(feature) = feature_record.feature(feature_list.offset_data()) else {
                continue;
            };
            feature.collect_name_ids(plan);
        }
    }
}

impl LayoutClosure for Gsub<'_> {
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
        let Ok(_) = self.closure_glyphs(&lookup_indices, &mut plan.glyphset_gsub) else {
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

        plan.gsub_lookups = remap_indices(lookup_indices);
        (plan.gsub_features, plan.gsub_features_w_duplicates) =
            remap_feature_indices(&feature_indices, &duplicate_feature_index_map);
        plan.gsub_script_langsys = script_langsys_map;
    }
}

impl Subset for Gsub<'_> {
    fn subset_with_state(
        &self,
        plan: &Plan,
        font: &FontRef,
        state: &mut SubsetState,
        s: &mut Serializer,
        _builder: &mut FontBuilder,
    ) -> Result<(), SubsetError> {
        subset_gsub(self, plan, font, state, s)
            .map_err(|_| SubsetError::SubsetTableError(Gsub::TAG))
    }
}

fn subset_gsub(
    gsub: &Gsub,
    plan: &Plan,
    font: &FontRef,
    state: &SubsetState,
    s: &mut Serializer,
) -> Result<(), SerializeErrorFlags> {
    let version_pos = s.embed(gsub.version())?;

    // script_list
    let script_list_offset_pos = s.embed(0_u16)?;

    let script_list = gsub
        .script_list()
        .map_err(|_| s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR))?;

    let mut c = SubsetLayoutContext::new(Gsub::TAG);
    Offset16::serialize_subset(&script_list, s, plan, &mut c, script_list_offset_pos)?;

    // feature list
    let feature_list_offset_pos = s.embed(0_u16)?;
    let feature_list = gsub
        .feature_list()
        .map_err(|_| s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR))?;
    Offset16::serialize_subset(&feature_list, s, plan, &mut c, feature_list_offset_pos)?;

    // lookup list
    let lookup_list_offset_pos = s.embed(0_u16)?;
    let lookup_list = gsub
        .lookup_list()
        .map_err(|_| s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR))?;
    Offset16::serialize_subset(
        &lookup_list,
        s,
        plan,
        (state, font, &plan.gsub_lookups),
        lookup_list_offset_pos,
    )?;

    if let Some(feature_variations) = gsub
        .feature_variations()
        .transpose()
        .map_err(|_| SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)?
    {
        let snap = s.snapshot();
        let feature_vars_offset_pos = s.embed(0_u32)?;
        match Offset32::serialize_subset(
            &feature_variations,
            s,
            plan,
            &mut c,
            feature_vars_offset_pos,
        ) {
            Ok(()) => (),
            // downgrade table version if there are no FeatureVariations
            Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY) => {
                s.revert_snapshot(snap);
                s.copy_assign(version_pos, MajorMinor::VERSION_1_0);
            }
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

impl<'a> SubsetTable<'a> for SubstitutionLookup<'_> {
    type ArgsForSubset = (&'a SubsetState, &'a FontRef<'a>, &'a FnvHashMap<u16, u16>);
    type Output = ();
    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        args: Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        let subtables = self
            .subtables()
            .map_err(|_| s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR))?;
        let lookup_type: u16 = match subtables {
            SubstitutionSubtables::Single(_) => 1,
            SubstitutionSubtables::Multiple(_) => 2,
            SubstitutionSubtables::Alternate(_) => 3,
            SubstitutionSubtables::Ligature(_) => 4,
            SubstitutionSubtables::Contextual(_) => 5,
            SubstitutionSubtables::ChainContextual(_) => 6,
            SubstitutionSubtables::Reverse(_) => 8,
        };
        s.embed(lookup_type)?;

        let lookup_flag = self.lookup_flag();
        let lookup_flag_pos = s.embed(lookup_flag)?;
        let lookup_count_pos = s.embed(0_u16)?;
        let lookup_count = subtables.subset(plan, s, args)?;
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

// TODO: support extension lookup type
impl<'a> SubsetTable<'a> for SubstitutionSubtables<'a> {
    type ArgsForSubset = (&'a SubsetState, &'a FontRef<'a>, &'a FnvHashMap<u16, u16>);
    type Output = u16;
    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        args: Self::ArgsForSubset,
    ) -> Result<u16, SerializeErrorFlags> {
        match self {
            SubstitutionSubtables::Single(subtables) => subtables.subset(plan, s, args),
            SubstitutionSubtables::Multiple(subtables) => subtables.subset(plan, s, args),
            SubstitutionSubtables::Alternate(subtables) => subtables.subset(plan, s, args),
            SubstitutionSubtables::Ligature(subtables) => subtables.subset(plan, s, args),
            SubstitutionSubtables::Contextual(subtables) => subtables.subset(plan, s, args),
            SubstitutionSubtables::ChainContextual(subtables) => subtables.subset(plan, s, args),
            SubstitutionSubtables::Reverse(subtables) => subtables.subset(plan, s, args),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use font_test_data::closure as test_data;
    use write_fonts::read::{types::NameId, FontRef, TableProvider};

    #[test]
    fn test_nameid_closure() {
        let mut plan = Plan::default();
        let font = FontRef::new(include_bytes!(
            "../test-data/fonts/NotoSansOriya-subset.ttf"
        ))
        .unwrap();
        let gsub = font.gsub().unwrap();
        gsub.collect_name_ids(&mut plan);
        assert!(plan.name_ids.is_empty());

        plan.gsub_features.insert(2, 1);
        gsub.collect_name_ids(&mut plan);
        assert_eq!(plan.name_ids.len(), 1);
        assert!(plan.name_ids.contains(NameId::new(257)));
    }

    #[test]
    fn test_prune_features_wo_variations() {
        let font = FontRef::new(test_data::CONTEXTUAL).unwrap();
        let gsub = font.gsub().unwrap();

        let mut lookup_indices = IntSet::empty();
        lookup_indices.extend(1_u16..=3_u16);

        let mut feature_indices = IntSet::empty();
        feature_indices.extend(0_u16..=1_u16);

        // only feature indexed at 1 intersect with lookup_indices
        let retained_features = gsub.prune_features(&lookup_indices, feature_indices);
        assert_eq!(retained_features.len(), 1);
        assert!(retained_features.contains(1));
    }

    #[test]
    fn test_prune_features_w_variations() {
        let font = FontRef::new(test_data::VARIATIONS_CLOSURE).unwrap();
        let gsub = font.gsub().unwrap();

        let mut lookup_indices = IntSet::empty();
        lookup_indices.insert(1_u16);

        let mut feature_indices = IntSet::empty();
        feature_indices.extend(0_u16..=1_u16);

        // feature indexed at 0 has an alternate version that intersects lookup indexed at 1
        let retained_features = gsub.prune_features(&lookup_indices, feature_indices);
        assert_eq!(retained_features.len(), 1);
        assert!(retained_features.contains(0));
    }
}
