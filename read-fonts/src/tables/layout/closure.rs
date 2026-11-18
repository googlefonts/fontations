//! Support Layout Closure

use types::{BigEndian, Compatible, GlyphId16, GlyphId24, Offset16};

use super::{
    ArrayOfOffsets, ChainedClassSequenceRule, ChainedClassSequenceRuleSet,
    ChainedClassSequenceRuleSet2, ChainedSequenceContext, ChainedSequenceContextFormat1,
    ChainedSequenceContextFormat2, ChainedSequenceContextFormat3, ChainedSequenceContextFormat4,
    ChainedSequenceContextFormat5, ChainedSequenceRule, ChainedSequenceRule2,
    ChainedSequenceRuleSet, ChainedSequenceRuleSet2, ClassDef, ClassDefFormat1, ClassDefFormat2,
    ClassSequenceRule, ClassSequenceRuleSet, ClassSequenceRuleSet2, CoverageTable, ExtensionLookup,
    Feature, FeatureList, FeatureVariations, GlyphId, LangSys, ReadError, Script, ScriptList,
    SequenceContext, SequenceContextFormat1, SequenceContextFormat2, SequenceContextFormat3,
    SequenceContextFormat4, SequenceContextFormat5, SequenceContextFormat6, SequenceLookupRecord,
    SequenceRule, SequenceRule2, SequenceRuleSet, SequenceRuleSet2, Subtables, Tag,
};
use crate::{
    collections::{FastHashMap, IntSet},
    tables::{gpos::PositionLookupList, gsub::SubstitutionLookupList},
    FontRead, MinByteRange,
};

use crate::limits::{
    MAX_FEATURE_INDICES, MAX_LANGSYS, MAX_LOOKUP_SUBTABLES, MAX_LOOKUP_VISIT_COUNT,
    MAX_OPS_EXCEEDED, MAX_RECURSION_DEPTH, MAX_SCRIPTS,
};

/// Charges `table_bytes` against the bytes a walk of a table's offset graph
/// may still read.
///
/// Offsets may share or overlap their targets, so what a table reaches is not
/// bounded by its size. A target counts once for every offset that leads to
/// it, as it does when HarfBuzz sanitizes a layout table.
fn charge_ops(ops_left: &mut usize, table_bytes: &[u8]) -> Result<(), ReadError> {
    *ops_left = ops_left
        .checked_sub(table_bytes.len())
        .ok_or(MAX_OPS_EXCEEDED)?;
    Ok(())
}

/// Charges `count` records against what a walk of a table's offset graph may
/// still read, like [`charge_ops`].
pub(crate) fn charge_records(ops_left: &mut usize, count: usize) -> Result<(), ReadError> {
    *ops_left = ops_left.checked_sub(count).ok_or(MAX_OPS_EXCEEDED)?;
    Ok(())
}

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
                if !scripts.contains(tag) || record.script_offset().is_null() {
                    continue;
                }
                let script = record.script(font_data)?;
                script.collect_features(&mut c, languages)?;
            }
        } else {
            for idx in scripts.iter().filter_map(|tag| self.index_for_tag(tag)) {
                let record = script_records[idx as usize];
                if record.script_offset().is_null() {
                    continue;
                }
                let script = record.script(font_data)?;
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
                if !languages.contains(tag) || record.lang_sys_offset().is_null() {
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
                let record = lang_sys_records[idx as usize];
                if record.lang_sys_offset().is_null() {
                    continue;
                }
                let lang_sys = record.lang_sys(font_data)?;
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

impl Feature<'_> {
    pub(crate) fn collect_lookups(&self) -> Vec<u16> {
        self.lookup_list_indices()
            .iter()
            .map(|idx| idx.get())
            .collect()
    }
}

impl FeatureList<'_> {
    /// `ops_left` is what is left of the [`max_ops`](crate::limits::max_ops)
    /// of the GSUB or GPOS table this list belongs to.
    pub(crate) fn collect_lookups(
        &self,
        feature_indices: &IntSet<u16>,
        ops_left: &mut usize,
    ) -> Result<IntSet<u16>, ReadError> {
        let features_records = self.feature_records();
        let num_features = self.feature_count();
        let font_data = self.offset_data();
        let mut lookup_idxes = IntSet::empty();

        if feature_indices.is_inverted() {
            for feature_rec in (0..num_features).filter_map(|i| {
                feature_indices
                    .contains(i)
                    .then(|| features_records.get(i as usize))
                    .flatten()
            }) {
                if feature_rec.feature_offset().is_null() {
                    continue;
                }
                let feature = feature_rec.feature(font_data)?;
                charge_ops(ops_left, feature.min_table_bytes())?;
                lookup_idxes.extend_unsorted(feature.collect_lookups());
            }
        } else {
            for feature_rec in feature_indices
                .iter()
                .filter_map(|i| features_records.get(i as usize))
            {
                if feature_rec.feature_offset().is_null() {
                    continue;
                }
                let feature = feature_rec.feature(font_data)?;
                charge_ops(ops_left, feature.min_table_bytes())?;
                lookup_idxes.extend_unsorted(feature.collect_lookups());
            }
        }
        Ok(lookup_idxes)
    }
}

impl FeatureVariations<'_> {
    /// `ops_left` is what is left of the [`max_ops`](crate::limits::max_ops)
    /// of the GSUB or GPOS table this table belongs to.
    pub(crate) fn collect_lookups(
        &self,
        feature_indices: &IntSet<u16>,
        ops_left: &mut usize,
    ) -> Result<IntSet<u16>, ReadError> {
        let mut out = IntSet::empty();
        if u64::from(self.feature_variation_record_count()) * 8
            > self.offset_data().len().saturating_sub(8) as u64
        {
            return Err(ReadError::OutOfBounds);
        }

        for variation_rec in self.feature_variation_records() {
            let Some(subs) = variation_rec
                .feature_table_substitution(self.offset_data())
                .transpose()?
            else {
                continue;
            };
            charge_ops(ops_left, subs.min_table_bytes())?;

            for sub_record in subs
                .substitutions()
                .iter()
                .filter(|sub_rec| feature_indices.contains(sub_rec.feature_index()))
            {
                if sub_record.alternate_feature_offset().is_null() {
                    continue;
                }
                let sub_f = sub_record.alternate_feature(subs.offset_data())?;
                charge_ops(ops_left, sub_f.min_table_bytes())?;
                out.extend_unsorted(sub_f.lookup_list_indices().iter().map(|i| i.get()));
            }
        }
        if self.version().compatible((1, 1)) {
            let records = self
                .lookup_variation_records()
                .ok_or(ReadError::OutOfBounds)?;
            for record in records
                .iter()
                .filter(|rec| feature_indices.contains(rec.feature_index()))
            {
                if record.feature_lookups_offset().is_null() {
                    continue;
                }
                let lookups = record.feature_lookups(self.offset_data())?;
                if u64::from(lookups.lookup_condition_count()) * 8
                    > lookups.offset_data().len().saturating_sub(10) as u64
                {
                    return Err(ReadError::OutOfBounds);
                }
                charge_ops(ops_left, lookups.min_table_bytes())?;
                for record in lookups.lookup_condition_records() {
                    if record.lookup_index_list_offset().is_null() {
                        continue;
                    }
                    let indices = record.lookup_index_list(lookups.offset_data())?;
                    if indices.min_table_bytes().is_empty() {
                        return Err(ReadError::OutOfBounds);
                    }
                    charge_ops(ops_left, indices.min_table_bytes())?;
                    // Closure includes every design-space branch; do not evaluate conditions.
                    out.extend_unsorted(indices.lookup_indices().iter().map(|i| i.get()));
                }
            }
        }
        Ok(out)
    }
}

pub(crate) enum LayoutLookupList<'a> {
    Gsub(&'a SubstitutionLookupList<'a>),
    Gpos(&'a PositionLookupList<'a>),
}

pub(crate) struct LookupClosureCtx<'a> {
    visited_lookups: IntSet<u16>,
    inactive_lookups: IntSet<u16>,
    glyph_set: &'a IntSet<GlyphId>,
    lookup_count: u16,
    nesting_level_left: usize,
    lookup_list: &'a LayoutLookupList<'a>,
    /// What the closure may still read. The lookup visit count doesn't bound
    /// that: any number of offsets can lead to one subtable, rule or ligature,
    /// which is read again for each of them.
    ops_left: usize,
    /// How many more subtables the lookups that are visited may have.
    subtables_left: usize,
}

impl<'a> LookupClosureCtx<'a> {
    /// A closure that may read `max_ops` records, which is
    /// [`max_ops`](crate::limits::max_ops) of the table the lookups are in.
    pub(crate) fn new(
        glyph_set: &'a IntSet<GlyphId>,
        lookup_list: &'a LayoutLookupList,
        max_ops: usize,
    ) -> Self {
        Self {
            visited_lookups: IntSet::empty(),
            inactive_lookups: IntSet::empty(),
            glyph_set,
            lookup_count: 0,
            nesting_level_left: MAX_RECURSION_DEPTH,
            lookup_list,
            ops_left: max_ops,
            subtables_left: MAX_LOOKUP_SUBTABLES - 1,
        }
    }

    /// Charges `count` records that are about to be read.
    pub(crate) fn charge(&mut self, count: usize) -> Result<(), ReadError> {
        charge_records(&mut self.ops_left, count)
    }

    /// Counts the subtables of a lookup that is about to be visited.
    pub(crate) fn visit_subtables(&mut self, count: usize) -> Result<(), ReadError> {
        charge_records(&mut self.subtables_left, count)
    }

    /// Whether `table` intersects the glyphs, charging what that reads.
    pub(crate) fn intersects(&mut self, table: &impl Intersect) -> Result<bool, ReadError> {
        table.intersects(self.glyph_set, &mut self.ops_left)
    }

    pub(crate) fn visited_lookups(&self) -> &IntSet<u16> {
        &self.visited_lookups
    }

    pub(crate) fn inactive_lookups(&self) -> &IntSet<u16> {
        &self.inactive_lookups
    }

    pub(crate) fn glyphs(&self) -> &IntSet<GlyphId> {
        self.glyph_set
    }

    pub(crate) fn set_lookup_inactive(&mut self, lookup_index: u16) {
        self.inactive_lookups.insert(lookup_index);
    }

    pub(crate) fn lookup_limit_exceed(&self) -> bool {
        self.lookup_count > MAX_LOOKUP_VISIT_COUNT
    }

    // return false if lookup limit exceeded or lookup visited,and visited set is not modified
    // Otherwise return true and insert lookup index into the visited set
    pub(crate) fn should_visit_lookup(&mut self, lookup_index: u16) -> bool {
        if self.lookup_count > MAX_LOOKUP_VISIT_COUNT {
            return false;
        }
        self.lookup_count += 1;
        self.visited_lookups.insert(lookup_index)
    }

    pub(crate) fn recurse(&mut self, lookup_index: u16) -> Result<(), ReadError> {
        if self.nesting_level_left == 0 {
            return Ok(());
        }

        if self.lookup_limit_exceed() || self.visited_lookups.contains(lookup_index) {
            return Ok(());
        }

        self.nesting_level_left -= 1;
        match self.lookup_list {
            LayoutLookupList::Gpos(lookuplist) => {
                match lookuplist.lookups().get(lookup_index as usize) {
                    Err(ReadError::NullOffset) | Err(ReadError::InvalidCollectionIndex(_)) => (),
                    lookup => lookup?.closure_lookups(self, lookup_index)?,
                }
            }
            LayoutLookupList::Gsub(lookuplist) => {
                match lookuplist.lookups().get(lookup_index as usize) {
                    Err(ReadError::NullOffset) | Err(ReadError::InvalidCollectionIndex(_)) => (),
                    lookup => lookup?.closure_lookups(self, lookup_index)?,
                }
            }
        }
        self.nesting_level_left += 1;
        Ok(())
    }
}

/// Compute the transitive closure of lookups
pub(crate) trait LookupClosure {
    fn closure_lookups(&self, _c: &mut LookupClosureCtx, _arg: u16) -> Result<(), ReadError> {
        Ok(())
    }
}

pub trait Intersect {
    /// Returns whether this table intersects `glyph_set`, for a walk of a
    /// table's offset graph that may read `ops_left` more records.
    ///
    /// A table that follows arrays of offsets charges what it reads through
    /// them, and returns [`MAX_OPS_EXCEEDED`] once that is more than
    /// `ops_left`: any number of offsets can lead to one target, so the size
    /// of the table doesn't bound what this reads. A table that only looks at
    /// its coverage tables charges nothing.
    ///
    /// A caller without a budget passes `&mut usize::MAX`.
    fn intersects(
        &self,
        glyph_set: &IntSet<GlyphId>,
        ops_left: &mut usize,
    ) -> Result<bool, ReadError>;
}

impl Intersect for ClassDef<'_> {
    fn intersects(
        &self,
        glyph_set: &IntSet<GlyphId>,
        ops_left: &mut usize,
    ) -> Result<bool, ReadError> {
        match self {
            ClassDef::Format1(table) => table.intersects(glyph_set, ops_left),
            ClassDef::Format2(table) => table.intersects(glyph_set, ops_left),
            _ => Ok(self
                .intersect_classes(glyph_set)
                .iter()
                .any(|class| class != 0)),
        }
    }
}

impl Intersect for ClassDefFormat1<'_> {
    fn intersects(
        &self,
        glyph_set: &IntSet<GlyphId>,
        _ops_left: &mut usize,
    ) -> Result<bool, ReadError> {
        let class_values = self.class_value_array();
        if class_values.is_empty() {
            return Ok(false);
        }

        let start = self.start_glyph_id().to_u32();
        let end = start + self.glyph_count() as u32;

        let mut start_glyph = GlyphId::from(start);
        if glyph_set.contains(start_glyph) && class_values[0] != 0 {
            return Ok(true);
        }

        while let Some(g) = glyph_set.next_after(start_glyph) {
            let g = g.to_u32();
            if g >= end {
                break;
            }
            let Some(class) = class_values.get((g - start) as usize) else {
                break;
            };
            if class.get() != 0 {
                return Ok(true);
            }
            start_glyph = GlyphId::from(g);
        }
        Ok(false)
    }
}

impl Intersect for ClassDefFormat2<'_> {
    fn intersects(
        &self,
        glyph_set: &IntSet<GlyphId>,
        _ops_left: &mut usize,
    ) -> Result<bool, ReadError> {
        let num_ranges = self.class_range_count();
        let num_bits = 16 - num_ranges.leading_zeros();
        if num_ranges as u64 > glyph_set.len() * num_bits as u64 {
            for g in glyph_set.iter() {
                if self.get(g) != 0 {
                    return Ok(true);
                }
            }
        } else {
            for record in self.class_range_records() {
                let first = GlyphId::from(record.start_glyph_id());
                let last = GlyphId::from(record.end_glyph_id());
                if glyph_set.intersects_range(first..=last) && record.class() != 0 {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
}

impl<'a, T, Ext> LookupClosure for Subtables<'a, T, Ext>
where
    T: LookupClosure + Intersect + FontRead<'a, Args = ()> + 'a,
    Ext: ExtensionLookup<'a, T> + 'a,
{
    /// Runs lookup closure on the subtables that intersect the glyph set, and
    /// marks the lookup at `lookup_index` inactive if none of them do.
    ///
    /// This checks each subtable only once, instead of running a separate
    /// `intersects` pass over the lookup first. A subtable that doesn't
    /// intersect the glyph set recurses into no lookups, so skipping it
    /// changes nothing.
    fn closure_lookups(
        &self,
        c: &mut LookupClosureCtx,
        lookup_index: u16,
    ) -> Result<(), ReadError> {
        let mut intersects = false;
        for t in self.iter().filter_map(|table| match table {
            Err(ReadError::NullOffset) => None,
            other => Some(other),
        }) {
            let t = t?;
            if c.intersects(&t)? {
                intersects = true;
                t.closure_lookups(c, lookup_index)?;
            }
        }
        if !intersects {
            c.set_lookup_inactive(lookup_index);
        }
        Ok(())
    }
}

impl<'a, T> Intersect for ArrayOfOffsets<'a, T, Offset16>
where
    T: Intersect + FontRead<'a, Args = ()> + 'a,
{
    fn intersects(
        &self,
        glyph_set: &IntSet<GlyphId>,
        ops_left: &mut usize,
    ) -> Result<bool, ReadError> {
        for t in self.iter().filter_map(|table| match table {
            Err(ReadError::NullOffset) => None,
            other => Some(other),
        }) {
            if t?.intersects(glyph_set, ops_left)? {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

// these are basically the same; but we need to jump through some hoops
// to get the fields to line up
pub(crate) enum ContextFormat1<'a> {
    Plain(SequenceContextFormat1<'a>),
    Chain(ChainedSequenceContextFormat1<'a>),
    PlainWide(SequenceContextFormat4<'a>),
    ChainWide(ChainedSequenceContextFormat4<'a>),
}

pub(crate) enum Format1RuleSet<'a> {
    Plain(SequenceRuleSet<'a>),
    Chain(ChainedSequenceRuleSet<'a>),
    PlainWide(SequenceRuleSet2<'a>),
    ChainWide(ChainedSequenceRuleSet2<'a>),
}

pub(crate) enum Format1Rule<'a> {
    Plain(SequenceRule<'a>),
    Chain(ChainedSequenceRule<'a>),
    PlainWide(SequenceRule2<'a>),
    ChainWide(ChainedSequenceRule2<'a>),
}

impl<'a> ContextFormat1<'a> {
    pub(crate) fn coverage(&self) -> Option<Result<CoverageTable<'_>, ReadError>> {
        match self {
            ContextFormat1::Plain(table) if !table.coverage_offset().is_null() => {
                Some(table.coverage())
            }
            ContextFormat1::Chain(table) if !table.coverage_offset().is_null() => {
                Some(table.coverage())
            }
            ContextFormat1::PlainWide(table) if !table.coverage_offset().is_null() => {
                Some(table.coverage())
            }
            ContextFormat1::ChainWide(table) if !table.coverage_offset().is_null() => {
                Some(table.coverage())
            }
            _ => None,
        }
    }

    pub(crate) fn rule_set_count(&self) -> usize {
        match self {
            Self::Plain(table) => table.seq_rule_sets().len(),
            Self::Chain(table) => table.chained_seq_rule_sets().len(),
            Self::PlainWide(table) => table.seq_rule_sets().len(),
            Self::ChainWide(table) => table.chained_seq_rule_sets().len(),
        }
    }

    pub(crate) fn rule_sets(
        &self,
    ) -> impl Iterator<Item = Option<Result<Format1RuleSet<'a>, ReadError>>> + '_ {
        let count = self.rule_set_count();
        (0..count).map(move |index| match self {
            Self::Plain(table) => table
                .seq_rule_sets()
                .get(index)
                .map(|rs| rs.map(Format1RuleSet::Plain)),
            Self::Chain(table) => table
                .chained_seq_rule_sets()
                .get(index)
                .map(|rs| rs.map(Format1RuleSet::Chain)),
            Self::PlainWide(table) => table
                .seq_rule_sets()
                .get(index)
                .map(|rs| rs.map(Format1RuleSet::PlainWide)),
            Self::ChainWide(table) => table
                .chained_seq_rule_sets()
                .get(index)
                .map(|rs| rs.map(Format1RuleSet::ChainWide)),
        })
    }
}

impl<'a> Format1RuleSet<'a> {
    pub(crate) fn rule_count(&self) -> usize {
        match self {
            Self::Plain(table) => table.seq_rules().len(),
            Self::Chain(table) => table.chained_seq_rules().len(),
            Self::PlainWide(table) => table.seq_rules().len(),
            Self::ChainWide(table) => table.chained_seq_rules().len(),
        }
    }

    pub(crate) fn rules(
        &self,
    ) -> impl Iterator<Item = Option<Result<Format1Rule<'a>, ReadError>>> + '_ {
        let count = self.rule_count();
        (0..count).map(move |index| {
            let rule = match self {
                Self::Plain(table) => table.seq_rules().get(index).map(Format1Rule::Plain),
                Self::Chain(table) => table.chained_seq_rules().get(index).map(Format1Rule::Chain),
                Self::PlainWide(table) => table.seq_rules().get(index).map(Format1Rule::PlainWide),
                Self::ChainWide(table) => table
                    .chained_seq_rules()
                    .get(index)
                    .map(Format1Rule::ChainWide),
            };
            match rule {
                Err(ReadError::NullOffset) => None,
                other => Some(other),
            }
        })
    }
}

impl Format1Rule<'_> {
    pub(crate) fn input_count(&self) -> usize {
        match self {
            Self::Plain(table) => table.input_sequence().len() + 1,
            Self::Chain(table) => table.input_sequence().len() + 1,
            Self::PlainWide(table) => table.input_sequence().len() + 1,
            Self::ChainWide(table) => table.input_sequence().len() + 1,
        }
    }

    pub(crate) fn input_glyph(&self, index: usize) -> Option<GlyphId> {
        match self {
            Self::Plain(table) => table.input_sequence().get(index).map(|g| g.get().into()),
            Self::Chain(table) => table.input_sequence().get(index).map(|g| g.get().into()),
            Self::PlainWide(table) => table.input_sequence().get(index).map(|g| g.get().into()),
            Self::ChainWide(table) => table.input_sequence().get(index).map(|g| g.get().into()),
        }
    }

    pub(crate) fn lookup_records(&self) -> &[SequenceLookupRecord] {
        match self {
            Self::Plain(table) => table.seq_lookup_records(),
            Self::Chain(table) => table.seq_lookup_records(),
            Self::PlainWide(table) => table.seq_lookup_records(),
            Self::ChainWide(table) => table.seq_lookup_records(),
        }
    }

    /// The number of glyphs and lookup records in this rule, which is what
    /// walking it costs.
    pub(crate) fn record_count(&self) -> usize {
        let context = match self {
            Self::Chain(table) => {
                table.backtrack_sequence().len() + table.lookahead_sequence().len()
            }
            Self::ChainWide(table) => {
                table.backtrack_sequence().len() + table.lookahead_sequence().len()
            }
            Self::Plain(_) | Self::PlainWide(_) => 0,
        };
        context + self.input_count() + self.lookup_records().len()
    }
}

impl Intersect for &[BigEndian<GlyphId16>] {
    fn intersects(
        &self,
        glyph_set: &IntSet<GlyphId>,
        _ops_left: &mut usize,
    ) -> Result<bool, ReadError> {
        Ok(self
            .iter()
            .all(|g| glyph_set.contains(GlyphId::from(g.get()))))
    }
}

impl Intersect for &[BigEndian<GlyphId24>] {
    fn intersects(
        &self,
        glyph_set: &IntSet<GlyphId>,
        _ops_left: &mut usize,
    ) -> Result<bool, ReadError> {
        Ok(self
            .iter()
            .all(|g| glyph_set.contains(GlyphId::from(g.get()))))
    }
}

impl Intersect for Format1Rule<'_> {
    fn intersects(
        &self,
        glyph_set: &IntSet<GlyphId>,
        ops_left: &mut usize,
    ) -> Result<bool, ReadError> {
        match self {
            Self::Plain(table) => table.input_sequence().intersects(glyph_set, ops_left),
            Self::Chain(table) => Ok(table.backtrack_sequence().intersects(glyph_set, ops_left)?
                && table.input_sequence().intersects(glyph_set, ops_left)?
                && table.lookahead_sequence().intersects(glyph_set, ops_left)?),
            Self::PlainWide(table) => table.input_sequence().intersects(glyph_set, ops_left),
            Self::ChainWide(table) => {
                Ok(table.backtrack_sequence().intersects(glyph_set, ops_left)?
                    && table.input_sequence().intersects(glyph_set, ops_left)?
                    && table.lookahead_sequence().intersects(glyph_set, ops_left)?)
            }
        }
    }
}

impl LookupClosure for Format1Rule<'_> {
    fn closure_lookups(&self, c: &mut LookupClosureCtx, _arg: u16) -> Result<(), ReadError> {
        c.charge(self.record_count())?;
        if c.lookup_limit_exceed() || !c.intersects(self)? {
            return Ok(());
        }

        for lookup_record in self.lookup_records() {
            let index = lookup_record.lookup_list_index();
            c.recurse(index)?;
        }
        Ok(())
    }
}

pub(crate) enum ContextFormat2<'a> {
    Plain(SequenceContextFormat2<'a>),
    Chain(ChainedSequenceContextFormat2<'a>),
    PlainWide(SequenceContextFormat5<'a>),
    ChainWide(ChainedSequenceContextFormat5<'a>),
}

pub(crate) enum Format2RuleSet<'a> {
    Plain(ClassSequenceRuleSet<'a>),
    Chain(ChainedClassSequenceRuleSet<'a>),
    PlainWide(ClassSequenceRuleSet2<'a>),
    ChainWide(ChainedClassSequenceRuleSet2<'a>),
}

pub(crate) enum Format2Rule<'a> {
    Plain(ClassSequenceRule<'a>),
    Chain(ChainedClassSequenceRule<'a>),
}

#[derive(Default)]
pub(crate) struct SeqCache {
    input: FastHashMap<u16, bool>,
    backtrack: FastHashMap<u16, bool>,
    lookahead: FastHashMap<u16, bool>,
}

impl SeqCache {
    /// The number of classes that have been looked up in a class definition.
    pub(crate) fn class_count(&self) -> usize {
        self.input.len() + self.backtrack.len() + self.lookahead.len()
    }
}

impl<'a> ContextFormat2<'a> {
    pub(crate) fn coverage(&self) -> Option<Result<CoverageTable<'_>, ReadError>> {
        match self {
            ContextFormat2::Plain(table) if !table.coverage_offset().is_null() => {
                Some(table.coverage())
            }
            ContextFormat2::Chain(table) if !table.coverage_offset().is_null() => {
                Some(table.coverage())
            }
            Self::PlainWide(table) if !table.coverage_offset().is_null() => Some(table.coverage()),
            Self::ChainWide(table) if !table.coverage_offset().is_null() => Some(table.coverage()),
            _ => None,
        }
    }

    pub(crate) fn input_class_def(&self) -> Option<Result<ClassDef<'_>, ReadError>> {
        match self {
            ContextFormat2::Plain(table_ref) if !table_ref.class_def_offset().is_null() => {
                Some(table_ref.class_def())
            }
            ContextFormat2::Chain(table_ref) if !table_ref.input_class_def_offset().is_null() => {
                Some(table_ref.input_class_def())
            }
            Self::PlainWide(table) if !table.class_def_offset().is_null() => {
                Some(table.class_def())
            }
            Self::ChainWide(table) if !table.input_class_def_offset().is_null() => {
                Some(table.input_class_def())
            }
            _ => None,
        }
    }

    pub(crate) fn backtrack_class_def(&self) -> Option<Result<ClassDef<'_>, ReadError>> {
        match self {
            Self::Chain(table) if !table.backtrack_class_def_offset().is_null() => {
                Some(table.backtrack_class_def())
            }
            Self::ChainWide(table) if !table.backtrack_class_def_offset().is_null() => {
                Some(table.backtrack_class_def())
            }
            _ => None,
        }
    }

    pub(crate) fn lookahead_class_def(&self) -> Option<Result<ClassDef<'_>, ReadError>> {
        match self {
            Self::Chain(table) if !table.lookahead_class_def_offset().is_null() => {
                Some(table.lookahead_class_def())
            }
            Self::ChainWide(table) if !table.lookahead_class_def_offset().is_null() => {
                Some(table.lookahead_class_def())
            }
            _ => None,
        }
    }

    pub(crate) fn rule_set_count(&self) -> usize {
        match self {
            Self::Plain(table) => table.class_seq_rule_sets().len(),
            Self::Chain(table) => table.chained_class_seq_rule_sets().len(),
            Self::PlainWide(table) => table.class_seq_rule_sets().len(),
            Self::ChainWide(table) => table.chained_class_seq_rule_sets().len(),
        }
    }

    pub(crate) fn rule_sets(
        &self,
    ) -> impl Iterator<Item = Option<Result<Format2RuleSet<'a>, ReadError>>> + '_ {
        let count = self.rule_set_count();
        (0..count).map(move |index| match self {
            Self::Plain(table) => table
                .class_seq_rule_sets()
                .get(index)
                .map(|rs| rs.map(Format2RuleSet::Plain)),
            Self::Chain(table) => table
                .chained_class_seq_rule_sets()
                .get(index)
                .map(|rs| rs.map(Format2RuleSet::Chain)),
            Self::PlainWide(table) => table
                .class_seq_rule_sets()
                .get(index)
                .map(|rs| rs.map(Format2RuleSet::PlainWide)),
            Self::ChainWide(table) => table
                .chained_class_seq_rule_sets()
                .get(index)
                .map(|rs| rs.map(Format2RuleSet::ChainWide)),
        })
    }
}

impl<'a> Format2RuleSet<'a> {
    pub(crate) fn rule_count(&self) -> usize {
        match self {
            Self::Plain(table) => table.class_seq_rules().len(),
            Self::Chain(table) => table.chained_class_seq_rules().len(),
            Self::PlainWide(table) => table.class_seq_rules().len(),
            Self::ChainWide(table) => table.chained_class_seq_rules().len(),
        }
    }

    pub(crate) fn rules(
        &self,
    ) -> impl Iterator<Item = Option<Result<Format2Rule<'a>, ReadError>>> + '_ {
        let count = self.rule_count();
        (0..count).map(move |index| {
            let rule = match self {
                Self::Plain(table) => table.class_seq_rules().get(index).map(Format2Rule::Plain),
                Self::Chain(table) => table
                    .chained_class_seq_rules()
                    .get(index)
                    .map(Format2Rule::Chain),
                Self::PlainWide(table) => {
                    table.class_seq_rules().get(index).map(Format2Rule::Plain)
                }
                Self::ChainWide(table) => table
                    .chained_class_seq_rules()
                    .get(index)
                    .map(Format2Rule::Chain),
            };
            match rule {
                Err(ReadError::NullOffset) => None,
                other => Some(other),
            }
        })
    }
}

impl Format2Rule<'_> {
    pub(crate) fn input_sequence(&self) -> &[BigEndian<u16>] {
        match self {
            Self::Plain(table) => table.input_sequence(),
            Self::Chain(table) => table.input_sequence(),
        }
    }

    pub(crate) fn lookup_records(&self) -> &[SequenceLookupRecord] {
        match self {
            Self::Plain(table) => table.seq_lookup_records(),
            Self::Chain(table) => table.seq_lookup_records(),
        }
    }

    /// Returns whether the classes of this rule intersect `glyphs`, and how
    /// many class values the check read, which is what it costs. The check
    /// stops at the first sequence that doesn't intersect, and reads none of
    /// the lookup records.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn intersects(
        &self,
        glyphs: &IntSet<GlyphId>,
        input_class_def: &ClassDef,
        backtrack_class_def: Option<&ClassDef>,
        lookahead_class_def: Option<&ClassDef>,
        seq_cache: &mut SeqCache,
    ) -> (bool, usize) {
        match self {
            Self::Plain(table) => table.intersects(glyphs, input_class_def, &mut seq_cache.input),
            Self::Chain(table) => table.intersects(
                glyphs,
                input_class_def,
                backtrack_class_def,
                lookahead_class_def,
                seq_cache,
            ),
        }
    }
}

fn intersects_class(
    class_def: &ClassDef,
    glyphs: &IntSet<GlyphId>,
    class: u16,
    cache: &mut FastHashMap<u16, bool>,
) -> bool {
    *cache
        .entry(class)
        .or_insert_with(|| class_def.intersects_class_glyphs(glyphs, u32::from(class)))
}
impl ClassSequenceRule<'_> {
    fn intersects(
        &self,
        glyphs: &IntSet<GlyphId>,
        input_class_def: &ClassDef,
        cache: &mut FastHashMap<u16, bool>,
    ) -> (bool, usize) {
        let input = self.input_sequence();
        let intersects = input
            .iter()
            .all(|c| intersects_class(input_class_def, glyphs, c.get(), cache));
        (intersects, input.len())
    }
}

impl ChainedClassSequenceRule<'_> {
    #[allow(clippy::too_many_arguments)]
    fn intersects(
        &self,
        glyphs: &IntSet<GlyphId>,
        input_class_def: &ClassDef,
        backtrack_class_def: Option<&ClassDef>,
        lookahead_class_def: Option<&ClassDef>,
        seq_cache: &mut SeqCache,
    ) -> (bool, usize) {
        let input = self.input_sequence();
        let mut read = input.len();
        if !input
            .iter()
            .all(|c| intersects_class(input_class_def, glyphs, c.get(), &mut seq_cache.input))
        {
            return (false, read);
        }

        if let Some(backtrack_class_def) = backtrack_class_def {
            let backtrack = self.backtrack_sequence();
            read += backtrack.len();
            if !backtrack.iter().all(|c| {
                intersects_class(
                    backtrack_class_def,
                    glyphs,
                    c.get(),
                    &mut seq_cache.backtrack,
                )
            }) {
                return (false, read);
            }
        } else if self.backtrack_glyph_count() != 0 {
            return (false, read);
        }

        if let Some(lookahead_class_def) = lookahead_class_def {
            let lookahead = self.lookahead_sequence();
            read += lookahead.len();
            if !lookahead.iter().all(|c| {
                intersects_class(
                    lookahead_class_def,
                    glyphs,
                    c.get(),
                    &mut seq_cache.lookahead,
                )
            }) {
                return (false, read);
            }
        } else if self.lookahead_glyph_count() != 0 {
            return (false, read);
        }
        (true, read)
    }
}

pub(crate) enum ContextFormat3<'a> {
    Plain(SequenceContextFormat3<'a>),
    Chain(ChainedSequenceContextFormat3<'a>),
    PlainWide(SequenceContextFormat6<'a>),
}

impl ContextFormat3<'_> {
    pub(crate) fn input_count(&self) -> usize {
        match self {
            Self::Plain(table) => table.coverages().len(),
            Self::Chain(table) => table.input_coverages().len(),
            Self::PlainWide(table) => table.coverages().len(),
        }
    }

    pub(crate) fn coverage(&self, index: usize) -> Result<CoverageTable<'_>, ReadError> {
        match self {
            Self::Plain(table) => table.coverages().get(index),
            Self::Chain(table) => table.input_coverages().get(index),
            Self::PlainWide(table) => table.coverages().get(index),
        }
    }

    pub(crate) fn lookup_records(&self) -> &[SequenceLookupRecord] {
        match self {
            ContextFormat3::Plain(table) => table.seq_lookup_records(),
            ContextFormat3::Chain(table) => table.seq_lookup_records(),
            ContextFormat3::PlainWide(table) => table.seq_lookup_records(),
        }
    }

    /// Returns whether every coverage table of this subtable intersects
    /// `glyphs`, charging the coverage tables that are looked at after the
    /// first: like any other subtable, one whose first coverage table doesn't
    /// intersect the glyphs costs nothing.
    #[inline(always)]
    fn matches_glyphs(
        &self,
        glyphs: &IntSet<GlyphId>,
        ops_left: &mut usize,
    ) -> Result<bool, ReadError> {
        match self {
            Self::Plain(table) => {
                all_coverages_intersect(table.coverages().iter(), glyphs, ops_left, 1)
            }
            Self::PlainWide(table) => {
                all_coverages_intersect(table.coverages().iter(), glyphs, ops_left, 1)
            }
            Self::Chain(table) => Ok(all_coverages_intersect(
                table.input_coverages().iter(),
                glyphs,
                ops_left,
                1,
            )? && all_coverages_intersect(
                table.backtrack_coverages().iter(),
                glyphs,
                ops_left,
                0,
            )? && all_coverages_intersect(
                table.lookahead_coverages().iter(),
                glyphs,
                ops_left,
                0,
            )?),
        }
    }
}

/// Returns true if every coverage table intersects `glyphs`, charging each
/// one that is looked at, but for the first `uncharged`, to `ops_left`.
///
/// A null offset counts as a coverage that doesn't intersect.
#[inline(always)]
fn all_coverages_intersect<'a>(
    coverages: impl Iterator<Item = Result<CoverageTable<'a>, ReadError>>,
    glyphs: &IntSet<GlyphId>,
    ops_left: &mut usize,
    uncharged: usize,
) -> Result<bool, ReadError> {
    for (i, coverage) in coverages.enumerate() {
        let coverage = match coverage {
            Err(ReadError::NullOffset) => return Ok(false),
            other => other?,
        };
        if i >= uncharged {
            charge_records(ops_left, 1 + coverage.record_count())?;
        }
        if !coverage.intersects(glyphs) {
            return Ok(false);
        }
    }
    Ok(true)
}

impl Intersect for ContextFormat1<'_> {
    fn intersects(
        &self,
        glyph_set: &IntSet<GlyphId>,
        ops_left: &mut usize,
    ) -> Result<bool, ReadError> {
        let Some(coverage) = self.coverage().transpose()? else {
            return Ok(false);
        };
        charge_records(ops_left, self.rule_set_count())?;
        for rule_set in coverage
            .iter()
            .zip(self.rule_sets())
            .filter_map(|(g, rule_set)| rule_set.filter(|_| glyph_set.contains(g)))
        {
            let rule_set = rule_set?;
            charge_records(ops_left, 1 + rule_set.rule_count())?;
            for rule in rule_set.rules() {
                let Some(rule) = rule.transpose()? else {
                    continue;
                };
                charge_records(ops_left, rule.record_count())?;
                if rule.intersects(glyph_set, ops_left)? {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
}

impl LookupClosure for ContextFormat1<'_> {
    fn closure_lookups(&self, c: &mut LookupClosureCtx, arg: u16) -> Result<(), ReadError> {
        let Some(coverage) = self.coverage().transpose()? else {
            return Ok(());
        };

        c.charge(self.rule_set_count())?;
        for (g, rule_set) in coverage
            .iter()
            .zip(self.rule_sets())
            .filter_map(|(g, rule_set)| rule_set.map(|rs| (g, rs)))
        {
            if !c.glyphs().contains(g) {
                continue;
            }
            if c.lookup_limit_exceed() {
                return Ok(());
            }
            let rule_set = rule_set?;
            c.charge(1 + rule_set.rule_count())?;
            for rule in rule_set.rules() {
                let Some(rule) = rule.transpose()? else {
                    continue;
                };
                rule.closure_lookups(c, arg)?;
            }
        }

        Ok(())
    }
}

impl ContextFormat2<'_> {
    /// What looking up one class in the largest of the class definitions
    /// costs: rules are matched class by class, and each class a rule names
    /// is looked up once.
    fn class_lookup_cost(
        input: &ClassDef,
        backtrack: Option<&ClassDef>,
        lookahead: Option<&ClassDef>,
    ) -> usize {
        [Some(input), backtrack, lookahead]
            .into_iter()
            .flatten()
            .map(ClassDef::record_count)
            .max()
            .unwrap_or_default()
    }
}

impl Intersect for ContextFormat2<'_> {
    fn intersects(
        &self,
        glyph_set: &IntSet<GlyphId>,
        ops_left: &mut usize,
    ) -> Result<bool, ReadError> {
        let Some(coverage) = self.coverage().transpose()? else {
            return Ok(false);
        };
        if !coverage.intersects(glyph_set) {
            return Ok(false);
        }
        let retained_coverage_glyphs = coverage.intersect_set(glyph_set);

        let Some(input_class_def) = self.input_class_def().transpose()? else {
            return Ok(false);
        };

        let backtrack_class_def = self.backtrack_class_def().transpose()?;
        let lookahead_class_def = self.lookahead_class_def().transpose()?;
        let class_lookup_cost = Self::class_lookup_cost(
            &input_class_def,
            backtrack_class_def.as_ref(),
            lookahead_class_def.as_ref(),
        );
        charge_records(
            ops_left,
            coverage.record_count() + input_class_def.record_count() + self.rule_set_count(),
        )?;

        let primary_classes = input_class_def.intersect_classes(&retained_coverage_glyphs);
        let mut seq_cache = SeqCache::default();
        for rule_set in self.rule_sets().enumerate().filter_map(|(c, rule_set)| {
            primary_classes
                .contains(c as u32)
                .then_some(rule_set)
                .flatten()
        }) {
            let rule_set = rule_set?;
            charge_records(ops_left, 1 + rule_set.rule_count())?;
            for rule in rule_set.rules() {
                let Some(rule) = rule.transpose()? else {
                    continue;
                };
                let cached_classes = seq_cache.class_count();
                let (intersects, classes_read) = rule.intersects(
                    glyph_set,
                    &input_class_def,
                    backtrack_class_def.as_ref(),
                    lookahead_class_def.as_ref(),
                    &mut seq_cache,
                );
                let class_lookups = seq_cache.class_count() - cached_classes;
                charge_records(
                    ops_left,
                    1 + classes_read + class_lookups * class_lookup_cost,
                )?;
                if intersects {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
}

impl LookupClosure for ContextFormat2<'_> {
    fn closure_lookups(&self, c: &mut LookupClosureCtx, _arg: u16) -> Result<(), ReadError> {
        let Some(coverage) = self.coverage().transpose()? else {
            return Ok(());
        };
        if !coverage.intersects(c.glyphs()) {
            return Ok(());
        }
        let retained_coverage_glyphs = coverage.intersect_set(c.glyphs());

        let Some(input_class_def) = self.input_class_def().transpose()? else {
            return Ok(());
        };

        let backtrack_class_def = self.backtrack_class_def().transpose()?;
        let lookahead_class_def = self.lookahead_class_def().transpose()?;
        let class_lookup_cost = Self::class_lookup_cost(
            &input_class_def,
            backtrack_class_def.as_ref(),
            lookahead_class_def.as_ref(),
        );
        c.charge(coverage.record_count() + input_class_def.record_count() + self.rule_set_count())?;

        let primary_classes = input_class_def.intersect_classes(&retained_coverage_glyphs);
        let mut seq_cache = SeqCache::default();
        for rule_set in self.rule_sets().enumerate().filter_map(|(c, rule_set)| {
            primary_classes
                .contains(c as u32)
                .then_some(rule_set)
                .flatten()
        }) {
            if c.lookup_limit_exceed() {
                return Ok(());
            }

            let rule_set = rule_set?;
            c.charge(1 + rule_set.rule_count())?;
            for rule in rule_set.rules() {
                if c.lookup_limit_exceed() {
                    return Ok(());
                }
                let Some(rule) = rule.transpose()? else {
                    continue;
                };

                let cached_classes = seq_cache.class_count();
                let (intersects, classes_read) = rule.intersects(
                    c.glyphs(),
                    &input_class_def,
                    backtrack_class_def.as_ref(),
                    lookahead_class_def.as_ref(),
                    &mut seq_cache,
                );
                let class_lookups = seq_cache.class_count() - cached_classes;
                c.charge(1 + classes_read + class_lookups * class_lookup_cost)?;
                if !intersects {
                    continue;
                }

                let lookup_records = rule.lookup_records();
                c.charge(lookup_records.len())?;
                for lookup_record in lookup_records {
                    let index = lookup_record.lookup_list_index();
                    c.recurse(index)?;
                }
            }
        }
        Ok(())
    }
}

impl Intersect for ContextFormat3<'_> {
    #[inline(always)]
    fn intersects(
        &self,
        glyph_set: &IntSet<GlyphId>,
        ops_left: &mut usize,
    ) -> Result<bool, ReadError> {
        self.matches_glyphs(glyph_set, ops_left)
    }
}

impl LookupClosure for ContextFormat3<'_> {
    fn closure_lookups(&self, c: &mut LookupClosureCtx, _arg: u16) -> Result<(), ReadError> {
        if !c.intersects(self)? {
            return Ok(());
        }

        c.charge(self.lookup_records().len())?;
        for lookup_record in self.lookup_records() {
            let index = lookup_record.lookup_list_index();
            c.recurse(index)?;
        }

        Ok(())
    }
}

impl Intersect for SequenceContext<'_> {
    #[inline]
    fn intersects(
        &self,
        glyph_set: &IntSet<GlyphId>,
        ops_left: &mut usize,
    ) -> Result<bool, ReadError> {
        match self {
            Self::Format1(table) => {
                ContextFormat1::Plain(table.clone()).intersects(glyph_set, ops_left)
            }
            Self::Format2(table) => {
                ContextFormat2::Plain(table.clone()).intersects(glyph_set, ops_left)
            }
            Self::Format5(table) => {
                ContextFormat2::PlainWide(table.clone()).intersects(glyph_set, ops_left)
            }
            Self::Format3(table) => {
                ContextFormat3::Plain(table.clone()).intersects(glyph_set, ops_left)
            }
            Self::Format4(table) => {
                ContextFormat1::PlainWide(table.clone()).intersects(glyph_set, ops_left)
            }
            Self::Format6(table) => {
                ContextFormat3::PlainWide(table.clone()).intersects(glyph_set, ops_left)
            }
        }
    }
}

impl LookupClosure for SequenceContext<'_> {
    fn closure_lookups(&self, c: &mut LookupClosureCtx, arg: u16) -> Result<(), ReadError> {
        match self {
            Self::Format1(table) => ContextFormat1::Plain(table.clone()).closure_lookups(c, arg),
            Self::Format2(table) => ContextFormat2::Plain(table.clone()).closure_lookups(c, arg),
            Self::Format5(table) => {
                ContextFormat2::PlainWide(table.clone()).closure_lookups(c, arg)
            }
            Self::Format3(table) => ContextFormat3::Plain(table.clone()).closure_lookups(c, arg),
            Self::Format4(table) => {
                ContextFormat1::PlainWide(table.clone()).closure_lookups(c, arg)
            }
            Self::Format6(table) => {
                ContextFormat3::PlainWide(table.clone()).closure_lookups(c, arg)
            }
        }
    }
}

impl Intersect for ChainedSequenceContext<'_> {
    #[inline(always)]
    fn intersects(
        &self,
        glyph_set: &IntSet<GlyphId>,
        ops_left: &mut usize,
    ) -> Result<bool, ReadError> {
        match self {
            Self::Format1(table) => {
                ContextFormat1::Chain(table.clone()).intersects(glyph_set, ops_left)
            }
            Self::Format2(table) => {
                ContextFormat2::Chain(table.clone()).intersects(glyph_set, ops_left)
            }
            Self::Format5(table) => {
                ContextFormat2::ChainWide(table.clone()).intersects(glyph_set, ops_left)
            }
            Self::Format3(table) => {
                ContextFormat3::Chain(table.clone()).intersects(glyph_set, ops_left)
            }
            Self::Format4(table) => {
                ContextFormat1::ChainWide(table.clone()).intersects(glyph_set, ops_left)
            }
        }
    }
}

impl LookupClosure for ChainedSequenceContext<'_> {
    fn closure_lookups(&self, c: &mut LookupClosureCtx, arg: u16) -> Result<(), ReadError> {
        match self {
            Self::Format1(table) => ContextFormat1::Chain(table.clone()).closure_lookups(c, arg),
            Self::Format2(table) => ContextFormat2::Chain(table.clone()).closure_lookups(c, arg),
            Self::Format5(table) => {
                ContextFormat2::ChainWide(table.clone()).closure_lookups(c, arg)
            }
            Self::Format3(table) => ContextFormat3::Chain(table.clone()).closure_lookups(c, arg),
            Self::Format4(table) => {
                ContextFormat1::ChainWide(table.clone()).closure_lookups(c, arg)
            }
        }
    }
}

#[cfg(test)]
mod lookup_variations_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{limits::max_ops, tables::gsub::Gsub, FontData};
    use font_test_data::bebuffer::BeBuffer;

    #[test]
    fn classdef_format1_short_read_no_panic() {
        // glyph_count is 5, but only one class value is present.
        let classdef = ClassDefFormat1::read(FontData::new(&[0, 1, 0, 10, 0, 5, 0, 1])).unwrap();
        let glyphs: IntSet<GlyphId> = [GlyphId::new(14)].into_iter().collect();
        let mut unlimited = usize::MAX;

        assert!(!classdef.intersects(&glyphs, &mut unlimited).unwrap());
    }

    #[test]
    fn classdef_format2_does_not_alias_wide_glyphs() {
        let bytes = [
            0, 2, 0, 3, 0, 1, 0, 1, 0, 1, 0, 2, 0, 2, 0, 1, 0, 3, 0, 3, 0, 1,
        ];
        let classdef = ClassDefFormat2::read(FontData::new(&bytes)).unwrap();
        let glyphs: IntSet<GlyphId> = [GlyphId::new(65537)].into_iter().collect();
        let mut unlimited = usize::MAX;
        assert!(!classdef.intersects(&glyphs, &mut unlimited).unwrap());
        assert!(!ClassDef::Format2(classdef)
            .intersects(&glyphs, &mut unlimited)
            .unwrap());
    }

    // Two tables, each the target of `count` offsets: a FeatureTableSubstitution and its
    // alternate Feature, or else a FeatureLookups and its LookupIndexList.
    fn feature_variations_with_shared_tables(count: u16, lookup_variations: bool) -> Vec<u8> {
        let n = u32::from(count);
        let mut buf = BeBuffer::new();
        if lookup_variations {
            buf = buf.extend([1u16, 1]).extend([0, n]);
            for _ in 0..n {
                buf = buf.push(0u16).push(12 + 6 * n);
            }
            buf = buf.extend([1u16, 0, 0]).push(n);
            for _ in 0..n {
                buf = buf.extend([0, 10 + 8 * n]);
            }
        } else {
            buf = buf.extend([1u16, 0]).push(n);
            for _ in 0..n {
                buf = buf.extend([0, 8 + 8 * n]);
            }
            buf = buf.extend([1u16, 0, count]);
            for _ in 0..n {
                buf = buf.push(0u16).push(6 + 6 * n);
            }
            buf = buf.push(0u16);
        }
        buf.push(count).extend(0..count).to_vec()
    }

    #[test]
    fn feature_variations_collect_lookups_is_bounded_by_table_size() {
        for lookup_variations in [false, true] {
            let bytes = feature_variations_with_shared_tables(4, lookup_variations);
            let table = FeatureVariations::read(FontData::new(&bytes)).unwrap();
            let mut ops_left = max_ops(bytes.len());
            let lookups = table
                .collect_lookups(&IntSet::all(), &mut ops_left)
                .unwrap();
            assert!(lookups.iter().eq(0..4));

            // 200 * 200 reads of a ~400 byte table: far more than the table's budget
            let bytes = feature_variations_with_shared_tables(200, lookup_variations);
            let table = FeatureVariations::read(FontData::new(&bytes)).unwrap();
            let mut ops_left = max_ops(bytes.len());
            assert_eq!(
                table.collect_lookups(&IntSet::all(), &mut ops_left),
                Err(MAX_OPS_EXCEEDED)
            );
        }
    }

    // A FeatureList whose `count` records all point at one Feature listing lookups `0..400`
    fn feature_list_with_shared_feature(count: u16) -> Vec<u8> {
        let mut buf = BeBuffer::new().push(count);
        for _ in 0..count {
            buf = buf.push(Tag::new(b"liga")).push(2 + 6 * count);
        }
        buf.extend([0u16, 400]).extend(0..400u16).to_vec()
    }

    #[test]
    fn feature_list_collect_lookups_is_bounded_by_table_size() {
        let bytes = feature_list_with_shared_feature(4);
        let list = FeatureList::read(FontData::new(&bytes)).unwrap();
        for features in [IntSet::all(), (0..4).collect()] {
            let mut ops_left = max_ops(bytes.len());
            let lookups = list.collect_lookups(&features, &mut ops_left).unwrap();
            assert!(lookups.iter().eq(0..400));
        }

        // 400 reads of an 804 byte Feature exceed the budget of a 3206 byte list
        let bytes = feature_list_with_shared_feature(400);
        let list = FeatureList::read(FontData::new(&bytes)).unwrap();
        for features in [IntSet::all(), (0..400).collect()] {
            let mut ops_left = max_ops(bytes.len());
            assert_eq!(
                list.collect_lookups(&features, &mut ops_left),
                Err(MAX_OPS_EXCEEDED)
            );
        }
    }

    // The budget comes from the whole GSUB table, as in HarfBuzz's sanitizer, not from the
    // bytes between the FeatureList and the end of the table.
    #[test]
    fn gsub_collect_lookups_is_bounded_by_the_whole_table() {
        // a GSUB header and `padding` bytes, then the 3206 byte FeatureList
        let gsub = |padding: u16| {
            let mut bytes = BeBuffer::new()
                .extend([1u16, 0, 0, 10 + padding, 0])
                .to_vec();
            bytes.resize(10 + usize::from(padding), 0);
            bytes.extend(feature_list_with_shared_feature(400));
            bytes
        };
        // 400 reads of the 804 byte Feature are within 64 times 5216 bytes
        let bytes = gsub(2000);
        let table = Gsub::read(FontData::new(&bytes)).unwrap();
        let lookups = table.collect_lookups(&IntSet::all()).unwrap();
        assert!(lookups.iter().eq(0..400));

        // but not within 64 times 3216 bytes
        let bytes = gsub(0);
        let table = Gsub::read(FontData::new(&bytes)).unwrap();
        assert_eq!(table.collect_lookups(&IntSet::all()), Err(MAX_OPS_EXCEEDED));
    }
}
