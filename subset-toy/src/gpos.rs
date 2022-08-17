//impl ToOwnedTable for super::Gpos<'_> {}

use write_fonts::{
    layout::{ClassDef, CoverageTableBuilder},
    tables::gpos::{
        Gpos, MarkBasePosFormat1, MarkLigPosFormat1, MarkMarkPosFormat1, PairPos, PairPosFormat1,
        PairPosFormat2, PairSet, PositionLookup, PositionLookupList, SinglePos, SinglePosFormat1,
        SinglePosFormat2,
    },
};

use crate::{Error, Plan, Subset};

impl Subset for Gpos {
    fn subset(&mut self, plan: &Plan) -> Result<bool, Error> {
        self.lookup_list_offset.subset(plan)?;
        self.feature_list_offset.subset(plan)?;
        self.script_list_offset.subset(plan)?;
        //FIXME: subset feature_variations
        Ok(true)
    }
}

impl Subset for PositionLookupList {
    fn subset(&mut self, plan: &Plan) -> Result<bool, Error> {
        let mut err = Ok(());
        let mut next_id = 0u16;
        let mut lookup_map = Vec::with_capacity(self.lookup_offsets.len());
        self.lookup_offsets
            .retain_mut(|lookup| match lookup.subset(plan) {
                Err(e) => {
                    err = Err(e);
                    lookup_map.push(None);
                    false
                }
                Ok(retain) => {
                    if retain {
                        lookup_map.push(Some(next_id));
                        next_id += 1;
                    } else {
                        lookup_map.push(None);
                    }
                    retain
                }
            });
        plan.set_gpos_lookup_map(lookup_map);
        err?;
        Ok(true)
    }
}

impl Subset for PositionLookup {
    fn subset(&mut self, plan: &Plan) -> Result<bool, Error> {
        match self {
            PositionLookup::Single(table) => table.subset(plan),
            PositionLookup::Pair(table) => table.subset(plan),
            PositionLookup::MarkToBase(table) => table.subset(plan),
            PositionLookup::MarkToMark(table) => table.subset(plan),
            PositionLookup::MarkToLig(table) => table.subset(plan),
            _ => panic!("unsupported lookup type for subsetting"),
            //PositionLookup::Cursive(_table) => Ok(true),
            //PositionLookup::Contextual(_table) => Ok(true),
            //PositionLookup::ChainContextual(_table) => Ok(true),
            //PositionLookup::Extension(_table) => Ok(true),
        }
    }
}

impl Subset for SinglePosFormat1 {
    fn subset(&mut self, plan: &Plan) -> Result<bool, Error> {
        self.coverage_offset.subset(plan)
    }
}

impl Subset for SinglePosFormat2 {
    fn subset(&mut self, plan: &Plan) -> Result<bool, Error> {
        let cov = self
            .coverage_offset
            .get()
            .ok_or_else(|| Error::new("debug me"))?;
        let mut iter = cov.iter().map(|gid| plan.remap_gid(gid).is_some());
        self.value_records.retain(|_| iter.next().unwrap());
        std::mem::drop(iter);
        self.coverage_offset.subset(plan)
    }
}

impl Subset for SinglePos {
    fn subset(&mut self, plan: &Plan) -> Result<bool, Error> {
        match self {
            SinglePos::Format1(table) => table.subset(plan),
            SinglePos::Format2(table) => table.subset(plan),
        }
    }
}

impl Subset for PairPos {
    fn subset(&mut self, plan: &Plan) -> Result<bool, Error> {
        match self {
            PairPos::Format1(table) => table.subset(plan),
            PairPos::Format2(table) => table.subset(plan),
        }
    }
}

impl Subset for PairPosFormat1 {
    fn subset(&mut self, plan: &Plan) -> Result<bool, Error> {
        let mut err = Ok(());
        let cov = self
            .coverage_offset
            .get()
            .ok_or_else(|| Error::new("debug me"))?;
        let mut iter_cov = cov.iter().map(|gid| plan.remap_gid(gid));
        let mut new_cov = CoverageTableBuilder::default();

        self.pair_set_offsets
            .retain_mut(|pair_set| match iter_cov.next().unwrap() {
                None => false,
                Some(gid) => match pair_set.subset(plan) {
                    Err(e) => {
                        err = Err(e);
                        false
                    }
                    Ok(true) => {
                        new_cov.add(gid);
                        true
                    }
                    Ok(false) => false,
                },
            });

        // needed for us to set the new coverage table, below
        std::mem::drop(iter_cov);

        let new_cov = new_cov.build();
        if new_cov.is_empty() {
            Ok(false)
        } else {
            self.coverage_offset.set(new_cov);
            Ok(true)
        }
    }
}

impl Subset for PairSet {
    fn subset(&mut self, plan: &Plan) -> Result<bool, Error> {
        self.pair_value_records
            .retain_mut(|rec| match plan.remap_gid(rec.second_glyph) {
                None => false,
                Some(new_gid) => {
                    rec.second_glyph = new_gid;
                    true
                }
            });
        Ok(!self.pair_value_records.is_empty())
    }
}

impl Subset for PairPosFormat2 {
    fn subset(&mut self, plan: &Plan) -> Result<bool, Error> {
        if !self.coverage_offset.subset(plan)? {
            return Ok(false);
        }

        self.class_def1_offset.subset(plan)?;
        self.class_def2_offset.subset(plan)?;
        if self.class_def1_offset.is_none() || self.class_def2_offset.is_none() {
            return Ok(false);
        }

        let existing_class1 = sorted_deduped_class_list(self.class_def1_offset.get().unwrap());
        let existing_class2 = sorted_deduped_class_list(self.class_def2_offset.get().unwrap());

        let mut class1idx = 0u16;
        self.class1_records.retain_mut(|class1record| {
            let result = existing_class1.contains(&class1idx);
            class1idx += 1;
            if result {
                let mut class2idx = 0_u16;
                class1record.class2_records.retain(|_| {
                    let result = existing_class2.contains(&class2idx);
                    class2idx += 1;
                    result
                });
            }
            result
        });

        remap_classes(self.class_def1_offset.get_mut().unwrap(), &existing_class1);
        remap_classes(self.class_def2_offset.get_mut().unwrap(), &existing_class2);
        Ok(true)
    }
}

// retained_classes must be sorted and deduplicated. Classes in the list will
// be remapped to their position in the list. All classes are expected to be
// present in the list.
fn remap_classes(class: &mut ClassDef, retained_classes: &[u16]) {
    match class {
        ClassDef::Format1(cls) => cls
            .class_value_array
            .iter_mut()
            .for_each(|val| *val = retained_classes.iter().position(|x| x == val).unwrap() as u16),
        ClassDef::Format2(cls) => cls.class_range_records.iter_mut().for_each(|rec| {
            rec.class = retained_classes
                .iter()
                .position(|x| *x == rec.class)
                .unwrap() as u16
        }),
    }
}

fn sorted_deduped_class_list(class: &ClassDef) -> Vec<u16> {
    let mut out = vec![0];
    out.extend(class.iter().map(|(_, cls)| cls));
    out.sort_unstable();
    out.dedup();
    out
}

impl Subset for MarkBasePosFormat1 {
    fn subset(&mut self, plan: &Plan) -> Result<bool, Error> {
        let mark_cov = self
            .mark_coverage_offset
            .get()
            .ok_or_else(|| Error::new("debug me"))?;
        let mut iter = mark_cov.iter().map(|gid| plan.remap_gid(gid).is_some());
        if let Some(marks) = self.mark_array_offset.get_mut() {
            marks.mark_records.retain(|_| iter.next().unwrap());
        }
        std::mem::drop(iter);

        let base_cov = self
            .base_coverage_offset
            .get()
            .ok_or_else(|| Error::new("debug me"))?;
        let mut iter = base_cov.iter().map(|gid| plan.remap_gid(gid).is_some());
        if let Some(bases) = self.base_array_offset.get_mut() {
            bases.base_records.retain(|_| iter.next().unwrap())
        }
        std::mem::drop(iter);

        let r =
            self.mark_coverage_offset.subset(plan)? && self.base_coverage_offset.subset(plan)?;
        Ok(r)
    }
}

impl Subset for MarkMarkPosFormat1 {
    fn subset(&mut self, plan: &Plan) -> Result<bool, Error> {
        let mark_cov = self
            .mark1_coverage_offset
            .get()
            .ok_or_else(|| Error::new("debug me"))?;
        let mut iter = mark_cov.iter().map(|gid| plan.remap_gid(gid).is_some());
        if let Some(marks) = self.mark1_array_offset.get_mut() {
            marks.mark_records.retain(|_| iter.next().unwrap());
        }
        std::mem::drop(iter);

        let mark_cov = self
            .mark2_coverage_offset
            .get()
            .ok_or_else(|| Error::new("debug me"))?;
        let mut iter = mark_cov.iter().map(|gid| plan.remap_gid(gid).is_some());

        if let Some(marks) = self.mark2_array_offset.get_mut() {
            marks.mark2_records.retain(|_| iter.next().unwrap());
        }

        std::mem::drop(iter);
        let r =
            self.mark1_coverage_offset.subset(plan)? && self.mark2_coverage_offset.subset(plan)?;
        Ok(r)
    }
}

impl Subset for MarkLigPosFormat1 {
    fn subset(&mut self, plan: &Plan) -> Result<bool, Error> {
        let mark_cov = self
            .mark_coverage_offset
            .get()
            .ok_or_else(|| Error::new("debug me"))?;
        let mut iter = mark_cov.iter().map(|gid| plan.remap_gid(gid).is_some());
        if let Some(marks) = self.mark_array_offset.get_mut() {
            marks.mark_records.retain(|_| iter.next().unwrap());
        }
        std::mem::drop(iter);

        let lig_cov = self
            .ligature_coverage_offset
            .get()
            .ok_or_else(|| Error::new("debug me"))?;
        let mut iter = lig_cov.iter().map(|gid| plan.remap_gid(gid).is_some());
        if let Some(ligs) = self.ligature_array_offset.get_mut() {
            ligs.ligature_attach_offsets
                .retain(|_| iter.next().unwrap());
        }
        std::mem::drop(iter);

        let r = self.mark_coverage_offset.subset(plan)?
            && self.ligature_coverage_offset.subset(plan)?;
        Ok(r)
    }
}
