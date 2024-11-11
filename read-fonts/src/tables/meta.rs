//! The [meta (Metadata)](https://docs.microsoft.com/en-us/typography/opentype/spec/meta) table

include!("../../generated/generated_meta.rs");

impl DataMapRecord {
    /// The data under this record, interpreted from length and offset.
    pub fn data<'a>(&self, data: FontData<'a>) -> Result<&'a [u8], ReadError> {
        let start = self.data_offset().to_usize();
        let end = start + self.data_length() as usize;

        data.as_bytes()
            .get(start..end)
            .ok_or(ReadError::OutOfBounds)
    }
}
