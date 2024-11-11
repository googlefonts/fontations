//! The [meta (Metadata)](https://docs.microsoft.com/en-us/typography/opentype/spec/meta) table

include!("../../generated/generated_meta.rs");

impl DataMapRecord {
    /// Required to append a variable length slice of bytes at the end of the
    /// table, referenced by length and offset in this record.
    fn compile_map_value(&self) -> MapValueAndLenWriter {
        MapValueAndLenWriter(self.data.as_slice())
    }
}

struct MapValueAndLenWriter<'a>(&'a [u8]);

impl FontWrite for MapValueAndLenWriter<'_> {
    fn write_into(&self, writer: &mut TableWriter) {
        let length = u32::try_from(self.0.len()).expect("meta record data too long: exceeds u32");

        writer.write_offset(&self.0, 4);
        length.write_into(writer);
    }
}

// TODO: is this necessary?
impl FontWrite for &[u8] {
    fn write_into(&self, writer: &mut TableWriter) {
        writer.write_slice(self);
    }
}
