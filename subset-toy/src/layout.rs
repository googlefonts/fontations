use font_types::GlyphId;
use write_fonts::layout::{
    ClassDef, ClassDefBuilder, CoverageTable, CoverageTableBuilder, Feature, FeatureList, LangSys,
    Lookup, Script, ScriptList,
};

use super::{Error, Plan, Subset};

impl Subset for FeatureList {
    fn subset(&mut self, plan: &Plan) -> Result<bool, Error> {
        let mut err = Ok(());
        let mut next_id = 0u16;
        let mut feature_map = Vec::with_capacity(self.feature_records.len());
        self.feature_records
            .retain_mut(|rec| match rec.feature_offset.subset(plan) {
                Err(e) => {
                    err = Err(e);
                    feature_map.push(None);
                    false
                }
                Ok(retain) => {
                    if retain {
                        feature_map.push(Some(next_id));
                        next_id += 1;
                    } else {
                        feature_map.push(None);
                    }
                    retain
                }
            });
        plan.set_gpos_feature_map(feature_map);
        err?;
        Ok(!self.feature_records.is_empty())
    }
}

impl Subset for Feature {
    fn subset(&mut self, plan: &Plan) -> Result<bool, Error> {
        self.lookup_list_indices
            .retain_mut(|id| match plan.remap_gpos_lookup(*id) {
                Some(new) => {
                    *id = new;
                    true
                }
                None => false,
            });
        Ok(!self.lookup_list_indices.is_empty())
    }
}

impl Subset for ScriptList {
    fn subset(&mut self, plan: &Plan) -> Result<bool, Error> {
        let mut err = Ok(());
        self.script_records
            .retain_mut(|rec| match rec.script_offset.subset(plan) {
                Err(e) => {
                    err = Err(e);
                    false
                }
                Ok(retain) => retain,
            });
        err?;
        Ok(!self.script_records.is_empty())
    }
}

impl Subset for Script {
    fn subset(&mut self, plan: &Plan) -> Result<bool, Error> {
        self.default_lang_sys_offset.subset(plan)?;
        let mut err = Ok(());
        self.lang_sys_records
            .retain_mut(|rec| match rec.lang_sys_offset.subset(plan) {
                Err(e) => {
                    err = Err(e);
                    false
                }
                Ok(retain) => retain,
            });
        err?;
        Ok(self.default_lang_sys_offset.get().is_some() || !self.lang_sys_records.is_empty())
    }
}

impl Subset for LangSys {
    fn subset(&mut self, plan: &Plan) -> Result<bool, Error> {
        if self.required_feature_index != 0xffff {
            self.required_feature_index = plan
                .remap_gpos_feature(self.required_feature_index)
                .unwrap_or(0xffff);
        }
        self.feature_indices
            .retain_mut(|id| match plan.remap_gpos_feature(*id) {
                Some(new) => {
                    *id = new;
                    true
                }
                None => false,
            });

        Ok(self.required_feature_index != 0xffff || !self.feature_indices.is_empty())
    }
}

impl Subset for ClassDef {
    fn subset(&mut self, plan: &Plan) -> Result<bool, Error> {
        let builder: ClassDefBuilder = self
            .iter()
            .flat_map(|(gid, cls)| plan.remap_gid(gid).map(|gid| (gid, cls)))
            .collect();
        if builder.items.is_empty() {
            Ok(false)
        } else {
            *self = builder.build();
            Ok(true)
        }
    }
}

impl Subset for CoverageTable {
    fn subset(&mut self, plan: &Plan) -> Result<bool, Error> {
        let gids = match self {
            CoverageTable::Format1(table) => {
                let mut glyphs = std::mem::take(&mut table.glyph_array);
                glyphs.retain_mut(|gid| match plan.remap_gid(*gid) {
                    Some(new_gid) => {
                        *gid = new_gid;
                        true
                    }
                    None => false,
                });
                glyphs
            }
            CoverageTable::Format2(table) => table
                .range_records
                .iter()
                .flat_map(|rcd| rcd.start_glyph_id.to_u16()..=rcd.end_glyph_id.to_u16())
                .map(GlyphId::new)
                .filter_map(|gid| plan.remap_gid(gid))
                .collect::<Vec<_>>(),
        };
        if gids.is_empty() {
            Ok(false)
        } else {
            let builder = CoverageTableBuilder::from_glyphs(gids);
            *self = builder.build();
            Ok(true)
        }
    }
}

impl<T: Subset> Subset for Lookup<T> {
    fn subset(&mut self, plan: &Plan) -> Result<bool, Error> {
        let mut err = Ok(());
        self.subtables.retain_mut(|table| match table.subset(plan) {
            Err(e) => {
                err = Err(e);
                false
            }
            Ok(retain) => retain,
        });
        err?;
        Ok(!self.subtables.is_empty())
    }
}
