// THIS FILE IS AUTOGENERATED.
// Any changes to this file will be overwritten.
// For more information about how codegen works, see font-codegen/README.md

#[allow(unused_imports)]
use crate::codegen_prelude::*;

/// The [Color Bitmap Location](https://learn.microsoft.com/en-us/typography/opentype/spec/cblc) table
#[derive(Debug, Clone, Copy)]
#[doc(hidden)]
pub struct CblcMarker {
    bitmap_sizes_byte_len: usize,
}

impl CblcMarker {
    pub fn major_version_byte_range(&self) -> Range<usize> {
        let start = 0;
        start..start + u16::RAW_BYTE_LEN
    }

    pub fn minor_version_byte_range(&self) -> Range<usize> {
        let start = self.major_version_byte_range().end;
        start..start + u16::RAW_BYTE_LEN
    }

    pub fn num_sizes_byte_range(&self) -> Range<usize> {
        let start = self.minor_version_byte_range().end;
        start..start + u32::RAW_BYTE_LEN
    }

    pub fn bitmap_sizes_byte_range(&self) -> Range<usize> {
        let start = self.num_sizes_byte_range().end;
        start..start + self.bitmap_sizes_byte_len
    }
}

impl TopLevelTable for Cblc<'_> {
    /// `CBLC`
    const TAG: Tag = Tag::new(b"CBLC");
}

impl<'a> FontRead<'a> for Cblc<'a> {
    fn read(data: FontData<'a>) -> Result<Self, ReadError> {
        let mut cursor = data.cursor();
        cursor.advance::<u16>();
        cursor.advance::<u16>();
        let num_sizes: u32 = cursor.read()?;
        let bitmap_sizes_byte_len = (num_sizes as usize)
            .checked_mul(BitmapSize::RAW_BYTE_LEN)
            .ok_or(ReadError::OutOfBounds)?;
        cursor.advance_by(bitmap_sizes_byte_len);
        cursor.finish(CblcMarker {
            bitmap_sizes_byte_len,
        })
    }
}

/// The [Color Bitmap Location](https://learn.microsoft.com/en-us/typography/opentype/spec/cblc) table
pub type Cblc<'a> = TableRef<'a, CblcMarker>;

#[allow(clippy::needless_lifetimes)]
impl<'a> Cblc<'a> {
    /// Major version of the CBLC table, = 3.
    pub fn major_version(&self) -> u16 {
        let range = self.shape.major_version_byte_range();
        self.data.read_at(range.start).unwrap()
    }

    /// Minor version of CBLC table, = 0.
    pub fn minor_version(&self) -> u16 {
        let range = self.shape.minor_version_byte_range();
        self.data.read_at(range.start).unwrap()
    }

    /// Number of BitmapSize records.
    pub fn num_sizes(&self) -> u32 {
        let range = self.shape.num_sizes_byte_range();
        self.data.read_at(range.start).unwrap()
    }

    /// BitmapSize records array.
    pub fn bitmap_sizes(&self) -> &'a [BitmapSize] {
        let range = self.shape.bitmap_sizes_byte_range();
        self.data.read_array(range).unwrap()
    }
}

#[cfg(feature = "experimental_traverse")]
impl<'a> SomeTable<'a> for Cblc<'a> {
    fn type_name(&self) -> &str {
        "Cblc"
    }
    fn get_field(&self, idx: usize) -> Option<Field<'a>> {
        match idx {
            0usize => Some(Field::new("major_version", self.major_version())),
            1usize => Some(Field::new("minor_version", self.minor_version())),
            2usize => Some(Field::new("num_sizes", self.num_sizes())),
            3usize => Some(Field::new(
                "bitmap_sizes",
                traversal::FieldType::array_of_records(
                    stringify!(BitmapSize),
                    self.bitmap_sizes(),
                    self.offset_data(),
                ),
            )),
            _ => None,
        }
    }
}

#[cfg(feature = "experimental_traverse")]
#[allow(clippy::needless_lifetimes)]
impl<'a> std::fmt::Debug for Cblc<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        (self as &dyn SomeTable<'a>).fmt(f)
    }
}
