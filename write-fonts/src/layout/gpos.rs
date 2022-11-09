//! the [GPOS] table
//!
//! [GPOS]: https://docs.microsoft.com/en-us/typography/opentype/spec/gpos

include!("../../generated/generated_gpos.rs");

use std::collections::HashSet;

use super::value_record::ValueRecord;
use super::{
    ChainedSequenceContext, ClassDef, CoverageTable, Device, FeatureList, FeatureVariations,
    Lookup, LookupList, LookupType, ScriptList, SequenceContext,
};

#[cfg(test)]
#[path = "../tests/gpos.rs"]
mod tests;

/// A GPOS lookup list table.
type PositionLookupList = LookupList<PositionLookup>;

table_newtype!(
    PositionSequenceContext,
    SequenceContext,
    read_fonts::layout::SequenceContext<'a>
);

table_newtype!(
    PositionChainContext,
    ChainedSequenceContext,
    read_fonts::layout::ChainedSequenceContext<'a>
);

impl Gpos {
    fn compute_version(&self) -> MajorMinor {
        if self.feature_variations.is_none() {
            MajorMinor::VERSION_1_0
        } else {
            MajorMinor::VERSION_1_1
        }
    }
}

lookup_type!(SinglePos, 1);
lookup_type!(PairPos, 2);
lookup_type!(CursivePosFormat1, 3);
lookup_type!(MarkBasePosFormat1, 4);
lookup_type!(MarkLigPosFormat1, 5);
lookup_type!(MarkMarkPosFormat1, 6);
lookup_type!(PositionSequenceContext, 7);
lookup_type!(PositionChainContext, 8);
lookup_type!(ExtensionSubtable, 9);

impl<T: LookupType + FontWrite> FontWrite for ExtensionPosFormat1<T> {
    fn write_into(&self, writer: &mut TableWriter) {
        1u16.write_into(writer);
        T::TYPE.write_into(writer);
        self.extension.write_into(writer);
    }
}

// these can't have auto impls because the traits don't support generics
impl<'a> FontRead<'a> for PositionLookup {
    fn read(data: FontData<'a>) -> Result<Self, ReadError> {
        read_fonts::layout::gpos::PositionLookup::read(data).map(|x| x.to_owned_table())
    }
}

impl<'a> FontRead<'a> for PositionLookupList {
    fn read(data: FontData<'a>) -> Result<Self, ReadError> {
        read_fonts::layout::gpos::PositionLookupList::read(data).map(|x| x.to_owned_table())
    }
}

impl AnchorTable {
    /// Create a new [`AnchorFormat1`] table.
    pub fn format_1(x_coordinate: i16, y_coordinate: i16) -> Self {
        Self::Format1(AnchorFormat1 {
            x_coordinate,
            y_coordinate,
        })
    }

    /// Create a new [`AnchorFormat2`] table.
    pub fn format_2(x_coordinate: i16, y_coordinate: i16, anchor_point: u16) -> Self {
        Self::Format2(AnchorFormat2 {
            x_coordinate,
            y_coordinate,
            anchor_point,
        })
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

    fn compute_class1_count(&self) -> u16 {
        self.class_def1.class_count()
    }

    fn compute_class2_count(&self) -> u16 {
        self.class_def2.class_count()
    }
}

impl MarkBasePosFormat1 {
    fn compute_mark_class_count(&self) -> u16 {
        self.mark_array.class_count()
    }
}

impl MarkMarkPosFormat1 {
    fn compute_mark_class_count(&self) -> u16 {
        self.mark1_array.class_count()
    }
}

impl MarkLigPosFormat1 {
    fn compute_mark_class_count(&self) -> u16 {
        self.mark_array.class_count()
    }
}

impl MarkArray {
    fn class_count(&self) -> u16 {
        self.mark_records
            .iter()
            .map(|rec| rec.mark_class)
            .collect::<HashSet<_>>()
            .len() as u16
    }
}
