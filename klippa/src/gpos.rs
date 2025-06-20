//! impl subset() for GPOS table

use crate::{
    collect_features_with_retained_subs, find_duplicate_features, prune_features, remap_indices,
    LayoutClosure, NameIdClosure, Plan, PruneLangSysContext,
};
use fnv::FnvHashMap;
use write_fonts::read::{collections::IntSet, tables::gpos::Gpos, types::Tag};

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
}
