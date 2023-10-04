// THIS FILE IS AUTOGENERATED.
// Any changes to this file will be overwritten.
// For more information about how codegen works, see font-codegen/README.md

#[allow(unused_imports)]
use crate::codegen_prelude::*;

/// The [Embedded Bitmap Data](https://learn.microsoft.com/en-us/typography/opentype/spec/ebdt) table
#[derive(Debug, Clone, Copy)]
#[doc(hidden)]
pub struct EbdtMarker {}

impl EbdtMarker {
    fn major_version_byte_range(&self) -> Range<usize> {
        let start = 0;
        start..start + u16::RAW_BYTE_LEN
    }
    fn minor_version_byte_range(&self) -> Range<usize> {
        let start = self.major_version_byte_range().end;
        start..start + u16::RAW_BYTE_LEN
    }
}

impl TopLevelTable for Ebdt<'_> {
    /// `EBDT`
    const TAG: Tag = Tag::new(b"EBDT");
}

impl<'a> FontRead<'a> for Ebdt<'a> {
    fn read(data: FontData<'a>) -> Result<Self, ReadError> {
        let mut cursor = data.cursor();
        cursor.advance::<u16>();
        cursor.advance::<u16>();
        cursor.finish(EbdtMarker {})
    }
}

/// The [Embedded Bitmap Data](https://learn.microsoft.com/en-us/typography/opentype/spec/ebdt) table
pub type Ebdt<'a> = TableRef<'a, EbdtMarker>;

impl<'a> Ebdt<'a> {
    /// Major version of the EBDT table, = 2.
    pub fn major_version(&self) -> u16 {
        let range = self.shape.major_version_byte_range();
        self.data.read_at(range.start).unwrap()
    }

    /// Minor version of EBDT table, = 0.
    pub fn minor_version(&self) -> u16 {
        let range = self.shape.minor_version_byte_range();
        self.data.read_at(range.start).unwrap()
    }
}

#[cfg(feature = "traversal")]
impl<'a> SomeTable<'a> for Ebdt<'a> {
    fn type_name(&self) -> &str {
        "Ebdt"
    }
    fn get_field(&self, idx: usize) -> Option<Field<'a>> {
        match idx {
            0usize => Some(Field::new("major_version", self.major_version())),
            1usize => Some(Field::new("minor_version", self.minor_version())),
            _ => None,
        }
    }
}

#[cfg(feature = "traversal")]
impl<'a> std::fmt::Debug for Ebdt<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        (self as &dyn SomeTable<'a>).fmt(f)
    }
}