use font_types::{Offset16, ReadScalar};

use crate::parse2::{FontData, FontRead, Format, ParseContext, ReadError, TableInfo, TableRef};
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

    fn parse<'a>(ctx: &mut ParseContext<'a>) -> Result<TableRef<'a, Self>, ReadError> {
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

    fn parse<'a>(ctx: &mut ParseContext<'a>) -> Result<TableRef<'a, Self>, ReadError> {
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

//impl<'a, T: TableInfo> FontRead<'a> for TableRef<'a, T> {
//fn read(bytes: &FontData<'a>) -> Result<Self, ReadError> {
////let mut ctx = ParseContext { data: *bytes };
////T::parse(&mut ctx)
//}
//}

enum SinglePos<'a> {
    Format1(TableRef<'a, SinglePosFormat1>),
    Format2(TableRef<'a, SinglePosFormat2>),
}
