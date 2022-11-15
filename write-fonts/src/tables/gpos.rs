//! the [GPOS] table
//!
//! [GPOS]: https://docs.microsoft.com/en-us/typography/opentype/spec/gpos

include!("../../generated/generated_gpos.rs");

use std::collections::HashSet;

//use super::layout::value_record::ValueRecord;
use super::layout::{
    ChainedSequenceContext, ClassDef, CoverageTable, Device, FeatureList, FeatureVariations,
    Lookup, LookupList, LookupType, ScriptList, SequenceContext,
};

#[cfg(test)]
#[path = "../tests/test_gpos.rs"]
mod spec_tests;

#[path = "./value_record.rs"]
mod value_record;
pub use value_record::ValueRecord;

/// A GPOS lookup list table.
type PositionLookupList = LookupList<PositionLookup>;

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
    fn compute_version(&self) -> MajorMinor {
        if self.feature_variations.is_none() {
            MajorMinor::VERSION_1_0
        } else {
            MajorMinor::VERSION_1_1
        }
    }
}

super::layout::lookup_type!(SinglePos, 1);
super::layout::lookup_type!(PairPos, 2);
super::layout::lookup_type!(CursivePosFormat1, 3);
super::layout::lookup_type!(MarkBasePosFormat1, 4);
super::layout::lookup_type!(MarkLigPosFormat1, 5);
super::layout::lookup_type!(MarkMarkPosFormat1, 6);
super::layout::lookup_type!(PositionSequenceContext, 7);
super::layout::lookup_type!(PositionChainContext, 8);
super::layout::lookup_type!(ExtensionSubtable, 9);

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
        read_fonts::tables::gpos::PositionLookup::read(data).map(|x| x.to_owned_table())
    }
}

impl<'a> FontRead<'a> for PositionLookupList {
    fn read(data: FontData<'a>) -> Result<Self, ReadError> {
        read_fonts::tables::gpos::PositionLookupList::read(data).map(|x| x.to_owned_table())
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

#[cfg(test)]
mod tests {

    use read_fonts::tables::{gpos as read_gpos, layout::LookupFlag};

    use super::*;

    // adapted from/motivated by https://github.com/fonttools/fonttools/issues/471
    #[test]
    fn gpos_1_zero() {
        let cov_one = CoverageTable::format_1(vec![GlyphId::new(2)]);
        let cov_two = CoverageTable::format_1(vec![GlyphId::new(4)]);
        let sub1 = SinglePos::format_1(cov_one, ValueRecord::default());
        let sub2 = SinglePos::format_1(
            cov_two,
            ValueRecord {
                x_advance: Some(500),
                ..Default::default()
            },
        );
        let lookup = Lookup::new(LookupFlag::default(), vec![sub1, sub2], 0);
        let bytes = crate::dump_table(&lookup).unwrap();

        let parsed = read_gpos::PositionLookup::read(FontData::new(&bytes)).unwrap();
        let read_gpos::PositionLookup::Single(table) = parsed else {
            panic!("something has gone seriously wrong");
        };

        assert_eq!(table.lookup_flag(), LookupFlag::empty());
        assert_eq!(table.sub_table_count(), 2);
        let read_gpos::SinglePos::Format1(sub1) = table.subtables().next().unwrap().unwrap() else {
            panic!("wrong table type");
        };
        let read_gpos::SinglePos::Format1(sub2) = table.subtables().nth(1).unwrap().unwrap() else {
            panic!("wrong table type");
        };

        assert_eq!(sub1.value_format(), ValueFormat::empty());
        assert_eq!(sub1.value_record(), read_gpos::ValueRecord::default());

        assert_eq!(sub2.value_format(), ValueFormat::X_ADVANCE);
        assert_eq!(
            sub2.value_record(),
            read_gpos::ValueRecord {
                x_advance: Some(500.into()),
                ..Default::default()
            }
        );
    }
}
