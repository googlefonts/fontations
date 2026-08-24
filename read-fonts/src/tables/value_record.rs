//! A GPOS ValueRecord

use font_types::Nullable;
use types::{BigEndian, F2Dot14, FixedSize, Offset16};

use super::ValueFormat;
use crate::{
    tables::{
        layout::DeviceOrVariationIndex,
        variations::{DeltaSetIndex, ItemVariationStore},
    },
    ResolveNullableOffset,
};

#[cfg(feature = "experimental_traverse")]
use crate::traversal::{Field, FieldType, RecordResolver, SomeRecord};
use crate::{ComputeSize, FontData, FontRead, ReadArgs, ReadError};

impl ValueFormat {
    /// A mask with all the device/variation index bits set
    pub const ANY_DEVICE_OR_VARIDX: Self = ValueFormat {
        bits: 0x0010 | 0x0020 | 0x0040 | 0x0080,
    };

    /// Return the number of bytes required to store a [`ValueRecord`] in this format.
    #[inline]
    pub fn record_byte_len(self) -> usize {
        self.bits().count_ones() as usize * u16::RAW_BYTE_LEN
    }
}

/// A context for resolving [`Value`]s and [`ValueRecord`]s.
///
/// In particular, this handles processing of the embedded
/// [`DeviceOrVariationIndex`] tables.
#[derive(Clone, Default)]
pub struct ValueContext<'a> {
    coords: &'a [F2Dot14],
    var_store: Option<ItemVariationStore<'a>>,
}

impl<'a> ValueContext<'a> {
    /// Creates a new value context that doesn't do any additional processing.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the normalized variation coordinates for this value context.
    pub fn with_coords(mut self, coords: &'a [F2Dot14]) -> Self {
        self.coords = coords;
        self
    }

    /// Sets the item variation store for this value context.
    ///
    /// This comes from the [`Gdef`](super::super::gdef::Gdef) table.
    pub fn with_var_store(mut self, var_store: Option<ItemVariationStore<'a>>) -> Self {
        self.var_store = var_store;
        self
    }

    fn var_store_and_coords(&self) -> Option<(&ItemVariationStore<'a>, &'a [F2Dot14])> {
        Some((self.var_store.as_ref()?, self.coords))
    }
}

/// A fully resolved [`ValueRecord`].
#[derive(Copy, Clone, PartialEq, Eq, Default, Debug)]
pub struct Value {
    pub format: ValueFormat,
    pub x_placement: i16,
    pub y_placement: i16,
    pub x_advance: i16,
    pub y_advance: i16,
    pub x_placement_delta: i32,
    pub y_placement_delta: i32,
    pub x_advance_delta: i32,
    pub y_advance_delta: i32,
}

impl Value {
    /// Reads a value directly from font data.
    ///
    /// The `offset_data` parameter must be the offset data for the table
    /// containing the value record.
    #[inline]
    pub fn read(
        offset_data: FontData,
        offset: usize,
        format: ValueFormat,
        context: &ValueContext,
    ) -> Result<Self, ReadError> {
        let mut value = Self {
            format,
            ..Default::default()
        };
        let mut cursor = offset_data.cursor();
        cursor.advance_by(offset);
        if format.contains(ValueFormat::X_PLACEMENT) {
            value.x_placement = cursor.read()?;
        }
        if format.contains(ValueFormat::Y_PLACEMENT) {
            value.y_placement = cursor.read()?;
        }
        if format.contains(ValueFormat::X_ADVANCE) {
            value.x_advance = cursor.read()?;
        }
        if format.contains(ValueFormat::Y_ADVANCE) {
            value.y_advance = cursor.read()?;
        }
        if !format.contains(ValueFormat::ANY_DEVICE_OR_VARIDX) {
            return Ok(value);
        }
        if let Some((ivs, coords)) = context.var_store_and_coords() {
            let compute_delta = |offset: u16| {
                let rec_offset = offset_data.read_at::<u16>(offset as usize).ok()? as usize;
                let format = offset_data.read_at::<u16>(rec_offset + 4).ok()?;
                // DeltaFormat specifier for a VariationIndex table
                // See <https://learn.microsoft.com/en-us/typography/opentype/spec/chapter2#device-and-variationindex-tables>
                const VARIATION_INDEX_FORMAT: u16 = 0x8000;
                if format != VARIATION_INDEX_FORMAT {
                    return Some(0);
                }
                let outer = offset_data.read_at::<u16>(rec_offset).ok()?;
                let inner = offset_data.read_at::<u16>(rec_offset + 2).ok()?;
                ivs.compute_delta(DeltaSetIndex { outer, inner }, coords)
                    .ok()
            };
            if format.contains(ValueFormat::X_PLACEMENT_DEVICE) {
                value.x_placement_delta = compute_delta(cursor.read()?).unwrap_or_default();
            }
            if format.contains(ValueFormat::Y_PLACEMENT_DEVICE) {
                value.y_placement_delta = compute_delta(cursor.read()?).unwrap_or_default();
            }
            if format.contains(ValueFormat::X_ADVANCE_DEVICE) {
                value.x_advance_delta = compute_delta(cursor.read()?).unwrap_or_default();
            }
            if format.contains(ValueFormat::Y_ADVANCE_DEVICE) {
                value.y_advance_delta = compute_delta(cursor.read()?).unwrap_or_default();
            }
        }
        Ok(value)
    }
}

/// A GPOS ValueRecord, with fields read on demand.
///
/// The contents of a value record are described by a [`ValueFormat`] stored in
/// the parent table, so a record cannot be located or interpreted on its own.
/// This type stores the position of the record within its parent table's data
/// alongside that format, and reads individual fields only when they are asked
/// for; constructing one performs no reads at all.
#[derive(Copy, Clone)]
pub struct ValueRecordRef<'a> {
    /// The offset data of the table *containing* the record.
    data: FontData<'a>,
    /// The position of the record within `data`.
    offset: u32,
    format: ValueFormat,
}

impl<'a> ValueRecordRef<'a> {
    /// Creates a value record positioned at `offset` within `data`.
    ///
    /// `data` must be the offset data of the table containing the record, and
    /// not merely the bytes of the record itself: the device and variation
    /// index offsets in a value record are resolved relative to that table.
    #[inline]
    pub fn new(data: FontData<'a>, offset: usize, format: ValueFormat) -> Self {
        Self {
            data,
            // an offset that doesn't fit is out of bounds by definition, and
            // saturating here lets every subsequent read fail cleanly
            offset: u32::try_from(offset).unwrap_or(u32::MAX),
            format,
        }
    }

    /// The format describing which fields this record contains.
    #[inline]
    pub fn format(&self) -> ValueFormat {
        self.format
    }

    /// The number of bytes occupied by this record.
    #[inline]
    pub fn byte_len(&self) -> usize {
        self.format.record_byte_len()
    }

    /// Returns `true` if this record contains no fields.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.format.is_empty()
    }

    /// The offset data of the table containing this record.
    #[inline]
    pub fn offset_data(&self) -> FontData<'a> {
        self.data
    }

    /// The position of this record within [`offset_data`](Self::offset_data).
    #[inline]
    pub fn offset(&self) -> usize {
        self.offset as usize
    }

    #[inline]
    pub fn x_placement(&self) -> Option<i16> {
        self.read_i16(ValueFormat::X_PLACEMENT)
    }

    #[inline]
    pub fn y_placement(&self) -> Option<i16> {
        self.read_i16(ValueFormat::Y_PLACEMENT)
    }

    #[inline]
    pub fn x_advance(&self) -> Option<i16> {
        self.read_i16(ValueFormat::X_ADVANCE)
    }

    #[inline]
    pub fn y_advance(&self) -> Option<i16> {
        self.read_i16(ValueFormat::Y_ADVANCE)
    }

    #[inline]
    pub fn x_placement_device(&self) -> Option<Result<DeviceOrVariationIndex<'a>, ReadError>> {
        self.read_device(ValueFormat::X_PLACEMENT_DEVICE)
    }

    #[inline]
    pub fn y_placement_device(&self) -> Option<Result<DeviceOrVariationIndex<'a>, ReadError>> {
        self.read_device(ValueFormat::Y_PLACEMENT_DEVICE)
    }

    #[inline]
    pub fn x_advance_device(&self) -> Option<Result<DeviceOrVariationIndex<'a>, ReadError>> {
        self.read_device(ValueFormat::X_ADVANCE_DEVICE)
    }

    #[inline]
    pub fn y_advance_device(&self) -> Option<Result<DeviceOrVariationIndex<'a>, ReadError>> {
        self.read_device(ValueFormat::Y_ADVANCE_DEVICE)
    }

    /// Returns the raw, unresolved offset for the given device field.
    ///
    /// The returned offset is relative to [`offset_data`](Self::offset_data).
    /// Returns `None` if the field is not present in this record's format, or
    /// if the record is truncated.
    #[inline]
    pub fn device_offset(&self, field: ValueFormat) -> Option<Nullable<Offset16>> {
        self.data.read_at(self.field_offset(field)?).ok()
    }

    /// The raw bytes of this record, or `None` if the data is truncated.
    #[inline]
    pub fn bytes(&self) -> Option<&'a [u8]> {
        let start = self.offset as usize;
        self.data
            .as_bytes()
            .get(start..start.checked_add(self.byte_len())?)
    }

    /// Returns the position of `field` within [`offset_data`](Self::offset_data),
    /// or `None` if this record's format doesn't include it.
    ///
    /// Fields are laid out in the order of their format bits and each occupies
    /// two bytes, so a field's position is fixed by the number of lower format
    /// bits that are set.
    #[inline]
    fn field_offset(&self, field: ValueFormat) -> Option<usize> {
        if !self.format.contains(field) {
            return None;
        }
        let preceding = (self.format.bits() & (field.bits() - 1)).count_ones() as usize;
        Some(self.offset as usize + preceding * u16::RAW_BYTE_LEN)
    }

    #[inline]
    fn read_i16(&self, field: ValueFormat) -> Option<i16> {
        self.data.read_at(self.field_offset(field)?).ok()
    }

    #[inline]
    fn read_device(
        &self,
        field: ValueFormat,
    ) -> Option<Result<DeviceOrVariationIndex<'a>, ReadError>> {
        let pos = self.field_offset(field)?;
        match self.data.read_at::<Nullable<Offset16>>(pos) {
            Ok(offset) => offset.resolve(self.data),
            Err(err) => Some(Err(err)),
        }
    }
}

/// Two records are equal when they describe the same positioning: same format,
/// and the same bytes for the fields that format selects.
impl PartialEq for ValueRecordRef<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.format == other.format && self.bytes() == other.bytes()
    }
}

impl Eq for ValueRecordRef<'_> {}

impl std::fmt::Debug for ValueRecordRef<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let mut f = f.debug_struct("ValueRecordRef");
        self.x_placement().map(|x| f.field("x_placement", &x));
        self.y_placement().map(|y| f.field("y_placement", &y));
        self.x_advance().map(|x| f.field("x_advance", &x));
        self.y_advance().map(|y| f.field("y_advance", &y));
        for (name, field) in [
            ("x_placement_device", ValueFormat::X_PLACEMENT_DEVICE),
            ("y_placement_device", ValueFormat::Y_PLACEMENT_DEVICE),
            ("x_advance_device", ValueFormat::X_ADVANCE_DEVICE),
            ("y_advance_device", ValueFormat::Y_ADVANCE_DEVICE),
        ] {
            match self.device_offset(field) {
                Some(offset) if !offset.is_null() => {
                    f.field(name, &offset);
                }
                _ => (),
            }
        }
        f.finish()
    }
}

/// A Positioning ValueRecord.
///
/// NOTE: we create these manually, since parsing is weird and depends on the
/// associated valueformat. That said, this isn't a great representation?
/// we could definitely do something much more in the zero-copy mode..
#[derive(Clone, Default, Eq)]
pub struct ValueRecord {
    pub x_placement: Option<BigEndian<i16>>,
    pub y_placement: Option<BigEndian<i16>>,
    pub x_advance: Option<BigEndian<i16>>,
    pub y_advance: Option<BigEndian<i16>>,
    pub x_placement_device: BigEndian<Nullable<Offset16>>,
    pub y_placement_device: BigEndian<Nullable<Offset16>>,
    pub x_advance_device: BigEndian<Nullable<Offset16>>,
    pub y_advance_device: BigEndian<Nullable<Offset16>>,
    #[doc(hidden)]
    // exposed so that we can preserve format when we round-trip a value record
    pub format: ValueFormat,
}

// we ignore the format for the purpose of equality testing, it's redundant
impl PartialEq for ValueRecord {
    fn eq(&self, other: &Self) -> bool {
        self.x_placement == other.x_placement
            && self.y_placement == other.y_placement
            && self.x_advance == other.x_advance
            && self.y_advance == other.y_advance
            && self.x_placement_device == other.x_placement_device
            && self.y_placement_device == other.y_placement_device
            && self.x_advance_device == other.x_advance_device
            && self.y_advance_device == other.y_advance_device
    }
}

impl ValueRecord {
    pub fn read(data: FontData, format: ValueFormat) -> Result<Self, ReadError> {
        let mut this = ValueRecord {
            format,
            ..Default::default()
        };
        let mut cursor = data.cursor();

        if format.contains(ValueFormat::X_PLACEMENT) {
            this.x_placement = Some(cursor.read_be()?);
        }
        if format.contains(ValueFormat::Y_PLACEMENT) {
            this.y_placement = Some(cursor.read_be()?);
        }
        if format.contains(ValueFormat::X_ADVANCE) {
            this.x_advance = Some(cursor.read_be()?);
        }
        if format.contains(ValueFormat::Y_ADVANCE) {
            this.y_advance = Some(cursor.read_be()?);
        }
        if format.contains(ValueFormat::X_PLACEMENT_DEVICE) {
            this.x_placement_device = cursor.read_be()?;
        }
        if format.contains(ValueFormat::Y_PLACEMENT_DEVICE) {
            this.y_placement_device = cursor.read_be()?;
        }
        if format.contains(ValueFormat::X_ADVANCE_DEVICE) {
            this.x_advance_device = cursor.read_be()?;
        }
        if format.contains(ValueFormat::Y_ADVANCE_DEVICE) {
            this.y_advance_device = cursor.read_be()?;
        }
        Ok(this)
    }

    pub fn x_placement(&self) -> Option<i16> {
        self.x_placement.map(|val| val.get())
    }

    pub fn y_placement(&self) -> Option<i16> {
        self.y_placement.map(|val| val.get())
    }

    pub fn x_advance(&self) -> Option<i16> {
        self.x_advance.map(|val| val.get())
    }

    pub fn y_advance(&self) -> Option<i16> {
        self.y_advance.map(|val| val.get())
    }

    pub fn x_placement_device<'a>(
        &self,
        data: FontData<'a>,
    ) -> Option<Result<DeviceOrVariationIndex<'a>, ReadError>> {
        self.x_placement_device.get().resolve(data)
    }

    pub fn y_placement_device<'a>(
        &self,
        data: FontData<'a>,
    ) -> Option<Result<DeviceOrVariationIndex<'a>, ReadError>> {
        self.y_placement_device.get().resolve(data)
    }

    pub fn x_advance_device<'a>(
        &self,
        data: FontData<'a>,
    ) -> Option<Result<DeviceOrVariationIndex<'a>, ReadError>> {
        self.x_advance_device.get().resolve(data)
    }

    pub fn y_advance_device<'a>(
        &self,
        data: FontData<'a>,
    ) -> Option<Result<DeviceOrVariationIndex<'a>, ReadError>> {
        self.y_advance_device.get().resolve(data)
    }

    /// Returns a resolved value for the given normalized coordinates and
    /// item variation store.
    ///
    /// The `offset_data` parameter must be the offset data for the table
    /// containing the value record.
    pub fn value(&self, offset_data: FontData, context: &ValueContext) -> Result<Value, ReadError> {
        let mut value = Value {
            format: self.format,
            x_placement: self.x_placement.unwrap_or_default().get(),
            y_placement: self.y_placement.unwrap_or_default().get(),
            x_advance: self.x_advance.unwrap_or_default().get(),
            y_advance: self.y_advance.unwrap_or_default().get(),
            ..Default::default()
        };
        if let Some((ivs, coords)) = context.var_store_and_coords() {
            let compute_delta = |value: DeviceOrVariationIndex| match value {
                DeviceOrVariationIndex::VariationIndex(var_idx) => {
                    let outer = var_idx.delta_set_outer_index();
                    let inner = var_idx.delta_set_inner_index();
                    ivs.compute_delta(DeltaSetIndex { outer, inner }, coords)
                        .ok()
                }
                _ => None,
            };
            if let Some(device) = self.x_placement_device(offset_data) {
                value.x_placement_delta = compute_delta(device?).unwrap_or_default();
            }
            if let Some(device) = self.y_placement_device(offset_data) {
                value.y_placement_delta = compute_delta(device?).unwrap_or_default();
            }
            if let Some(device) = self.x_advance_device(offset_data) {
                value.x_advance_delta = compute_delta(device?).unwrap_or_default();
            }
            if let Some(device) = self.y_advance_device(offset_data) {
                value.y_advance_delta = compute_delta(device?).unwrap_or_default();
            }
        }
        Ok(value)
    }
}

impl ReadArgs for ValueRecord {
    type Args = ValueFormat;
}

impl<'a> FontRead<'a> for ValueRecord {
    fn read_with_args(data: FontData<'a>, args: Self::Args) -> Result<Self, ReadError> {
        ValueRecord::read(data, args)
    }
}

impl std::fmt::Debug for ValueRecord {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let mut f = f.debug_struct("ValueRecord");
        self.x_placement.map(|x| f.field("x_placement", &x));
        self.y_placement.map(|y| f.field("y_placement", &y));
        self.x_advance.map(|x| f.field("x_advance", &x));
        self.y_advance.map(|y| f.field("y_advance", &y));
        if !self.x_placement_device.get().is_null() {
            f.field("x_placement_device", &self.x_placement_device.get());
        }
        if !self.y_placement_device.get().is_null() {
            f.field("y_placement_device", &self.y_placement_device.get());
        }
        if !self.x_advance_device.get().is_null() {
            f.field("x_advance_device", &self.x_advance_device.get());
        }
        if !self.y_advance_device.get().is_null() {
            f.field("y_advance_device", &self.y_advance_device.get());
        }
        f.finish()
    }
}

impl ComputeSize for ValueRecord {
    #[inline]
    fn compute_size(args: ValueFormat) -> Result<usize, ReadError> {
        Ok(args.record_byte_len())
    }
}

#[cfg(feature = "experimental_traverse")]
impl<'a> ValueRecord {
    pub(crate) fn traversal_type(&self, data: FontData<'a>) -> FieldType<'a> {
        FieldType::Record(self.clone().traverse(data))
    }

    pub(crate) fn get_field(&self, idx: usize, data: FontData<'a>) -> Option<Field<'a>> {
        let fields = [
            self.x_placement.is_some().then_some("x_placement"),
            self.y_placement.is_some().then_some("y_placement"),
            self.x_advance.is_some().then_some("x_advance"),
            self.y_advance.is_some().then_some("y_advance"),
            (!self.x_placement_device.get().is_null()).then_some("x_placement_device"),
            (!self.y_placement_device.get().is_null()).then_some("y_placement_device"),
            (!self.x_advance_device.get().is_null()).then_some("x_advance_device"),
            (!self.y_advance_device.get().is_null()).then_some("y_advance_device"),
        ];

        let name = fields.iter().filter_map(|x| *x).nth(idx)?;
        let typ: FieldType = match name {
            "x_placement" => self.x_placement().unwrap().into(),
            "y_placement" => self.y_placement().unwrap().into(),
            "x_advance" => self.x_advance().unwrap().into(),
            "y_advance" => self.y_advance().unwrap().into(),
            "x_placement_device" => {
                FieldType::offset(self.x_placement_device.get(), self.x_placement_device(data))
            }
            "y_placement_device" => {
                FieldType::offset(self.y_placement_device.get(), self.y_placement_device(data))
            }
            "x_advance_device" => {
                FieldType::offset(self.x_advance_device.get(), self.x_advance_device(data))
            }
            "y_advance_device" => {
                FieldType::offset(self.y_advance_device.get(), self.y_advance_device(data))
            }
            _ => panic!("hmm"),
        };

        Some(Field::new(name, typ))
    }
}

#[cfg(feature = "experimental_traverse")]
impl<'a> SomeRecord<'a> for ValueRecord {
    fn traverse(self, data: FontData<'a>) -> RecordResolver<'a> {
        RecordResolver {
            name: "ValueRecord",
            data,
            get_field: Box::new(move |idx, data| self.get_field(idx, data)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanity_check_format_const() {
        let format = ValueFormat::X_ADVANCE_DEVICE
            | ValueFormat::Y_ADVANCE_DEVICE
            | ValueFormat::Y_PLACEMENT_DEVICE
            | ValueFormat::X_PLACEMENT_DEVICE;
        assert_eq!(format, ValueFormat::ANY_DEVICE_OR_VARIDX);
        assert_eq!(format.record_byte_len(), 4 * 2);
    }

    /// The lazy reader locates fields with a popcount over the format bits;
    /// the owned reader walks them in order with a cursor. They must agree for
    /// every combination of format bits.
    #[test]
    fn lazy_fields_match_owned_for_every_format() {
        // give every field slot a distinct, recognizable value
        let bytes: Vec<u8> = (0..8u16).flat_map(|i| (0x1100 + i).to_be_bytes()).collect();
        // pad the front so we exercise a non-zero record offset
        const PAD: usize = 6;
        let mut padded = vec![0u8; PAD];
        padded.extend_from_slice(&bytes);
        let data = FontData::new(&padded);

        for bits in 0..=u8::MAX {
            let format = ValueFormat { bits: bits as u16 };
            let owned = ValueRecord::read(FontData::new(&bytes), format).unwrap();
            let lazy = ValueRecordRef::new(data, PAD, format);

            assert_eq!(lazy.format(), format);
            assert_eq!(lazy.byte_len(), format.record_byte_len());
            assert_eq!(lazy.x_placement(), owned.x_placement(), "{format:?}");
            assert_eq!(lazy.y_placement(), owned.y_placement(), "{format:?}");
            assert_eq!(lazy.x_advance(), owned.x_advance(), "{format:?}");
            assert_eq!(lazy.y_advance(), owned.y_advance(), "{format:?}");

            for (field, expected) in [
                (ValueFormat::X_PLACEMENT_DEVICE, owned.x_placement_device),
                (ValueFormat::Y_PLACEMENT_DEVICE, owned.y_placement_device),
                (ValueFormat::X_ADVANCE_DEVICE, owned.x_advance_device),
                (ValueFormat::Y_ADVANCE_DEVICE, owned.y_advance_device),
            ] {
                // the owned reader leaves absent device fields as null
                let expected = format.contains(field).then_some(expected.get());
                assert_eq!(lazy.device_offset(field), expected, "{format:?} {field:?}");
            }
        }
    }

    /// Reads past the end of the data must fail cleanly rather than panic or
    /// read a neighbouring field.
    #[test]
    fn lazy_fields_out_of_bounds() {
        let format = ValueFormat::X_PLACEMENT | ValueFormat::Y_ADVANCE;
        // only enough room for the first of the two fields
        let bytes = [0u8, 1, 0, 2];
        let lazy = ValueRecordRef::new(FontData::new(&bytes), 2, format);
        assert_eq!(lazy.x_placement(), Some(2));
        assert_eq!(lazy.y_advance(), None);

        // an offset that can't even be represented
        let huge = ValueRecordRef::new(FontData::new(&bytes), usize::MAX, format);
        assert_eq!(huge.x_placement(), None);
    }
}
