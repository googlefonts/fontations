//! impl subset() for GSUB table
mod multiple_subst;
mod single_subst;

use crate::{
    collect_features_with_retained_subs, find_duplicate_features, prune_features, remap_indices,
    LayoutClosure, NameIdClosure, Plan, PruneLangSysContext,
};
use fnv::FnvHashMap;
use write_fonts::read::{collections::IntSet, tables::gsub::Gsub, types::Tag};

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
        plan.gsub_features = remap_indices(feature_indices);
        plan.gsub_script_langsys = script_langsys_map;
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
