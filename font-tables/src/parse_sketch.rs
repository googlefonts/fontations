use std::ops::Range;

use font_types::{BigEndian, Offset16, ReadScalar};

use crate::parse2::{FontData, FontRead, Format, ReadError, TableInfo, TableRef};
use crate::tables::gpos::ValueFormat;

impl ReadScalar for ValueFormat {
    const SIZE: usize = 2;

    fn read(bytes: &[u8]) -> Option<Self> {
        ReadScalar::read(bytes).and_then(Self::from_bits)
    }
}
struct SinglePosFormat1;

impl Format<u16> for SinglePosFormat1 {
    const FORMAT: u16 = 1;
}

impl Format<u16> for SinglePosFormat2 {
    const FORMAT: u16 = 2;
}

#[derive(Debug, Clone, Copy)]
struct SinglePosFormat1Shape {}

struct SinglePosFormat2;

#[derive(Debug, Clone, Copy)]
struct SinglePosFormat2Shape {}

impl SinglePosFormat1Shape {
    #[inline]
    fn pos_format(&self, data: &FontData) -> u16 {
        data.read_at(0).unwrap_or_default()
    }

    #[inline]
    fn coverage_offset(&self, data: &FontData) -> Offset16 {
        data.read_at(2).unwrap()
    }

    #[inline]
    fn value_format(&self, data: &FontData) -> ValueFormat {
        data.read_at(4).unwrap()
    }
}

impl TableInfo for SinglePosFormat1 {
    type Info = SinglePosFormat1Shape;

    fn parse<'a>(ctx: &FontData<'a>) -> Result<TableRef<'a, Self>, ReadError> {
        let mut cursor = ctx.cursor();
        let _pos_format = cursor.advance::<u16>();
        let _coverage_offset = cursor.advance::<Offset16>();
        let value_format = cursor.read::<ValueFormat>()?;
        let value_record_len = value_format.record_byte_len();
        cursor.advance_by(value_record_len);
        cursor.finish(SinglePosFormat1Shape {})
    }
}

impl TableInfo for SinglePosFormat2 {
    type Info = SinglePosFormat2Shape;

    fn parse<'a>(ctx: &FontData<'a>) -> Result<TableRef<'a, Self>, ReadError> {
        let mut cursor = ctx.cursor();
        let _pos_format = cursor.advance_by(std::mem::size_of::<u16>());
        let _coverage_offset = cursor.advance_by(std::mem::size_of::<Offset16>());
        let value_format = cursor.read::<ValueFormat>()?;
        let value_count: u16 = cursor.read()?;
        cursor.advance_by(value_format.record_byte_len() * value_count as usize);
        cursor.finish(SinglePosFormat2Shape {})
    }
}

impl TableRef<'_, SinglePosFormat1> {
    fn format(&self) -> u16 {
        self.shape.pos_format(&self.data)
    }
}

impl<'a, T: TableInfo> FontRead<'a> for TableRef<'a, T> {
    fn read(data: &FontData<'a>) -> Result<Self, ReadError> {
        T::parse(data)
    }
}

// how we handle formats:
impl<'a> FontRead<'a> for SinglePos<'a> {
    fn read(data: &FontData<'a>) -> Result<Self, ReadError> {
        let format: u16 = data.read_at(0)?;
        match format {
            SinglePosFormat1::FORMAT => SinglePosFormat1::parse(data).map(Self::Format1),
            SinglePosFormat2::FORMAT => SinglePosFormat2::parse(data).map(Self::Format2),
            _ => Err(ReadError::InvalidFormat),
        }
    }
}

// #[format(u16)]
enum SinglePos<'a> {
    Format1(TableRef<'a, SinglePosFormat1>),
    Format2(TableRef<'a, SinglePosFormat2>),
}

struct Cmap4;

impl Format<u16> for Cmap4 {
    const FORMAT: u16 = 4;
}

#[derive(Debug, Clone, Copy)]
struct Cmap4Shape {
    start_code: u32,
    id_delta: u32,
    id_range_offsets: u32,
    glyph_id_array: u32,
}

impl TableInfo for Cmap4 {
    type Info = Cmap4Shape;

    fn parse<'a>(data: &FontData<'a>) -> Result<TableRef<'a, Self>, ReadError> {
        let mut cursor = data.cursor();
        let _format: u16 = cursor.read_validate(|format| {
            (format == Self::FORMAT)
                .then_some(format)
                .ok_or(ReadError::InvalidFormat)
        })?;
        let _length: u16 = cursor.read()?;
        cursor.advance::<u16>(); // length
        cursor.advance::<u16>(); // language
        let seg_count_x2: u16 = cursor.read()?;
        cursor.advance::<u16>(); // search_range
        cursor.advance::<u16>(); // entry_selector
        cursor.advance::<u16>(); // range_shift
        cursor.advance_by(seg_count_x2 as usize);
        cursor.advance::<u16>(); // reserved_pad
        let start_code = cursor.position()?;
        cursor.advance_by(seg_count_x2 as usize);
        let id_delta = cursor.position()?;
        cursor.advance_by(seg_count_x2 as usize);
        let id_range_offsets = cursor.position()?;
        cursor.advance_by(seg_count_x2 as usize);
        let glyph_id_array = cursor.position()?;
        cursor.finish(Cmap4Shape {
            start_code,
            id_delta,
            id_range_offsets,
            glyph_id_array,
        })
    }
}

impl Cmap4Shape {
    fn format(&self) -> usize {
        0
    }

    fn length(&self) -> usize {
        self.format() + u16::SIZE
    }

    fn seg_count_x2(&self) -> usize {
        self.length() + u16::SIZE
    }

    // etc etc

    fn start_code(&self) -> Range<usize> {
        self.start_code as usize..self.id_delta as usize
    }
}

impl<'a> TableRef<'a, Cmap4> {
    fn format(&self) -> u16 {
        self.data.read_at(self.shape.format()).unwrap_or_default()
    }

    fn start_code(&self) -> &'a [BigEndian<u16>] {
        self.data
            .read_array(self.shape.start_code())
            .unwrap_or_default()
    }
}
