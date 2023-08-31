//! the [GPOS] table
//!
//! [GPOS]: https://docs.microsoft.com/en-us/typography/opentype/spec/gpos

include!("../../generated/generated_gpos.rs");

use std::collections::HashSet;

//use super::layout::value_record::ValueRecord;
use super::layout::{
    ChainedSequenceContext, ClassDef, CoverageTable, DeviceOrVariationIndex, FeatureList,
    FeatureVariations, Lookup, LookupList, LookupSubtable, LookupType, ScriptList, SequenceContext,
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

    fn check_length_and_format_conformance(&self, ctx: &mut ValidationCtx) {
        let n_class_1s = self.class_def1.class_count();
        let n_class_2s = self.class_def2.class_count();
        let format_1 = self.compute_value_format1();
        let format_2 = self.compute_value_format2();
        if self.class1_records.len() != n_class_1s as usize {
            ctx.report("class1_records length must match number of class1 classes");
        }
        ctx.in_field("class1_records", |ctx| {
            ctx.in_array(|ctx| {
                for record in &self.class1_records {
                    ctx.array_item(|ctx| {
                        if record.class2_records.len() != n_class_2s as usize {
                            ctx.report(
                                "class2_records length must match number of class2 classes ",
                            );
                        }
                        if record.class2_records.iter().any(|rec| {
                            rec.value_record1.format() != format_1
                                || rec.value_record2.format() != format_2
                        }) {
                            ctx.report("all value records should report the same format");
                        }
                    })
                }
            })
        });
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
        let sub2 = SinglePos::format_1(cov_two, ValueRecord::default().with_x_advance(500));
        let lookup = Lookup::new(LookupFlag::default(), vec![sub1, sub2], 0);
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
        let class1 = ClassDef::from_iter([(GlyphId::new(5), 0), (GlyphId::new(6), 1)]);
        // class 0 is 'all the rest', here, always implicitly present
        let class2 = ClassDef::from_iter([(GlyphId::new(8), 1)]);

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
        let class1 = ClassDef::from_iter([(GlyphId::new(5), 0), (GlyphId::new(6), 1)]);
        // class 0 is 'all the rest', here, always implicitly present
        let class2 = ClassDef::from_iter([(GlyphId::new(8), 1)]);
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
}
