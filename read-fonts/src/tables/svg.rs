//! The [SVG](https://learn.microsoft.com/en-us/typography/opentype/spec/svg) table

use core::cmp::Ordering;

include!("../../generated/generated_svg.rs");

/// An [SVG document](https://learn.microsoft.com/en-us/typography/opentype/spec/svg). Is not
/// guaranteed to be valid and might be compressed.
pub struct SVGDocument<'a>(&'a [u8]);

impl<'a> SVGDocument<'a> {
    /// Get the raw data of the SVG document.
    pub fn get(&self) -> &'a [u8] {
        self.0
    }
}

impl<'a> SVG<'a> {
    pub fn glyph_data(&self, glyph_id: GlyphId) -> Result<Option<SVGDocument<'a>>, ReadError> {
        let document_list = self.svg_document_list()?;
        let svg_document = document_list.document_records()
            .binary_search_by(|r| {
                if r.start_glyph_id.get() > glyph_id {
                    Ordering::Greater
                }   else if r.end_glyph_id.get() < glyph_id {
                    Ordering::Less
                }   else {
                    Ordering::Equal
                }
            })
            .ok()
            .and_then(|index| document_list.document_records().get(index))
            .and_then(|r| {
                let all_data = document_list.data.as_bytes();
                all_data.get(r.svg_doc_offset.get() as usize..(r.svg_doc_offset.get() + r.svg_doc_length.get()) as usize)
            } )
            .map(|data| SVGDocument(data));

        Ok(svg_document)
    }
}
