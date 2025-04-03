//! Support Layout Closure

use super::{FeatureList, LangSys, ReadError, Script, ScriptList, Tag};
use crate::collections::IntSet;

const MAX_SCRIPTS: u16 = 500;
const MAX_LANGSYS: u16 = 2000;
const MAX_FEATURE_INDICES: u16 = 1500;

struct CollectFeaturesContext<'a> {
    script_count: u16,
    langsys_count: u16,
    feature_index_count: u16,
    visited_script: IntSet<u32>,
    visited_langsys: IntSet<u32>,
    feature_indices: &'a mut IntSet<u16>,
    feature_indices_filter: IntSet<u16>,
    table_head: usize,
}

impl<'a> CollectFeaturesContext<'a> {
    pub(crate) fn new(
        features: &IntSet<Tag>,
        table_head: usize,
        feature_list: &'a FeatureList<'a>,
        feature_indices: &'a mut IntSet<u16>,
    ) -> Self {
        Self {
            script_count: 0,
            langsys_count: 0,
            feature_index_count: 0,
            visited_script: IntSet::empty(),
            visited_langsys: IntSet::empty(),
            feature_indices,
            feature_indices_filter: feature_list
                .feature_records()
                .iter()
                .enumerate()
                .filter(|(_i, record)| features.contains(record.feature_tag()))
                .map(|(idx, _)| idx as u16)
                .collect(),
            table_head,
        }
    }

    /// Return true if the script limit has been exceeded or the script is visited before
    pub(crate) fn script_visited(&mut self, s: &Script) -> bool {
        if self.script_count > MAX_SCRIPTS {
            return true;
        }

        self.script_count += 1;

        let delta = (s.offset_data().as_bytes().as_ptr() as usize - self.table_head) as u32;
        !self.visited_script.insert(delta)
    }

    /// Return true if the Langsys limit has been exceeded or the Langsys is visited before
    pub(crate) fn langsys_visited(&mut self, langsys: &LangSys) -> bool {
        if self.langsys_count > MAX_LANGSYS {
            return true;
        }

        self.langsys_count += 1;

        let delta = (langsys.offset_data().as_bytes().as_ptr() as usize - self.table_head) as u32;
        !self.visited_langsys.insert(delta)
    }

    /// Returns true if the feature limit has been exceeded
    pub(crate) fn feature_indices_limit_exceeded(&mut self, count: u16) -> bool {
        let (new_count, overflow) = self.feature_index_count.overflowing_add(count);
        if overflow {
            self.feature_index_count = MAX_FEATURE_INDICES;
            return true;
        }
        self.feature_index_count = new_count;
        new_count > MAX_FEATURE_INDICES
    }
}

impl ScriptList<'_> {
    /// Return a set of all feature indices underneath the specified scripts, languages and features
    pub(crate) fn collect_features(
        &self,
        layout_table_head: usize,
        feature_list: &FeatureList,
        scripts: &IntSet<Tag>,
        languages: &IntSet<Tag>,
        features: &IntSet<Tag>,
    ) -> Result<IntSet<u16>, ReadError> {
        let mut out = IntSet::empty();
        let mut c =
            CollectFeaturesContext::new(features, layout_table_head, feature_list, &mut out);
        let script_records = self.script_records();
        let font_data = self.offset_data();
        if scripts.is_inverted() {
            for record in script_records {
                let tag = record.script_tag();
                if !scripts.contains(tag) {
                    continue;
                }
                let script = record.script(font_data)?;
                script.collect_features(&mut c, languages)?;
            }
        } else {
            for idx in scripts.iter().filter_map(|tag| self.index_for_tag(tag)) {
                let script = script_records[idx as usize].script(font_data)?;
                script.collect_features(&mut c, languages)?;
            }
        }
        Ok(out)
    }
}

impl Script<'_> {
    fn collect_features(
        &self,
        c: &mut CollectFeaturesContext,
        languages: &IntSet<Tag>,
    ) -> Result<(), ReadError> {
        if c.script_visited(self) {
            return Ok(());
        }

        let lang_sys_records = self.lang_sys_records();
        let font_data = self.offset_data();

        if let Some(default_lang_sys) = self.default_lang_sys().transpose()? {
            default_lang_sys.collect_features(c);
        }

        if languages.is_inverted() {
            for record in lang_sys_records {
                let tag = record.lang_sys_tag();
                if !languages.contains(tag) {
                    continue;
                }
                let lang_sys = record.lang_sys(font_data)?;
                lang_sys.collect_features(c);
            }
        } else {
            for idx in languages
                .iter()
                .filter_map(|tag| self.lang_sys_index_for_tag(tag))
            {
                let lang_sys = lang_sys_records[idx as usize].lang_sys(font_data)?;
                lang_sys.collect_features(c);
            }
        }
        Ok(())
    }
}

impl LangSys<'_> {
    fn collect_features(&self, c: &mut CollectFeaturesContext) {
        if c.langsys_visited(self) {
            return;
        }

        if c.feature_indices_filter.is_empty() {
            return;
        }

        let required_feature_idx = self.required_feature_index();
        if required_feature_idx != 0xFFFF
            && !c.feature_indices_limit_exceeded(1)
            && c.feature_indices_filter.contains(required_feature_idx)
        {
            c.feature_indices.insert(required_feature_idx);
        }

        if c.feature_indices_limit_exceeded(self.feature_index_count()) {
            return;
        }

        for feature_index in self.feature_indices() {
            let idx = feature_index.get();
            if !c.feature_indices_filter.contains(idx) {
                continue;
            }
            c.feature_indices.insert(idx);
            c.feature_indices_filter.remove(idx);
        }
    }
}
