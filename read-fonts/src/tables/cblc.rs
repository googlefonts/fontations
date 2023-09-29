//! The [CBLC (Color Bitmap Location)](https://docs.microsoft.com/en-us/typography/opentype/spec/cblc) table

include!("../../generated/generated_cblc.rs");

#[cfg(feature = "traversal")]
impl SbitLineMetrics {
    pub(crate) fn traversal_type<'a>(&self, data: FontData<'a>) -> FieldType<'a> {
        FieldType::Record(self.clone().traverse(data))
    }

    pub(crate) fn get_field<'a>(&self, idx: usize, data: FontData<'a>) -> Option<Field<'a>> {
        None
    }
}
