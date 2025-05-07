//! The [Extended Kerning (kerx)](https://developer.apple.com/fonts/TrueType-Reference-Manual/RM06/Chap6kerx.html) table.

use super::aat::{safe_read_array_to_end, ExtendedStateTable, LookupU16, LookupU32};

include!("../../generated/generated_kerx.rs");

impl VarSize for Subtable<'_> {
    type Size = u32;

    fn read_len_at(data: FontData, pos: usize) -> Option<usize> {
        // The default implementation assumes that the length field itself
        // is not included in the total size which is not true of this
        // table.
        data.read_at::<u32>(pos).ok().map(|size| size as usize)
    }
}

impl<'a> Subtable<'a> {
    // length, coverage, tuple_count: all u32
    pub const HEADER_SIZE: usize = u32::RAW_BYTE_LEN * 3;

    /// True if the table has vertical kerning values.
    #[inline]
    pub fn is_vertical(&self) -> bool {
        self.coverage() & 0x80000000 != 0
    }

    /// True if the table has horizontal kerning values.    
    #[inline]
    pub fn is_horizontal(&self) -> bool {
        !self.is_vertical()
    }

    /// True if the table has cross-stream kerning values.
    ///
    /// If text is normally written horizontally, adjustments will be
    /// vertical. If adjustment values are positive, the text will be
    /// moved up. If they are negative, the text will be moved down.
    /// If text is normally written vertically, adjustments will be
    /// horizontal. If adjustment values are positive, the text will be
    /// moved to the right. If they are negative, the text will be moved
    /// to the left.
    #[inline]
    pub fn is_cross_stream(&self) -> bool {
        self.coverage() & 0x40000000 != 0
    }

    /// True if the table has variation kerning values.
    #[inline]
    pub fn is_variable(&self) -> bool {
        self.coverage() & 0x20000000 != 0
    }

    /// Process direction flag. If clear, process the glyphs forwards,
    /// that is, from first to last in the glyph stream. If we, process
    /// them from last to first. This flag only applies to state-table
    /// based 'kerx' subtables (types 1 and 4).
    #[inline]
    pub fn process_direction(&self) -> bool {
        self.coverage() & 0x10000000 != 0
    }

    /// Returns an enum representing the actual subtable data.
    pub fn kind(&self) -> Result<SubtableKind<'a>, ReadError> {
        SubtableKind::read_with_args(FontData::new(self.data()), &self.coverage())
    }
}

/// The various `kerx` subtable formats.
#[derive(Clone)]
pub enum SubtableKind<'a> {
    Format0(Subtable0<'a>),
    Format1(Subtable1<'a>),
    Format2(Subtable2<'a>),
    Format4(Subtable4<'a>),
    Format6(Subtable6<'a>),
}

impl ReadArgs for SubtableKind<'_> {
    type Args = u32;
}

impl<'a> FontReadWithArgs<'a> for SubtableKind<'a> {
    fn read_with_args(data: FontData<'a>, args: &Self::Args) -> Result<Self, ReadError> {
        // Format is low byte of coverage
        let format = *args & 0xFF;
        match format {
            0 => Ok(Self::Format0(Subtable0::read(data)?)),
            1 => Ok(Self::Format1(Subtable1::read(data)?)),
            2 => Ok(Self::Format2(Subtable2::read(data)?)),
            // No format 3
            4 => Ok(Self::Format4(Subtable4::read(data)?)),
            // No format 5
            6 => Ok(Self::Format6(Subtable6::read(data)?)),
            _ => Err(ReadError::InvalidFormat(format as _)),
        }
    }
}

impl Subtable0<'_> {
    /// Returns the kerning adjustment for the given pair.
    pub fn kerning(&self, left: GlyphId, right: GlyphId) -> Option<i16> {
        let left: GlyphId16 = left.try_into().ok()?;
        let right: GlyphId16 = right.try_into().ok()?;
        fn make_key(left: GlyphId16, right: GlyphId16) -> u32 {
            left.to_u32() << 16 | right.to_u32()
        }
        let pairs = self.pairs();
        let idx = pairs
            .binary_search_by_key(&make_key(left, right), |pair| {
                make_key(pair.left(), pair.right())
            })
            .ok()?;
        pairs.get(idx).map(|pair| pair.value())
    }
}

/// The type 1 `kerx` subtable.
#[derive(Clone)]
pub struct Subtable1<'a> {
    pub state_table: ExtendedStateTable<'a, BigEndian<u16>>,
    /// Contains the set of kerning values, one for each state.
    pub values: &'a [BigEndian<i16>],
}

impl<'a> FontRead<'a> for Subtable1<'a> {
    fn read(data: FontData<'a>) -> Result<Self, ReadError> {
        let state_table = ExtendedStateTable::read(data)?;
        let mut cursor = data.cursor();
        cursor.advance_by(ExtendedStateTable::<()>::HEADER_LEN);
        let values_offset = cursor.read::<u32>()? as usize;
        let values = super::aat::safe_read_array_to_end(&data, values_offset)?;
        Ok(Self {
            state_table,
            values,
        })
    }
}

/// The type 2 `kerx` subtable.
#[derive(Clone)]
pub struct Subtable2<'a> {
    pub data: FontData<'a>,
    /// Left-hand offset table.
    pub left_offset_table: LookupU16<'a>,
    /// Right-hand offset table.
    pub right_offset_table: LookupU16<'a>,
    /// Kerning values.
    pub array: &'a [BigEndian<i16>],
}

impl<'a> FontRead<'a> for Subtable2<'a> {
    fn read(data: FontData<'a>) -> Result<Self, ReadError> {
        let mut cursor = data.cursor();
        // Skip rowWidth field
        cursor.advance_by(u32::RAW_BYTE_LEN);
        // The offsets here are from the beginning of the subtable and not
        // from the "data" section, so we need to hand parse and subtract
        // the header size.
        let left_offset = (cursor.read::<u32>()? as usize)
            .checked_sub(Subtable::HEADER_SIZE)
            .ok_or(ReadError::OutOfBounds)?;
        let right_offset = (cursor.read::<u32>()? as usize)
            .checked_sub(Subtable::HEADER_SIZE)
            .ok_or(ReadError::OutOfBounds)?;
        let array_offset = (cursor.read::<u32>()? as usize)
            .checked_sub(Subtable::HEADER_SIZE)
            .ok_or(ReadError::OutOfBounds)?;
        let left_offset_table =
            LookupU16::read(data.slice(left_offset..).ok_or(ReadError::OutOfBounds)?)?;
        let right_offset_table =
            LookupU16::read(data.slice(right_offset..).ok_or(ReadError::OutOfBounds)?)?;
        let array = safe_read_array_to_end(&data, array_offset)?;
        Ok(Self {
            data,
            left_offset_table,
            right_offset_table,
            array,
        })
    }
}

impl Subtable2<'_> {
    /// Returns the kerning adjustment for the given pair.
    pub fn kerning(&self, left: GlyphId, right: GlyphId) -> Option<i16> {
        let left: u16 = left.to_u32().try_into().ok()?;
        let right: u16 = right.to_u32().try_into().ok()?;
        let left_idx = self.left_offset_table.value(left).unwrap_or(0) as usize;
        let right_idx = self.right_offset_table.value(right).unwrap_or(0) as usize;
        self.array
            .get(left_idx + right_idx)
            .map(|value| value.get())
    }
}

/// The type 4 `kerx` subtable.
#[derive(Clone)]
pub struct Subtable4<'a> {
    pub state_table: ExtendedStateTable<'a, BigEndian<u16>>,
    /// Flags for control point positioning.
    pub flags: u32,
    pub actions: Subtable4Actions<'a>,
}

impl<'a> FontRead<'a> for Subtable4<'a> {
    fn read(data: FontData<'a>) -> Result<Self, ReadError> {
        let state_table = ExtendedStateTable::read(data)?;
        let mut cursor = data.cursor();
        cursor.advance_by(ExtendedStateTable::<()>::HEADER_LEN);
        let flags = cursor.read::<u32>()?;
        let action_type = (flags & 0xC0000000) >> 30;
        let offset = (flags & 0x00FFFFFF) as usize;
        let actions = match action_type {
            0 => Subtable4Actions::ControlPoints(safe_read_array_to_end(&data, offset)?),
            1 => Subtable4Actions::AnchorPoints(safe_read_array_to_end(&data, offset)?),
            2 => Subtable4Actions::ControlPointCoords(safe_read_array_to_end(&data, offset)?),
            _ => {
                return Err(ReadError::MalformedData(
                    "invalid action type in kerx subtable 4",
                ))
            }
        };
        Ok(Self {
            state_table,
            flags,
            actions,
        })
    }
}

/// Actions for the type 4 `kerx` subtable.
#[derive(Clone)]
pub enum Subtable4Actions<'a> {
    /// Sequence of glyph outline point indices.
    ControlPoints(&'a [BigEndian<u16>]),
    /// Sequence of indices into the `ankr` table.
    AnchorPoints(&'a [BigEndian<u16>]),
    /// Sequence of coordinate values.
    ControlPointCoords(&'a [BigEndian<i16>]),
}

/// The type 6 `kerx` subtable.
#[derive(Clone)]
pub enum Subtable6<'a> {
    ShortValues(LookupU16<'a>, LookupU16<'a>, &'a [BigEndian<i16>]),
    LongValues(LookupU32<'a>, LookupU32<'a>, &'a [BigEndian<i32>]),
}

impl<'a> FontRead<'a> for Subtable6<'a> {
    fn read(data: FontData<'a>) -> Result<Self, ReadError> {
        let mut cursor = data.cursor();
        let flags = cursor.read::<u32>()?;
        // Skip rowCount and columnCount
        cursor.advance_by(u32::RAW_BYTE_LEN * 2);
        // All offsets are relative to the parent subtable
        let row_index_table_offset = (cursor.read::<u32>()? as usize)
            .checked_sub(Subtable::HEADER_SIZE)
            .ok_or(ReadError::OutOfBounds)?;
        let column_index_table_offset = (cursor.read::<u32>()? as usize)
            .checked_sub(Subtable::HEADER_SIZE)
            .ok_or(ReadError::OutOfBounds)?;
        let kerning_array_offset = (cursor.read::<u32>()? as usize)
            .checked_sub(Subtable::HEADER_SIZE)
            .ok_or(ReadError::OutOfBounds)?;
        let row_data = data
            .slice(row_index_table_offset..)
            .ok_or(ReadError::OutOfBounds)?;
        let column_data = data
            .slice(column_index_table_offset..)
            .ok_or(ReadError::OutOfBounds)?;
        if flags & 1 == 0 {
            let rows = LookupU16::read(row_data)?;
            let columns = LookupU16::read(column_data)?;
            let kerning_array = safe_read_array_to_end(&data, kerning_array_offset)?;
            Ok(Self::ShortValues(rows, columns, kerning_array))
        } else {
            let rows = LookupU32::read(row_data)?;
            let columns = LookupU32::read(column_data)?;
            let kerning_array = safe_read_array_to_end(&data, kerning_array_offset)?;
            Ok(Self::LongValues(rows, columns, kerning_array))
        }
    }
}

impl Subtable6<'_> {
    /// Returns the kerning adjustment for the given pair.
    pub fn kerning(&self, left: GlyphId, right: GlyphId) -> Option<i32> {
        let left: u16 = left.to_u32().try_into().ok()?;
        let right: u16 = right.to_u32().try_into().ok()?;
        match self {
            Self::ShortValues(rows, columns, array) => {
                let left_idx = rows.value(left).unwrap_or_default();
                let right_idx = columns.value(right).unwrap_or_default();
                let idx = left_idx as usize + right_idx as usize;
                array.get(idx).map(|value| value.get() as i32)
            }
            Self::LongValues(rows, columns, array) => {
                let left_idx = rows.value(left).unwrap_or_default();
                let right_idx = columns.value(right).unwrap_or_default();
                let idx = (left_idx as usize).checked_sub(right_idx as usize)?;
                array.get(idx).map(|value| value.get())
            }
        }
    }
}

#[cfg(feature = "experimental_traverse")]
impl<'a> SomeRecord<'a> for Subtable<'a> {
    fn traverse(self, data: FontData<'a>) -> RecordResolver<'a> {
        RecordResolver {
            name: "Subtable",
            get_field: Box::new(move |idx, _data| match idx {
                0usize => Some(Field::new("coverage", self.coverage())),
                1usize => Some(Field::new("tuple_count", self.tuple_count())),
                _ => None,
            }),
            data,
        }
    }
}
