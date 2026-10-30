//! Bound the expanded owned layout graph while offsets still borrow the input.
//!
//! Count every occurrence, including shared offsets: to_owned_table expands
//! them independently. Arrays and inline records count too, so small offset
//! graphs and zero-sized value records cannot bypass the bound. Keep the field
//! lists aligned with write-fonts' FromObjRef implementations.
use read_fonts::{
    array::{ComputedArray, VarLenArray},
    tables::{gpos, gsub, jstf, layout},
    types::{BigEndian, Scalar},
    ArrayOfNullableOffsets, ArrayOfOffsets, FontData, FontRead, FontReadAt, Offset, ReadError,
    VarSize,
};

type Result = std::result::Result<(), ReadError>;

pub(super) fn check(table: &impl Cost) -> Result {
    table.check_budget(
        FontData::new(&[]),
        &mut Budget {
            remaining: 200_000,
            depth: 0,
        },
    )
}

pub(super) struct Budget {
    remaining: usize,
    depth: usize,
}
impl Budget {
    fn consume(&mut self, count: usize) -> Result {
        self.remaining = self
            .remaining
            .checked_sub(count)
            .ok_or(ReadError::MalformedData("owned layout expansion limit"))?;
        Ok(())
    }
    fn node(&mut self, f: impl FnOnce(&mut Self) -> Result) -> Result {
        self.consume(1)?;
        if self.depth >= 64 {
            return Err(ReadError::MalformedData("owned layout depth limit"));
        }
        self.depth += 1;
        let result = f(self);
        self.depth -= 1;
        result
    }
}
pub(super) trait Cost {
    fn check_budget(&self, data: FontData, budget: &mut Budget) -> Result;
}
impl<T: Cost> Cost for Option<T> {
    fn check_budget(&self, data: FontData, budget: &mut Budget) -> Result {
        if let Some(value) = self {
            value.check_budget(data, budget)?;
        }
        Ok(())
    }
}
impl<T: Cost> Cost for std::result::Result<T, ReadError> {
    fn check_budget(&self, data: FontData, budget: &mut Budget) -> Result {
        // Match FromObjRef: an unresolved offset produces a default marker.
        if let Ok(value) = self {
            value.check_budget(data, budget)?;
        }
        Ok(())
    }
}
impl<T: Cost> Cost for &[T] {
    fn check_budget(&self, data: FontData, budget: &mut Budget) -> Result {
        budget.consume(self.len())?;
        for item in *self {
            item.check_budget(data, budget)?;
        }
        Ok(())
    }
}
impl<T: Scalar> Cost for BigEndian<T> {
    fn check_budget(&self, _: FontData, _: &mut Budget) -> Result {
        Ok(())
    }
}
impl Cost for u8 {
    fn check_budget(&self, _: FontData, _: &mut Budget) -> Result {
        Ok(())
    }
}
impl<'a, T: FontReadAt<'a> + Cost> Cost for ComputedArray<'a, T>
where
    T::Args: Copy + 'static,
{
    fn check_budget(&self, data: FontData, budget: &mut Budget) -> Result {
        for item in self.iter() {
            budget.consume(1)?;
            item.check_budget(data, budget)?;
        }
        Ok(())
    }
}
impl<'a, T: FontRead<'a, Args = ()> + VarSize + Cost> Cost for VarLenArray<'a, T> {
    fn check_budget(&self, data: FontData, budget: &mut Budget) -> Result {
        for item in self.iter() {
            budget.consume(1)?;
            item.check_budget(data, budget)?;
        }
        Ok(())
    }
}
macro_rules! offsets {
    ($ty:ident) => {
        impl<'a, T: FontRead<'a> + Cost, O: Scalar + Offset> Cost for $ty<'a, T, O>
        where
            T::Args: 'static,
        {
            fn check_budget(&self, data: FontData, budget: &mut Budget) -> Result {
                budget.consume(self.len())?;
                for item in self.iter() {
                    item.check_budget(data, budget)?;
                }
                Ok(())
            }
        }
    };
}
offsets!(ArrayOfOffsets);
offsets!(ArrayOfNullableOffsets);
macro_rules! record {
    ($ty:ty; $($field:ident),*) => {
        impl Cost for $ty {
            fn check_budget(&self, data: FontData, budget: &mut Budget) -> Result {
                budget.node(|budget| {
                    $(self.$field().check_budget(data, budget)?;)*
                    Ok(())
                })
            }
        }
    };
}
macro_rules! offset_record {
    ($ty:ty; $($field:ident),*) => {
        impl Cost for $ty {
            fn check_budget(&self, data: FontData, budget: &mut Budget) -> Result {
                budget.node(|budget| {
                    $(self.$field(data).check_budget(data, budget)?;)*
                    Ok(())
                })
            }
        }
    };
}
macro_rules! table {
    ($ty:ty; $($field:ident),*) => {
        table!($ty; $($field),*; obj, budget => {});
    };
    ($ty:ty; $($field:ident),*; $obj:ident, $budget:ident => $extra:block) => {
        impl Cost for $ty {
            fn check_budget(&self, _: FontData, budget: &mut Budget) -> Result {
                let data = self.offset_data();
                let $obj = self;
                budget.node(|$budget| {
                    $extra
                    $($obj.$field().check_budget(data, $budget)?;)*
                    Ok(())
                })
            }
        }
    };
}
macro_rules! leaf {
    ($($ty:ty),* $(,)?) => {
        $(impl Cost for $ty {
            fn check_budget(&self, _: FontData, budget: &mut Budget) -> Result {
                budget.consume(1)
            }
        })*
    };
}
macro_rules! variants {
    ($ty:ty; $($variant:ident),+) => {
        impl Cost for $ty {
            fn check_budget(&self, data: FontData, budget: &mut Budget) -> Result {
                match self { $(Self::$variant(value) => value.check_budget(data, budget),)+ }
            }
        }
    };
}
macro_rules! lookup {
    ($ty:ident; $field:ident) => {
        impl<'a, T: FontRead<'a, Args = ()> + Cost> Cost for layout::$ty<'a, T>
        where
            T::Args: 'static,
        {
            fn check_budget(&self, _: FontData, budget: &mut Budget) -> Result {
                budget.node(|budget| self.$field().check_budget(self.offset_data(), budget))
            }
        }
    };
}
lookup!(Lookup; subtables);
lookup!(LookupList; lookups);
lookup!(LookupList2; lookups);
impl<'a, T: FontRead<'a, Args = ()> + Cost> Cost for gpos::ExtensionPosFormat1<'a, T> {
    fn check_budget(&self, _: FontData, budget: &mut Budget) -> Result {
        budget.node(|budget| self.extension().check_budget(self.offset_data(), budget))
    }
}
impl<'a, T: FontRead<'a, Args = ()> + Cost> Cost for gsub::ExtensionSubstFormat1<'a, T> {
    fn check_budget(&self, _: FontData, budget: &mut Budget) -> Result {
        budget.node(|budget| self.extension().check_budget(self.offset_data(), budget))
    }
}
variants!(layout::FeatureParams<'_>; Size, StylisticSet, CharacterVariant);
record!(gpos::ValueRecord<'_>; x_placement_device, y_placement_device, x_advance_device, y_advance_device);

table!(layout::ScriptList<'_>; script_records);
offset_record!(layout::ScriptRecord; script);
table!(layout::Script<'_>; default_lang_sys, lang_sys_records);
offset_record!(layout::LangSysRecord; lang_sys);
table!(layout::LangSys<'_>; feature_indices);
table!(layout::FeatureList<'_>; feature_records);
offset_record!(layout::FeatureRecord; feature);
table!(layout::Feature<'_>; feature_params, lookup_list_indices);
table!(layout::CoverageFormat1<'_>; glyph_array);
table!(layout::CoverageFormat2<'_>; range_records);
variants!(layout::CoverageTable<'_>; Format1, Format2, Format3, Format4);
table!(layout::ClassDefFormat1<'_>; class_value_array);
table!(layout::ClassDefFormat2<'_>; class_range_records);
variants!(layout::ClassDef<'_>; Format1, Format2, Format3, Format4);
table!(layout::SequenceContextFormat1<'_>; coverage, seq_rule_sets);
table!(layout::SequenceRuleSet<'_>; seq_rules);
table!(layout::SequenceRule<'_>; input_sequence, seq_lookup_records);
table!(layout::SequenceContextFormat2<'_>; coverage, class_def, class_seq_rule_sets);
table!(layout::ClassSequenceRuleSet<'_>; class_seq_rules);
table!(layout::ClassSequenceRule<'_>; input_sequence, seq_lookup_records);
table!(layout::SequenceContextFormat3<'_>; coverages, seq_lookup_records);
variants!(layout::SequenceContext<'_>; Format1, Format2, Format3, Format4, Format5, Format6);
table!(layout::ChainedSequenceContextFormat1<'_>; coverage, chained_seq_rule_sets);
table!(layout::ChainedSequenceRuleSet<'_>; chained_seq_rules);
table!(layout::ChainedSequenceRule<'_>; backtrack_sequence, input_sequence, lookahead_sequence, seq_lookup_records);
table!(layout::ChainedSequenceContextFormat2<'_>; coverage, backtrack_class_def, input_class_def, lookahead_class_def, chained_class_seq_rule_sets);
table!(layout::ChainedClassSequenceRuleSet<'_>; chained_class_seq_rules);
table!(layout::ChainedClassSequenceRule<'_>; backtrack_sequence, input_sequence, lookahead_sequence, seq_lookup_records);
table!(layout::ChainedSequenceContextFormat3<'_>; backtrack_coverages, input_coverages, lookahead_coverages, seq_lookup_records);
variants!(layout::ChainedSequenceContext<'_>; Format1, Format2, Format3, Format4, Format5);
table!(layout::Device<'_>; delta_value);
variants!(layout::DeviceOrVariationIndex<'_>; Device, VariationIndex);
table!(layout::FeatureVariations<'_>; feature_variation_records, lookup_variation_records);
offset_record!(layout::FeatureVariationRecord; condition_set, feature_table_substitution);
offset_record!(layout::LookupVariationRecord; feature_lookups);
table!(layout::FeatureLookups<'_>; lookup_condition_records);
offset_record!(layout::LookupConditionRecord; condition, lookup_index_list);
table!(layout::LookupIndexList<'_>; lookup_indices);
table!(layout::ConditionSet<'_>; conditions);
variants!(layout::Condition<'_>; Format1AxisRange, Format2VariableValue, Format3And, Format4Or, Format5Negate);
table!(layout::ConditionFormat3<'_>; conditions);
table!(layout::ConditionFormat4<'_>; conditions);
table!(layout::ConditionFormat5<'_>; condition);
table!(layout::FeatureTableSubstitution<'_>; substitutions);
offset_record!(layout::FeatureTableSubstitutionRecord; alternate_feature);
table!(layout::CharacterVariantParams<'_>; character);
table!(layout::CoverageFormat3<'_>; glyph_array);
table!(layout::CoverageFormat4<'_>; range_records);
table!(layout::ClassDefFormat3<'_>; class_value_array);
table!(layout::ClassDefFormat4<'_>; class_range_records);
table!(layout::SequenceContextFormat4<'_>; coverage, seq_rule_sets);
table!(layout::SequenceRuleSet2<'_>; seq_rules);
table!(layout::SequenceRule2<'_>; input_sequence, seq_lookup_records);
table!(layout::SequenceContextFormat5<'_>; coverage, class_def, class_seq_rule_sets);
table!(layout::ClassSequenceRuleSet2<'_>; class_seq_rules);
table!(layout::SequenceContextFormat6<'_>; coverages, seq_lookup_records);
table!(layout::ChainedSequenceContextFormat4<'_>; coverage, chained_seq_rule_sets);
table!(layout::ChainedSequenceRuleSet2<'_>; chained_seq_rules);
table!(layout::ChainedSequenceRule2<'_>; backtrack_sequence, input_sequence, lookahead_sequence, seq_lookup_records);
table!(layout::ChainedSequenceContextFormat5<'_>; coverage, backtrack_class_def, input_class_def, lookahead_class_def, chained_class_seq_rule_sets);
table!(layout::ChainedClassSequenceRuleSet2<'_>; chained_class_seq_rules);
table!(gpos::Gpos<'_>; original_script_list, original_feature_list, original_lookup_list, feature_variations, script_list2, feature_list2, lookup_list2);
variants!(gpos::PositionLookup<'_>; Single, Pair, Cursive, MarkToBase, MarkToLig, MarkToMark, Contextual, ChainContextual, Extension);
variants!(gpos::AnchorTable<'_>; Format1, Format2, Format3);
table!(gpos::AnchorFormat3<'_>; x_device, y_device);
table!(gpos::MarkArray<'_>; mark_records);
offset_record!(gpos::MarkRecord; mark_anchor);
variants!(gpos::SinglePos<'_>; Format1, Format2, Format3, Format4);
table!(gpos::SinglePosFormat1<'_>; coverage, value_record);
table!(gpos::SinglePosFormat2<'_>; coverage, value_records; obj, budget => { budget.consume(usize::from(obj.value_count()))?; });
variants!(gpos::PairPos<'_>; Format1, Format2, Format3, Format4);
table!(gpos::PairPosFormat1<'_>; coverage, pair_sets);
table!(gpos::PairSet<'_>; pair_value_records);
record!(gpos::PairValueRecord<'_>; value_record1, value_record2);
table!(gpos::PairPosFormat2<'_>; coverage, class_def1, class_def2, class1_records; obj, budget => { budget.consume((obj.class1_count() as usize).saturating_mul(obj.class2_count() as usize).max(obj.class1_count() as usize))?; });
record!(gpos::Class1Record<'_>; class2_records);
record!(gpos::Class2Record<'_>; value_record1, value_record2);
table!(gpos::CursivePosFormat1<'_>; coverage, entry_exit_record);
offset_record!(gpos::EntryExitRecord; entry_anchor, exit_anchor);
table!(gpos::MarkBasePosFormat1<'_>; mark_coverage, base_coverage, mark_array, base_array);
table!(gpos::BaseArray<'_>; base_records);
offset_record!(gpos::BaseRecord<'_>; base_anchors);
table!(gpos::MarkLigPosFormat1<'_>; mark_coverage, ligature_coverage, mark_array, ligature_array);
table!(gpos::LigatureArray<'_>; ligature_attaches);
table!(gpos::LigatureAttach<'_>; component_records);
offset_record!(gpos::ComponentRecord<'_>; ligature_anchors);
table!(gpos::MarkMarkPosFormat1<'_>; mark1_coverage, mark2_coverage, mark1_array, mark2_array);
table!(gpos::Mark2Array<'_>; mark2_records);
offset_record!(gpos::Mark2Record<'_>; mark2_anchors);
variants!(gpos::ExtensionSubtable<'_>; Single, Pair, Cursive, MarkToBase, MarkToLig, MarkToMark, Contextual, ChainContextual);
table!(gpos::SinglePosFormat3<'_>; coverage, value_record);
table!(gpos::SinglePosFormat4<'_>; coverage, value_records; obj, budget => { budget.consume(usize::from(obj.value_count()))?; });
table!(gpos::PairPosFormat3<'_>; coverage, pair_sets);
table!(gpos::PairSet2<'_>; pair_value_records);
record!(gpos::PairValueRecord2<'_>; value_record1, value_record2);
table!(gpos::PairPosFormat4<'_>; coverage, class_def1, class_def2, class1_records; obj, budget => { budget.consume((obj.class1_count() as usize).saturating_mul(obj.class2_count() as usize).max(obj.class1_count() as usize))?; });
table!(gpos::CursivePosFormat2<'_>; coverage, entry_exit_record);
offset_record!(gpos::EntryExitRecord2; entry_anchor, exit_anchor);
table!(gpos::MarkBasePosFormat2<'_>; mark_coverage, base_coverage, mark_array, base_array);
table!(gpos::BaseArray2<'_>; base_records);
offset_record!(gpos::BaseRecord2<'_>; base_anchors);
table!(gpos::MarkLigPosFormat2<'_>; mark_coverage, ligature_coverage, mark_array, ligature_array);
table!(gpos::LigatureArray2<'_>; ligature_attaches);
table!(gpos::LigatureAttach2<'_>; component_records);
offset_record!(gpos::ComponentRecord2<'_>; ligature_anchors);
table!(gpos::MarkMarkPosFormat2<'_>; mark1_coverage, mark2_coverage, mark1_array, mark2_array);
table!(gpos::Mark2Array2<'_>; mark2_records);
offset_record!(gpos::Mark2Record2<'_>; mark2_anchors);
table!(gpos::MarkArray2<'_>; mark_records);
offset_record!(gpos::MarkRecord2; mark_anchor);
variants!(gpos::CursivePos<'_>; Format1, Format2);
variants!(gpos::MarkBasePos<'_>; Format1, Format2);
variants!(gpos::MarkLigPos<'_>; Format1, Format2);
variants!(gpos::MarkMarkPos<'_>; Format1, Format2);
table!(gsub::Gsub<'_>; original_script_list, original_feature_list, original_lookup_list, feature_variations, script_list2, feature_list2, lookup_list2);
variants!(gsub::SubstitutionLookup<'_>; Single, Multiple, Alternate, Ligature, Contextual, ChainContextual, Extension, Reverse);
variants!(gsub::SingleSubst<'_>; Format1, Format2, Format3, Format4);
table!(gsub::SingleSubstFormat1<'_>; coverage);
table!(gsub::SingleSubstFormat2<'_>; coverage, substitute_glyph_ids);
table!(gsub::MultipleSubstFormat1<'_>; coverage, sequences);
table!(gsub::Sequence<'_>; substitute_glyph_ids);
table!(gsub::AlternateSubstFormat1<'_>; coverage, alternate_sets);
table!(gsub::AlternateSet<'_>; alternate_glyph_ids);
table!(gsub::LigatureSubstFormat1<'_>; coverage, ligature_sets);
table!(gsub::LigatureSet<'_>; ligatures);
table!(gsub::Ligature<'_>; component_glyph_ids);
variants!(gsub::ExtensionSubtable<'_>; Single, Multiple, Alternate, Ligature, Contextual, ChainContextual, Reverse);
table!(gsub::ReverseChainSingleSubstFormat1<'_>; coverage, backtrack_coverages, lookahead_coverages, substitute_glyph_ids);
table!(gsub::SingleSubstFormat3<'_>; coverage);
table!(gsub::SingleSubstFormat4<'_>; coverage, substitute_glyph_ids);
table!(gsub::MultipleSubstFormat2<'_>; coverage, sequences);
table!(gsub::AlternateSubstFormat2<'_>; coverage, alternate_sets);
table!(gsub::LigatureSubstFormat2<'_>; coverage, ligature_sets);
table!(gsub::ReverseChainSingleSubstFormat2<'_>; coverage, backtrack_coverages, lookahead_coverages, substitute_glyph_ids);
table!(gsub::Sequence2<'_>; substitute_glyph_ids);
table!(gsub::AlternateSet2<'_>; alternate_glyph_ids);
table!(gsub::LigatureSet2<'_>; ligatures);
table!(gsub::Ligature2<'_>; component_glyph_ids);
variants!(gsub::MultipleSubst<'_>; Format1, Format2);
variants!(gsub::AlternateSubst<'_>; Format1, Format2);
variants!(gsub::LigatureSubst<'_>; Format1, Format2);
variants!(gsub::ReverseChainSingleSubst<'_>; Format1, Format2);
table!(jstf::Jstf<'_>; jstf_script_records);
offset_record!(jstf::JstfScriptRecord; jstf_script);
table!(jstf::JstfScript<'_>; extender_glyph, def_jstf_lang_sys, jstf_lang_sys_records);
table!(jstf::ExtenderGlyph<'_>; extender_glyphs);
offset_record!(jstf::JstfLangSysRecord; jstf_lang_sys);
table!(jstf::JstfLangSys<'_>; jstf_priorities);
table!(jstf::JstfPriority<'_>; gsub_shrinkage_enable, gsub_shrinkage_disable, gpos_shrinkage_enable, gpos_shrinkage_disable, shrinkage_jstf_max, gsub_extension_enable, gsub_extension_disable, gpos_extension_enable, gpos_extension_disable, extension_jstf_max);
table!(jstf::JstfModList<'_>; lookup_indices);

leaf!(
    layout::RangeRecord,
    layout::ClassRangeRecord,
    layout::SequenceLookupRecord,
    layout::VariationIndex<'_>,
    layout::ConditionFormat1<'_>,
    layout::ConditionFormat2<'_>,
    layout::SizeParams<'_>,
    layout::StylisticSetParams<'_>,
    layout::RangeRecord2,
    layout::ClassRangeRecord2,
    gpos::AnchorFormat1<'_>,
    gpos::AnchorFormat2<'_>,
);
