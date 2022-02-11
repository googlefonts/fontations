toy_table_macro::tables! {
    GlyphHeader {
        number_of_contours: Int16,
        x_min: Int16,
        y_min: Int16,
        x_max: Int16,
        y_max: Int16,
    }
}
//impl<'a> Glyf<'a> {
//pub fn new(data: Blob<'a>) -> Option<Self> {
//Some(Self { data })
//}

//pub fn get(&self, offset: usize) -> Option<GlyphHeader> {
//self.data
//.get(offset..self.data.len())
//.and_then(GlyphHeader::read)
//}

//pub fn get_zc(&self, offset: usize) -> Option<&'a GlyphHeaderZero> {
//let verified: LayoutVerified<_, GlyphHeaderZero> =
//self.data
//.get(offset..self.data.len())
//.and_then(|blob| LayoutVerified::new_unaligned(blob.as_bytes()))?;
//Some(verified.into_ref())
//}

//pub fn get_view(&self, offset: usize) -> Option<GlyphHeaderDerivedView> {
//self.data
//.get(offset..self.data.len())
//.and_then(<GlyphHeader as FontThing>::View::read)
//}
//}
