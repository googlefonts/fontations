// THIS FILE IS AUTOGENERATED.
// Any changes to this file will be overwritten.
// For more information about how codegen works, see font-codegen/README.md

#[allow(unused_imports)]
use crate::codegen_prelude::*;

/// The OpenType [Table Directory](https://docs.microsoft.com/en-us/typography/opentype/spec/otff#table-directory)
#[derive(Debug, Clone, Copy)]
#[doc(hidden)]
pub struct TableDirectoryMarker {
    table_records_byte_len: usize,
}

impl TableDirectoryMarker {
    fn sfnt_version_byte_range(&self) -> Range<usize> {
        let start = 0;
        start..start + u32::RAW_BYTE_LEN
    }
    fn num_tables_byte_range(&self) -> Range<usize> {
        let start = self.sfnt_version_byte_range().end;
        start..start + u16::RAW_BYTE_LEN
    }
    fn search_range_byte_range(&self) -> Range<usize> {
        let start = self.num_tables_byte_range().end;
        start..start + u16::RAW_BYTE_LEN
    }
    fn entry_selector_byte_range(&self) -> Range<usize> {
        let start = self.search_range_byte_range().end;
        start..start + u16::RAW_BYTE_LEN
    }
    fn range_shift_byte_range(&self) -> Range<usize> {
        let start = self.entry_selector_byte_range().end;
        start..start + u16::RAW_BYTE_LEN
    }
    fn table_records_byte_range(&self) -> Range<usize> {
        let start = self.range_shift_byte_range().end;
        start..start + self.table_records_byte_len
    }
}

impl TableInfo for TableDirectoryMarker {
    #[allow(unused_parens)]
    fn parse(data: FontData) -> Result<TableRef<Self>, ReadError> {
        let mut cursor = data.cursor();
        cursor.advance::<u32>();
        let num_tables: u16 = cursor.read()?;
        cursor.advance::<u16>();
        cursor.advance::<u16>();
        cursor.advance::<u16>();
        let table_records_byte_len = num_tables as usize * TableRecord::RAW_BYTE_LEN;
        cursor.advance_by(table_records_byte_len);
        cursor.finish(TableDirectoryMarker {
            table_records_byte_len,
        })
    }
}

/// The OpenType [Table Directory](https://docs.microsoft.com/en-us/typography/opentype/spec/otff#table-directory)
pub type TableDirectory<'a> = TableRef<'a, TableDirectoryMarker>;

impl<'a> TableDirectory<'a> {
    /// 0x00010000 or 0x4F54544F
    pub fn sfnt_version(&self) -> u32 {
        let range = self.shape.sfnt_version_byte_range();
        self.data.read_at(range.start).unwrap()
    }

    /// Number of tables.
    pub fn num_tables(&self) -> u16 {
        let range = self.shape.num_tables_byte_range();
        self.data.read_at(range.start).unwrap()
    }

    pub fn search_range(&self) -> u16 {
        let range = self.shape.search_range_byte_range();
        self.data.read_at(range.start).unwrap()
    }

    pub fn entry_selector(&self) -> u16 {
        let range = self.shape.entry_selector_byte_range();
        self.data.read_at(range.start).unwrap()
    }

    pub fn range_shift(&self) -> u16 {
        let range = self.shape.range_shift_byte_range();
        self.data.read_at(range.start).unwrap()
    }

    /// Table records array—one for each top-level table in the font
    pub fn table_records(&self) -> &'a [TableRecord] {
        let range = self.shape.table_records_byte_range();
        self.data.read_array(range).unwrap()
    }
}

#[cfg(feature = "traversal")]
impl<'a> SomeTable<'a> for TableDirectory<'a> {
    fn type_name(&self) -> &str {
        "TableDirectory"
    }
    fn get_field(&self, idx: usize) -> Option<Field<'a>> {
        match idx {
            0usize => Some(Field::new("sfnt_version", self.sfnt_version())),
            1usize => Some(Field::new("num_tables", self.num_tables())),
            2usize => Some(Field::new("search_range", self.search_range())),
            3usize => Some(Field::new("entry_selector", self.entry_selector())),
            4usize => Some(Field::new("range_shift", self.range_shift())),
            5usize => Some(Field::new(
                "table_records",
                traversal::FieldType::array_of_records(
                    stringify!(TableRecord),
                    self.table_records(),
                    self.offset_data(),
                ),
            )),
            _ => None,
        }
    }
}

#[cfg(feature = "traversal")]
impl<'a> std::fmt::Debug for TableDirectory<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        (self as &dyn SomeTable<'a>).fmt(f)
    }
}

/// Record for a table in a font.
#[derive(Clone, Debug)]
#[repr(C)]
#[repr(packed)]
pub struct TableRecord {
    /// Table identifier.
    pub tag: BigEndian<Tag>,
    /// Checksum for the table.
    pub checksum: BigEndian<u32>,
    /// Offset from the beginning of the font data.
    pub offset: BigEndian<Offset32>,
    /// Length of the table.
    pub length: BigEndian<u32>,
}

impl TableRecord {
    /// Table identifier.
    pub fn tag(&self) -> Tag {
        self.tag.get()
    }

    /// Checksum for the table.
    pub fn checksum(&self) -> u32 {
        self.checksum.get()
    }

    /// Offset from the beginning of the font data.
    pub fn offset(&self) -> Offset32 {
        self.offset.get()
    }

    /// Length of the table.
    pub fn length(&self) -> u32 {
        self.length.get()
    }
}

impl FixedSized for TableRecord {
    const RAW_BYTE_LEN: usize =
        Tag::RAW_BYTE_LEN + u32::RAW_BYTE_LEN + Offset32::RAW_BYTE_LEN + u32::RAW_BYTE_LEN;
}

#[cfg(feature = "traversal")]
impl<'a> SomeRecord<'a> for TableRecord {
    fn traverse(self, data: FontData<'a>) -> RecordResolver<'a> {
        RecordResolver {
            name: "TableRecord",
            get_field: Box::new(move |idx, _data| match idx {
                0usize => Some(Field::new("tag", self.tag())),
                1usize => Some(Field::new("checksum", self.checksum())),
                2usize => Some(Field::new(
                    "offset",
                    FieldType::unknown_offset(self.offset()),
                )),
                3usize => Some(Field::new("length", self.length())),
                _ => None,
            }),
            data,
        }
    }
}
