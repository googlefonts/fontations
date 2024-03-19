//impl ToOwnedTable for super::Gpos<'_> {}

use write_fonts::tables::{
    gpos::{
        Gpos, MarkBasePosFormat1, MarkLigPosFormat1, MarkMarkPosFormat1, PairPos, PairPosFormat1,
        PairPosFormat2, PairSet, PositionLookup, SinglePos, SinglePosFormat1, SinglePosFormat2,
    },
    layout::{ClassDef, CoverageTableBuilder, LookupList},
};

use crate::{Error, Plan, Subset};

impl Subset for Gpos {
    fn subset(&mut self, plan: &Plan) -> Result<bool, Error> {
        self.lookup_list.subset(plan)?;
        self.feature_list.subset(plan)?;
        self.script_list.subset(plan)?;
        //FIXME: subset feature_variations
        Ok(true)
    }
}

impl Subset for LookupList<PositionLookup> {
    fn subset(&mut self, plan: &Plan) -> Result<bool, Error> {
        let mut err = Ok(());
        let mut next_id = 0u16;
        let mut lookup_map = Vec::with_capacity(self.lookups.len());
        self.lookups.retain_mut(|lookup| match lookup.subset(plan) {
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
        self.coverage.subset(plan)
    }
}

impl Subset for SinglePosFormat2 {
    fn subset(&mut self, plan: &Plan) -> Result<bool, Error> {
        let mut iter = self
            .coverage
            .iter()
            .map(|gid| plan.remap_gid(gid).is_some());
        self.value_records.retain(|_| iter.next().unwrap());
        std::mem::drop(iter);
        self.coverage.subset(plan)
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
        let mut iter_cov = self.coverage.iter().map(|gid| plan.remap_gid(gid));
        let mut new_cov = CoverageTableBuilder::default();

        self.pair_sets
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
            self.coverage.set(new_cov);
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
        if !self.coverage.subset(plan)? {
            return Ok(false);
        }

        if !(self.class_def1.subset(plan)? && self.class_def2.subset(plan)?) {
            return Ok(false);
        }

        let existing_class1 = sorted_deduped_class_list(&self.class_def1);
        let existing_class2 = sorted_deduped_class_list(&self.class_def2);

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

        remap_classes(self.class_def1.as_mut(), &existing_class1);
        remap_classes(self.class_def2.as_mut(), &existing_class2);
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
        let mut iter = self
            .mark_coverage
            .iter()
            .map(|gid| plan.remap_gid(gid).is_some());

        self.mark_array
            .mark_records
            .retain(|_| iter.next().unwrap());

        // convince borrowk that we are done with this.
        std::mem::drop(iter);

        let mut iter = self
            .base_coverage
            .iter()
            .map(|gid| plan.remap_gid(gid).is_some());
        self.base_array
            .base_records
            .retain(|_| iter.next().unwrap());
        std::mem::drop(iter);

        let r = self.mark_coverage.subset(plan)? && self.base_coverage.subset(plan)?;
        Ok(r)
    }
}

impl Subset for MarkMarkPosFormat1 {
    fn subset(&mut self, plan: &Plan) -> Result<bool, Error> {
        let mut iter = self
            .mark1_coverage
            .iter()
            .map(|gid| plan.remap_gid(gid).is_some());

        self.mark1_array
            .mark_records
            .retain(|_| iter.next().unwrap());

        // convince borrowk that we are done with this.
        std::mem::drop(iter);

        let mut iter = self
            .mark2_coverage
            .iter()
            .map(|gid| plan.remap_gid(gid).is_some());

        self.mark2_array
            .mark2_records
            .retain(|_| iter.next().unwrap());

        std::mem::drop(iter);
        let r = self.mark1_coverage.subset(plan)? && self.mark2_coverage.subset(plan)?;
        Ok(r)
    }
}

impl Subset for MarkLigPosFormat1 {
    fn subset(&mut self, plan: &Plan) -> Result<bool, Error> {
        let mut iter = self
            .mark_coverage
            .iter()
            .map(|gid| plan.remap_gid(gid).is_some());

        self.mark_array
            .mark_records
            .retain(|_| iter.next().unwrap());
        std::mem::drop(iter);

        let mut iter = self
            .ligature_coverage
            .iter()
            .map(|gid| plan.remap_gid(gid).is_some());
        self.ligature_array
            .ligature_attaches
            .retain(|_| iter.next().unwrap());
        std::mem::drop(iter);

        let r = self.mark_coverage.subset(plan)? && self.ligature_coverage.subset(plan)?;
        Ok(r)
    }
}
