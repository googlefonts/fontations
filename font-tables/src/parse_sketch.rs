#![allow(dead_code)]
use std::ops::Range;

use font_types::{BigEndian, FixedSized, MajorMinor, Offset16, ReadScalar};

use crate::parse2::{FontData, FontRead, Format, ReadError, TableInfo, TableRef};
use crate::tables::gpos::ValueFormat;

impl FixedSized for ValueFormat {
    const RAW_BYTE_LEN: usize = 2;
}

impl ReadScalar for ValueFormat {
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
            other => Err(ReadError::InvalidFormat(other)),
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

impl TableInfo for Cmap4 {
    type Info = Cmap4Shape;

    fn parse<'a>(data: &FontData<'a>) -> Result<TableRef<'a, Self>, ReadError> {
        let mut cursor = data.cursor();
        let _format: u16 = cursor.read_validate(|value| {
            value == &Self::FORMAT
            //.then_some(format)
            //.ok_or(ReadError::InvalidFormat)
        })?;
        cursor.advance::<u16>(); // length
        cursor.advance::<u16>(); // language
        let seg_count_x2: u16 = cursor.read()?;
        cursor.advance::<u16>(); // search_range
        cursor.advance::<u16>(); // entry_selector
        cursor.advance::<u16>(); // range_shift
        let end_code_byte_len = seg_count_x2 as usize;
        cursor.advance_by(end_code_byte_len);
        cursor.advance::<u16>(); // reserved_pad
                                 //let start_code = cursor.position()?;
        let start_code_byte_len = seg_count_x2 as usize;
        cursor.advance_by(start_code_byte_len);
        let id_delta_byte_len = seg_count_x2 as usize;
        cursor.advance_by(id_delta_byte_len);
        let id_range_offsets_byte_len = seg_count_x2 as usize;
        cursor.advance_by(id_range_offsets_byte_len);
        cursor.finish(Cmap4Shape {
            end_code_byte_len,
            start_code_byte_len,
            id_delta_byte_len,
            id_range_offsets_byte_len,
        })
    }
}

#[derive(Debug, Clone, Copy)]
struct Cmap4Shape {
    end_code_byte_len: usize,
    start_code_byte_len: usize,
    id_delta_byte_len: usize,
    id_range_offsets_byte_len: usize,
    //glyph_id_array_byte_len: usize,
}

impl Cmap4Shape {
    fn format_byte_range(&self) -> Range<usize> {
        let start = 0;
        start..start + u16::RAW_BYTE_LEN
    }

    fn length_byte_range(&self) -> Range<usize> {
        let start = self.format_byte_range().end;
        start..start + u16::RAW_BYTE_LEN
    }

    fn seg_count_x2_byte_range(&self) -> Range<usize> {
        let start = self.length_byte_range().end;
        start..start + u16::RAW_BYTE_LEN
    }

    // etc etc
    fn range_shift_byte_range(&self) -> Range<usize> {
        let start = self.seg_count_x2_byte_range().end + 6;
        start..start + u16::RAW_BYTE_LEN
    }

    fn end_code_byte_range(&self) -> Range<usize> {
        let start = self.range_shift_byte_range().end;
        start..start + self.end_code_byte_len
    }

    fn reserved_pad_byte_range(&self) -> Range<usize> {
        let start = self.end_code_byte_range().end;
        start..start + u16::RAW_BYTE_LEN
    }
}

impl<'a> TableRef<'a, Cmap4> {
    fn format(&self) -> u16 {
        self.data
            .read_at(self.shape.format_byte_range().start)
            .unwrap_or_default()
    }

    fn end_code_byte_range(&self) -> &'a [BigEndian<u16>] {
        self.data
            .read_array(self.shape.end_code_byte_range())
            .unwrap_or_default()
    }
}

struct Gdef;
#[derive(Debug, Clone, Copy)]
struct GdefShape {
    mark_glyph_sets_def_byte_start: Option<usize>,
}

impl GdefShape {}

impl GdefShape {
    fn major_version(&self) -> usize {
        0
    }

    fn minor_version(&self) -> usize {
        self.major_version() + u16::RAW_BYTE_LEN
    }

    fn mark_glyph_sets_def_offset(&self) -> Option<usize> {
        // just pretend
        self.mark_glyph_sets_def_byte_start
    }
}

impl TableInfo for Gdef {
    type Info = GdefShape;

    fn parse<'a>(data: &FontData<'a>) -> Result<TableRef<'a, Self>, ReadError> {
        let mut cursor = data.cursor();
        //let _major_version = cursor.read_validate(|major_version: &u16| major_version == &1)?;
        //let minor_version = cursor.read::<u16>()?;
        let version: MajorMinor = cursor.read()?;
        let mark_glyph_sets_def_byte_start = version
            .compatible(MajorMinor::VERSION_1_1)
            .then(|| cursor.position())
            .transpose()?;
        // let's pretend this is an array:

        let mark_glyph_sets_def_byte_len = version
            .compatible(MajorMinor::VERSION_1_1)
            .then(|| 1usize + 1);

        mark_glyph_sets_def_byte_len.map(|value| cursor.advance_by(value));

        cursor.advance::<Offset16>();

        cursor.finish(GdefShape {
            mark_glyph_sets_def_byte_start,
        })
    }
}

impl<'a> TableRef<'a, Gdef> {
    fn minor_version(&self) -> u16 {
        self.data.read_at(self.shape.minor_version()).unwrap()
    }

    fn mark_glyph_sets_def_offset(&self) -> Option<Offset16> {
        let off = self.shape.mark_glyph_sets_def_offset()?;
        Some(self.data.read_at(off).unwrap())
    }
}

/// a record
#[derive(Clone, Debug)]
#[repr(C)]
#[repr(packed)]
pub struct MarkRecord {
    /// Class defined for the associated mark.
    pub mark_class: BigEndian<u16>,
    /// Offset to Anchor table, from beginning of MarkArray table.
    pub mark_anchor_offset: BigEndian<Offset16>,
}

impl FixedSized for MarkRecord {
    const RAW_BYTE_LEN: usize = u16::RAW_BYTE_LEN + Offset16::RAW_BYTE_LEN;
}
