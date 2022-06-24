//! the [GDEF] table
//!
//! [GDEF]: https://docs.microsoft.com/en-us/typography/opentype/spec/gdef

use font_types::{BigEndian, FontRead, FontReadWithArgs, Tag};

use crate::layout::{Lookup, LookupList};

/// 'GDEF'
pub const TAG: Tag = Tag::new(b"GPOS");

include!("../../generated/generated_gpos_parse.rs");

impl ValueFormat {
    /// Return the number of bytes required to store a [`ValueRecord`] in this format.
    #[inline]
    pub fn record_byte_len(self) -> usize {
        self.bits().count_ones() as usize * 2
    }
}

pub struct PositionLookupList<'a>(LookupList<'a>);

pub struct PositionLookup<'a>(Lookup<'a>);

impl<'a> FontRead<'a> for PositionLookup<'a> {
    fn read(bytes: &'a [u8]) -> Option<Self> {
        Lookup::read(bytes).map(Self)
    }
}

impl<'a> FontRead<'a> for PositionLookupList<'a> {
    fn read(bytes: &'a [u8]) -> Option<Self> {
        LookupList::read(bytes).map(Self)
    }
}

#[derive(Clone, Default, PartialEq)]
pub struct ValueRecord {
    pub x_placement: Option<BigEndian<i16>>,
    pub y_placement: Option<BigEndian<i16>>,
    pub x_advance: Option<BigEndian<i16>>,
    pub y_advance: Option<BigEndian<i16>>,
    pub x_placement_device: Option<BigEndian<i16>>,
    pub y_placement_device: Option<BigEndian<i16>>,
    pub x_advance_device: Option<BigEndian<i16>>,
    pub y_advance_device: Option<BigEndian<i16>>,
}

impl ValueRecord {
    pub fn read(bytes: &[u8], format: ValueFormat) -> Option<(Self, &[u8])> {
        let mut this = ValueRecord::default();
        let mut words = bytes.chunks(2);

        if format.contains(ValueFormat::X_PLACEMENT) {
            this.x_placement = FontRead::read(words.next()?);
        }
        if format.contains(ValueFormat::Y_PLACEMENT) {
            this.y_placement = FontRead::read(words.next()?);
        }
        if format.contains(ValueFormat::X_ADVANCE) {
            this.x_advance = FontRead::read(words.next()?);
        }
        if format.contains(ValueFormat::Y_ADVANCE) {
            this.y_advance = FontRead::read(words.next()?);
        }
        if format.contains(ValueFormat::X_PLACEMENT_DEVICE) {
            this.x_placement_device = FontRead::read(words.next()?);
        }
        if format.contains(ValueFormat::Y_PLACEMENT_DEVICE) {
            this.y_placement_device = FontRead::read(words.next()?);
        }
        if format.contains(ValueFormat::X_ADVANCE_DEVICE) {
            this.x_advance_device = FontRead::read(words.next()?);
        }
        if format.contains(ValueFormat::Y_ADVANCE_DEVICE) {
            this.y_advance_device = FontRead::read(words.next()?);
        }
        let len = format.bits().count_ones() as usize * 2;
        bytes.get(len..).map(|b| (this, b))
    }
}

impl<'a> FontReadWithArgs<'a, ValueFormat> for ValueRecord {
    fn read_with_args(bytes: &'a [u8], args: &ValueFormat) -> Option<(Self, &'a [u8])> {
        ValueRecord::read(bytes, *args)
    }
}

impl std::fmt::Debug for ValueRecord {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let mut f = f.debug_struct("ValueRecord");
        self.x_placement.map(|x| f.field("x_placement", &x));
        self.y_placement.map(|y| f.field("y_placement", &y));
        self.x_advance.map(|x| f.field("x_advance", &x));
        self.y_advance.map(|y| f.field("y_advance", &y));
        self.x_placement_device
            .map(|x| f.field("x_placement_device", &x));
        self.y_placement_device
            .map(|y| f.field("y_placement_device", &y));
        self.x_advance_device
            .map(|x| f.field("x_advance_device", &x));
        self.y_advance_device
            .map(|y| f.field("y_advance_device", &y));
        f.finish()
    }
}

#[derive(Debug, Default, PartialEq)]
pub struct PairValueRecord {
    pub second_glyph: BigEndian<u16>,
    pub value_record1: ValueRecord,
    pub value_record2: ValueRecord,
}

impl<'a> FontReadWithArgs<'a, (ValueFormat, ValueFormat)> for PairValueRecord {
    fn read_with_args(
        bytes: &'a [u8],
        args: &(ValueFormat, ValueFormat),
    ) -> Option<(Self, &'a [u8])> {
        let second_glyph = FontRead::read(bytes)?;
        let (value_record1, bytes) = ValueRecord::read_with_args(bytes.get(2..)?, &args.0)?;
        let (value_record2, bytes) = ValueRecord::read_with_args(bytes, &args.1)?;
        Some((
            PairValueRecord {
                second_glyph,
                value_record1,
                value_record2,
            },
            bytes,
        ))
    }
}

#[cfg(feature = "compile")]
pub mod compile {

    use std::collections::HashSet;

    use font_types::{Offset, Offset16, Offset32, OffsetHost};

    use crate::compile::{FontWrite, OffsetMarker, ToOwnedObj, ToOwnedTable};
    use crate::layout::compile::{ChainedSequenceContext, Lookup, SequenceContext};

    pub use super::ValueRecord;
    include!("../../generated/generated_gpos_compile.rs");

    //TODO: we can get rid of all this once we have auto-getters for offset types?
    impl super::PairPosFormat1<'_> {
        pub(crate) fn pair_sets_to_owned(&self) -> Option<Vec<OffsetMarker<Offset16, PairSet>>> {
            let offset_bytes = self.bytes();
            let format1 = self.value_format1();
            let format2 = self.value_format2();
            self.pair_set_offsets()
                .iter()
                .map(|off| {
                    off.get()
                        .read_with_args::<_, super::PairSet>(offset_bytes, &(format1, format2))
                        .and_then(|pair| pair.to_owned_obj(offset_bytes).map(OffsetMarker::new))
                })
                .collect()
        }
    }

    impl super::MarkBasePosFormat1<'_> {
        pub(crate) fn base_array_to_owned(&self) -> Option<OffsetMarker<Offset16, BaseArray>> {
            self.base_array_offset()
                .read_with_args::<_, super::BaseArray>(self.bytes(), &self.mark_class_count())
                .and_then(|x| x.to_owned_obj(self.bytes()))
                .map(OffsetMarker::new)
        }
    }

    impl super::MarkLigPosFormat1<'_> {
        pub(crate) fn ligature_array_to_owned(
            &self,
        ) -> Option<OffsetMarker<Offset16, LigatureArray>> {
            let lig_array = self
                .ligature_array_offset()
                .read_with_args::<_, super::LigatureArray>(
                    self.bytes(),
                    &self.mark_class_count(),
                )?;
            let ligature_attach_offsets = lig_array
                .ligature_attach_offsets()
                .iter()
                .map(|off| {
                    OffsetMarker::new_maybe_null(
                        off.get()
                            .read_with_args::<_, super::LigatureAttach>(
                                lig_array.bytes(),
                                &self.mark_class_count(),
                            )
                            .and_then(|obj| obj.to_owned_obj(lig_array.bytes())),
                    )
                })
                .collect();
            Some(OffsetMarker::new(LigatureArray {
                ligature_attach_offsets,
            }))
        }
    }

    impl super::MarkMarkPosFormat1<'_> {
        pub(crate) fn mark2_array_to_owned(&self) -> Option<OffsetMarker<Offset16, Mark2Array>> {
            let mark2array = self
                .mark2_array_offset()
                .read_with_args::<_, super::Mark2Array>(self.bytes(), &self.mark_class_count())?;
            Some(OffsetMarker::new_maybe_null(mark2array.to_owned_table()))
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

    #[derive(Debug, PartialEq)]
    pub struct PositionLookupList {
        pub lookup_offsets: Vec<OffsetMarker<Offset16, GposLookup>>,
    }

    #[derive(Debug, PartialEq)]
    pub enum GposLookup {
        Single(Lookup<SinglePos>),
        Pair(Lookup<PairPos>),
        Cursive(Lookup<CursivePosFormat1>),
        MarkToBase(Lookup<MarkBasePosFormat1>),
        MarkToMark(Lookup<MarkMarkPosFormat1>),
        MarkToLig(Lookup<MarkLigPosFormat1>),
        Contextual(Lookup<SequenceContext>),
        ChainContextual(Lookup<ChainedSequenceContext>),
        Extension(Lookup<Extension>),
    }

    #[derive(Debug, PartialEq)]
    pub struct Extension {
        pub extension_lookup_type: u16,
        pub extension_offset: OffsetMarker<Offset32, Box<dyn FontWrite>>,
    }

    impl FontWrite for GposLookup {
        fn write_into(&self, writer: &mut crate::compile::TableWriter) {
            match self {
                GposLookup::Single(lookup) => lookup.write_into(writer),
                GposLookup::Pair(lookup) => lookup.write_into(writer),
                GposLookup::Cursive(lookup) => lookup.write_into(writer),
                GposLookup::MarkToBase(lookup) => lookup.write_into(writer),
                GposLookup::MarkToMark(lookup) => lookup.write_into(writer),
                GposLookup::MarkToLig(lookup) => lookup.write_into(writer),
                GposLookup::Contextual(lookup) => lookup.write_into(writer),
                GposLookup::ChainContextual(lookup) => lookup.write_into(writer),
                GposLookup::Extension(lookup) => lookup.write_into(writer),
            }
        }
    }

    impl FontWrite for Extension {
        fn write_into(&self, writer: &mut crate::compile::TableWriter) {
            1u16.write_into(writer);
            self.extension_lookup_type.write_into(writer);
            self.extension_offset.write_into(writer);
        }
    }

    impl ToOwnedObj for super::ExtensionPosFormat1<'_> {
        type Owned = Extension;

        fn to_owned_obj(&self, _: &[u8]) -> Option<Self::Owned> {
            let off = self.extension_offset();
            let data = self.bytes();
            let boxed_inner: Box<dyn FontWrite> = match self.extension_lookup_type() {
                1 => Box::new(off.read::<super::SinglePos>(data)?.to_owned_table()?),
                2 => Box::new(off.read::<super::PairPos>(data)?.to_owned_table()?),
                3 => Box::new(
                    off.read::<super::CursivePosFormat1>(data)?
                        .to_owned_table()?,
                ),
                4 => Box::new(
                    off.read::<super::MarkBasePosFormat1>(data)?
                        .to_owned_table()?,
                ),
                5 => Box::new(
                    off.read::<super::MarkMarkPosFormat1>(data)?
                        .to_owned_table()?,
                ),
                6 => Box::new(
                    off.read::<super::MarkLigPosFormat1>(data)?
                        .to_owned_table()?,
                ),
                7 => Box::new(
                    off.read::<crate::layout::SequenceContext>(data)?
                        .to_owned_table()?,
                ),
                8 => Box::new(
                    off.read::<crate::layout::ChainedSequenceContext>(data)?
                        .to_owned_table()?,
                ),
                _ => return None,
            };
            Some(Extension {
                extension_lookup_type: self.extension_lookup_type(),
                extension_offset: OffsetMarker::new(boxed_inner),
            })
        }
    }

    impl ToOwnedTable for super::ExtensionPosFormat1<'_> {}

    impl FontWrite for PositionLookupList {
        fn write_into(&self, writer: &mut crate::compile::TableWriter) {
            u16::try_from(self.lookup_offsets.len())
                .unwrap()
                .write_into(writer);
            self.lookup_offsets.write_into(writer);
        }
    }

    impl ToOwnedObj for super::PositionLookup<'_> {
        type Owned = GposLookup;

        fn to_owned_obj(&self, _offset_data: &[u8]) -> Option<Self::Owned> {
            match self.0.lookup_type() {
                1 => self
                    .0
                    .to_owned_explicit::<super::SinglePos>()
                    .map(GposLookup::Single),
                2 => self
                    .0
                    .to_owned_explicit::<super::PairPos>()
                    .map(GposLookup::Pair),
                3 => self
                    .0
                    .to_owned_explicit::<super::CursivePosFormat1>()
                    .map(GposLookup::Cursive),
                4 => self
                    .0
                    .to_owned_explicit::<super::MarkBasePosFormat1>()
                    .map(GposLookup::MarkToBase),
                5 => self
                    .0
                    .to_owned_explicit::<super::MarkMarkPosFormat1>()
                    .map(GposLookup::MarkToMark),
                6 => self
                    .0
                    .to_owned_explicit::<super::MarkLigPosFormat1>()
                    .map(GposLookup::MarkToLig),
                7 => self
                    .0
                    .to_owned_explicit::<crate::layout::SequenceContext>()
                    .map(GposLookup::Contextual),
                8 => self
                    .0
                    .to_owned_explicit::<crate::layout::ChainedSequenceContext>()
                    .map(GposLookup::ChainContextual),
                9 => self
                    .0
                    .to_owned_explicit::<super::ExtensionPosFormat1>()
                    .map(GposLookup::Extension),
                _ => None,
            }
        }
    }

    impl ToOwnedTable for super::PositionLookup<'_> {}

    impl ToOwnedObj for super::PositionLookupList<'_> {
        type Owned = PositionLookupList;

        fn to_owned_obj(&self, _offset_data: &[u8]) -> Option<Self::Owned> {
            Some(PositionLookupList {
                lookup_offsets: self
                    .0
                    .iter_lookups()
                    .map(|x| {
                        super::PositionLookup(x)
                            .to_owned_table()
                            .map(OffsetMarker::new)
                    })
                    .collect::<Option<_>>()?,
            })
        }
    }

    impl ToOwnedObj for super::ValueRecord {
        type Owned = super::ValueRecord;
        fn to_owned_obj(&self, _offset_data: &[u8]) -> Option<Self::Owned> {
            Some(self.clone())
        }
    }

    impl FontWrite for ValueRecord {
        fn write_into(&self, writer: &mut crate::compile::TableWriter) {
            self.x_placement.map(|v| v.write_into(writer));
            self.y_placement.map(|v| v.write_into(writer));
            self.x_advance.map(|v| v.write_into(writer));
            self.y_advance.map(|v| v.write_into(writer));
            self.x_placement_device.map(|v| v.write_into(writer));
            self.y_placement_device.map(|v| v.write_into(writer));
            self.x_advance_device.map(|v| v.write_into(writer));
            self.y_advance_device.map(|v| v.write_into(writer));
        }
    }

    #[derive(Debug, Default, PartialEq)]
    pub struct PairValueRecord {
        pub second_glyph: u16,
        pub value_record1: ValueRecord,
        pub value_record2: ValueRecord,
    }

    impl ToOwnedObj for super::PairValueRecord {
        type Owned = PairValueRecord;
        fn to_owned_obj(&self, _offset_data: &[u8]) -> Option<Self::Owned> {
            Some(PairValueRecord {
                second_glyph: self.second_glyph.get(),
                value_record1: self.value_record1.clone(),
                value_record2: self.value_record2.clone(),
            })
        }
    }

    impl FontWrite for PairValueRecord {
        fn write_into(&self, writer: &mut crate::compile::TableWriter) {
            self.second_glyph.write_into(writer);
            self.value_record1.write_into(writer);
            self.value_record2.write_into(writer);
        }
    }
}

#[cfg(feature = "compile")]
#[cfg(test)]
mod compile_tests {
    use crate::assert_hex_eq;
    use crate::compile::{ToOwnedObj, ToOwnedTable};
    use font_types::OffsetHost;

    use super::*;

    #[test]
    fn singleposformat1() {
        // https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#example-2-singleposformat1-subtable

        #[rustfmt::skip]
        let bytes = [
            0x00, 0x01, 0x00, 0x08, 0x00, 0x02, 0xFF, 0xB0, 0x00, 0x02, 0x00,
            0x01, 0x01, 0xB3, 0x01, 0xBC, 0x00, 0x00,
        ];

        let table = SinglePosFormat1::read(&bytes).unwrap();
        let owned = table.to_owned_table().unwrap();
        let dumped = crate::compile::dump_table(&owned);
        let reloaded = SinglePosFormat1::read(&dumped).unwrap();
        assert_eq!(table.value_format(), reloaded.value_format());
        assert_eq!(table.value_record(), reloaded.value_record());
        let cov1 = table.coverage().unwrap();
        let cov2 = reloaded.coverage().unwrap();

        assert_eq!(
            cov1.iter().collect::<Vec<_>>(),
            cov2.iter().collect::<Vec<_>>()
        );

        assert_hex_eq!(&bytes, &dumped);
    }

    #[test]
    fn singleposformat2() {
        // https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#example-3-singleposformat2-subtable

        #[rustfmt::skip]
        let bytes = [
            0x00, 0x02, 0x00, 0x14, 0x00, 0x05, 0x00, 0x03, 0x00, 0x32, 0x00,
            0x32, 0x00, 0x19, 0x00, 0x19, 0x00, 0x0A, 0x00, 0x0A, 0x00, 0x01,
            0x00, 0x03, 0x00, 0x4F, 0x01, 0x25, 0x01, 0x29,
        ];

        let table = SinglePosFormat2::read(&bytes).unwrap();
        let owned = table.to_owned_table().unwrap();
        let dumped = crate::compile::dump_table(&owned);
        let reloaded = SinglePosFormat2::read(&dumped).unwrap();
        assert_eq!(table.value_format(), reloaded.value_format());
        assert_eq!(table.value_count(), reloaded.value_count());
        assert_hex_eq!(&bytes, &dumped);
    }

    #[test]
    fn pairposformat1() {
        // https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#example-4-pairposformat1-subtable

        #[rustfmt::skip]
        let bytes = [
            0x00, 0x01, 0x00, 0x1E, 0x00, 0x04, 0x00, 0x01, 0x00, 0x02, 0x00,
            0x0E, 0x00, 0x16, 0x00, 0x01, 0x00, 0x59, 0xFF, 0xE2, 0xFF, 0xEC,
            0x00, 0x01, 0x00, 0x59, 0xFF, 0xD8, 0xFF, 0xE7, 0x00, 0x01, 0x00,
            0x02, 0x00, 0x2D, 0x00, 0x31,
        ];

        let table = PairPosFormat1::read(&bytes).unwrap();
        let owned = table.to_owned_table().unwrap();
        let dumped = crate::compile::dump_table(&owned);
        let reloaded = PairPosFormat1::read(&dumped).unwrap();
        assert_eq!(table.value_format1(), reloaded.value_format1());
        assert_eq!(table.value_format2(), reloaded.value_format2());

        // we order the coverage table before the pairsets
        //assert_hex_eq!(&bytes, &dumped);
    }

    #[test]
    fn pairposformat2() {
        // https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#example-5-pairposformat2-subtable

        #[rustfmt::skip]
        let bytes = [
            0x00, 0x02, 0x00, 0x18, 0x00, 0x04, 0x00, 0x00, 0x00, 0x22, 0x00,
            0x32, 0x00, 0x02, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0xFF, 0xCE, 0x00, 0x01, 0x00, 0x03, 0x00, 0x46, 0x00, 0x47, 0x00,
            0x49, 0x00, 0x02, 0x00, 0x02, 0x00, 0x46, 0x00, 0x47, 0x00, 0x01,
            0x00, 0x49, 0x00, 0x49, 0x00, 0x01, 0x00, 0x02, 0x00, 0x01, 0x00,
            0x6A, 0x00, 0x6B, 0x00, 0x01,
        ];
        let table = PairPosFormat2::read(&bytes).unwrap();
        assert_eq!(table.value_format1().record_byte_len(), 2);
        assert_eq!(table.value_format2().record_byte_len(), 0);
        assert_eq!(table.class1_records().iter().count(), 2);
        let owned = table.to_owned_table().unwrap();
        assert_eq!(owned.class1_records.len(), 2);
        let first = &owned.class1_records[0];
        assert_eq!(first.class2_records.len(), 2);
        let dumped = crate::compile::dump_table(&owned);
        let reloaded = PairPosFormat2::read(&dumped).unwrap();
        assert_eq!(table.value_format1(), reloaded.value_format1());
        assert_eq!(table.value_format2(), reloaded.value_format2());

        // we order the coverage table before the pairsets
        assert_hex_eq!(&bytes, &dumped);
    }

    #[test]
    fn cursiveposformat1() {
        // https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#example-6-cursiveposformat1-subtable

        #[rustfmt::skip]
        let bytes = [
            0x00, 0x01, 0x00, 0x0E, 0x00, 0x02, 0x00, 0x16, 0x00, 0x1C, 0x00,
            0x22, 0x00, 0x28, 0x00, 0x01, 0x00, 0x02, 0x02, 0x03, 0x02, 0x7E,
            0x00, 0x01, 0x05, 0xDC, 0x00, 0x2C, 0x00, 0x01, 0x00, 0x00, 0xFF,
            0xEC, 0x00, 0x01, 0x05, 0xDC, 0x00, 0x2C, 0x00, 0x01, 0x00, 0x00,
            0xFF, 0xEC,
        ];

        let table = CursivePosFormat1::read(&bytes).unwrap();
        let owned = table.to_owned_table().unwrap();
        let dumped = crate::compile::dump_table(&owned);
        let reloaded = CursivePosFormat1::read(&dumped).unwrap();
        assert_eq!(
            table.entry_exit_record().len(),
            reloaded.entry_exit_record().len()
        );
        for (one, two) in table
            .entry_exit_record()
            .iter()
            .zip(reloaded.entry_exit_record().iter())
        {
            let entry1: AnchorTable = table.resolve_offset(one.entry_anchor_offset()).unwrap();
            let entry2: AnchorTable = reloaded.resolve_offset(two.entry_anchor_offset()).unwrap();
            assert!(anchor_eq(&entry1, &entry2));
            let exit1: AnchorTable = table.resolve_offset(one.exit_anchor_offset()).unwrap();
            let exit2: AnchorTable = reloaded.resolve_offset(two.exit_anchor_offset()).unwrap();
            assert!(anchor_eq(&exit1, &exit2));
        }

        // hex is not equal because we deduplicate a table
        //assert_hex_eq!(&bytes, &dumped);
        // we order the coverage table before the pairsets
    }

    fn anchor_eq(one: &AnchorTable, two: &AnchorTable) -> bool {
        match (one, two) {
            (AnchorTable::Format1(one), AnchorTable::Format1(two)) => {
                one.x_coordinate() == two.x_coordinate() && one.y_coordinate() == two.y_coordinate()
            }
            (AnchorTable::Format2(one), AnchorTable::Format2(two)) => {
                one.x_coordinate() == two.x_coordinate()
                    && one.y_coordinate() == two.y_coordinate()
                    && one.anchor_point() == two.anchor_point()
            }
            (AnchorTable::Format3(one), AnchorTable::Format3(two)) => {
                one.x_coordinate() == two.x_coordinate() && one.y_coordinate() == two.y_coordinate()
            }
            (_, _) => false,
        }
    }

    #[test]
    fn markbaseposformat1() {
        // https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#example-7-markbaseposformat1-subtable

        #[rustfmt::skip]
        let bytes = [
            0x00, 0x01, 0x00, 0x0C, 0x00, 0x14, 0x00, 0x02, 0x00, 0x1A, 0x00,
            0x30, 0x00, 0x01, 0x00, 0x02, 0x03, 0x33, 0x03, 0x3F, 0x00, 0x01,
            0x00, 0x01, 0x01, 0x90, 0x00, 0x02, 0x00, 0x00, 0x00, 0x0A, 0x00,
            0x01, 0x00, 0x10, 0x00, 0x01, 0x01, 0x5A, 0xFF, 0x9E, 0x00, 0x01,
            0x01, 0x05, 0x00, 0x58, 0x00, 0x01, 0x00, 0x06, 0x00, 0x0C, 0x00,
            0x01, 0x03, 0x3E, 0x06, 0x40, 0x00, 0x01, 0x03, 0x3E, 0xFF, 0xAD,
        ];

        let table = MarkBasePosFormat1::read(&bytes).unwrap();
        let owned = table.to_owned_table().unwrap();
        let marks = owned.mark_array_offset.get().unwrap();
        assert_eq!(marks.mark_records.len(), 2);
        let dumped = crate::compile::dump_table(&owned);

        assert_hex_eq!(&bytes, &dumped);
    }

    #[test]
    fn markligposformat1() {
        // https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#example-8-markligposformat1-subtable

        #[rustfmt::skip]
        let bytes = [
            0x00, 0x01, 0x00, 0x0C, 0x00, 0x14, 0x00, 0x02, 0x00, 0x1A, 0x00,
            0x30, 0x00, 0x01, 0x00, 0x02, 0x03, 0x3C, 0x03, 0x3F, 0x00, 0x01,
            0x00, 0x01, 0x02, 0x34, 0x00, 0x02, 0x00, 0x00, 0x00, 0x0A, 0x00,
            0x01, 0x00, 0x10, 0x00, 0x01, 0x01, 0x5A, 0xFF, 0x9E, 0x00, 0x01,
            0x01, 0x05, 0x01, 0xE8, 0x00, 0x01, 0x00, 0x04, 0x00, 0x03, 0x00,
            0x0E, 0x00, 0x00, 0x00, 0x00, 0x00, 0x14, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x01, 0x02, 0x71, 0x07, 0x08, 0x00, 0x01, 0x01, 0x78, 0xFE,
            0x90,
        ];

        let table = MarkLigPosFormat1::read(&bytes).unwrap();
        let owned = table.to_owned_table().unwrap();
        let marks = owned.mark_array_offset.get().unwrap();
        assert_eq!(marks.mark_records.len(), 2);
        let dumped = crate::compile::dump_table(&owned);

        assert_hex_eq!(&bytes, &dumped);
    }

    #[test]
    fn markmarkposformat1() {
        // https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#example-9-markmarkposformat1-subtable

        #[rustfmt::skip]
        let bytes = [
            0x00, 0x01, 0x00, 0x0C, 0x00, 0x12, 0x00, 0x01, 0x00, 0x18, 0x00,
            0x24, 0x00, 0x01, 0x00, 0x01, 0x02, 0x96, 0x00, 0x01, 0x00, 0x01,
            0x02, 0x89, 0x00, 0x01, 0x00, 0x00, 0x00, 0x06, 0x00, 0x01, 0x00,
            0xBD, 0xFF, 0x99, 0x00, 0x01, 0x00, 0x04, 0x00, 0x01, 0x00, 0xDD,
            0x01, 0x2D,
        ];

        let table = MarkMarkPosFormat1::read(&bytes).unwrap();
        let owned = table.to_owned_table().unwrap();
        let dumped = crate::compile::dump_table(&owned);

        assert_hex_eq!(&bytes, &dumped);
    }

    #[test]
    fn contextualposformat1() {
        // https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#example-10-contextual-positioning-format-1

        #[rustfmt::skip]
        let bytes = [
            0x00, 0x01, 0x00, 0x08, 0x00, 0x01, 0x00, 0x0E, 0x00, 0x01, 0x00,
            0x01, 0x02, 0xA6, 0x00, 0x01, 0x00, 0x04, 0x00, 0x03, 0x00, 0x01,
            0x02, 0xDD, 0x02, 0xC6, 0x00, 0x02, 0x00, 0x01,
        ];

        let table = crate::layout::SequenceContextFormat1::read(&bytes).unwrap();
        let owned = table.to_owned_table().unwrap();
        let dumped = crate::compile::dump_table(&owned);

        assert_hex_eq!(&bytes, &dumped);
    }

    #[test]
    fn contextualposformat2() {
        // https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#example-11-contextual-positioning-format-1

        #[rustfmt::skip]
        let bytes = [
            0x00, 0x02, 0x00, 0x12, 0x00, 0x20, 0x00, 0x05, 0x00, 0x00, 0x00,
            0x60, 0x00, 0x70, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x05,
            0x00, 0x29, 0x00, 0x33, 0x00, 0x37, 0x00, 0x39, 0x00, 0x3A, 0x00,
            0x02, 0x00, 0x0A, 0x00, 0x29, 0x00, 0x29, 0x00, 0x02, 0x00, 0x33,
            0x00, 0x33, 0x00, 0x02, 0x00, 0x37, 0x00, 0x37, 0x00, 0x01, 0x00,
            0x39, 0x00, 0x3A, 0x00, 0x01, 0x00, 0x42, 0x00, 0x42, 0x00, 0x03,
            0x00, 0x46, 0x00, 0x46, 0x00, 0x03, 0x00, 0x4A, 0x00, 0x4A, 0x00,
            0x03, 0x00, 0x51, 0x00, 0x51, 0x00, 0x03, 0x00, 0x56, 0x00, 0x56,
            0x00, 0x03, 0x00, 0xF5, 0x00, 0xF6, 0x00, 0x04, 0x00, 0x01, 0x00,
            0x04, 0x00, 0x03, 0x00, 0x01, 0x00, 0x03, 0x00, 0x04, 0x00, 0x02,
            0x00, 0x01, 0x00, 0x01, 0x00, 0x04, 0x00, 0x03, 0x00, 0x01, 0x00,
            0x03, 0x00, 0x04, 0x00, 0x00, 0x00, 0x02,
        ];

        let table = crate::layout::SequenceContextFormat2::read(&bytes).unwrap();
        let owned = table.to_owned_table().unwrap();
        let dumped = crate::compile::dump_table(&owned);

        assert_hex_eq!(&bytes, &dumped);
    }

    #[test]
    fn contextualposformat3() {
        // https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#example-12-contextual-positioning-format-3

        #[rustfmt::skip]
        let bytes = [
            0x00, 0x03, 0x00, 0x03, 0x00, 0x01, 0x00, 0x10, 0x00, 0x3C, 0x00,
            0x44, 0x00, 0x01, 0x00, 0x01, 0x00, 0x01, 0x00, 0x14, 0x00, 0x33,
            0x00, 0x35, 0x00, 0x37, 0x00, 0x39, 0x00, 0x3B, 0x00, 0x3C, 0x00,
            0x3F, 0x00, 0x40, 0x00, 0x41, 0x00, 0x42, 0x00, 0x43, 0x00, 0x44,
            0x00, 0x45, 0x00, 0x46, 0x00, 0x47, 0x00, 0x48, 0x00, 0x49, 0x00,
            0x4A, 0x00, 0x4B, 0x00, 0x4C, 0x00, 0x01, 0x00, 0x02, 0x01, 0x1E,
            0x01, 0x2D, 0x00, 0x02, 0x00, 0x01, 0x00, 0x33, 0x00, 0x4C, 0x00,
            0x00,
        ];

        let table = crate::layout::SequenceContextFormat3::read(&bytes).unwrap();
        let owned = table.to_owned_table().unwrap();
        let dumped = crate::compile::dump_table(&owned);

        assert_hex_eq!(&bytes, &dumped);
    }

    #[test]
    fn sequencelookuprecord() {
        // https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#example-13-sequencelookuprecord
        let bytes = [0x00, 0x01, 0x00, 0x01];
        let table = crate::layout::SequenceLookupRecord::read(&bytes).unwrap();
        assert_eq!(table.sequence_index(), 1);
        assert_eq!(table.lookup_list_index(), 1);
    }

    //FIXME: turn this back on when we support device records
    //#[test]
    //fn valueformattable() {
    //// https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#example-14-valueformat-table-and-valuerecord

    //#[rustfmt::skip]
    //let bytes = [
    //0x00, 0x01, 0x00, 0x0E, 0x00, 0x99, 0x00, 0x50, 0x00, 0xD2,
    //0x00, 0x18, 0x00, 0x20, 0x00, 0x02, 0x00, 0x01, 0x00, 0xC8,
    //0x00, 0xD1, 0x00, 0x00, 0x00, 0x0B, 0x00, 0x0F, 0x00, 0x01,
    //0x55, 0x40, 0x00, 0x0B, 0x00, 0x0F, 0x00, 0x01, 0x55, 0x40,
    //];

    //let table = SinglePosFormat1::read(&bytes).unwrap();
    //let owned = table.to_owned_table().unwrap();
    //let dumped = crate::compile::dump_table(&owned);

    //assert_hex_eq!(&bytes, &dumped);
    //}

    #[test]
    fn anchorformat1() {
        // https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#example-15-anchorformat1-table

        let bytes = [0x00, 0x01, 0x00, 0xBD, 0xFF, 0x99];
        let table = AnchorFormat1::read(&bytes).unwrap();
        let owned = table.to_owned_obj(&[]).unwrap();

        assert_eq!(owned.x_coordinate, 189);
        assert_eq!(owned.y_coordinate, -103);
        let dumped = crate::compile::dump_table(&owned);

        assert_hex_eq!(&bytes, &dumped);
    }

    #[test]
    fn anchorformat2() {
        // https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#example-16-anchorformat2-table

        let bytes = [0x00, 0x02, 0x01, 0x42, 0x03, 0x84, 0x00, 0x0D];
        let table = AnchorFormat2::read(&bytes).unwrap();
        let owned = table.to_owned_obj(&[]).unwrap();
        let dumped = crate::compile::dump_table(&owned);

        assert_hex_eq!(&bytes, &dumped);
    }

    //FIXME: enable when we have device tables working
    //#[test]
    //fn anchorformat3() {
    //// https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#example-17-anchorformat3-table

    //let bytes = [
    //0x00, 0x03, 0x01, 0x17, 0x05, 0x15, 0x00, 0x0A, 0x00, 0x14,
    //0x00, 0x0C, 0x00, 0x11, 0x00, 0x02, 0x11, 0x11, 0x22, 0x00,
    //0x00, 0x0C, 0x00, 0x11, 0x00, 0x02, 0x11, 0x11, 0x22, 0x00,
    //];
    //let table = AnchorFormat3::read(&bytes).unwrap();
    //let owned = table.to_owned_obj(&[]).unwrap();
    //let dumped = crate::compile::dump_table(&owned);

    //assert_hex_eq!(&bytes, &dumped);
    //}

    //NOTE: I think the sample bites are missing the actual anchor tables??
    // and so we can't really round-trip this...
    //#[test]
    //fn markarraytable() {
    //// https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#example-18-markarray-table-and-markrecord

    //let bytes = [0x00, 0x02, 0x00, 0x00, 0x00, 0x0A, 0x00, 0x01, 0x00, 0x10];
    //let table = MarkArray::read(&bytes).unwrap();
    //let owned = table.to_owned_obj(&[]).unwrap();
    //let dumped = crate::compile::dump_table(&owned);

    //assert_hex_eq!(&bytes, &dumped);
    //}
}
