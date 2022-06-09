//! the [GDEF] table
//!
//! [GDEF]: https://docs.microsoft.com/en-us/typography/opentype/spec/gdef

#[path = "../../generated/generated_gpos.rs"]
mod generated;

pub use generated::*;

use font_types::{BigEndian, FontRead, FontReadWithArgs, Offset16, Tag};

use crate::layout::{Lookup, LookupList};

/// 'GDEF'
pub const TAG: Tag = Tag::new(b"GPOS");

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

//TODO: can we get rid of this, like with LigatureArray?
pub struct BaseArray<'a> {
    // passed in from above
    mark_class_count: u16,
    pub base_count: BigEndian<u16>,
    base_records: &'a [BigEndian<Offset16>],
}

impl<'a> FontReadWithArgs<'a, u16> for BaseArray<'a> {
    fn read_with_args(bytes: &'a [u8], args: &u16) -> Option<(Self, &'a [u8])> {
        let mark_class_count = *args;
        let (base_count, bytes) =
            zerocopy::LayoutVerified::<_, BigEndian<u16>>::new_unaligned_from_prefix(bytes)?;
        let (base_records, bytes) = zerocopy::LayoutVerified::new_slice_unaligned_from_prefix(
            bytes,
            mark_class_count as usize * base_count.get() as usize,
        )?;
        Some((
            BaseArray {
                base_count: base_count.read(),
                mark_class_count,
                base_records: base_records.into_slice(),
            },
            bytes,
        ))
    }
}

pub struct BaseRecord<'a> {
    pub base_anchor_offsets: &'a [BigEndian<Offset16>],
}

impl<'a> BaseArray<'a> {
    pub fn base_records(&self) -> impl Iterator<Item = BaseRecord<'a>> + '_ {
        self.base_records
            .chunks(self.mark_class_count as usize)
            .map(|base_anchor_offsets| BaseRecord {
                base_anchor_offsets,
            })
    }
}

#[cfg(feature = "compile")]
pub mod compile {

    use font_types::{Offset, Offset16, Offset32, OffsetHost};

    use crate::compile::{FontWrite, OffsetMarker, ToOwnedObj, ToOwnedTable};
    use crate::layout::compile::{ChainedSequenceContext, Lookup, SequenceContext};

    pub use super::generated::compile::*;
    pub use super::ValueRecord;

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

    #[derive(Debug, PartialEq)]
    pub struct BaseArray {
        pub base_records: Vec<BaseRecord>,
    }

    #[derive(Debug, PartialEq)]
    pub struct BaseRecord {
        pub base_anchor_offsets: Vec<OffsetMarker<Offset16, AnchorTable>>,
    }

    impl ToOwnedObj for super::BaseRecord<'_> {
        type Owned = BaseRecord;

        fn to_owned_obj(&self, offset_data: &[u8]) -> Option<Self::Owned> {
            let base_anchor_offsets = self
                .base_anchor_offsets
                .iter()
                .map(|off| {
                    off.get()
                        .read::<super::AnchorTable>(offset_data)
                        .and_then(|x| x.to_owned_obj(offset_data))
                        .map(OffsetMarker::new)
                })
                .collect::<Option<_>>()?;
            Some(BaseRecord {
                base_anchor_offsets,
            })
        }
    }

    impl ToOwnedObj for super::BaseArray<'_> {
        type Owned = BaseArray;

        fn to_owned_obj(&self, offset_data: &[u8]) -> Option<Self::Owned> {
            self.base_records()
                .map(|x| x.to_owned_obj(offset_data))
                .collect::<Option<Vec<_>>>()
                .map(|base_records| BaseArray { base_records })
        }
    }

    impl FontWrite for BaseRecord {
        fn write_into(&self, writer: &mut crate::compile::TableWriter) {
            self.base_anchor_offsets.write_into(writer);
        }
    }

    impl FontWrite for BaseArray {
        fn write_into(&self, writer: &mut crate::compile::TableWriter) {
            u16::try_from(self.base_records.len())
                .unwrap()
                .write_into(writer);
            self.base_records.write_into(writer)
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
