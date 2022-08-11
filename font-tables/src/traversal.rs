//! Generic tree traversal
//!
//! This module defines functionality that allows untyped access to font table
//! data. This is used as the basis for things like debug printing.

use std::{fmt::Debug, ops::Deref};

use font_types::{
    F2Dot14, FWord, Fixed, GlyphId, LongDateTime, MajorMinor, Tag, UfWord, Uint24, Version16Dot16,
};

use crate::ReadError;

/// Types of fields in font tables.
///
/// Fields can either be scalars, offsets to tables, or arrays.
pub enum FieldType<'a> {
    I8(i8),
    U8(u8),
    I16(i16),
    U16(u16),
    I32(i32),
    U32(u32),
    U24(Uint24),
    Tag(Tag),
    FWord(FWord),
    UfWord(UfWord),
    MajorMinor(MajorMinor),
    Version16Dot16(Version16Dot16),
    F2Dot14(F2Dot14),
    Fixed(Fixed),
    LongDateTime(LongDateTime),
    GlyphId(GlyphId),
    Offset(Result<Box<dyn SomeTable<'a> + 'a>, ReadError>),
    Unimplemented,
    // used for fields in other versions of a table
    None,
}

/// A generic field in a font table
pub struct Field<'a> {
    name: &'static str,
    typ: FieldType<'a>,
}

pub trait SomeTable<'a> {
    fn type_name(&self) -> &str;
    fn get_field(&self, idx: usize) -> Option<Field<'a>>;
}

impl<'a> SomeTable<'a> for Box<dyn SomeTable<'a> + 'a> {
    fn type_name(&self) -> &str {
        self.deref().type_name()
    }

    fn get_field(&self, idx: usize) -> Option<Field<'a>> {
        self.deref().get_field(idx)
    }
}

fn iter_fields<'a, 'b>(table: &'b (dyn SomeTable<'a> + 'b)) -> FieldIter<'a, 'b> {
    FieldIter { table, idx: 0 }
}

pub trait SomeArray<'a> {
    fn len(&self) -> usize;
    fn get(&self, idx: usize) -> Option<FieldType<'a>>;
}

pub struct FieldIter<'a, 'b> {
    table: &'b dyn SomeTable<'a>,
    idx: usize,
}

impl<'a, 'b> Iterator for FieldIter<'a, 'b> {
    type Item = Field<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let this = self.idx;
        self.idx += 1;
        self.table.get_field(this)
    }
}

struct ArrayIter<'a, 'b> {
    array: &'b dyn SomeArray<'a>,
    idx: usize,
}

impl<'a, 'b> Iterator for ArrayIter<'a, 'b> {
    type Item = FieldType<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let this = self.idx;
        self.idx += 1;
        self.array.get(this)
    }
}

impl<'a> Field<'a> {
    pub fn new(name: &'static str, typ: impl Into<FieldType<'a>>) -> Self {
        Field {
            name,
            typ: typ.into(),
        }
    }
}

/// A wrapper type that implements `Debug` for any table.
pub struct DebugPrintTable<'a, 'b>(pub &'b dyn SomeTable<'a>);

/// A wrapper type that implements `Debug` for any array.
pub struct DebugPrintArray<'a, 'b>(pub &'b dyn SomeArray<'a>);

impl<'a> Debug for FieldType<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::I8(arg0) => arg0.fmt(f),
            Self::U8(arg0) => arg0.fmt(f),
            Self::I16(arg0) => arg0.fmt(f),
            Self::U16(arg0) => arg0.fmt(f),
            Self::I32(arg0) => arg0.fmt(f),
            Self::U32(arg0) => arg0.fmt(f),
            Self::U24(arg0) => arg0.fmt(f),
            Self::Tag(arg0) => arg0.fmt(f),
            Self::FWord(arg0) => arg0.fmt(f),
            Self::UfWord(arg0) => arg0.fmt(f),
            Self::MajorMinor(arg0) => arg0.fmt(f),
            Self::Version16Dot16(arg0) => arg0.fmt(f),
            Self::F2Dot14(arg0) => arg0.fmt(f),
            Self::Fixed(arg0) => arg0.fmt(f),
            Self::LongDateTime(arg0) => arg0.fmt(f),
            Self::GlyphId(arg0) => arg0.fmt(f),
            Self::None => write!(f, "None"),

            Self::Offset(arg0) => arg0.fmt(f),
            Self::Unimplemented => write!(f, "Unimplemented"),
        }
    }
}

impl<'a, 'b> std::fmt::Debug for DebugPrintTable<'a, 'b> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let mut debug_struct = f.debug_struct(self.0.type_name());
        for field in iter_fields(self.0) {
            debug_struct.field(field.name, &field.typ);
        }
        debug_struct.finish()
    }
}

impl<'a> Debug for dyn SomeTable<'a> + 'a {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        DebugPrintTable(self).fmt(f)
    }
}

impl<'a> From<u8> for FieldType<'a> {
    fn from(src: u8) -> FieldType<'a> {
        FieldType::U8(src)
    }
}

impl<'a> From<i8> for FieldType<'a> {
    fn from(src: i8) -> FieldType<'a> {
        FieldType::I8(src)
    }
}

impl<'a> From<u16> for FieldType<'a> {
    fn from(src: u16) -> FieldType<'a> {
        FieldType::U16(src)
    }
}

impl<'a> From<i16> for FieldType<'a> {
    fn from(src: i16) -> FieldType<'a> {
        FieldType::I16(src)
    }
}

impl<'a> From<u32> for FieldType<'a> {
    fn from(src: u32) -> FieldType<'a> {
        FieldType::U32(src)
    }
}

impl<'a> From<i32> for FieldType<'a> {
    fn from(src: i32) -> FieldType<'a> {
        FieldType::I32(src)
    }
}

impl<'a> From<Uint24> for FieldType<'a> {
    fn from(src: Uint24) -> FieldType<'a> {
        FieldType::U24(src)
    }
}

impl<'a> From<Tag> for FieldType<'a> {
    fn from(src: Tag) -> FieldType<'a> {
        FieldType::Tag(src)
    }
}

impl<'a> From<FWord> for FieldType<'a> {
    fn from(src: FWord) -> FieldType<'a> {
        FieldType::FWord(src)
    }
}

impl<'a> From<UfWord> for FieldType<'a> {
    fn from(src: UfWord) -> FieldType<'a> {
        FieldType::UfWord(src)
    }
}

impl<'a> From<Fixed> for FieldType<'a> {
    fn from(src: Fixed) -> FieldType<'a> {
        FieldType::Fixed(src)
    }
}

impl<'a> From<F2Dot14> for FieldType<'a> {
    fn from(src: F2Dot14) -> FieldType<'a> {
        FieldType::F2Dot14(src)
    }
}

impl<'a> From<LongDateTime> for FieldType<'a> {
    fn from(src: LongDateTime) -> FieldType<'a> {
        FieldType::LongDateTime(src)
    }
}

impl<'a> From<MajorMinor> for FieldType<'a> {
    fn from(src: MajorMinor) -> FieldType<'a> {
        FieldType::MajorMinor(src)
    }
}

impl<'a> From<Version16Dot16> for FieldType<'a> {
    fn from(src: Version16Dot16) -> FieldType<'a> {
        FieldType::Version16Dot16(src)
    }
}

impl<'a> From<GlyphId> for FieldType<'a> {
    fn from(src: GlyphId) -> FieldType<'a> {
        FieldType::GlyphId(src)
    }
}

impl<'a, T: Into<FieldType<'a>>> From<Option<T>> for FieldType<'a> {
    fn from(src: Option<T>) -> Self {
        match src {
            Some(t) => t.into(),
            None => FieldType::None,
        }
    }
}

impl<'a, T: SomeTable<'a> + 'a> From<Result<T, ReadError>> for FieldType<'a> {
    fn from(src: Result<T, ReadError>) -> Self {
        FieldType::Offset(src.map(|table| Box::new(table) as Box<dyn SomeTable<'a>>))
    }
}

impl<'a> From<()> for FieldType<'a> {
    fn from(_src: ()) -> FieldType<'a> {
        FieldType::Unimplemented
    }
}
