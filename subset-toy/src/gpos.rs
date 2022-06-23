//impl ToOwnedTable for super::Gpos<'_> {}

use write_fonts::tables::gpos::{
    Gpos, MarkBasePosFormat1, MarkLigPosFormat1, MarkMarkPosFormat1, PairPos, PairPosFormat1,
    PairPosFormat2, PairSet, PositionLookup, PositionLookupList, SinglePos, SinglePosFormat1,
    SinglePosFormat2,
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
            PositionLookup::Cursive(_table) => Ok(true),
            PositionLookup::MarkToBase(table) => table.subset(plan),
            PositionLookup::MarkToMark(table) => table.subset(plan),
            PositionLookup::MarkToLig(table) => table.subset(plan),
            PositionLookup::Contextual(_table) => Ok(true),
            PositionLookup::ChainContextual(_table) => Ok(true),
            PositionLookup::Extension(_table) => Ok(true),
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
        let mut iter = cov.iter().map(|gid| plan.remap_gid(gid).is_some());
        self.pair_set_offsets.retain_mut(|pair_set| {
            iter.next().unwrap()
                && match pair_set.subset(plan) {
                    Err(e) => {
                        err = Err(e);
                        false
                    }
                    Ok(retain) => retain,
                }
        });
        std::mem::drop(iter);
        self.coverage_offset.subset(plan)
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

        // we could remove some of the class records but it's tricky because
        // they're indexed based on class nos., so we could only remove
        // the ones at the back?
        Ok(true)
    }
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
