//! impl subset() for GSUB table

use crate::{
    collect_features_with_retained_subs, feature_intersects_lookups, NameIdClosure, Plan,
    PruneFeaturesLangSys, PruneLangSysContext,
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

impl PruneFeaturesLangSys for Gsub<'_> {
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

        let mut out = IntSet::empty();
        let Ok(feature_list) = self.feature_list() else {
            return out;
        };
        let feature_records = feature_list.feature_records();

        for i in feature_indices.iter() {
            let Some(feature_rec) = feature_records.get(i as usize) else {
                continue;
            };
            let feature_tag = feature_rec.feature_tag();
            // never drop feature "pref"
            // ref: https://github.com/harfbuzz/harfbuzz/blob/fc6231726e514f96bfbb098283aab332fc6b45fb/src/hb-ot-layout-gsubgpos.hh#L4822
            if feature_tag == Tag::new(b"pref") {
                out.insert(i);
                continue;
            }

            let Ok(feature) = feature_rec.feature(feature_list.offset_data()) else {
                return out;
            };
            // always keep "size" feature even if it's empty
            // ref: https://github.com/fonttools/fonttools/blob/e857fe5ef7b25e92fd829a445357e45cde16eb04/Lib/fontTools/subset/__init__.py#L1627
            if !feature.feature_params_offset().is_null() {
                if feature_tag == Tag::new(b"size") {
                    out.insert(i);
                    continue;
                }
            }

            if !feature_intersects_lookups(&feature, lookup_indices)
                && !alternate_features.contains(i)
            {
                continue;
            }
            out.insert(i);
        }
        out
    }

    fn prune_langsys(
        &self,
        feature_index_map: &FnvHashMap<u16, u16>,
        layout_scripts: &IntSet<Tag>,
    ) -> (FnvHashMap<u16, IntSet<u16>>, IntSet<u16>) {
        let mut c = PruneLangSysContext::new(feature_index_map);
        let Ok(script_list) = self.script_list() else {
            return (c.script_langsys_map(), c.feature_indices());
        };

        for (i, script_rec) in script_list.script_records().iter().enumerate() {
            let script_tag = script_rec.script_tag();
            if !layout_scripts.contains(script_tag) {
                continue;
            }

            let Ok(script) = script_rec.script(script_list.offset_data()) else {
                return (c.script_langsys_map(), c.feature_indices());
            };
            c.prune_script_langsys(i as u16, &script);
        }
        (c.script_langsys_map(), c.feature_indices())
    }
}

#[cfg(test)]
mod test {
    use super::*;
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
}
