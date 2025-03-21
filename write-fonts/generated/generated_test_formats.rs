// THIS FILE IS AUTOGENERATED.
// Any changes to this file will be overwritten.
// For more information about how codegen works, see font-codegen/README.md

#[allow(unused_imports)]
use crate::codegen_prelude::*;

#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Table1 {
    pub heft: u32,
    pub flex: u16,
}

impl Table1 {
    /// Construct a new `Table1`
    pub fn new(heft: u32, flex: u16) -> Self {
        Self { heft, flex }
    }
}

impl FontWrite for Table1 {
    #[allow(clippy::unnecessary_cast)]
    fn write_into(&self, writer: &mut TableWriter) {
        (1 as u16).write_into(writer);
        self.heft.write_into(writer);
        self.flex.write_into(writer);
    }
    fn table_type(&self) -> TableType {
        TableType::Named("Table1")
    }
}

impl Validate for Table1 {
    fn validate_impl(&self, _ctx: &mut ValidationCtx) {}
}

impl<'a> FromObjRef<read_fonts::codegen_test::formats::Table1<'a>> for Table1 {
    fn from_obj_ref(obj: &read_fonts::codegen_test::formats::Table1<'a>, _: FontData) -> Self {
        Table1 {
            heft: obj.heft(),
            flex: obj.flex(),
        }
    }
}

#[allow(clippy::needless_lifetimes)]
impl<'a> FromTableRef<read_fonts::codegen_test::formats::Table1<'a>> for Table1 {}

impl<'a> FontRead<'a> for Table1 {
    fn read(data: FontData<'a>) -> Result<Self, ReadError> {
        <read_fonts::codegen_test::formats::Table1 as FontRead>::read(data)
            .map(|x| x.to_owned_table())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Table2 {
    pub values: Vec<u16>,
}

impl Table2 {
    /// Construct a new `Table2`
    pub fn new(values: Vec<u16>) -> Self {
        Self { values }
    }
}

impl FontWrite for Table2 {
    #[allow(clippy::unnecessary_cast)]
    fn write_into(&self, writer: &mut TableWriter) {
        (2 as u16).write_into(writer);
        (u16::try_from(array_len(&self.values)).unwrap()).write_into(writer);
        self.values.write_into(writer);
    }
    fn table_type(&self) -> TableType {
        TableType::Named("Table2")
    }
}

impl Validate for Table2 {
    fn validate_impl(&self, ctx: &mut ValidationCtx) {
        ctx.in_table("Table2", |ctx| {
            ctx.in_field("values", |ctx| {
                if self.values.len() > (u16::MAX as usize) {
                    ctx.report("array exceeds max length");
                }
            });
        })
    }
}

impl<'a> FromObjRef<read_fonts::codegen_test::formats::Table2<'a>> for Table2 {
    fn from_obj_ref(obj: &read_fonts::codegen_test::formats::Table2<'a>, _: FontData) -> Self {
        let offset_data = obj.offset_data();
        Table2 {
            values: obj.values().to_owned_obj(offset_data),
        }
    }
}

#[allow(clippy::needless_lifetimes)]
impl<'a> FromTableRef<read_fonts::codegen_test::formats::Table2<'a>> for Table2 {}

impl<'a> FontRead<'a> for Table2 {
    fn read(data: FontData<'a>) -> Result<Self, ReadError> {
        <read_fonts::codegen_test::formats::Table2 as FontRead>::read(data)
            .map(|x| x.to_owned_table())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Table3 {
    pub something: u16,
}

impl FontWrite for Table3 {
    #[allow(clippy::unnecessary_cast)]
    fn write_into(&self, writer: &mut TableWriter) {
        (3 as u16).write_into(writer);
        self.something.write_into(writer);
    }
    fn table_type(&self) -> TableType {
        TableType::Named("Table3")
    }
}

impl Validate for Table3 {
    fn validate_impl(&self, _ctx: &mut ValidationCtx) {}
}

impl<'a> FromObjRef<read_fonts::codegen_test::formats::Table3<'a>> for Table3 {
    fn from_obj_ref(obj: &read_fonts::codegen_test::formats::Table3<'a>, _: FontData) -> Self {
        Table3 {
            something: obj.something(),
        }
    }
}

#[allow(clippy::needless_lifetimes)]
impl<'a> FromTableRef<read_fonts::codegen_test::formats::Table3<'a>> for Table3 {}

impl<'a> FontRead<'a> for Table3 {
    fn read(data: FontData<'a>) -> Result<Self, ReadError> {
        <read_fonts::codegen_test::formats::Table3 as FontRead>::read(data)
            .map(|x| x.to_owned_table())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum MyTable {
    Format1(Table1),
    MyFormat22(Table2),
    Format3(Table3),
}

impl MyTable {
    /// Construct a new `Table1` subtable
    pub fn format_1(heft: u32, flex: u16) -> Self {
        Self::Format1(Table1::new(heft, flex))
    }

    /// Construct a new `Table2` subtable
    pub fn my_format_22(values: Vec<u16>) -> Self {
        Self::MyFormat22(Table2::new(values))
    }
}

impl Default for MyTable {
    fn default() -> Self {
        Self::Format1(Default::default())
    }
}

impl FontWrite for MyTable {
    fn write_into(&self, writer: &mut TableWriter) {
        match self {
            Self::Format1(item) => item.write_into(writer),
            Self::MyFormat22(item) => item.write_into(writer),
            Self::Format3(item) => item.write_into(writer),
        }
    }
    fn table_type(&self) -> TableType {
        match self {
            Self::Format1(item) => item.table_type(),
            Self::MyFormat22(item) => item.table_type(),
            Self::Format3(item) => item.table_type(),
        }
    }
}

impl Validate for MyTable {
    fn validate_impl(&self, ctx: &mut ValidationCtx) {
        match self {
            Self::Format1(item) => item.validate_impl(ctx),
            Self::MyFormat22(item) => item.validate_impl(ctx),
            Self::Format3(item) => item.validate_impl(ctx),
        }
    }
}

impl FromObjRef<read_fonts::codegen_test::formats::MyTable<'_>> for MyTable {
    fn from_obj_ref(obj: &read_fonts::codegen_test::formats::MyTable, _: FontData) -> Self {
        use read_fonts::codegen_test::formats::MyTable as ObjRefType;
        match obj {
            ObjRefType::Format1(item) => MyTable::Format1(item.to_owned_table()),
            ObjRefType::MyFormat22(item) => MyTable::MyFormat22(item.to_owned_table()),
            ObjRefType::Format3(item) => MyTable::Format3(item.to_owned_table()),
        }
    }
}

impl FromTableRef<read_fonts::codegen_test::formats::MyTable<'_>> for MyTable {}

impl<'a> FontRead<'a> for MyTable {
    fn read(data: FontData<'a>) -> Result<Self, ReadError> {
        <read_fonts::codegen_test::formats::MyTable as FontRead>::read(data)
            .map(|x| x.to_owned_table())
    }
}

impl From<Table1> for MyTable {
    fn from(src: Table1) -> MyTable {
        MyTable::Format1(src)
    }
}

impl From<Table2> for MyTable {
    fn from(src: Table2) -> MyTable {
        MyTable::MyFormat22(src)
    }
}

impl From<Table3> for MyTable {
    fn from(src: Table3) -> MyTable {
        MyTable::Format3(src)
    }
}
