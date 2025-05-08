//! Support Layout Closure

use types::{BigEndian, GlyphId16};

use super::{
    ArrayOfOffsets, ChainedClassSequenceRule, ChainedClassSequenceRuleSet, ChainedSequenceContext,
    ChainedSequenceContextFormat1, ChainedSequenceContextFormat2, ChainedSequenceContextFormat3,
    ChainedSequenceRule, ChainedSequenceRuleSet, ClassDef, ClassDefFormat1, ClassDefFormat2,
    ClassSequenceRule, ClassSequenceRuleSet, CoverageTable, ExtensionLookup, FeatureList, FontRead,
    GlyphId, LangSys, ReadError, Script, ScriptList, SequenceContext, SequenceContextFormat1,
    SequenceContextFormat2, SequenceContextFormat3, SequenceLookupRecord, SequenceRule,
    SequenceRuleSet, Subtables, Tag,
};
use crate::collections::IntSet;

const MAX_SCRIPTS: u16 = 500;
const MAX_LANGSYS: u16 = 2000;
const MAX_FEATURE_INDICES: u16 = 1500;
const MAX_NESTING_LEVEL: u8 = 64;
const MAX_LOOKUP_VISIT_COUNT: u16 = 35000;

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

#[allow(dead_code)]
pub(crate) struct LookupClosureCtx<'a> {
    visited_lookups: IntSet<u16>,
    inactive_lookups: IntSet<u16>,
    glyph_set: &'a IntSet<GlyphId>,
    lookup_count: u16,
    nesting_level_left: u8,
}

impl<'a> LookupClosureCtx<'a> {
    pub(crate) fn new(glyph_set: &'a IntSet<GlyphId>) -> Self {
        Self {
            visited_lookups: IntSet::empty(),
            inactive_lookups: IntSet::empty(),
            glyph_set,
            lookup_count: 0,
            nesting_level_left: MAX_NESTING_LEVEL,
        }
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

    #[allow(dead_code)]
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
}

/// Compute the transitive closure of lookups
pub(crate) trait LookupClosure {
    fn closure_lookups(&self, _c: &mut LookupClosureCtx, _arg: u16) -> Result<(), ReadError> {
        Ok(())
    }
    fn intersects(&self, glyph_set: &IntSet<GlyphId>) -> Result<bool, ReadError>;
}

impl LookupClosure for ClassDef<'_> {
    fn intersects(&self, glyph_set: &IntSet<GlyphId>) -> Result<bool, ReadError> {
        match self {
            ClassDef::Format1(table) => table.intersects(glyph_set),
            ClassDef::Format2(table) => table.intersects(glyph_set),
        }
    }
}

impl LookupClosure for ClassDefFormat1<'_> {
    fn intersects(&self, glyph_set: &IntSet<GlyphId>) -> Result<bool, ReadError> {
        let glyph_count = self.glyph_count();
        if glyph_count == 0 {
            return Ok(false);
        }

        let start = self.start_glyph_id().to_u32();
        let end = start + glyph_count as u32;

        let start_glyph = GlyphId::from(start);
        let class_values = self.class_value_array();
        if glyph_set.contains(start_glyph) && class_values[0] != 0 {
            return Ok(true);
        }

        while let Some(g) = glyph_set.iter_after(start_glyph).next() {
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
        }
        Ok(false)
    }
}

impl LookupClosure for ClassDefFormat2<'_> {
    fn intersects(&self, glyph_set: &IntSet<GlyphId>) -> Result<bool, ReadError> {
        let num_ranges = self.class_range_count();
        let num_bits = 16 - num_ranges.leading_zeros();
        if num_ranges as u64 > glyph_set.len() * num_bits as u64 {
            for g in glyph_set.iter().map(|g| GlyphId16::from(g.to_u32() as u16)) {
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
    T: LookupClosure + FontRead<'a> + 'a,
    Ext: ExtensionLookup<'a, T> + 'a,
{
    fn intersects(&self, glyph_set: &IntSet<GlyphId>) -> Result<bool, ReadError> {
        for sub in self.iter() {
            if sub?.intersects(glyph_set)? {
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
}

pub(crate) enum Format1RuleSet<'a> {
    Plain(SequenceRuleSet<'a>),
    Chain(ChainedSequenceRuleSet<'a>),
}

pub(crate) enum Format1Rule<'a> {
    Plain(SequenceRule<'a>),
    Chain(ChainedSequenceRule<'a>),
}

impl ContextFormat1<'_> {
    pub(crate) fn coverage(&self) -> Result<CoverageTable, ReadError> {
        match self {
            ContextFormat1::Plain(table) => table.coverage(),
            ContextFormat1::Chain(table) => table.coverage(),
        }
    }

    pub(crate) fn rule_sets(
        &self,
    ) -> impl Iterator<Item = Option<Result<Format1RuleSet, ReadError>>> {
        let (left, right) = match self {
            ContextFormat1::Plain(table) => (
                Some(
                    table
                        .seq_rule_sets()
                        .iter()
                        .map(|rs| rs.map(|rs| rs.map(Format1RuleSet::Plain))),
                ),
                None,
            ),
            ContextFormat1::Chain(table) => (
                None,
                Some(
                    table
                        .chained_seq_rule_sets()
                        .iter()
                        .map(|rs| rs.map(|rs| rs.map(Format1RuleSet::Chain))),
                ),
            ),
        };
        left.into_iter()
            .flatten()
            .chain(right.into_iter().flatten())
    }
}

impl Format1RuleSet<'_> {
    pub(crate) fn rules(&self) -> impl Iterator<Item = Result<Format1Rule, ReadError>> {
        let (left, right) = match self {
            Self::Plain(table) => (
                Some(
                    table
                        .seq_rules()
                        .iter()
                        .map(|rule| rule.map(Format1Rule::Plain)),
                ),
                None,
            ),
            Self::Chain(table) => (
                None,
                Some(
                    table
                        .chained_seq_rules()
                        .iter()
                        .map(|rule| rule.map(Format1Rule::Chain)),
                ),
            ),
        };
        left.into_iter()
            .flatten()
            .chain(right.into_iter().flatten())
    }
}

impl Format1Rule<'_> {
    pub(crate) fn input_sequence(&self) -> &[BigEndian<GlyphId16>] {
        match self {
            Self::Plain(table) => table.input_sequence(),
            Self::Chain(table) => table.input_sequence(),
        }
    }

    pub(crate) fn matches_glyphs(&self, glyphs: &IntSet<GlyphId16>) -> bool {
        let (backtrack, lookahead) = match self {
            Format1Rule::Plain(_) => (None, None),
            Format1Rule::Chain(table) => (
                Some(table.backtrack_sequence()),
                Some(table.lookahead_sequence()),
            ),
        };
        self.input_sequence()
            .iter()
            .chain(backtrack.into_iter().flatten())
            .chain(lookahead.into_iter().flatten())
            .all(|gid| glyphs.contains(gid.get()))
    }

    pub(crate) fn lookup_records(&self) -> &[SequenceLookupRecord] {
        match self {
            Self::Plain(table) => table.seq_lookup_records(),
            Self::Chain(table) => table.seq_lookup_records(),
        }
    }
}

impl LookupClosure for &[BigEndian<GlyphId16>] {
    fn intersects(&self, glyph_set: &IntSet<GlyphId>) -> Result<bool, ReadError> {
        Ok(self
            .iter()
            .all(|g| glyph_set.contains(GlyphId::from(g.get()))))
    }
}

impl LookupClosure for Format1Rule<'_> {
    fn intersects(&self, glyph_set: &IntSet<GlyphId>) -> Result<bool, ReadError> {
        match self {
            Self::Plain(table) => table.input_sequence().intersects(glyph_set),
            Self::Chain(table) => Ok(table.backtrack_sequence().intersects(glyph_set)?
                && table.input_sequence().intersects(glyph_set)?
                && table.lookahead_sequence().intersects(glyph_set)?),
        }
    }
}

pub(crate) enum ContextFormat2<'a> {
    Plain(SequenceContextFormat2<'a>),
    Chain(ChainedSequenceContextFormat2<'a>),
}

pub(crate) enum Format2RuleSet<'a> {
    Plain(ClassSequenceRuleSet<'a>),
    Chain(ChainedClassSequenceRuleSet<'a>),
}

pub(crate) enum Format2Rule<'a> {
    Plain(ClassSequenceRule<'a>),
    Chain(ChainedClassSequenceRule<'a>),
}

impl ContextFormat2<'_> {
    pub(crate) fn coverage(&self) -> Result<CoverageTable<'_>, ReadError> {
        match self {
            ContextFormat2::Plain(table) => table.coverage(),
            ContextFormat2::Chain(table) => table.coverage(),
        }
    }

    pub(crate) fn input_class_def(&self) -> Result<ClassDef<'_>, ReadError> {
        match self {
            ContextFormat2::Plain(table_ref) => table_ref.class_def(),
            ContextFormat2::Chain(table_ref) => table_ref.input_class_def(),
        }
    }

    pub(crate) fn rule_sets(
        &self,
    ) -> impl Iterator<Item = Option<Result<Format2RuleSet, ReadError>>> {
        let (left, right) = match self {
            ContextFormat2::Plain(table) => (
                Some(
                    table
                        .class_seq_rule_sets()
                        .iter()
                        .map(|rs| rs.map(|rs| rs.map(Format2RuleSet::Plain))),
                ),
                None,
            ),
            ContextFormat2::Chain(table) => (
                None,
                Some(
                    table
                        .chained_class_seq_rule_sets()
                        .iter()
                        .map(|rs| rs.map(|rs| rs.map(Format2RuleSet::Chain))),
                ),
            ),
        };
        left.into_iter()
            .flatten()
            .chain(right.into_iter().flatten())
    }
}

impl Format2RuleSet<'_> {
    pub(crate) fn rules(&self) -> impl Iterator<Item = Result<Format2Rule, ReadError>> {
        let (left, right) = match self {
            Format2RuleSet::Plain(table) => (
                Some(
                    table
                        .class_seq_rules()
                        .iter()
                        .map(|rule| rule.map(Format2Rule::Plain)),
                ),
                None,
            ),
            Format2RuleSet::Chain(table) => (
                None,
                Some(
                    table
                        .chained_class_seq_rules()
                        .iter()
                        .map(|rule| rule.map(Format2Rule::Chain)),
                ),
            ),
        };
        left.into_iter()
            .flatten()
            .chain(right.into_iter().flatten())
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

    //TODO: Fix glyph closure: this one is incorrect in case of ChainedContext, replace it with intersects()
    pub(crate) fn matches_classes(&self, classes: &IntSet<u16>) -> bool {
        let (backtrack, lookahead) = match self {
            Self::Plain(_) => (None, None),
            Self::Chain(table) => (
                Some(table.backtrack_sequence()),
                Some(table.lookahead_sequence()),
            ),
        };
        self.input_sequence()
            .iter()
            .chain(backtrack.into_iter().flatten())
            .chain(lookahead.into_iter().flatten())
            .all(|gid| classes.contains(gid.get()))
    }

    pub(crate) fn intersects(
        &self,
        input_classes: &IntSet<u16>,
        backtrack_classes: &IntSet<u16>,
        lookahead_classes: &IntSet<u16>,
    ) -> bool {
        match self {
            Self::Plain(table) => table.intersects(input_classes),
            Self::Chain(table) => {
                table.intersects(input_classes, backtrack_classes, lookahead_classes)
            }
        }
    }
}

impl ClassSequenceRule<'_> {
    fn intersects(&self, input_classes: &IntSet<u16>) -> bool {
        self.input_sequence()
            .iter()
            .all(|c| input_classes.contains(c.get()))
    }
}

impl ChainedClassSequenceRule<'_> {
    fn intersects(
        &self,
        input_classes: &IntSet<u16>,
        backtrack_classes: &IntSet<u16>,
        lookahead_classes: &IntSet<u16>,
    ) -> bool {
        self.input_sequence()
            .iter()
            .all(|c| input_classes.contains(c.get()))
            && self
                .backtrack_sequence()
                .iter()
                .all(|c| backtrack_classes.contains(c.get()))
            && self
                .lookahead_sequence()
                .iter()
                .all(|c| lookahead_classes.contains(c.get()))
    }
}

pub(crate) enum ContextFormat3<'a> {
    Plain(SequenceContextFormat3<'a>),
    Chain(ChainedSequenceContextFormat3<'a>),
}

impl ContextFormat3<'_> {
    pub(crate) fn coverages(&self) -> ArrayOfOffsets<CoverageTable> {
        match self {
            ContextFormat3::Plain(table) => table.coverages(),
            ContextFormat3::Chain(table) => table.input_coverages(),
        }
    }

    pub(crate) fn lookup_records(&self) -> &[SequenceLookupRecord] {
        match self {
            ContextFormat3::Plain(table) => table.seq_lookup_records(),
            ContextFormat3::Chain(table) => table.seq_lookup_records(),
        }
    }

    pub(crate) fn matches_glyphs(&self, glyphs: &IntSet<GlyphId>) -> Result<bool, ReadError> {
        let (backtrack, lookahead) = match self {
            Self::Plain(_) => (None, None),
            Self::Chain(table) => (
                Some(table.backtrack_coverages()),
                Some(table.lookahead_coverages()),
            ),
        };

        for coverage in self
            .coverages()
            .iter()
            .chain(backtrack.into_iter().flat_map(|x| x.iter()))
            .chain(lookahead.into_iter().flat_map(|x| x.iter()))
        {
            if !coverage?.intersects(glyphs) {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

impl LookupClosure for ContextFormat1<'_> {
    fn intersects(&self, glyph_set: &IntSet<GlyphId>) -> Result<bool, ReadError> {
        let coverage = self.coverage()?;
        for rule_set in coverage
            .iter()
            .zip(self.rule_sets())
            .filter_map(|(g, rule_set)| rule_set.filter(|_| glyph_set.contains(GlyphId::from(g))))
        {
            for rule in rule_set?.rules() {
                if rule?.intersects(glyph_set)? {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
}

impl LookupClosure for ContextFormat2<'_> {
    fn intersects(&self, glyph_set: &IntSet<GlyphId>) -> Result<bool, ReadError> {
        let coverage = self.coverage()?;
        let retained_coverage_glyphs = coverage.intersect_set(glyph_set);
        if retained_coverage_glyphs.is_empty() {
            return Ok(false);
        }

        let input_class_def = self.input_class_def()?;
        let coverage_glyph_classes = input_class_def.intersect_classes(&retained_coverage_glyphs);
        let input_glyph_classes = input_class_def.intersect_classes(glyph_set);

        let backtrack_classes = match self {
            Self::Plain(_) => IntSet::empty(),
            Self::Chain(table) => table.backtrack_class_def()?.intersect_classes(glyph_set),
        };

        let lookahead_classes = match self {
            Self::Plain(_) => IntSet::empty(),
            Self::Chain(table) => table.lookahead_class_def()?.intersect_classes(glyph_set),
        };

        for rule_set in self.rule_sets().enumerate().filter_map(|(c, rule_set)| {
            coverage_glyph_classes
                .contains(c as u16)
                .then_some(rule_set)
                .flatten()
        }) {
            for rule in rule_set?.rules() {
                if rule?.intersects(&input_glyph_classes, &backtrack_classes, &lookahead_classes) {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
}

impl LookupClosure for ContextFormat3<'_> {
    fn intersects(&self, glyph_set: &IntSet<GlyphId>) -> Result<bool, ReadError> {
        self.matches_glyphs(glyph_set)
    }
}

impl LookupClosure for SequenceContext<'_> {
    fn intersects(&self, glyph_set: &IntSet<GlyphId>) -> Result<bool, ReadError> {
        match self {
            Self::Format1(table) => ContextFormat1::Plain(table.clone()).intersects(glyph_set),
            Self::Format2(table) => ContextFormat2::Plain(table.clone()).intersects(glyph_set),
            Self::Format3(table) => ContextFormat3::Plain(table.clone()).intersects(glyph_set),
        }
    }
}

impl LookupClosure for ChainedSequenceContext<'_> {
    fn intersects(&self, glyph_set: &IntSet<GlyphId>) -> Result<bool, ReadError> {
        match self {
            Self::Format1(table) => ContextFormat1::Chain(table.clone()).intersects(glyph_set),
            Self::Format2(table) => ContextFormat2::Chain(table.clone()).intersects(glyph_set),
            Self::Format3(table) => ContextFormat3::Chain(table.clone()).intersects(glyph_set),
        }
    }
}
