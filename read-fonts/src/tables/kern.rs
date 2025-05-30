//! The [Kerning (kern)](https://developer.apple.com/fonts/TrueType-Reference-Manual/RM06/Chap6kern.html) table.

use super::aat::StateTable;
pub use super::kerx::Subtable0Pair;

include!("../../generated/generated_kern.rs");

/// The kerning table.
#[derive(Clone)]
pub enum Kern<'a> {
    Ot(OtKern<'a>),
    Aat(AatKern<'a>),
}

impl TopLevelTable for Kern<'_> {
    const TAG: Tag = Tag::new(b"kern");
}

impl<'a> FontRead<'a> for Kern<'a> {
    fn read(data: FontData<'a>) -> Result<Self, ReadError> {
        if data.read_at::<u16>(0)? == 0 {
            OtKern::read(data).map(Self::Ot)
        } else {
            AatKern::read(data).map(Self::Aat)
        }
    }
}

impl<'a> Kern<'a> {
    /// Returns an iterator over all of the subtables in this `kern` table.
    pub fn subtables(&self) -> Subtables<'a> {
        let (data, is_aat, n_tables) = match self {
            Self::Ot(table) => (table.subtable_data(), false, table.n_tables() as u32),
            Self::Aat(table) => (table.subtable_data(), true, table.n_tables()),
        };
        let data = FontData::new(data);
        Subtables {
            data,
            is_aat,
            n_tables,
        }
    }
}

/// Iterator over the subtables of a `kern` table.
#[derive(Clone)]
pub struct Subtables<'a> {
    data: FontData<'a>,
    is_aat: bool,
    n_tables: u32,
}

impl<'a> Iterator for Subtables<'a> {
    type Item = Result<Subtable<'a>, ReadError>;

    fn next(&mut self) -> Option<Self::Item> {
        let len = if self.is_aat {
            self.data.read_at::<u32>(0).ok()? as usize
        } else if self.n_tables == 1 {
            // For OT kern tables with a single subtable, ignore the length
            // and allow the single subtable to extend to the end of the full
            // table. Some fonts do this to bypass the 16-bit limit of the
            // length field
            self.data.len()
        } else {
            self.data.read_at::<u16>(2).ok()? as usize
        };
        if len == 0 {
            return None;
        }
        let data = self.data.take_up_to(len)?;
        Some(Subtable::read_with_args(data, &self.is_aat))
    }
}

impl OtSubtable<'_> {
    // version, length and coverage: all u16
    const HEADER_LEN: usize = u16::RAW_BYTE_LEN * 3;
}

impl AatSubtable<'_> {
    // length: u32, coverage and tuple_index: u16
    const HEADER_LEN: usize = u32::RAW_BYTE_LEN + u16::RAW_BYTE_LEN * 2;
}

/// A subtable in the `kern` table.
#[derive(Clone)]
pub enum Subtable<'a> {
    Ot(OtSubtable<'a>),
    Aat(AatSubtable<'a>),
}

impl ReadArgs for Subtable<'_> {
    type Args = bool;
}

impl<'a> FontReadWithArgs<'a> for Subtable<'a> {
    fn read_with_args(data: FontData<'a>, args: &Self::Args) -> Result<Self, ReadError> {
        if *args {
            Ok(Self::Aat(AatSubtable::read(data)?))
        } else {
            Ok(Self::Ot(OtSubtable::read(data)?))
        }
    }
}

impl<'a> Subtable<'a> {
    /// True if the table has vertical kerning values.
    #[inline]
    pub fn is_vertical(&self) -> bool {
        match self {
            Self::Ot(subtable) => subtable.coverage() & (1 << 0) == 0,
            Self::Aat(subtable) => subtable.coverage() & 0x8000 != 0,
        }
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
        match self {
            Self::Ot(subtable) => subtable.coverage() & (1 << 2) != 0,
            Self::Aat(subtable) => subtable.coverage() & 0x4000 != 0,
        }
    }

    /// True if the table has variation kerning values.
    #[inline]
    pub fn is_variable(&self) -> bool {
        match self {
            Self::Ot(_) => false,
            Self::Aat(subtable) => subtable.coverage() & 0x2000 != 0,
        }
    }

    /// Returns an enum representing the actual subtable data.    
    pub fn kind(&self) -> Result<SubtableKind<'a>, ReadError> {
        let (data, format) = self.data_and_format();
        let is_aat = matches!(self, Self::Aat(_));
        SubtableKind::read_with_args(FontData::new(data), &(format, is_aat))
    }

    fn data_and_format(&self) -> (&'a [u8], u8) {
        match self {
            Self::Ot(subtable) => (subtable.data(), ((subtable.coverage() & 0xFF00) >> 8) as u8),
            Self::Aat(subtable) => (subtable.data(), subtable.coverage() as u8),
        }
    }
}

/// The various `kern` subtable formats.
#[derive(Clone)]
pub enum SubtableKind<'a> {
    Format0(Subtable0<'a>),
    Format1(Subtable1<'a>),
    Format2(Subtable2<'a>),
    Format3(Subtable3<'a>),
}

impl ReadArgs for SubtableKind<'_> {
    type Args = (u8, bool);
}

impl<'a> FontReadWithArgs<'a> for SubtableKind<'a> {
    fn read_with_args(data: FontData<'a>, args: &Self::Args) -> Result<Self, ReadError> {
        let (format, is_aat) = *args;
        let header_len = if is_aat {
            AatSubtable::HEADER_LEN
        } else {
            OtSubtable::HEADER_LEN
        };
        match format {
            0 => Ok(Self::Format0(Subtable0::read(data)?)),
            1 => Ok(Self::Format1(Subtable1::read(data)?)),
            2 => Ok(Self::Format2(Subtable2::read_with_args(data, &header_len)?)),
            3 => Ok(Self::Format3(Subtable3::read(data)?)),
            _ => Err(ReadError::InvalidFormat(format as _)),
        }
    }
}

impl Subtable0<'_> {
    /// Returns the kerning adjustment for the given pair.
    pub fn kerning(&self, left: GlyphId, right: GlyphId) -> Option<i32> {
        super::kerx::pair_kerning(self.pairs(), left, right)
    }
}

/// The type 1 `kern` subtable.
#[derive(Clone)]
pub struct Subtable1<'a> {
    pub state_table: StateTable<'a>,
    /// Contains the set of kerning values, one for each state.
    pub values: &'a [BigEndian<i16>],
}

impl<'a> FontRead<'a> for Subtable1<'a> {
    fn read(data: FontData<'a>) -> Result<Self, ReadError> {
        let state_table = StateTable::read(data)?;
        let values = super::aat::safe_read_array_to_end(&data, 0)?;
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
    /// Size of the header of the containing subtable.
    pub header_len: usize,
    /// Left-hand offset table.
    pub left_offset_table: Subtable2ClassTable<'a>,
    /// Right-hand offset table.
    pub right_offset_table: Subtable2ClassTable<'a>,
    /// Offset to kerning value array.
    pub array_offset: usize,
}

impl ReadArgs for Subtable2<'_> {
    type Args = usize;
}

impl<'a> FontReadWithArgs<'a> for Subtable2<'a> {
    fn read_with_args(data: FontData<'a>, args: &Self::Args) -> Result<Self, ReadError> {
        let mut cursor = data.cursor();
        let header_len = *args;
        // Skip rowWidth field
        cursor.advance_by(u16::RAW_BYTE_LEN);
        // The offsets here are from the beginning of the subtable and not
        // from the "data" section, so we need to hand parse and subtract
        // the header size.
        let left_offset = (cursor.read::<u16>()? as usize)
            .checked_sub(header_len)
            .ok_or(ReadError::OutOfBounds)?;
        let right_offset = (cursor.read::<u16>()? as usize)
            .checked_sub(header_len)
            .ok_or(ReadError::OutOfBounds)?;
        let array_offset = (cursor.read::<u16>()? as usize)
            .checked_sub(header_len)
            .ok_or(ReadError::OutOfBounds)?;
        let left_offset_table =
            Subtable2ClassTable::read(data.slice(left_offset..).ok_or(ReadError::OutOfBounds)?)?;
        let right_offset_table =
            Subtable2ClassTable::read(data.slice(right_offset..).ok_or(ReadError::OutOfBounds)?)?;
        Ok(Self {
            data,
            header_len,
            left_offset_table,
            right_offset_table,
            array_offset,
        })
    }
}

impl Subtable2<'_> {
    /// Returns the kerning adjustment for the given pair.
    pub fn kerning(&self, left: GlyphId, right: GlyphId) -> Option<i32> {
        let left_offset = self.left_offset_table.value(left).unwrap_or(0) as usize;
        let right_offset = self.right_offset_table.value(right).unwrap_or(0) as usize;
        let left_offset = left_offset.checked_sub(self.header_len)?;
        if left_offset < self.array_offset {
            return None;
        }
        let offset = left_offset.checked_add(right_offset)?;
        self.data
            .read_at::<i16>(offset)
            .ok()
            .map(|value| value as i32)
    }
}

impl Subtable2ClassTable<'_> {
    fn value(&self, glyph_id: GlyphId) -> Option<u16> {
        let glyph_id: u16 = glyph_id.to_u32().try_into().ok()?;
        let index = glyph_id.checked_sub(self.first_glyph().to_u16())?;
        self.offsets()
            .get(index as usize)
            .map(|offset| offset.get())
    }
}

impl Subtable3<'_> {
    /// Returns the kerning adjustment for the given pair.
    pub fn kerning(&self, left: GlyphId, right: GlyphId) -> Option<i32> {
        let left_class = self.left_class().get(left.to_u32() as usize).copied()? as usize;
        let right_class = self.right_class().get(right.to_u32() as usize).copied()? as usize;
        let index = left_class * self.right_class_count() as usize + right_class;
        self.kern_value().get(index).map(|value| value.get() as i32)
    }
}
