//! Counts what the tables of GSUB, GPOS and GDEF reach through their offsets

use super::{for_each_intersected_coverage_index, OpsBudget, OpsWalk};
use read_fonts::{
    collections::IntSet,
    tables::{
        gdef::{
            AttachList, AttachPoint, Gdef, LigCaretList, LigCaretList2, LigGlyph, MarkGlyphSets,
            SelectedLigCaretList,
        },
        gpos::{
            self, BaseArray, BaseArray2, CursivePos, CursivePosFormat1, CursivePosFormat2,
            LigatureAttach, LigatureAttach2, Mark2Array, Mark2Array2, MarkArray, MarkArray2,
            MarkBasePos, MarkBasePosFormat1, MarkBasePosFormat2, MarkLigPos, MarkLigPosFormat1,
            MarkLigPosFormat2, MarkMarkPos, MarkMarkPosFormat1, MarkMarkPosFormat2, PairPos,
            PairPosFormat1, PairPosFormat2, PairPosFormat3, PairPosFormat4, PairSet, PairSet2,
            SinglePos, SinglePosFormat1, SinglePosFormat2, SinglePosFormat3, SinglePosFormat4,
        },
        gsub::{
            self, AlternateSet, AlternateSet2, AlternateSubst, AlternateSubstFormat1,
            AlternateSubstFormat2, Ligature, Ligature2, LigatureSet, LigatureSet2, LigatureSubst,
            LigatureSubstFormat1, LigatureSubstFormat2, MultipleSubst, MultipleSubstFormat1,
            MultipleSubstFormat2, ReverseChainSingleSubst, ReverseChainSingleSubstFormat1,
            ReverseChainSingleSubstFormat2, Sequence, Sequence2, SingleSubst, SingleSubstFormat1,
            SingleSubstFormat2, SingleSubstFormat3, SingleSubstFormat4,
        },
        layout::{
            ChainedClassSequenceRule, ChainedClassSequenceRuleSet, ChainedClassSequenceRuleSet2,
            ChainedSequenceContext, ChainedSequenceContextFormat1, ChainedSequenceContextFormat2,
            ChainedSequenceContextFormat3, ChainedSequenceContextFormat4,
            ChainedSequenceContextFormat5, ChainedSequenceRule, ChainedSequenceRule2,
            ChainedSequenceRuleSet, ChainedSequenceRuleSet2, ClassDef, ClassSequenceRule,
            ClassSequenceRuleSet, ClassSequenceRuleSet2, CoverageTable, SequenceContext,
            SequenceContextFormat1, SequenceContextFormat2, SequenceContextFormat3,
            SequenceContextFormat4, SequenceContextFormat5, SequenceContextFormat6, SequenceRule,
            SequenceRule2, SequenceRuleSet, SequenceRuleSet2,
        },
    },
    types::{GlyphId, Scalar},
    ArrayOfNullableOffsets, ArrayOfOffsets, FontRead, Offset, ReadError,
};

/// Counts everything a GDEF table reaches through its offsets, as often as
/// it is reached, against a walk of `budget`.
///
/// HarfBuzz's sanitizer walks a table like this before it subsets it, and
/// rejects one that makes it read more than its budget.
///
/// Returns false if the walk ran out of budget.
pub(crate) fn count_gdef_ops(gdef: &Gdef, budget: &mut OpsBudget) -> bool {
    let walk = &mut budget.walk();
    // GDEF is small: all of it is counted, whatever the glyphs of the subset.
    let glyphs = &IntSet::all();
    gdef.glyph_class_def().count_all(walk, glyphs)
        && gdef.attach_list().count_all(walk, glyphs)
        && gdef.lig_caret_list().count_all(walk, glyphs)
        && gdef.mark_attach_class_def().count_all(walk, glyphs)
        && gdef.mark_glyph_sets_def().count_all(walk, glyphs)
}

/// A table that counts its bytes, and those of every table it reaches through
/// its offsets, as often as it is reached, against the budget of a walk.
///
/// Subsetting a lookup subtable reads all the tables below it once for every
/// offset that leads to them. Without a bound on what the offsets share,
/// lookups of a few kilobytes can keep the subsetter busy for minutes, so a
/// walk counts a subtable before it reads what the subtable reaches.
/// HarfBuzz's sanitizer counts the same for every subtable of a table before
/// it subsets any of them.
///
/// Unlike the sanitizer, this doesn't follow the offsets to anchors, caret
/// values and device tables, which is most of the offsets of a GPOS table.
/// The first two are a few bytes each, so the offset that leads to one is
/// count enough, and device tables are counted when they are copied, by
/// [`Serializer::check_ops`](crate::serialize::Serializer::check_ops).
pub(crate) trait CountOps {
    /// Counts what a walk reads of this table for a subset of `glyphs`. False
    /// once the walk is out of budget.
    fn count_ops(&self, walk: &mut OpsWalk, glyphs: &IntSet<GlyphId>) -> bool;
}

/// What offsets resolve to. A null offset, or one whose target can't be read,
/// leads to nothing that a later walk could read either.
trait Targets {
    /// Counts each target; false once the walk is out of budget.
    fn count_all(self, walk: &mut OpsWalk, glyphs: &IntSet<GlyphId>) -> bool;
}

impl<T: CountOps> Targets for Result<T, ReadError> {
    fn count_all(self, walk: &mut OpsWalk, glyphs: &IntSet<GlyphId>) -> bool {
        self.map_or(true, |table| table.count_ops(walk, glyphs))
    }
}

impl<T: CountOps> Targets for Option<Result<T, ReadError>> {
    fn count_all(self, walk: &mut OpsWalk, glyphs: &IntSet<GlyphId>) -> bool {
        self.is_none_or(|table| table.count_all(walk, glyphs))
    }
}

impl<'a, T, O> Targets for ArrayOfOffsets<'a, T, O>
where
    T: CountOps + FontRead<'a> + 'a,
    T::Args: Copy + 'static,
    O: Scalar + Offset,
{
    fn count_all(self, walk: &mut OpsWalk, glyphs: &IntSet<GlyphId>) -> bool {
        self.iter().all(|table| table.count_all(walk, glyphs))
    }
}

impl<'a, T, O> Targets for ArrayOfNullableOffsets<'a, T, O>
where
    T: CountOps + FontRead<'a> + 'a,
    T::Args: Copy + 'static,
    O: Scalar + Offset,
{
    fn count_all(self, walk: &mut OpsWalk, glyphs: &IntSet<GlyphId>) -> bool {
        self.iter().all(|table| table.count_all(walk, glyphs))
    }
}

/// Tables that count their own bytes, and then the targets of the offsets
/// that the listed methods resolve.
macro_rules! count_ops {
    ($($table:ident $(-> $($targets:ident),+)?;)+) => {$(
        count_ops!(@impl $table $($($targets),+)?);
    )+};
    (@impl $table:ident) => {
        impl CountOps for $table<'_> {
            fn count_ops(&self, walk: &mut OpsWalk, _glyphs: &IntSet<GlyphId>) -> bool {
                walk.visit(self)
            }
        }
    };
    (@impl $table:ident $($targets:ident),+) => {
        impl CountOps for $table<'_> {
            fn count_ops(&self, walk: &mut OpsWalk, glyphs: &IntSet<GlyphId>) -> bool {
                walk.visit(self) $(&& self.$targets().count_all(walk, glyphs))+
            }
        }
    };
}

/// Tables that are one of several formats.
macro_rules! count_ops_of_format {
    ($($table:ident: $($format:ident),+;)+) => {$(
        impl CountOps for $table<'_> {
            fn count_ops(&self, walk: &mut OpsWalk, glyphs: &IntSet<GlyphId>) -> bool {
                match self {
                    $(Self::$format(table) => table.count_ops(walk, glyphs),)+
                }
            }
        }
    )+};
}

/// Subtables of an extension lookup, which are counted as what they extend to.
macro_rules! count_ops_of_extension {
    ($($table:ty: $($lookup_type:ident),+;)+) => {$(
        impl CountOps for $table {
            fn count_ops(&self, walk: &mut OpsWalk, glyphs: &IntSet<GlyphId>) -> bool {
                match self {
                    $(Self::$lookup_type(table) => table.extension().count_all(walk, glyphs),)+
                }
            }
        }
    )+};
}

count_ops_of_extension! {
    gsub::ExtensionSubtable<'_>: Single, Multiple, Alternate, Ligature, Contextual,
        ChainContextual, Reverse;
    gpos::ExtensionSubtable<'_>: Single, Pair, Cursive, MarkToBase, MarkToLig, MarkToMark,
        Contextual, ChainContextual;
}

count_ops_of_format! {
    SingleSubst: Format1, Format2, Format3, Format4;
    MultipleSubst: Format1, Format2;
    AlternateSubst: Format1, Format2;
    LigatureSubst: Format1, Format2;
    ReverseChainSingleSubst: Format1, Format2;
    SequenceContext: Format1, Format2, Format3, Format4, Format5, Format6;
    ChainedSequenceContext: Format1, Format2, Format3, Format4, Format5;
    SinglePos: Format1, Format2, Format3, Format4;
    PairPos: Format1, Format2, Format3, Format4;
    CursivePos: Format1, Format2;
    MarkBasePos: Format1, Format2;
    MarkLigPos: Format1, Format2;
    MarkMarkPos: Format1, Format2;
    SelectedLigCaretList: Original, Extended;
}

count_ops! {
    CoverageTable;
    ClassDef;

    SingleSubstFormat2 -> coverage;
    SingleSubstFormat4 -> coverage;
    Sequence;
    Sequence2;
    AlternateSet;
    AlternateSet2;
    LigatureSet -> ligatures;
    LigatureSet2 -> ligatures;
    Ligature;
    Ligature2;
    ReverseChainSingleSubstFormat1 -> coverage, backtrack_coverages, lookahead_coverages;
    ReverseChainSingleSubstFormat2 -> coverage, backtrack_coverages, lookahead_coverages;

    SequenceRuleSet -> seq_rules;
    SequenceRuleSet2 -> seq_rules;
    SequenceRule;
    SequenceRule2;
    SequenceContextFormat2 -> coverage, class_def, class_seq_rule_sets;
    SequenceContextFormat5 -> coverage, class_def, class_seq_rule_sets;
    ClassSequenceRuleSet -> class_seq_rules;
    ClassSequenceRuleSet2 -> class_seq_rules;
    ClassSequenceRule;
    SequenceContextFormat3 -> coverages;
    SequenceContextFormat6 -> coverages;

    ChainedSequenceRuleSet -> chained_seq_rules;
    ChainedSequenceRuleSet2 -> chained_seq_rules;
    ChainedSequenceRule;
    ChainedSequenceRule2;
    ChainedSequenceContextFormat2 -> coverage, backtrack_class_def, input_class_def,
        lookahead_class_def, chained_class_seq_rule_sets;
    ChainedSequenceContextFormat5 -> coverage, backtrack_class_def, input_class_def,
        lookahead_class_def, chained_class_seq_rule_sets;
    ChainedClassSequenceRuleSet -> chained_class_seq_rules;
    ChainedClassSequenceRuleSet2 -> chained_class_seq_rules;
    ChainedClassSequenceRule;
    ChainedSequenceContextFormat3 -> backtrack_coverages, input_coverages, lookahead_coverages;

    PairSet;
    PairSet2;
    CursivePosFormat1 -> coverage;
    CursivePosFormat2 -> coverage;
    MarkBasePosFormat1 -> mark_coverage, base_coverage, mark_array, base_array;
    MarkBasePosFormat2 -> mark_coverage, base_coverage, mark_array, base_array;
    MarkMarkPosFormat1 -> mark1_coverage, mark2_coverage, mark1_array, mark2_array;
    MarkMarkPosFormat2 -> mark1_coverage, mark2_coverage, mark1_array, mark2_array;
    MarkArray;
    MarkArray2;
    BaseArray;
    BaseArray2;
    LigatureAttach;
    LigatureAttach2;
    Mark2Array;
    Mark2Array2;

    AttachList -> coverage, attach_points;
    AttachPoint;
    LigCaretList -> coverage, lig_glyphs;
    LigCaretList2 -> coverage, lig_glyphs;
    LigGlyph;
    MarkGlyphSets -> coverages;
}

/// An array of offsets, of which a walk follows the ones it has a use for.
trait Offsets {
    fn offset_count(&self) -> usize;

    /// Counts the target of the offset at `index`; false once the walk is out
    /// of budget.
    fn count_target(&self, index: usize, walk: &mut OpsWalk, glyphs: &IntSet<GlyphId>) -> bool;
}

impl<'a, T, O> Offsets for ArrayOfOffsets<'a, T, O>
where
    T: CountOps + FontRead<'a> + 'a,
    T::Args: Copy + 'static,
    O: Scalar + Offset,
{
    fn offset_count(&self) -> usize {
        self.len()
    }

    fn count_target(&self, index: usize, walk: &mut OpsWalk, glyphs: &IntSet<GlyphId>) -> bool {
        self.get(index).count_all(walk, glyphs)
    }
}

impl<'a, T, O> Offsets for ArrayOfNullableOffsets<'a, T, O>
where
    T: CountOps + FontRead<'a> + 'a,
    T::Args: Copy + 'static,
    O: Scalar + Offset,
{
    fn offset_count(&self) -> usize {
        self.len()
    }

    fn count_target(&self, index: usize, walk: &mut OpsWalk, glyphs: &IntSet<GlyphId>) -> bool {
        self.get(index).count_all(walk, glyphs)
    }
}

/// Counts a coverage table, and the targets of the `offsets` that go with the
/// glyphs it covers, for the glyphs of the subset.
///
/// The walks of the lookups read no other targets: one that belongs to a
/// glyph outside the subset is dropped unread.
fn count_covered_targets(
    coverage: Result<CoverageTable, ReadError>,
    offsets: &impl Offsets,
    walk: &mut OpsWalk,
    glyphs: &IntSet<GlyphId>,
) -> bool {
    let Ok(coverage) = coverage else {
        return true;
    };
    let offset_count = u32::try_from(offsets.offset_count()).unwrap_or(u32::MAX);
    coverage.count_ops(walk, glyphs)
        && for_each_intersected_coverage_index(&coverage, glyphs, offset_count, |index| {
            if offsets.count_target(index as usize, walk, glyphs) {
                Ok(())
            } else {
                Err(())
            }
        })
        .is_ok()
}

/// Subtables with a set of rules, or of the like, for each glyph they cover.
macro_rules! count_ops_of_covered_sets {
    ($($table:ident -> $sets:ident;)+) => {$(
        impl CountOps for $table<'_> {
            fn count_ops(&self, walk: &mut OpsWalk, glyphs: &IntSet<GlyphId>) -> bool {
                walk.visit(self)
                    && count_covered_targets(self.coverage(), &self.$sets(), walk, glyphs)
            }
        }
    )+};
}

count_ops_of_covered_sets! {
    MultipleSubstFormat1 -> sequences;
    MultipleSubstFormat2 -> sequences;
    AlternateSubstFormat1 -> alternate_sets;
    AlternateSubstFormat2 -> alternate_sets;
    LigatureSubstFormat1 -> ligature_sets;
    LigatureSubstFormat2 -> ligature_sets;
    SequenceContextFormat1 -> seq_rule_sets;
    SequenceContextFormat4 -> seq_rule_sets;
    ChainedSequenceContextFormat1 -> chained_seq_rule_sets;
    ChainedSequenceContextFormat4 -> chained_seq_rule_sets;
    PairPosFormat1 -> pair_sets;
    PairPosFormat3 -> pair_sets;
}

/// Mark to ligature positioning, with a table of anchors for each ligature
/// glyph that is covered.
macro_rules! count_ops_of_ligature_attaches {
    ($($table:ident;)+) => {$(
        impl CountOps for $table<'_> {
            fn count_ops(&self, walk: &mut OpsWalk, glyphs: &IntSet<GlyphId>) -> bool {
                walk.visit(self)
                    && self.mark_coverage().count_all(walk, glyphs)
                    && self.mark_array().count_all(walk, glyphs)
                    && self.ligature_array().map_or(true, |ligature_array| {
                        walk.visit(&ligature_array)
                            && count_covered_targets(
                                self.ligature_coverage(),
                                &ligature_array.ligature_attaches(),
                                walk,
                                glyphs,
                            )
                    })
            }
        }
    )+};
}

count_ops_of_ligature_attaches! {
    MarkLigPosFormat1;
    MarkLigPosFormat2;
}

/// Counts a coverage table, and half the glyphs it covers.
///
/// A subtable with nothing but a coverage table is subset glyph by glyph, and
/// a few range records cover any number of glyphs, so HarfBuzz's sanitizer
/// counts the glyphs as well.
/// ref: <https://github.com/harfbuzz/harfbuzz/blob/136b04439/src/OT/Layout/GSUB/SingleSubstFormat1.hh#L126>
fn count_covered_glyphs(
    coverage: Result<CoverageTable, ReadError>,
    walk: &mut OpsWalk,
    glyphs: &IntSet<GlyphId>,
) -> bool {
    coverage.map_or(true, |coverage| {
        coverage.count_ops(walk, glyphs) && walk.charge(coverage.population() / 2)
    })
}

/// Subtables with one delta or value record for all the glyphs they cover.
macro_rules! count_ops_of_covered_glyphs {
    ($($table:ident;)+) => {$(
        impl CountOps for $table<'_> {
            fn count_ops(&self, walk: &mut OpsWalk, glyphs: &IntSet<GlyphId>) -> bool {
                walk.visit(self) && count_covered_glyphs(self.coverage(), walk, glyphs)
            }
        }
    )+};
}

count_ops_of_covered_glyphs! {
    SingleSubstFormat1;
    SingleSubstFormat3;
    SinglePosFormat1;
    SinglePosFormat3;
}

/// Subtables with an array of value records, which take no bytes if their
/// format is empty. Such records are counted one by one.
/// ref: <https://github.com/harfbuzz/harfbuzz/blob/136b04439/src/OT/Layout/GPOS/ValueFormat.hh#L418-L422>
macro_rules! count_ops_of_value_records {
    ($($table:ident;)+) => {$(
        impl CountOps for $table<'_> {
            fn count_ops(&self, walk: &mut OpsWalk, glyphs: &IntSet<GlyphId>) -> bool {
                walk.visit(self)
                    && self.coverage().count_all(walk, glyphs)
                    && (self.value_format().record_byte_len() != 0
                        || walk.charge(self.value_count().into()))
            }
        }
    )+};
}

count_ops_of_value_records! {
    SinglePosFormat2;
    SinglePosFormat4;
}

/// Pair positioning by class, with a pair of value records for every pair of
/// classes. Records of two empty formats are counted one by one.
/// ref: <https://github.com/harfbuzz/harfbuzz/blob/136b04439/src/OT/Layout/GPOS/PairPosFormat2.hh#L182-L189>
macro_rules! count_ops_of_class_pairs {
    ($($table:ident;)+) => {$(
        impl CountOps for $table<'_> {
            fn count_ops(&self, walk: &mut OpsWalk, glyphs: &IntSet<GlyphId>) -> bool {
                let formats = self.value_format1().union(self.value_format2());
                let record_count =
                    usize::from(self.class1_count()).saturating_mul(self.class2_count().into());
                walk.visit(self)
                    && self.coverage().count_all(walk, glyphs)
                    && self.class_def1().count_all(walk, glyphs)
                    && self.class_def2().count_all(walk, glyphs)
                    && (formats.record_byte_len() != 0 || walk.charge(record_count))
            }
        }
    )+};
}

count_ops_of_class_pairs! {
    PairPosFormat2;
    PairPosFormat4;
}

#[cfg(test)]
mod test {
    use super::*;
    use font_test_data::bebuffer::BeBuffer;
    use read_fonts::FontData;

    #[test]
    fn test_count_gdef_ops() {
        // The `count` LigGlyph offsets of a LigCaretList lead to one LigGlyph of 100 carets.
        let within_budget = |count: u16| {
            let data = BeBuffer::new()
                .extend([1u16, 0, 0, 0, 12, 0])
                .extend([210 + 2 * count, count])
                .extend(std::iter::repeat_n(4 + 2 * count, usize::from(count)))
                .push(100u16)
                .extend(std::iter::repeat_n(202u16, 100))
                .extend([1u16, 100])
                .extend([2u16, 1, 1, count, 0]);
            let gdef = Gdef::read(FontData::new(&data)).unwrap();
            count_gdef_ops(&gdef, &mut OpsBudget::new(gdef.offset_data()))
        };

        // 50 reads of the 202 byte LigGlyph are within 64 times the 332 bytes of the table
        assert!(within_budget(50));
        // but 300 reads are not within 64 times 832 bytes
        assert!(!within_budget(300));
    }
}
