//! the [GPOS] table
//!
//! [GPOS]: https://docs.microsoft.com/en-us/typography/opentype/spec/gpos

#[path = "./value_record.rs"]
mod value_record;

#[cfg(feature = "std")]
mod closure;

use crate::array::ComputedArray;

/// reexport stuff from layout that we use
pub use super::layout::{
    ClassDef, CoverageTable, Device, DeviceOrVariationIndex, FeatureList, FeatureVariations,
    Lookup, ScriptList,
};
use super::layout::{ExtensionLookup, LookupFlag, Subtables};
pub use value_record::{Value, ValueContext, ValueRecord, ValueRecordRef};

#[cfg(test)]
#[path = "../tests/test_gpos.rs"]
mod spec_tests;

include!("../../generated/generated_gpos.rs");

/// A typed GPOS [LookupList](super::layout::LookupList) table
pub type PositionLookupList<'a> = super::layout::LookupList<'a, PositionLookup<'a>>;

/// A GPOS [SequenceContext](super::layout::SequenceContext)
pub type PositionSequenceContext<'a> = super::layout::SequenceContext<'a>;

/// A GPOS [ChainedSequenceContext](super::layout::ChainedSequenceContext)
pub type PositionChainContext<'a> = super::layout::ChainedSequenceContext<'a>;

impl<'a> AnchorTable<'a> {
    /// Attempt to resolve the `Device` or `VariationIndex` table for the
    /// x_coordinate, if present
    pub fn x_device(&self) -> Option<Result<DeviceOrVariationIndex<'a>, ReadError>> {
        match self {
            AnchorTable::Format3(inner) => inner.x_device(),
            _ => None,
        }
    }

    /// Attempt to resolve the `Device` or `VariationIndex` table for the
    /// y_coordinate, if present
    pub fn y_device(&self) -> Option<Result<DeviceOrVariationIndex<'a>, ReadError>> {
        match self {
            AnchorTable::Format3(inner) => inner.y_device(),
            _ => None,
        }
    }
}

impl<'a, T: FontRead<'a, Args = ()>> ExtensionLookup<'a, T> for ExtensionPosFormat1<'a, T> {
    fn extension(&self) -> Result<T, ReadError> {
        self.extension()
    }
}

type PosSubtables<'a, T> = Subtables<'a, T, ExtensionPosFormat1<'a, T>>;

/// The subtables from a GPOS lookup.
///
/// This type is a convenience that removes the need to dig into the
/// [`PositionLookup`] enum in order to access subtables, and it also abstracts
/// away the distinction between extension and non-extension lookups.
pub enum PositionSubtables<'a> {
    Single(PosSubtables<'a, SinglePos<'a>>),
    Pair(PosSubtables<'a, PairPos<'a>>),
    Cursive(PosSubtables<'a, CursivePosFormat1<'a>>),
    MarkToBase(PosSubtables<'a, MarkBasePosFormat1<'a>>),
    MarkToLig(PosSubtables<'a, MarkLigPosFormat1<'a>>),
    MarkToMark(PosSubtables<'a, MarkMarkPosFormat1<'a>>),
    Contextual(PosSubtables<'a, PositionSequenceContext<'a>>),
    ChainContextual(PosSubtables<'a, PositionChainContext<'a>>),
    /// An extension lookup did not have any subtables
    EmptyExtension,
}

impl<'a> PositionLookup<'a> {
    pub fn lookup_flag(&self) -> LookupFlag {
        self.of_unit_type().lookup_flag()
    }

    /// Different enumerations for GSUB and GPOS
    pub fn lookup_type(&self) -> u16 {
        self.of_unit_type().lookup_type()
    }

    pub fn mark_filtering_set(&self) -> Option<u16> {
        self.of_unit_type().mark_filtering_set()
    }

    /// Return the subtables for this lookup.
    ///
    /// This method handles both extension and non-extension lookups, and saves
    /// the caller needing to dig into the `PositionLookup` enum itself.
    pub fn subtables(&self) -> Result<PositionSubtables<'a>, ReadError> {
        let raw_lookup = self.of_unit_type();
        let offsets = raw_lookup.subtable_offsets();
        let data = raw_lookup.offset_data();
        match raw_lookup.lookup_type() {
            1 => Ok(PositionSubtables::Single(Subtables::new(offsets, data))),
            2 => Ok(PositionSubtables::Pair(Subtables::new(offsets, data))),
            3 => Ok(PositionSubtables::Cursive(Subtables::new(offsets, data))),
            4 => Ok(PositionSubtables::MarkToBase(Subtables::new(offsets, data))),
            5 => Ok(PositionSubtables::MarkToLig(Subtables::new(offsets, data))),
            6 => Ok(PositionSubtables::MarkToMark(Subtables::new(offsets, data))),
            7 => Ok(PositionSubtables::Contextual(Subtables::new(offsets, data))),
            8 => Ok(PositionSubtables::ChainContextual(Subtables::new(
                offsets, data,
            ))),
            9 => {
                // look through subtable offsets to try and find a lookup type.
                // this is robust in the case where the first subtable offset is
                // malformed, but a later one is okay.
                let Some(lookup_type) = offsets.iter().find_map(|off| {
                    off.get()
                        .resolve::<ExtensionPosFormat1<()>>(data)
                        .ok()
                        .map(|ext| ext.extension_lookup_type())
                }) else {
                    return Ok(PositionSubtables::EmptyExtension);
                };

                match lookup_type {
                    1 => Ok(PositionSubtables::Single(Subtables::new_ext(offsets, data))),
                    2 => Ok(PositionSubtables::Pair(Subtables::new_ext(offsets, data))),
                    3 => Ok(PositionSubtables::Cursive(Subtables::new_ext(
                        offsets, data,
                    ))),
                    4 => Ok(PositionSubtables::MarkToBase(Subtables::new_ext(
                        offsets, data,
                    ))),
                    5 => Ok(PositionSubtables::MarkToLig(Subtables::new_ext(
                        offsets, data,
                    ))),
                    6 => Ok(PositionSubtables::MarkToMark(Subtables::new_ext(
                        offsets, data,
                    ))),
                    7 => Ok(PositionSubtables::Contextual(Subtables::new_ext(
                        offsets, data,
                    ))),
                    8 => Ok(PositionSubtables::ChainContextual(Subtables::new_ext(
                        offsets, data,
                    ))),
                    other => Err(ReadError::InvalidFormat(other as _)),
                }
            }
            other => Err(ReadError::InvalidFormat(other as _)),
        }
    }
}

impl<'a> SinglePosFormat1<'a> {
    /// Returns the value record for this subtable, without reading it.
    #[inline]
    pub fn value_record_ref(&self) -> ValueRecordRef<'a> {
        ValueRecordRef::new(
            self.offset_data(),
            self.value_record_byte_range().start,
            self.value_format(),
        )
    }
}

impl<'a> SinglePosFormat2<'a> {
    /// Returns the value record at `index`, without reading it.
    ///
    /// `index` is a coverage index; returns `None` if it is out of range.
    #[inline]
    pub fn value_record_ref(&self, index: usize) -> Option<ValueRecordRef<'a>> {
        if index >= self.value_count() as usize {
            return None;
        }
        let format = self.value_format();
        let offset =
            self.value_records_byte_range().start + index.checked_mul(format.record_byte_len())?;
        Some(ValueRecordRef::new(self.offset_data(), offset, format))
    }
}

/// A [`PairValueRecord`] whose value records are read on demand.
#[derive(Copy, Clone)]
pub struct PairValueRecordRef<'a> {
    /// The offset data of the containing [`PairSet`].
    data: FontData<'a>,
    offset: u32,
    format1: ValueFormat,
    format2: ValueFormat,
}

impl<'a> PairValueRecordRef<'a> {
    /// Glyph ID of the second glyph in the pair.
    #[inline]
    pub fn second_glyph(&self) -> GlyphId16 {
        self.data
            .read_at(self.offset as usize)
            .unwrap_or_else(|_| GlyphId16::new(0))
    }

    /// Positioning for the first glyph in the pair.
    #[inline]
    pub fn value_record1(&self) -> ValueRecordRef<'a> {
        ValueRecordRef::new(
            self.data,
            self.offset as usize + GlyphId16::RAW_BYTE_LEN,
            self.format1,
        )
    }

    /// Positioning for the second glyph in the pair.
    #[inline]
    pub fn value_record2(&self) -> ValueRecordRef<'a> {
        ValueRecordRef::new(
            self.data,
            self.offset as usize + GlyphId16::RAW_BYTE_LEN + self.format1.record_byte_len(),
            self.format2,
        )
    }
}

/// The pair value records of a [`PairSet`], located but not read.
///
/// Records are addressed by a fixed stride derived from the two value formats,
/// so locating one costs a multiply and no reads. This makes the array cheap to
/// scan and cheap to binary search on
/// [`second_glyph`](PairValueRecordRef::second_glyph).
#[derive(Copy, Clone)]
pub struct PairValueRecordRefs<'a> {
    data: FontData<'a>,
    start: u32,
    stride: u32,
    len: u32,
    format1: ValueFormat,
    format2: ValueFormat,
}

impl<'a> PairValueRecordRefs<'a> {
    /// The number of records in the array.
    #[inline]
    pub fn len(&self) -> usize {
        self.len as usize
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns the record at `index`, or `None` if out of range.
    #[inline]
    pub fn get(&self, index: usize) -> Option<PairValueRecordRef<'a>> {
        if index >= self.len as usize {
            return None;
        }
        Some(PairValueRecordRef {
            data: self.data,
            offset: self.start + self.stride * index as u32,
            format1: self.format1,
            format2: self.format2,
        })
    }

    /// Returns an iterator over the records.
    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = PairValueRecordRef<'a>> + 'a + Clone {
        let this = *self;
        (0..self.len as usize).map(move |i| PairValueRecordRef {
            data: this.data,
            offset: this.start + this.stride * i as u32,
            format1: this.format1,
            format2: this.format2,
        })
    }

    /// Returns the record for `second_glyph`, using a binary search.
    ///
    /// Pair value records are ordered by second glyph, so this reads only the
    /// glyph ID of each probed record.
    pub fn find(&self, second_glyph: GlyphId16) -> Option<PairValueRecordRef<'a>> {
        let (mut lo, mut hi) = (0usize, self.len as usize);
        while lo < hi {
            // deliberately not usize::midpoint, which widens to u128; these
            // values are bounded by a u16 count
            #[allow(clippy::manual_midpoint)]
            let mid = (lo + hi) / 2;
            let record = self.get(mid)?;
            match record.second_glyph().cmp(&second_glyph) {
                core::cmp::Ordering::Less => lo = mid + 1,
                core::cmp::Ordering::Greater => hi = mid,
                core::cmp::Ordering::Equal => return Some(record),
            }
        }
        None
    }
}

impl<'a> PairSet<'a> {
    /// Returns the pair value records, located but not read.
    ///
    /// Unlike [`pair_value_records`][Self::pair_value_records], reading a
    /// record's fields is deferred until they are asked for, so scanning or
    /// searching the array does not build any value records.
    pub fn pair_value_record_refs(&self) -> PairValueRecordRefs<'a> {
        let format1 = self.value_format1();
        let format2 = self.value_format2();
        let range = self.pair_value_records_byte_range();
        let stride =
            GlyphId16::RAW_BYTE_LEN + format1.record_byte_len() + format2.record_byte_len();
        // matches `pair_value_records`, which yields nothing at all when the
        // declared array doesn't fit in the available data
        let len = match (stride, self.data.slice(range.clone())) {
            (0, _) | (_, None) => 0,
            (stride, Some(_)) => (range.end - range.start) / stride,
        };
        PairValueRecordRefs {
            data: self.data,
            start: range.start as u32,
            stride: stride as u32,
            len: len as u32,
            format1,
            format2,
        }
    }
}

/// The two-dimensional array of value records in a [`PairPosFormat2`], located
/// but not read.
///
/// The class counts, value formats and record stride are read once when this is
/// built, so addressing a record afterwards is pure arithmetic. Callers walking
/// many class pairs should build this once and reuse it rather than going
/// through [`PairPosFormat2::value_record_refs`], which rebuilds it every call.
#[derive(Copy, Clone)]
pub struct ClassValueRecords<'a> {
    data: FontData<'a>,
    start: u32,
    record_size: u32,
    format1_len: u32,
    class1_count: u16,
    class2_count: u16,
    format1: ValueFormat,
    format2: ValueFormat,
}

impl<'a> ClassValueRecords<'a> {
    /// Number of classes in classDef1, including class 0.
    #[inline]
    pub fn class1_count(&self) -> u16 {
        self.class1_count
    }

    /// Number of classes in classDef2, including class 0.
    #[inline]
    pub fn class2_count(&self) -> u16 {
        self.class2_count
    }

    /// Returns the pair of value records for the given classes.
    ///
    /// Returns `None` if either class is out of range.
    #[inline]
    pub fn get(&self, class1: u16, class2: u16) -> Option<[ValueRecordRef<'a>; 2]> {
        if class1 >= self.class1_count || class2 >= self.class2_count {
            return None;
        }
        // Compute an offset into the 2D array of positioning records. Every
        // term is bounded by a u16 count times a u16-derived stride, so this
        // cannot overflow usize on any supported target.
        let record_offset = self.start as usize
            + class1 as usize * self.record_size as usize * self.class2_count as usize
            + class2 as usize * self.record_size as usize;
        Some([
            ValueRecordRef::new(self.data, record_offset, self.format1),
            ValueRecordRef::new(
                self.data,
                record_offset + self.format1_len as usize,
                self.format2,
            ),
        ])
    }
}

impl<'a> PairPosFormat2<'a> {
    /// Returns the two-dimensional array of class value records, located but
    /// not read.
    pub fn class_value_records(&self) -> ClassValueRecords<'a> {
        let format1 = self.value_format1();
        let format2 = self.value_format2();
        let format1_len = format1.record_byte_len();
        ClassValueRecords {
            data: self.offset_data(),
            start: self.class1_records_byte_range().start as u32,
            record_size: (format1_len + format2.record_byte_len()) as u32,
            format1_len: format1_len as u32,
            class1_count: self.class1_count(),
            class2_count: self.class2_count(),
            format1,
            format2,
        }
    }

    /// Returns the pair of value records for the given classes, located but
    /// not read.
    ///
    /// Returns `None` if either class is out of range. When reading more than
    /// one class pair, prefer [`class_value_records`][Self::class_value_records],
    /// which reads the table's counts and formats only once.
    #[inline]
    pub fn value_record_refs(&self, class1: u16, class2: u16) -> Option<[ValueRecordRef<'a>; 2]> {
        self.class_value_records().get(class1, class2)
    }
}

impl PairPosFormat2<'_> {
    /// Returns the pair of values for the given classes, optionally accounting
    /// for variations.
    ///
    /// The `class1` and `class2` parameters can be computed by passing the
    /// first and second glyphs of the pair to the [`ClassDef`]s returned by
    /// [`Self::class_def1`] and [`Self::class_def2`] respectively.
    #[inline]
    pub fn values(
        &self,
        class1: u16,
        class2: u16,
        context: &ValueContext,
    ) -> Result<[Value; 2], ReadError> {
        let format1 = self.value_format1();
        let format1_len = format1.record_byte_len();
        let format2 = self.value_format2();
        let record_size = format1_len + format2.record_byte_len();
        let data = self.offset_data();
        // Compute an offset into the 2D array of positioning records
        let record_offset = (class1 as usize * record_size * self.class2_count() as usize)
            + (class2 as usize * record_size)
            + self.class1_records_byte_range().start;
        Ok([
            Value::read(data, record_offset, format1, context)?,
            Value::read(data, record_offset + format1_len, format2, context)?,
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pair_pos2_values_match_value_records() {
        let data = FontData::new(font_test_data::gpos::PAIRPOSFORMAT2);
        let table = PairPosFormat2::read(data).unwrap();
        let class1_count = table.class1_count();
        let class2_count = table.class2_count();
        let records = table.class1_records();
        let context = ValueContext::default();
        for class1 in 0..class1_count {
            let class1_record = records.get(class1 as usize).unwrap();
            let class2_records = class1_record.class2_records();
            for class2 in 0..class2_count {
                let record = class2_records.get(class2 as usize).unwrap();
                let value_records = [record.value_record1, record.value_record2]
                    .map(|rec| rec.value(data, &context).unwrap());
                let values = table.values(class1, class2, &context).unwrap();
                assert_eq!(value_records, values);
            }
        }
    }

    /// The lazy accessors must locate exactly the records the generated
    /// accessors read.
    fn assert_same_record(lazy: ValueRecordRef, owned: &ValueRecord, data: FontData) {
        assert_eq!(lazy.format(), owned.format);
        assert_eq!(lazy.x_placement(), owned.x_placement());
        assert_eq!(lazy.y_placement(), owned.y_placement());
        assert_eq!(lazy.x_advance(), owned.x_advance());
        assert_eq!(lazy.y_advance(), owned.y_advance());
        // the resolved device tables aren't comparable, but they're determined
        // by the raw offset and the data they resolve against, so compare those
        // and check that the lazy record's base actually resolves.
        for (field, lazy_device, owned_offset, owned_device) in [
            (
                ValueFormat::X_PLACEMENT_DEVICE,
                lazy.x_placement_device(),
                owned.x_placement_device,
                owned.x_placement_device(data),
            ),
            (
                ValueFormat::Y_PLACEMENT_DEVICE,
                lazy.y_placement_device(),
                owned.y_placement_device,
                owned.y_placement_device(data),
            ),
            (
                ValueFormat::X_ADVANCE_DEVICE,
                lazy.x_advance_device(),
                owned.x_advance_device,
                owned.x_advance_device(data),
            ),
            (
                ValueFormat::Y_ADVANCE_DEVICE,
                lazy.y_advance_device(),
                owned.y_advance_device,
                owned.y_advance_device(data),
            ),
        ] {
            let expected = lazy.format().contains(field).then_some(owned_offset.get());
            assert_eq!(lazy.device_offset(field), expected, "{field:?}");
            assert_eq!(lazy_device.is_some(), owned_device.is_some(), "{field:?}");
            if let Some(resolved) = lazy_device {
                assert!(resolved.is_ok(), "{field:?} failed to resolve");
            }
        }
    }

    #[test]
    fn single_pos1_lazy_matches_generated() {
        let data = FontData::new(font_test_data::gpos::SINGLEPOSFORMAT1);
        let table = SinglePosFormat1::read(data).unwrap();
        assert_same_record(table.value_record_ref(), &table.value_record(), data);
    }

    /// Exercises a record carrying device tables, whose offsets resolve against
    /// the containing table rather than the record.
    #[test]
    fn single_pos1_lazy_matches_generated_with_devices() {
        let data = FontData::new(font_test_data::gpos::VALUEFORMATTABLE);
        let table = SinglePosFormat1::read(data).unwrap();
        let lazy = table.value_record_ref();
        assert!(lazy.format().intersects(ValueFormat::ANY_DEVICE_OR_VARIDX));
        assert!(lazy.x_placement_device().is_some());
        assert_same_record(lazy, &table.value_record(), data);
    }

    #[test]
    fn single_pos2_lazy_matches_generated() {
        let data = FontData::new(font_test_data::gpos::SINGLEPOSFORMAT2);
        let table = SinglePosFormat2::read(data).unwrap();
        let records = table.value_records();
        assert!(!records.is_empty());
        for i in 0..records.len() {
            assert_same_record(
                table.value_record_ref(i).unwrap(),
                &records.get(i).unwrap(),
                data,
            );
        }
        assert!(table.value_record_ref(records.len()).is_none());
    }

    #[test]
    fn pair_set_lazy_matches_generated() {
        let data = FontData::new(font_test_data::gpos::PAIRPOSFORMAT1);
        let table = PairPosFormat1::read(data).unwrap();
        let mut seen = 0;
        for set_offset in table.pair_set_offsets() {
            let pair_set = set_offset
                .get()
                .resolve_with_args::<PairSet>(data, (table.value_format1(), table.value_format2()))
                .unwrap();
            let set_data = pair_set.offset_data();
            let owned = pair_set.pair_value_records();
            let lazy = pair_set.pair_value_record_refs();
            assert_eq!(lazy.len(), owned.len());
            assert!(!lazy.is_empty());
            for (i, lazy_rec) in lazy.iter().enumerate() {
                let owned_rec = owned.get(i).unwrap();
                assert_eq!(lazy_rec.second_glyph(), owned_rec.second_glyph());
                assert_same_record(lazy_rec.value_record1(), &owned_rec.value_record1, set_data);
                assert_same_record(lazy_rec.value_record2(), &owned_rec.value_record2, set_data);

                // and the binary search finds the same record
                let found = lazy.find(owned_rec.second_glyph()).unwrap();
                assert_eq!(found.second_glyph(), owned_rec.second_glyph());
                seen += 1;
            }
            assert!(lazy.find(GlyphId16::new(0xFFFF)).is_none());
        }
        assert!(seen > 0);
    }

    #[test]
    fn pair_pos2_lazy_matches_generated() {
        let data = FontData::new(font_test_data::gpos::PAIRPOSFORMAT2);
        let table = PairPosFormat2::read(data).unwrap();
        let class1_count = table.class1_count();
        let class2_count = table.class2_count();
        let records = table.class1_records();
        for class1 in 0..class1_count {
            let class2_records = records
                .get(class1 as usize)
                .unwrap()
                .class2_records()
                .clone();
            for class2 in 0..class2_count {
                let owned = class2_records.get(class2 as usize).unwrap();
                let [lazy1, lazy2] = table.value_record_refs(class1, class2).unwrap();
                assert_same_record(lazy1, &owned.value_record1, data);
                assert_same_record(lazy2, &owned.value_record2, data);
            }
        }
        assert!(table.value_record_refs(class1_count, 0).is_none());
        assert!(table.value_record_refs(0, class2_count).is_none());
    }

    #[test]
    fn default_for_generics() {
        let ExtensionSubtable::Single(inner) = ExtensionSubtable::default() else {
            panic!("this is quite bad");
        };

        // this is invalid, but we the default impl for the extension offset
        // will be the first variant of the enum anyway
        assert_eq!(inner.extension_lookup_type(), 0);

        let SinglePos::Format1(_hmm) = inner.extension().unwrap_or_default() else {
            panic!("unexpected");
        };
    }
}
