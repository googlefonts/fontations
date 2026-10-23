//! the [GPOS] table
//!
//! [GPOS]: https://docs.microsoft.com/en-us/typography/opentype/spec/gpos

include!("../../generated/generated_gpos.rs");

impl ReadArgs for PositionLookupList2 {
    type Args = ();
}

impl<'a> FontRead<'a> for PositionLookupList2 {
    fn read_with_args(data: FontData<'a>, _: ()) -> Result<Self, ReadError> {
        read_fonts::tables::gpos::PositionLookupList2::read(data).map(|x| x.to_owned_table())
    }
}

//use super::layout::value_record::ValueRecord;
use super::{
    layout::{
        ChainedSequenceContext, ClassDef, CoverageTable, DeviceOrVariationIndex, FeatureList,
        FeatureVariations, Lookup, LookupList, LookupSubtable, LookupType, ScriptList,
        SequenceContext, VariationIndex,
    },
    variations::{common_builder::RemapVarStore, ivs_builder::VariationIndexRemapping},
};

#[cfg(test)]
mod extended;
#[cfg(test)]
mod spec_tests;

pub mod builders;
mod value_record;
pub use value_record::ValueRecord;

/// A GPOS lookup list table.
pub type PositionLookupList = LookupList<PositionLookup>;

/// A lookup list with the original 16-bit offsets.
pub type OriginalPositionLookupList = PositionLookupList;

/// A lookup list with 32-bit offsets.
pub type PositionLookupList2 = super::layout::LookupList2<PositionLookup>;

super::layout::table_newtype!(
    PositionSequenceContext,
    SequenceContext,
    read_fonts::tables::layout::SequenceContext<'a>
);

super::layout::table_newtype!(
    PositionChainContext,
    ChainedSequenceContext,
    read_fonts::tables::layout::ChainedSequenceContext<'a>
);

impl Gpos {
    /// Creates a GPOS table using the original 16-bit header offsets.
    pub fn new(
        script_list: ScriptList,
        feature_list: FeatureList,
        lookup_list: PositionLookupList,
    ) -> Self {
        Self {
            script_list: Some(script_list).into(),
            feature_list: Some(feature_list).into(),
            lookup_list: Some(lookup_list).into(),
            ..Default::default()
        }
    }

    fn compute_version(&self) -> MajorMinor {
        if self.script_list2.is_some()
            || self.feature_list2.is_some()
            || self.lookup_list2.is_some()
        {
            MajorMinor::new(1, 2)
        } else if self.feature_variations.is_none() {
            MajorMinor::VERSION_1_0
        } else {
            MajorMinor::VERSION_1_1
        }
    }
}

super::layout::lookup_type!(gpos, SinglePos, 1);
super::layout::lookup_type!(gpos, PairPos, 2);
super::layout::lookup_type!(gpos, CursivePosFormat1, 3);
super::layout::lookup_type!(gpos, MarkBasePosFormat1, 4);
super::layout::lookup_type!(gpos, MarkLigPosFormat1, 5);
super::layout::lookup_type!(gpos, MarkMarkPosFormat1, 6);
super::layout::lookup_type!(gpos, PositionSequenceContext, 7);
super::layout::lookup_type!(gpos, PositionChainContext, 8);
super::layout::lookup_type!(gpos, ExtensionSubtable, 9);

impl<T: LookupSubtable + FontWrite> FontWrite for ExtensionPosFormat1<T> {
    fn write_into(&self, writer: &mut TableWriter) {
        1u16.write_into(writer);
        T::TYPE.write_into(writer);
        self.extension.write_into(writer);
    }
}

// these can't have auto impls because the traits don't support generics
impl ReadArgs for PositionLookup {
    type Args = ();
}

impl<'a> FontRead<'a> for PositionLookup {
    fn read_with_args(data: FontData<'a>, _: ()) -> Result<Self, ReadError> {
        read_fonts::tables::gpos::PositionLookup::read(data).map(|x| x.to_owned_table())
    }
}

impl ReadArgs for PositionLookupList {
    type Args = ();
}

impl<'a> FontRead<'a> for PositionLookupList {
    fn read_with_args(data: FontData<'a>, _: ()) -> Result<Self, ReadError> {
        read_fonts::tables::gpos::OriginalPositionLookupList::read(data).map(|x| x.to_owned_table())
    }
}

impl SinglePosFormat1 {
    fn compute_value_format(&self) -> ValueFormat {
        self.value_record.format()
    }
}

impl SinglePosFormat2 {
    fn compute_value_format(&self) -> ValueFormat {
        self.value_records
            .first()
            .map(ValueRecord::format)
            .unwrap_or(ValueFormat::empty())
    }
}

impl PairPosFormat1 {
    fn compute_value_format1(&self) -> ValueFormat {
        self.pair_sets
            .first()
            .and_then(|pairset| pairset.pair_value_records.first())
            .map(|rec| rec.value_record1.format())
            .unwrap_or(ValueFormat::empty())
    }

    fn compute_value_format2(&self) -> ValueFormat {
        self.pair_sets
            .first()
            .and_then(|pairset| pairset.pair_value_records.first())
            .map(|rec| rec.value_record2.format())
            .unwrap_or(ValueFormat::empty())
    }

    fn check_format_consistency(&self, ctx: &mut ValidationCtx) {
        let vf1 = self.compute_value_format1();
        let vf2 = self.compute_value_format2();
        ctx.with_array_items(self.pair_sets.iter(), |ctx, item| {
            ctx.in_field("pair_value_records", |ctx| {
                if item.pair_value_records.iter().any(|pairset| {
                    pairset.value_record1.format() != vf1 || pairset.value_record2.format() != vf2
                }) {
                    ctx.report("all ValueRecords must have same format")
                }
            })
        })
    }
}

impl PairPosFormat2 {
    fn compute_value_format1(&self) -> ValueFormat {
        self.class1_records
            .first()
            .and_then(|rec| rec.class2_records.first())
            .map(|rec| rec.value_record1.format())
            .unwrap_or(ValueFormat::empty())
    }

    fn compute_value_format2(&self) -> ValueFormat {
        self.class1_records
            .first()
            .and_then(|rec| rec.class2_records.first())
            .map(|rec| rec.value_record2.format())
            .unwrap_or(ValueFormat::empty())
    }

    fn compute_class1_count(&self) -> usize {
        if self.class1_records.is_empty() {
            self.class_def1.max_class() as usize + 1
        } else {
            self.class1_records.len()
        }
    }

    fn compute_class2_count(&self) -> usize {
        self.class1_records
            .first()
            .filter(|record| !record.class2_records.is_empty())
            .map_or_else(
                || self.class_def2.max_class() as usize + 1,
                |record| record.class2_records.len(),
            )
    }

    fn check_length_and_format_conformance(&self, ctx: &mut ValidationCtx) {
        let n_class_1s = self.compute_class1_count();
        let n_class_2s = self.compute_class2_count();
        if n_class_1s > u16::MAX as usize || n_class_2s > u16::MAX as usize {
            ctx.report("class counts must fit in u16");
        }
        if self.class_def1.max_class() as usize >= n_class_1s
            || self.class_def2.max_class() as usize >= n_class_2s
        {
            ctx.report("class definitions must fit within the class record matrix");
        }
        let format_1 = self.compute_value_format1();
        let format_2 = self.compute_value_format2();
        // Readers need not materialize records whose two value formats are
        // empty. In that case infer the minimum dimensions from ClassDefs.
        let empty_records = self
            .class1_records
            .iter()
            .all(|record| record.class2_records.is_empty());
        ctx.in_field("class1_records", |ctx| {
            ctx.with_array_items(self.class1_records.iter(), |ctx, c1rec| {
                if !empty_records && c1rec.class2_records.len() != n_class_2s {
                    ctx.report("class2_records lengths must match the matrix width");
                }
                if c1rec.class2_records.iter().any(|rec| {
                    rec.value_record1.format() != format_1 || rec.value_record2.format() != format_2
                }) {
                    ctx.report("all value records should report the same format");
                }
            })
        });
    }
}

impl MarkBasePosFormat1 {
    fn compute_mark_class_count(&self) -> usize {
        self.base_array.base_records.first().map_or_else(
            || self.mark_array.minimum_class_count(),
            |record| record.base_anchors.len(),
        )
    }

    fn check_class_count_conformance(&self, ctx: &mut ValidationCtx) {
        validate_mark_class_matrix(
            self.mark_array.minimum_class_count(),
            self.compute_mark_class_count(),
            self.base_array
                .base_records
                .iter()
                .map(|record| record.base_anchors.len()),
            ctx,
        );
    }
}

impl MarkMarkPosFormat1 {
    fn compute_mark_class_count(&self) -> usize {
        self.mark2_array.mark2_records.first().map_or_else(
            || self.mark1_array.minimum_class_count(),
            |record| record.mark2_anchors.len(),
        )
    }

    fn check_class_count_conformance(&self, ctx: &mut ValidationCtx) {
        validate_mark_class_matrix(
            self.mark1_array.minimum_class_count(),
            self.compute_mark_class_count(),
            self.mark2_array
                .mark2_records
                .iter()
                .map(|record| record.mark2_anchors.len()),
            ctx,
        );
    }
}

impl MarkLigPosFormat1 {
    fn compute_mark_class_count(&self) -> usize {
        self.ligature_array
            .ligature_attaches
            .iter()
            .flat_map(|attach| attach.component_records.iter())
            .next()
            .map_or_else(
                || self.mark_array.minimum_class_count(),
                |record| record.ligature_anchors.len(),
            )
    }

    fn check_class_count_conformance(&self, ctx: &mut ValidationCtx) {
        validate_mark_class_matrix(
            self.mark_array.minimum_class_count(),
            self.compute_mark_class_count(),
            self.ligature_array
                .ligature_attaches
                .iter()
                .flat_map(|attach| attach.component_records.iter())
                .map(|record| record.ligature_anchors.len()),
            ctx,
        );
    }
}

fn validate_mark_class_matrix(
    minimum_class_count: usize,
    class_count: usize,
    mut row_widths: impl Iterator<Item = usize>,
    ctx: &mut ValidationCtx,
) {
    if class_count > usize::from(u16::MAX) {
        ctx.report("mark class count exceeds u16::MAX");
    }
    if minimum_class_count > class_count {
        ctx.report("mark classes must be less than the mark class count");
    }
    if row_widths.any(|width| width != class_count) {
        ctx.report("anchor rows must all have mark_class_count entries");
    }
}

impl MarkArray {
    // Minimum required count. Anchor rows can include unused trailing classes.
    fn minimum_class_count(&self) -> usize {
        self.mark_records
            .iter()
            .map(|rec| rec.mark_class)
            .max()
            .map_or(0, |class| usize::from(class) + 1)
    }
}

impl RemapVarStore<VariationIndex> for ValueRecord {
    fn remap_variation_indices(&mut self, key_map: &VariationIndexRemapping) {
        for table in [
            self.x_placement_device.as_mut(),
            self.y_placement_device.as_mut(),
            self.x_advance_device.as_mut(),
            self.y_advance_device.as_mut(),
        ]
        .into_iter()
        .flatten()
        {
            table.remap_variation_indices(key_map)
        }
    }
}

impl RemapVarStore<VariationIndex> for DeviceOrVariationIndex {
    fn remap_variation_indices(&mut self, key_map: &VariationIndexRemapping) {
        if let DeviceOrVariationIndex::PendingVariationIndex(table) = self {
            *self = key_map.get(table.delta_set_id).unwrap().into();
        }
    }
}

impl RemapVarStore<VariationIndex> for AnchorTable {
    fn remap_variation_indices(&mut self, key_map: &VariationIndexRemapping) {
        if let AnchorTable::Format3(table) = self {
            table
                .x_device
                .as_mut()
                .into_iter()
                .chain(table.y_device.as_mut())
                .for_each(|x| x.remap_variation_indices(key_map))
        }
    }
}

impl RemapVarStore<VariationIndex> for Gpos {
    fn remap_variation_indices(&mut self, key_map: &VariationIndexRemapping) {
        if let Some(list) = self.lookup_list.as_mut() {
            for lookup in &mut list.lookups {
                lookup.remap_variation_indices(key_map);
            }
        }
        if let Some(list) = self.lookup_list2.as_mut() {
            for lookup in &mut list.lookups {
                lookup.remap_variation_indices(key_map);
            }
        }
    }
}

impl RemapVarStore<VariationIndex> for PositionLookupList {
    fn remap_variation_indices(&mut self, key_map: &VariationIndexRemapping) {
        for lookup in &mut self.lookups {
            lookup.remap_variation_indices(key_map)
        }
    }
}

impl RemapVarStore<VariationIndex> for PositionLookup {
    fn remap_variation_indices(&mut self, key_map: &VariationIndexRemapping) {
        match self {
            PositionLookup::Single(lookup) => lookup.remap_variation_indices(key_map),
            PositionLookup::Pair(lookup) => lookup.remap_variation_indices(key_map),
            PositionLookup::Cursive(lookup) => lookup.remap_variation_indices(key_map),
            PositionLookup::MarkToBase(lookup) => lookup.remap_variation_indices(key_map),
            PositionLookup::MarkToLig(lookup) => lookup.remap_variation_indices(key_map),
            PositionLookup::MarkToMark(lookup) => lookup.remap_variation_indices(key_map),
            PositionLookup::Extension(lookup) => lookup.remap_variation_indices(key_map),

            // don't contain any metrics directly
            PositionLookup::Contextual(_) | PositionLookup::ChainContextual(_) => (),
        }
    }
}

impl RemapVarStore<VariationIndex> for ExtensionSubtable {
    fn remap_variation_indices(&mut self, key_map: &VariationIndexRemapping) {
        match self {
            ExtensionSubtable::Single(table) => table.remap_variation_indices(key_map),
            ExtensionSubtable::Pair(table) => table.remap_variation_indices(key_map),
            ExtensionSubtable::Cursive(table) => table.remap_variation_indices(key_map),
            ExtensionSubtable::MarkToBase(table) => table.remap_variation_indices(key_map),
            ExtensionSubtable::MarkToLig(table) => table.remap_variation_indices(key_map),
            ExtensionSubtable::MarkToMark(table) => table.remap_variation_indices(key_map),

            // don't contain any metrics directly
            ExtensionSubtable::Contextual(_) | ExtensionSubtable::ChainContextual(_) => (),
        }
    }
}

impl<T: RemapVarStore<VariationIndex>> RemapVarStore<VariationIndex> for ExtensionPosFormat1<T> {
    fn remap_variation_indices(&mut self, key_map: &VariationIndexRemapping) {
        self.extension.as_mut().remap_variation_indices(key_map)
    }
}

impl<T: RemapVarStore<VariationIndex>> RemapVarStore<VariationIndex> for Lookup<T> {
    fn remap_variation_indices(&mut self, key_map: &VariationIndexRemapping) {
        for subtable in &mut self.subtables {
            subtable.remap_variation_indices(key_map)
        }
    }
}

impl RemapVarStore<VariationIndex> for SinglePos {
    fn remap_variation_indices(&mut self, key_map: &VariationIndexRemapping) {
        match self {
            SinglePos::Format1(table) => table.remap_variation_indices(key_map),
            SinglePos::Format2(table) => table.remap_variation_indices(key_map),
            SinglePos::Format3(table) => table.remap_variation_indices(key_map),
            SinglePos::Format4(table) => table.remap_variation_indices(key_map),
        }
    }
}

impl RemapVarStore<VariationIndex> for SinglePosFormat1 {
    fn remap_variation_indices(&mut self, key_map: &VariationIndexRemapping) {
        self.value_record.remap_variation_indices(key_map);
    }
}

impl RemapVarStore<VariationIndex> for SinglePosFormat2 {
    fn remap_variation_indices(&mut self, key_map: &VariationIndexRemapping) {
        for rec in &mut self.value_records {
            rec.remap_variation_indices(key_map);
        }
    }
}

impl RemapVarStore<VariationIndex> for PairPosFormat1 {
    fn remap_variation_indices(&mut self, key_map: &VariationIndexRemapping) {
        for pairset in &mut self.pair_sets {
            for pairrec in &mut pairset.pair_value_records {
                pairrec.value_record1.remap_variation_indices(key_map);
                pairrec.value_record2.remap_variation_indices(key_map);
            }
        }
    }
}

impl RemapVarStore<VariationIndex> for PairPosFormat2 {
    fn remap_variation_indices(&mut self, key_map: &VariationIndexRemapping) {
        for class1rec in &mut self.class1_records {
            for class2rec in &mut class1rec.class2_records {
                class2rec.value_record1.remap_variation_indices(key_map);
                class2rec.value_record2.remap_variation_indices(key_map);
            }
        }
    }
}

impl RemapVarStore<VariationIndex> for PairPos {
    fn remap_variation_indices(&mut self, key_map: &VariationIndexRemapping) {
        match self {
            PairPos::Format1(table) => table.remap_variation_indices(key_map),
            PairPos::Format2(table) => table.remap_variation_indices(key_map),
            PairPos::Format3(table) => table.remap_variation_indices(key_map),
            PairPos::Format4(table) => table.remap_variation_indices(key_map),
        }
    }
}

impl RemapVarStore<VariationIndex> for MarkBasePosFormat1 {
    fn remap_variation_indices(&mut self, key_map: &VariationIndexRemapping) {
        self.mark_array.as_mut().remap_variation_indices(key_map);
        for rec in &mut self.base_array.as_mut().base_records {
            for anchor in &mut rec.base_anchors {
                if let Some(anchor) = anchor.as_mut() {
                    anchor.remap_variation_indices(key_map);
                }
            }
        }
    }
}

impl RemapVarStore<VariationIndex> for MarkMarkPosFormat1 {
    fn remap_variation_indices(&mut self, key_map: &VariationIndexRemapping) {
        self.mark1_array.as_mut().remap_variation_indices(key_map);
        for rec in &mut self.mark2_array.as_mut().mark2_records {
            for anchor in &mut rec.mark2_anchors {
                if let Some(anchor) = anchor.as_mut() {
                    anchor.remap_variation_indices(key_map);
                }
            }
        }
    }
}

impl RemapVarStore<VariationIndex> for MarkLigPosFormat1 {
    fn remap_variation_indices(&mut self, key_map: &VariationIndexRemapping) {
        self.mark_array.as_mut().remap_variation_indices(key_map);
        for lig in &mut self.ligature_array.as_mut().ligature_attaches {
            for rec in &mut lig.component_records {
                for anchor in &mut rec.ligature_anchors {
                    if let Some(anchor) = anchor.as_mut() {
                        anchor.remap_variation_indices(key_map);
                    }
                }
            }
        }
    }
}

impl RemapVarStore<VariationIndex> for CursivePosFormat1 {
    fn remap_variation_indices(&mut self, key_map: &VariationIndexRemapping) {
        for rec in &mut self.entry_exit_record {
            for anchor in [rec.entry_anchor.as_mut(), rec.exit_anchor.as_mut()]
                .into_iter()
                .flatten()
            {
                anchor.remap_variation_indices(key_map);
            }
        }
    }
}

impl RemapVarStore<VariationIndex> for MarkArray {
    fn remap_variation_indices(&mut self, key_map: &VariationIndexRemapping) {
        for rec in &mut self.mark_records {
            rec.mark_anchor.remap_variation_indices(key_map);
        }
    }
}

super::layout::lookup_type!(gpos, CursivePos, 3);
super::layout::lookup_type!(gpos, CursivePosFormat2, 3);
super::layout::original_lookup!(PositionLookup, Cursive, CursivePosFormat1, CursivePos);

super::layout::lookup_type!(gpos, MarkBasePos, 4);
super::layout::lookup_type!(gpos, MarkBasePosFormat2, 4);
super::layout::original_lookup!(PositionLookup, MarkToBase, MarkBasePosFormat1, MarkBasePos);

super::layout::lookup_type!(gpos, MarkLigPos, 5);
super::layout::lookup_type!(gpos, MarkLigPosFormat2, 5);
super::layout::original_lookup!(PositionLookup, MarkToLig, MarkLigPosFormat1, MarkLigPos);

super::layout::lookup_type!(gpos, MarkMarkPos, 6);
super::layout::lookup_type!(gpos, MarkMarkPosFormat2, 6);
super::layout::original_lookup!(PositionLookup, MarkToMark, MarkMarkPosFormat1, MarkMarkPos);

impl SinglePosFormat3 {
    fn compute_value_format(&self) -> ValueFormat {
        self.value_record.format()
    }
}

impl SinglePosFormat4 {
    fn compute_value_format(&self) -> ValueFormat {
        self.value_records
            .first()
            .map(ValueRecord::format)
            .unwrap_or(ValueFormat::empty())
    }

    fn check_format_consistency(&self, ctx: &mut ValidationCtx) {
        let format = self.compute_value_format();
        if self
            .value_records
            .iter()
            .any(|record| record.format() != format)
        {
            ctx.report("all ValueRecords must have same format");
        }
    }
}

impl PairPosFormat3 {
    fn compute_value_format1(&self) -> ValueFormat {
        self.pair_sets
            .iter()
            .find_map(|pairset| pairset.pair_value_records.first())
            .map(|rec| rec.value_record1.format())
            .unwrap_or(ValueFormat::empty())
    }

    fn compute_value_format2(&self) -> ValueFormat {
        self.pair_sets
            .iter()
            .find_map(|pairset| pairset.pair_value_records.first())
            .map(|rec| rec.value_record2.format())
            .unwrap_or(ValueFormat::empty())
    }

    fn check_format_consistency(&self, ctx: &mut ValidationCtx) {
        let vf1 = self.compute_value_format1();
        let vf2 = self.compute_value_format2();
        ctx.with_array_items(self.pair_sets.iter(), |ctx, item| {
            ctx.in_field("pair_value_records", |ctx| {
                if item.pair_value_records.iter().any(|pairset| {
                    pairset.value_record1.format() != vf1 || pairset.value_record2.format() != vf2
                }) {
                    ctx.report("all ValueRecords must have same format")
                }
            })
        })
    }
}

impl PairPosFormat4 {
    fn compute_value_format1(&self) -> ValueFormat {
        self.class1_records
            .first()
            .and_then(|rec| rec.class2_records.first())
            .map(|rec| rec.value_record1.format())
            .unwrap_or(ValueFormat::empty())
    }

    fn compute_value_format2(&self) -> ValueFormat {
        self.class1_records
            .first()
            .and_then(|rec| rec.class2_records.first())
            .map(|rec| rec.value_record2.format())
            .unwrap_or(ValueFormat::empty())
    }

    fn compute_class1_count(&self) -> u16 {
        self.class1_records
            .len()
            .try_into()
            .expect("class count exceeds 16 bits")
    }

    fn compute_class2_count(&self) -> u16 {
        self.class1_records
            .first()
            .map_or(0, |row| row.class2_records.len())
            .try_into()
            .expect("class count exceeds 16 bits")
    }

    fn check_length_and_format_conformance(&self, ctx: &mut ValidationCtx) {
        let n_class_1s = self.class1_records.len();
        let n_class_2s = self
            .class1_records
            .first()
            .map_or(0, |row| row.class2_records.len());
        if n_class_1s > u16::MAX as usize || n_class_2s > u16::MAX as usize {
            ctx.report("pair positioning class counts must fit in 16 bits");
        }
        let format_1 = self.compute_value_format1();
        let format_2 = self.compute_value_format2();
        if self
            .class_def1
            .iter()
            .any(|(_, class)| class as usize >= n_class_1s)
            || self
                .class_def2
                .iter()
                .any(|(_, class)| class as usize >= n_class_2s)
        {
            ctx.report("class definitions must index the value record matrix");
        }
        ctx.in_field("class1_records", |ctx| {
            ctx.with_array_items(self.class1_records.iter(), |ctx, c1rec| {
                if c1rec.class2_records.len() != n_class_2s {
                    ctx.report("class2_records length must match number of class2 classes ");
                }
                if c1rec.class2_records.iter().any(|rec| {
                    rec.value_record1.format() != format_1 || rec.value_record2.format() != format_2
                }) {
                    ctx.report("all value records should report the same format");
                }
            })
        });
    }
}

impl MarkBasePosFormat2 {
    fn compute_mark_class_count(&self) -> usize {
        self.base_array.base_records.first().map_or_else(
            || self.mark_array.class_count() as usize,
            |record| record.base_anchors.len(),
        )
    }

    fn check_class_dimensions(&self, ctx: &mut ValidationCtx) {
        validate_mark_class_matrix(
            self.mark_array.class_count() as usize,
            self.compute_mark_class_count(),
            self.base_array
                .base_records
                .iter()
                .map(|record| record.base_anchors.len()),
            ctx,
        );
    }
}

impl MarkMarkPosFormat2 {
    fn compute_mark_class_count(&self) -> usize {
        self.mark2_array.mark2_records.first().map_or_else(
            || self.mark1_array.class_count() as usize,
            |record| record.mark2_anchors.len(),
        )
    }

    fn check_class_dimensions(&self, ctx: &mut ValidationCtx) {
        validate_mark_class_matrix(
            self.mark1_array.class_count() as usize,
            self.compute_mark_class_count(),
            self.mark2_array
                .mark2_records
                .iter()
                .map(|record| record.mark2_anchors.len()),
            ctx,
        );
    }
}

impl MarkLigPosFormat2 {
    fn compute_mark_class_count(&self) -> usize {
        self.ligature_array
            .ligature_attaches
            .iter()
            .flat_map(|attach| attach.component_records.iter())
            .next()
            .map_or_else(
                || self.mark_array.class_count() as usize,
                |record| record.ligature_anchors.len(),
            )
    }

    fn check_class_dimensions(&self, ctx: &mut ValidationCtx) {
        validate_mark_class_matrix(
            self.mark_array.class_count() as usize,
            self.compute_mark_class_count(),
            self.ligature_array
                .ligature_attaches
                .iter()
                .flat_map(|attach| attach.component_records.iter())
                .map(|record| record.ligature_anchors.len()),
            ctx,
        );
    }
}

impl MarkArray2 {
    fn class_count(&self) -> u32 {
        self.mark_records
            .iter()
            .map(|rec| u32::from(rec.mark_class) + 1)
            .max()
            .unwrap_or(0)
    }

    fn check_class_count(&self, ctx: &mut ValidationCtx) {
        if self.class_count() > u16::MAX as u32 {
            ctx.report("mark class count must fit in 16 bits");
        }
    }
}

impl RemapVarStore<VariationIndex> for SinglePosFormat3 {
    fn remap_variation_indices(&mut self, key_map: &VariationIndexRemapping) {
        self.value_record.remap_variation_indices(key_map);
    }
}

impl RemapVarStore<VariationIndex> for SinglePosFormat4 {
    fn remap_variation_indices(&mut self, key_map: &VariationIndexRemapping) {
        for rec in &mut self.value_records {
            rec.remap_variation_indices(key_map);
        }
    }
}

impl RemapVarStore<VariationIndex> for PairPosFormat3 {
    fn remap_variation_indices(&mut self, key_map: &VariationIndexRemapping) {
        for pairset in &mut self.pair_sets {
            for pairrec in &mut pairset.pair_value_records {
                pairrec.value_record1.remap_variation_indices(key_map);
                pairrec.value_record2.remap_variation_indices(key_map);
            }
        }
    }
}

impl RemapVarStore<VariationIndex> for PairPosFormat4 {
    fn remap_variation_indices(&mut self, key_map: &VariationIndexRemapping) {
        for class1rec in &mut self.class1_records {
            for class2rec in &mut class1rec.class2_records {
                class2rec.value_record1.remap_variation_indices(key_map);
                class2rec.value_record2.remap_variation_indices(key_map);
            }
        }
    }
}

impl RemapVarStore<VariationIndex> for MarkBasePosFormat2 {
    fn remap_variation_indices(&mut self, key_map: &VariationIndexRemapping) {
        self.mark_array.as_mut().remap_variation_indices(key_map);
        for rec in &mut self.base_array.as_mut().base_records {
            for anchor in &mut rec.base_anchors {
                if let Some(anchor) = anchor.as_mut() {
                    anchor.remap_variation_indices(key_map);
                }
            }
        }
    }
}

impl RemapVarStore<VariationIndex> for MarkMarkPosFormat2 {
    fn remap_variation_indices(&mut self, key_map: &VariationIndexRemapping) {
        self.mark1_array.as_mut().remap_variation_indices(key_map);
        for rec in &mut self.mark2_array.as_mut().mark2_records {
            for anchor in &mut rec.mark2_anchors {
                if let Some(anchor) = anchor.as_mut() {
                    anchor.remap_variation_indices(key_map);
                }
            }
        }
    }
}

impl RemapVarStore<VariationIndex> for MarkLigPosFormat2 {
    fn remap_variation_indices(&mut self, key_map: &VariationIndexRemapping) {
        self.mark_array.as_mut().remap_variation_indices(key_map);
        for lig in &mut self.ligature_array.as_mut().ligature_attaches {
            for rec in &mut lig.component_records {
                for anchor in &mut rec.ligature_anchors {
                    if let Some(anchor) = anchor.as_mut() {
                        anchor.remap_variation_indices(key_map);
                    }
                }
            }
        }
    }
}

impl RemapVarStore<VariationIndex> for CursivePosFormat2 {
    fn remap_variation_indices(&mut self, key_map: &VariationIndexRemapping) {
        for rec in &mut self.entry_exit_record {
            for anchor in [rec.entry_anchor.as_mut(), rec.exit_anchor.as_mut()]
                .into_iter()
                .flatten()
            {
                anchor.remap_variation_indices(key_map);
            }
        }
    }
}

impl RemapVarStore<VariationIndex> for MarkArray2 {
    fn remap_variation_indices(&mut self, key_map: &VariationIndexRemapping) {
        for rec in &mut self.mark_records {
            rec.mark_anchor.remap_variation_indices(key_map);
        }
    }
}

impl RemapVarStore<VariationIndex> for CursivePos {
    fn remap_variation_indices(&mut self, key_map: &VariationIndexRemapping) {
        match self {
            Self::Format1(t) => t.remap_variation_indices(key_map),
            Self::Format2(t) => t.remap_variation_indices(key_map),
        }
    }
}

impl RemapVarStore<VariationIndex> for MarkBasePos {
    fn remap_variation_indices(&mut self, key_map: &VariationIndexRemapping) {
        match self {
            Self::Format1(t) => t.remap_variation_indices(key_map),
            Self::Format2(t) => t.remap_variation_indices(key_map),
        }
    }
}

impl RemapVarStore<VariationIndex> for MarkMarkPos {
    fn remap_variation_indices(&mut self, key_map: &VariationIndexRemapping) {
        match self {
            Self::Format1(t) => t.remap_variation_indices(key_map),
            Self::Format2(t) => t.remap_variation_indices(key_map),
        }
    }
}

impl RemapVarStore<VariationIndex> for MarkLigPos {
    fn remap_variation_indices(&mut self, key_map: &VariationIndexRemapping) {
        match self {
            Self::Format1(t) => t.remap_variation_indices(key_map),
            Self::Format2(t) => t.remap_variation_indices(key_map),
        }
    }
}

#[cfg(test)]
mod tests {

    use read_fonts::tables::{gpos as read_gpos, layout::LookupFlag};

    use crate::tables::layout::VariationIndex;

    use super::*;

    #[test]
    fn remap_variation_indices_through_extension() {
        use crate::tables::variations::{
            ivs_builder::VariationStoreBuilder, RegionAxisCoordinates, VariationRegion,
        };
        use font_types::F2Dot14;

        let region = VariationRegion::new(vec![RegionAxisCoordinates {
            start_coord: F2Dot14::from_f32(0.0),
            peak_coord: F2Dot14::from_f32(1.0),
            end_coord: F2Dot14::from_f32(1.0),
        }]);
        let mut var_store = VariationStoreBuilder::new(1);
        let temp_id = var_store.add_deltas(vec![(region, -20)]);
        let (_, key_map) = var_store.build();

        let value_record = ValueRecord::new()
            .with_x_advance(-20)
            .with_x_advance_device(DeviceOrVariationIndex::pending_variation_index(temp_id));
        let pair_set = PairSet::new(vec![PairValueRecord::new(
            GlyphId16::new(2),
            value_record,
            ValueRecord::default(),
        )]);
        let pair_pos = PairPos::format_1(
            CoverageTable::format_1(vec![GlyphId16::new(1)]),
            vec![pair_set],
        );

        // the same subtable, in a plain lookup and wrapped in an extension lookup
        let mut plain =
            PositionLookup::Pair(Lookup::new(LookupFlag::default(), vec![pair_pos.clone()]));
        let mut extension = PositionLookup::Extension(Lookup::new(
            LookupFlag::default(),
            vec![ExtensionSubtable::Pair(ExtensionPosFormat1::new(
                <PairPos as LookupSubtable>::TYPE.to_raw(),
                pair_pos,
            ))],
        ));

        plain.remap_variation_indices(&key_map);
        extension.remap_variation_indices(&key_map);

        // writing a PendingVariationIndex panics, so dump_table succeeding is the assertion
        let plain_bytes = crate::dump_table(&plain).unwrap();
        let ext_bytes = crate::dump_table(&extension).unwrap();

        let parsed = read_gpos::PositionLookup::read(FontData::new(&ext_bytes)).unwrap();
        let read_gpos::PositionSubtables::Pair(subtables) = parsed.subtables().unwrap() else {
            panic!("expected a pair lookup");
        };
        let read_gpos::PairPos::Format1(subtable) = subtables.get(0).unwrap() else {
            panic!("expected format 1");
        };
        let pair_set = subtable.pair_sets().get(0).unwrap();
        let record = &pair_set.pair_value_records().get(0).unwrap();
        let read_fonts::tables::layout::DeviceOrVariationIndex::VariationIndex(var_idx) =
            record.value_record1.x_advance_device().unwrap().unwrap()
        else {
            panic!("expected a VariationIndex");
        };
        let expected = key_map.get(temp_id).unwrap();
        assert_eq!(
            (
                var_idx.delta_set_outer_index(),
                var_idx.delta_set_inner_index()
            ),
            (
                expected.delta_set_outer_index,
                expected.delta_set_inner_index
            )
        );
        // the extension wrapper is the only difference, so the inner bytes must match
        assert_eq!(plain_bytes.len() + 8, ext_bytes.len());
    }

    // adapted from/motivated by https://github.com/fonttools/fonttools/issues/471
    #[test]
    fn gpos_1_zero() {
        let cov_one = CoverageTable::format_1(vec![GlyphId16::new(2)]);
        let cov_two = CoverageTable::format_1(vec![GlyphId16::new(4)]);
        let sub1 = SinglePos::format_1(cov_one, ValueRecord::default());
        let sub2 = SinglePos::format_1(cov_two, ValueRecord::default().with_x_advance(500));
        let lookup = Lookup::new(LookupFlag::default(), vec![sub1, sub2]);
        let bytes = crate::dump_table(&lookup).unwrap();

        let parsed = read_gpos::PositionLookup::read(FontData::new(&bytes)).unwrap();
        let read_gpos::PositionLookup::Single(table) = parsed else {
            panic!("something has gone seriously wrong");
        };

        assert_eq!(table.lookup_flag(), LookupFlag::empty());
        assert_eq!(table.sub_table_count(), 2);
        let read_gpos::SinglePos::Format1(sub1) = table.subtables().get(0).unwrap() else {
            panic!("wrong table type");
        };
        let read_gpos::SinglePos::Format1(sub2) = table.subtables().get(1).unwrap() else {
            panic!("wrong table type");
        };

        assert_eq!(sub1.value_format(), ValueFormat::empty());
        assert!(sub1.value_record().is_empty());

        assert_eq!(sub2.value_format(), ValueFormat::X_ADVANCE);
        assert_eq!(sub2.value_record().x_advance(), Some(500));
        assert_eq!(sub2.value_record().y_advance(), None);
    }

    #[test]
    fn pairpos_class_counts_include_unused_slots() {
        let class1 = ClassDef::from_iter([(GlyphId16::new(5), 2)]);
        let class2 = ClassDef::from_iter([(GlyphId16::new(8), 4)]);
        assert_eq!(class1.class_count(), 3);
        assert_eq!(class2.class_count(), 5);
        let records = (0..3)
            .map(|row| {
                Class1Record::new(
                    (0..5)
                        .map(|col| {
                            Class2Record::new(
                                ValueRecord::new().with_x_advance(row * 10 + col),
                                ValueRecord::new(),
                            )
                        })
                        .collect(),
                )
            })
            .collect();
        let table = PairPos::format_2(
            [GlyphId16::new(5)].into_iter().collect(),
            class1,
            class2,
            records,
        );
        let bytes = crate::dump_table(&table).unwrap();
        let read = read_gpos::PairPosFormat2::read(FontData::new(&bytes)).unwrap();
        assert_eq!(read.class1_count(), 3);
        assert_eq!(read.class2_count(), 5);
        assert_eq!(
            read.value_record_refs(2, 4).unwrap()[0].x_advance(),
            Some(24)
        );
        let invalid = ClassDef::from_iter([(GlyphId16::new(8), u16::MAX)]);
        assert_eq!(invalid.class_count(), u32::from(u16::MAX) + 1);
        let mut owned: PairPosFormat2 = read.to_owned_table();
        owned.class_def2 = invalid.into();
        assert!(crate::dump_table(&owned).is_err());
    }

    #[test]
    fn pairpos_roundtrip_preserves_unused_trailing_slots() {
        let table = PairPos::format_2(
            [GlyphId16::new(5)].into_iter().collect(),
            ClassDef::format_1(GlyphId16::new(5), vec![1]),
            ClassDef::format_1(GlyphId16::new(8), vec![1]),
            (0..2)
                .map(|row| {
                    Class1Record::new(
                        (0..2)
                            .map(|col| {
                                Class2Record::new(
                                    ValueRecord::new().with_x_advance(row * 10 + col),
                                    ValueRecord::new(),
                                )
                            })
                            .collect(),
                    )
                })
                .collect(),
        );
        let mut bytes = crate::dump_table(&table).unwrap();
        for field in [8, 10] {
            let offset = u16::from_be_bytes(bytes[field..field + 2].try_into().unwrap()) as usize;
            bytes[offset + 6..offset + 8].copy_from_slice(&0u16.to_be_bytes());
        }
        let read = read_gpos::PairPosFormat2::read(FontData::new(&bytes)).unwrap();
        assert_eq!((read.class1_count(), read.class2_count()), (2, 2));
        assert_eq!(
            read.value_record_refs(1, 1).unwrap()[0].x_advance(),
            Some(11)
        );
        let owned: PairPosFormat2 = read.to_owned_table();
        let output = crate::dump_table(&owned).unwrap();
        let output = read_gpos::PairPosFormat2::read(FontData::new(&output)).unwrap();
        assert_eq!((output.class1_count(), output.class2_count()), (2, 2));
        assert_eq!(
            output.value_record_refs(1, 1).unwrap()[0].x_advance(),
            Some(11)
        );
        let mut invalid = owned.clone();
        invalid.class1_records[1].class2_records.pop();
        assert!(crate::dump_table(&invalid).is_err());
        let mut invalid = owned.clone();
        invalid.class_def1 = ClassDef::format_1(GlyphId16::new(5), vec![2]).into();
        assert!(crate::dump_table(&invalid).is_err());
        let empty = PairPosFormat2::new(
            [GlyphId16::new(5)].into_iter().collect(),
            ClassDef::format_1(GlyphId16::new(5), vec![2]),
            ClassDef::format_1(GlyphId16::new(8), vec![4]),
            vec![],
        );
        let bytes = crate::dump_table(&empty).unwrap();
        let read = read_gpos::PairPosFormat2::read(FontData::new(&bytes)).unwrap();
        assert_eq!((read.class1_count(), read.class2_count()), (3, 5));
        assert_eq!(read.value_format1(), ValueFormat::empty());
    }

    #[test]
    fn mark_class_counts_include_unused_slots() {
        let mark = MarkArray::new(vec![MarkRecord::new(2, AnchorTable::format_1(10, 20))]);
        let anchors = vec![None, None, Some(AnchorTable::format_1(30, 40))];
        let coverage = || [GlyphId16::new(5)].into_iter().collect();
        let base = MarkBasePosFormat1::new(
            coverage(),
            coverage(),
            mark.clone(),
            BaseArray::new(vec![BaseRecord::new(anchors.clone())]),
        );
        let lig = MarkLigPosFormat1::new(
            coverage(),
            coverage(),
            mark.clone(),
            LigatureArray::new(vec![LigatureAttach::new(vec![ComponentRecord::new(
                anchors.clone(),
            )])]),
        );
        let mark2 = MarkMarkPosFormat1::new(
            coverage(),
            coverage(),
            mark,
            Mark2Array::new(vec![Mark2Record::new(anchors)]),
        );
        let bytes = crate::dump_table(&base).unwrap();
        let read = read_gpos::MarkBasePosFormat1::read(FontData::new(&bytes)).unwrap();
        assert_eq!(read.mark_class_count(), 3);
        assert_eq!(
            read.base_array()
                .unwrap()
                .base_records()
                .get(0)
                .unwrap()
                .base_anchors(FontData::new(&bytes))
                .len(),
            3
        );
        let bytes = crate::dump_table(&lig).unwrap();
        assert_eq!(
            read_gpos::MarkLigPosFormat1::read(FontData::new(&bytes))
                .unwrap()
                .mark_class_count(),
            3
        );
        let bytes = crate::dump_table(&mark2).unwrap();
        assert_eq!(
            read_gpos::MarkMarkPosFormat1::read(FontData::new(&bytes))
                .unwrap()
                .mark_class_count(),
            3
        );
    }

    #[test]
    fn extended_marks_preserve_unused_trailing_classes() {
        let marks = MarkArray2::new(vec![MarkRecord2::new(2, AnchorTable::format_1(10, 20))]);
        let coverage = || [GlyphId16::new(5)].into_iter().collect();
        let anchors = || vec![None, None, Some(AnchorTable::format_1(30, 40)), None];
        let base = MarkBasePosFormat2::new(
            coverage(),
            coverage(),
            marks.clone(),
            BaseArray2::new(vec![BaseRecord2::new(anchors())]),
        );
        let lig = MarkLigPosFormat2::new(
            coverage(),
            coverage(),
            marks.clone(),
            LigatureArray2::new(vec![LigatureAttach2::new(vec![ComponentRecord2::new(
                anchors(),
            )])]),
        );
        let mark = MarkMarkPosFormat2::new(
            coverage(),
            coverage(),
            marks,
            Mark2Array2::new(vec![Mark2Record2::new(anchors())]),
        );
        let bytes = crate::dump_table(&base).unwrap();
        let read = read_gpos::MarkBasePosFormat2::read(FontData::new(&bytes)).unwrap();
        assert_eq!(read.mark_class_count(), 4);
        let owned: MarkBasePosFormat2 = read.to_owned_table();
        assert_eq!(owned, base);
        assert!(crate::dump_table(&owned).is_ok());
        let bytes = crate::dump_table(&lig).unwrap();
        let read = read_gpos::MarkLigPosFormat2::read(FontData::new(&bytes)).unwrap();
        assert_eq!(read.mark_class_count(), 4);
        let owned: MarkLigPosFormat2 = read.to_owned_table();
        assert_eq!(owned, lig);
        assert!(crate::dump_table(&owned).is_ok());
        let bytes = crate::dump_table(&mark).unwrap();
        let read = read_gpos::MarkMarkPosFormat2::read(FontData::new(&bytes)).unwrap();
        assert_eq!(read.mark_class_count(), 4);
        let owned: MarkMarkPosFormat2 = read.to_owned_table();
        assert_eq!(owned, mark);
        assert!(crate::dump_table(&owned).is_ok());
        let mut invalid = base.clone();
        invalid
            .base_array
            .base_records
            .push(BaseRecord2::new(vec![None; 3]));
        assert!(crate::dump_table(&invalid).is_err());
        let mut invalid = base;
        invalid.mark_array.mark_records[0].mark_class = 4;
        assert!(crate::dump_table(&invalid).is_err());
    }

    #[test]
    fn mark_attachment_roundtrips_preserve_trailing_unused_classes() {
        fn check_anchors<'a>(
            anchors: read_fonts::ArrayOfNullableOffsets<'a, read_gpos::AnchorTable<'a>, Offset16>,
            class_count: u16,
            expected_x: i16,
        ) {
            assert_eq!(anchors.len(), usize::from(class_count));
            assert_eq!(anchors.get(0).unwrap().unwrap().x_coordinate(), expected_x);
            assert!(anchors.iter().skip(1).all(|anchor| anchor.is_none()));
        }

        for class_count in [2u16, 4] {
            let mark = MarkArray::new(vec![MarkRecord::new(0, AnchorTable::format_1(10, 20))]);
            let anchors = |x| {
                let mut anchors = vec![None; usize::from(class_count)];
                anchors[0] = Some(AnchorTable::format_1(x, 40));
                anchors
            };
            let mark_coverage = || [GlyphId16::new(5)].into_iter().collect();
            let base_coverage = || [GlyphId16::new(7), GlyphId16::new(8)].into_iter().collect();
            let base = MarkBasePosFormat1::new(
                mark_coverage(),
                base_coverage(),
                mark.clone(),
                BaseArray::new(vec![
                    BaseRecord::new(anchors(30)),
                    BaseRecord::new(anchors(50)),
                ]),
            );
            let lig = MarkLigPosFormat1::new(
                mark_coverage(),
                base_coverage(),
                mark.clone(),
                LigatureArray::new(vec![
                    LigatureAttach::new(vec![]),
                    LigatureAttach::new(vec![
                        ComponentRecord::new(anchors(30)),
                        ComponentRecord::new(anchors(50)),
                    ]),
                ]),
            );
            let mark2 = MarkMarkPosFormat1::new(
                mark_coverage(),
                base_coverage(),
                mark,
                Mark2Array::new(vec![
                    Mark2Record::new(anchors(30)),
                    Mark2Record::new(anchors(50)),
                ]),
            );

            let mut bytes = crate::dump_table(&base).unwrap();
            // Set the input's declared count independently of the writer's
            // computed count, reproducing fonts with only class 0 in use.
            bytes[6..8].copy_from_slice(&class_count.to_be_bytes());
            let owned = MarkBasePosFormat1::read(FontData::new(&bytes)).unwrap();
            let bytes = crate::dump_table(&owned).unwrap();
            let read = read_gpos::MarkBasePosFormat1::read(FontData::new(&bytes)).unwrap();
            assert_eq!(read.mark_class_count(), class_count);
            let array = read.base_array().unwrap();
            assert_eq!(array.base_count(), 2);
            for (record, x) in array.base_records().iter().zip([30, 50]) {
                check_anchors(
                    record.unwrap().base_anchors(array.offset_data()),
                    class_count,
                    x,
                );
            }

            let mut bytes = crate::dump_table(&lig).unwrap();
            bytes[6..8].copy_from_slice(&class_count.to_be_bytes());
            let owned = MarkLigPosFormat1::read(FontData::new(&bytes)).unwrap();
            let bytes = crate::dump_table(&owned).unwrap();
            let read = read_gpos::MarkLigPosFormat1::read(FontData::new(&bytes)).unwrap();
            assert_eq!(read.mark_class_count(), class_count);
            let array = read.ligature_array().unwrap();
            assert_eq!(
                array.ligature_attaches().get(0).unwrap().component_count(),
                0
            );
            let attach = array.ligature_attaches().get(1).unwrap();
            assert_eq!(attach.component_count(), 2);
            for (record, x) in attach.component_records().iter().zip([30, 50]) {
                check_anchors(
                    record.unwrap().ligature_anchors(attach.offset_data()),
                    class_count,
                    x,
                );
            }

            let mut bytes = crate::dump_table(&mark2).unwrap();
            bytes[6..8].copy_from_slice(&class_count.to_be_bytes());
            let owned = MarkMarkPosFormat1::read(FontData::new(&bytes)).unwrap();
            let bytes = crate::dump_table(&owned).unwrap();
            let read = read_gpos::MarkMarkPosFormat1::read(FontData::new(&bytes)).unwrap();
            assert_eq!(read.mark_class_count(), class_count);
            let array = read.mark2_array().unwrap();
            assert_eq!(array.mark2_count(), 2);
            for (record, x) in array.mark2_records().iter().zip([30, 50]) {
                check_anchors(
                    record.unwrap().mark2_anchors(array.offset_data()),
                    class_count,
                    x,
                );
            }
        }
    }

    #[test]
    fn mark_attachment_class_matrix_dimensions_are_validated() {
        for (mark_class, widths) in [(0, vec![2, 1]), (1, vec![1]), (u16::MAX, vec![])] {
            let mark = MarkArray::new(vec![MarkRecord::new(
                mark_class,
                AnchorTable::format_1(10, 20),
            )]);
            let coverage = || [GlyphId16::new(5)].into_iter().collect();
            let base_coverage = || {
                (0..widths.len())
                    .map(|index| GlyphId16::new(7 + index as u16))
                    .collect()
            };
            let base = MarkBasePosFormat1::new(
                coverage(),
                base_coverage(),
                mark.clone(),
                BaseArray::new(
                    widths
                        .iter()
                        .map(|&width| BaseRecord::new(vec![None; width]))
                        .collect(),
                ),
            );
            let lig = MarkLigPosFormat1::new(
                coverage(),
                base_coverage(),
                mark.clone(),
                LigatureArray::new(
                    widths
                        .iter()
                        .map(|&width| {
                            LigatureAttach::new(vec![ComponentRecord::new(vec![None; width])])
                        })
                        .collect(),
                ),
            );
            let mark2 = MarkMarkPosFormat1::new(
                coverage(),
                base_coverage(),
                mark,
                Mark2Array::new(
                    widths
                        .iter()
                        .map(|&width| Mark2Record::new(vec![None; width]))
                        .collect(),
                ),
            );
            assert!(crate::dump_table(&base).is_err());
            assert!(crate::dump_table(&lig).is_err());
            assert!(crate::dump_table(&mark2).is_err());
        }
    }

    #[test]
    fn empty_mark_attachment_matrices_use_the_required_class_count() {
        let mark = MarkArray::new(vec![MarkRecord::new(2, AnchorTable::format_1(10, 20))]);
        let coverage = || [GlyphId16::new(5)].into_iter().collect();
        let base = MarkBasePosFormat1::new(
            coverage(),
            CoverageTable::default(),
            mark.clone(),
            BaseArray::default(),
        );
        let lig = MarkLigPosFormat1::new(
            coverage(),
            CoverageTable::default(),
            mark.clone(),
            LigatureArray::default(),
        );
        let mark2 = MarkMarkPosFormat1::new(
            coverage(),
            CoverageTable::default(),
            mark,
            Mark2Array::default(),
        );
        let bytes = crate::dump_table(&base).unwrap();
        assert_eq!(
            read_gpos::MarkBasePosFormat1::read(FontData::new(&bytes))
                .unwrap()
                .mark_class_count(),
            3
        );
        let bytes = crate::dump_table(&lig).unwrap();
        assert_eq!(
            read_gpos::MarkLigPosFormat1::read(FontData::new(&bytes))
                .unwrap()
                .mark_class_count(),
            3
        );
        let bytes = crate::dump_table(&mark2).unwrap();
        assert_eq!(
            read_gpos::MarkMarkPosFormat1::read(FontData::new(&bytes))
                .unwrap()
                .mark_class_count(),
            3
        );
    }

    // shared between a pair of tests below
    fn make_rec(i: u16) -> ValueRecord {
        // '0' here is shorthand for 'no device table'
        if i == 0 {
            return ValueRecord::new().with_explicit_value_format(ValueFormat::X_ADVANCE_DEVICE);
        }
        ValueRecord::new().with_x_advance_device(VariationIndex::new(0xff, i))
    }

    #[test]
    fn compile_devices_pairpos2() {
        let class1 = ClassDef::from_iter([(GlyphId16::new(5), 0), (GlyphId16::new(6), 1)]);
        // class 0 is 'all the rest', here, always implicitly present
        let class2 = ClassDef::from_iter([(GlyphId16::new(8), 1)]);

        // two c1recs, each with two c2recs
        let class1recs = vec![
            Class1Record::new(vec![
                Class2Record::new(make_rec(0), make_rec(0)),
                Class2Record::new(make_rec(1), make_rec(2)),
            ]),
            Class1Record::new(vec![
                Class2Record::new(make_rec(0), make_rec(0)),
                Class2Record::new(make_rec(2), make_rec(3)),
            ]),
        ];
        let coverage = class1.iter().map(|(gid, _)| gid).collect();
        let a_table = PairPos::format_2(coverage, class1, class2, class1recs);

        let bytes = crate::dump_table(&a_table).unwrap();
        let read_back = PairPosFormat2::read(bytes.as_slice().into()).unwrap();

        assert!(read_back.class1_records[0].class2_records[0]
            .value_record1
            .x_advance_device
            .is_none());
        assert!(read_back.class1_records[1].class2_records[1]
            .value_record1
            .x_advance_device
            .is_some());

        let DeviceOrVariationIndex::VariationIndex(dev2) = read_back.class1_records[0]
            .class2_records[1]
            .value_record2
            .x_advance_device
            .as_ref()
            .unwrap()
        else {
            panic!("not a variation index")
        };
        assert_eq!(dev2.delta_set_inner_index, 2);
    }

    #[should_panic(expected = "all value records should report the same format")]
    #[test]
    fn validate_bad_pairpos2() {
        let class1 = ClassDef::from_iter([(GlyphId16::new(5), 0), (GlyphId16::new(6), 1)]);
        // class 0 is 'all the rest', here, always implicitly present
        let class2 = ClassDef::from_iter([(GlyphId16::new(8), 1)]);
        let coverage = class1.iter().map(|(gid, _)| gid).collect();

        // two c1recs, each with two c2recs
        let class1recs = vec![
            Class1Record::new(vec![
                Class2Record::new(make_rec(0), make_rec(0)),
                Class2Record::new(make_rec(1), make_rec(2)),
            ]),
            Class1Record::new(vec![
                Class2Record::new(make_rec(0), make_rec(0)),
                // this is now the wrong type
                Class2Record::new(make_rec(2), make_rec(3).with_x_advance(0x514)),
            ]),
        ];
        let ppf2 = PairPos::format_2(coverage, class1, class2, class1recs);
        crate::dump_table(&ppf2).unwrap();
    }

    #[test]
    fn validate_pairpos1() {
        let coverage: CoverageTable = [1, 2].into_iter().map(GlyphId16::new).collect();
        let good_table = PairPosFormat1::new(
            coverage.clone(),
            vec![
                PairSet::new(vec![PairValueRecord::new(
                    GlyphId16::new(5),
                    ValueRecord::new().with_x_advance(5),
                    ValueRecord::new(),
                )]),
                PairSet::new(vec![PairValueRecord::new(
                    GlyphId16::new(1),
                    ValueRecord::new().with_x_advance(42),
                    ValueRecord::new(),
                )]),
            ],
        );

        let bad_table = PairPosFormat1::new(
            coverage,
            vec![
                PairSet::new(vec![PairValueRecord::new(
                    GlyphId16::new(5),
                    ValueRecord::new().with_x_advance(5),
                    ValueRecord::new(),
                )]),
                PairSet::new(vec![PairValueRecord::new(
                    GlyphId16::new(1),
                    //this is a different format, which is not okay
                    ValueRecord::new().with_x_placement(42),
                    ValueRecord::new(),
                )]),
            ],
        );

        assert!(crate::dump_table(&good_table).is_ok());
        assert!(matches!(
            crate::dump_table(&bad_table),
            Err(crate::error::Error::ValidationFailed(_))
        ));
    }
}
