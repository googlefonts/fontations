//! The [Kerning (kern)](https://developer.apple.com/fonts/TrueType-Reference-Manual/RM06/Chap6kern.html) table.

use super::aat::StateTable;

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
    pub fn subtables(&self) -> impl Iterator<Item = Result<Subtable<'a>, ReadError>> + 'a + Clone {
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
struct Subtables<'a> {
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
            // table. Some fonts abuse this to bypass the 16-bit limit of the
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

    /// True if the table is represented by a state machine.
    #[inline]
    pub fn is_state_machine(&self) -> bool {
        // Only format 1 is a state machine
        self.data_and_format().1 == 1
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
    Format1(StateTable<'a>),
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
            1 => Ok(Self::Format1(StateTable::read(data)?)),
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
        let index = self
            .kern_index()
            .get(left_class * self.right_class_count() as usize + right_class)
            .copied()? as usize;
        self.kern_value().get(index).map(|value| value.get() as i32)
    }
}

#[cfg(test)]
mod tests {
    use font_test_data::bebuffer::BeBuffer;

    use super::*;

    #[test]
    fn ot_format_0() {
        let kern = Kern::read(FontData::new(font_test_data::kern::KERN_VER_0_FMT_0_DATA)).unwrap();
        let Kern::Ot(ot_kern) = &kern else {
            panic!("Should be an OpenType kerning table");
        };
        assert_eq!(ot_kern.version(), 0);
        assert_eq!(ot_kern.n_tables(), 1);
        let subtables = kern.subtables().collect::<Vec<_>>();
        assert_eq!(subtables.len(), 1);
        let subtable = subtables.first().unwrap().as_ref().unwrap();
        assert!(subtable.is_horizontal());
        let Subtable::Ot(ot_subtable) = subtable else {
            panic!("Should be an OpenType subtable");
        };
        assert_eq!(ot_subtable.coverage(), 1);
        assert_eq!(ot_subtable.length(), 32);
        check_format_0(subtable);
    }

    #[test]
    fn aat_format_0() {
        let kern = Kern::read(FontData::new(font_test_data::kern::KERN_VER_1_FMT_0_DATA)).unwrap();
        let Kern::Aat(aat_kern) = &kern else {
            panic!("Should be an AAT kerning table");
        };
        assert_eq!(aat_kern.version(), MajorMinor::VERSION_1_0);
        assert_eq!(aat_kern.n_tables(), 1);
        let subtables = kern.subtables().collect::<Vec<_>>();
        assert_eq!(subtables.len(), 1);
        let subtable = subtables.first().unwrap().as_ref().unwrap();
        assert!(subtable.is_horizontal());
        let Subtable::Aat(aat_subtable) = subtable else {
            panic!("Should be an AAT subtable");
        };
        assert_eq!(aat_subtable.coverage(), 0);
        assert_eq!(aat_subtable.length(), 34);
        check_format_0(subtable);
    }

    fn check_format_0(subtable: &Subtable) {
        let SubtableKind::Format0(format0) = subtable.kind().unwrap() else {
            panic!("Should be a format 0 subtable");
        };
        const EXPECTED: &[(u32, u32, i32)] = &[(4, 12, -40), (4, 28, 40), (5, 40, -50)];
        let pairs = format0
            .pairs()
            .iter()
            .map(|pair| {
                (
                    pair.left().to_u32(),
                    pair.right().to_u32(),
                    pair.value() as i32,
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(pairs, EXPECTED);
        for (left, right, value) in EXPECTED.iter().copied() {
            assert_eq!(
                format0.kerning(left.into(), right.into()),
                Some(value),
                "left = {left}, right = {right}"
            );
        }
    }

    #[test]
    fn format_2() {
        let kern = Kern::read(FontData::new(font_test_data::kern::KERN_VER_1_FMT_2_DATA)).unwrap();
        let subtables = kern.subtables().filter_map(|t| t.ok()).collect::<Vec<_>>();
        assert_eq!(subtables.len(), 3);
        // First subtable is format 0 so ignore it
        check_format_2(
            &subtables[1],
            &[
                (68, 60, -100),
                (68, 61, -20),
                (68, 88, -20),
                (69, 67, -30),
                (69, 69, -30),
                (69, 70, -30),
                (69, 71, -30),
                (69, 73, -30),
                (69, 81, -30),
                (69, 83, -30),
                (72, 67, -20),
                (72, 69, -20),
                (72, 70, -20),
                (72, 71, -20),
                (72, 73, -20),
                (72, 81, -20),
                (72, 83, -20),
                (81, 60, -100),
                (81, 61, -20),
                (81, 88, -20),
                (82, 60, -100),
                (82, 61, -20),
                (82, 88, -20),
                (84, 67, -50),
                (84, 69, -50),
                (84, 70, -50),
                (84, 71, -50),
                (84, 73, -50),
                (84, 81, -50),
                (84, 83, -50),
                (88, 67, -20),
                (88, 69, -20),
                (88, 70, -20),
                (88, 71, -20),
                (88, 73, -20),
                (88, 81, -20),
                (88, 83, -20),
            ],
        );
        check_format_2(
            &subtables[2],
            &[
                (60, 67, -100),
                (60, 69, -100),
                (60, 70, -100),
                (60, 71, -100),
                (60, 73, -100),
                (60, 81, -100),
                (60, 83, -100),
            ],
        );
    }

    fn check_format_2(subtable: &Subtable, expected: &[(u32, u32, i32)]) {
        let SubtableKind::Format2(format2) = subtable.kind().unwrap() else {
            panic!("Should be a format 2 subtable");
        };
        for (left, right, value) in expected.iter().copied() {
            assert_eq!(
                format2.kerning(left.into(), right.into()),
                Some(value),
                "left = {left}, right = {right}"
            );
        }
    }

    #[test]
    fn format_3() {
        // Build a simple NxM kerning array with 5 glyphs
        let mut buf = BeBuffer::new();
        buf = buf.push(5u16); // glyphCount
        buf = buf.push(4u8); // kernValueCount
        buf = buf.push(3u8); // leftClassCount
        buf = buf.push(2u8); // rightClassCount
        buf = buf.push(0u8); // unused flags
        buf = buf.extend([0i16, -10, -20, 12]); // kernValues
        buf = buf.extend([0u8, 2, 1, 1, 2]); // leftClass
        buf = buf.extend([0u8, 1, 1, 0, 1]); // rightClass
        buf = buf.extend([0u8, 1, 2, 3, 2, 1]); // kernIndex
        let format3 = Subtable3::read(FontData::new(buf.as_slice())).unwrap();
        const EXPECTED: [(u32, u32, i32); 25] = [
            (0, 0, 0),
            (0, 1, -10),
            (0, 2, -10),
            (0, 3, 0),
            (0, 4, -10),
            (1, 0, -20),
            (1, 1, -10),
            (1, 2, -10),
            (1, 3, -20),
            (1, 4, -10),
            (2, 0, -20),
            (2, 1, 12),
            (2, 2, 12),
            (2, 3, -20),
            (2, 4, 12),
            (3, 0, -20),
            (3, 1, 12),
            (3, 2, 12),
            (3, 3, -20),
            (3, 4, 12),
            (4, 0, -20),
            (4, 1, -10),
            (4, 2, -10),
            (4, 3, -20),
            (4, 4, -10),
        ];
        for (left, right, value) in EXPECTED {
            assert_eq!(
                format3.kerning(left.into(), right.into()),
                Some(value),
                "left = {left}, right = {right}"
            );
        }
    }
}
