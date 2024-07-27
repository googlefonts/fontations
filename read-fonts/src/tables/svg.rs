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

#[cfg(test)]
mod tests {
    use crate::test_helpers::BeBuffer;
    use super::*;

    #[test]
    fn read_dummy_svg_file() {
        let data: [u16; 32] = [
            // Version
            0,
            // SVGDocumentListOffset
            0, 10,
            // Reserved
            0, 0,
            // SVGDocumentList
            // numEntries
            3,
            // documentRecords
            // Record 1
            // startGlyphID
            1,
            // endGlyphID
            3,
            // svgDocOffset
            0, 38,
            // svgDocLength
            0, 10,
            // Record 2
            // startGlyphID
            6,
            // endGlyphID
            7,
            // svgDocOffset
            0, 48,
            // svgDocLength
            0, 6,
            // Record 3
            // startGlyphID
            9,
            // endGlyphID
            9,
            // svgDocOffset
            0, 38,
            // svgDocLength
            0, 10,
            // SVG Documents. Not actual valid SVGs, but just dummy data.
            // Document 1
            1, 0, 0, 0, 1,
            // Document 2
            2, 0, 0
        ];

        let mut buf = BeBuffer::new();
        buf = buf.extend(data);

        let table = SVG::read(buf.font_data()).unwrap();

        let first_document = &[0, 1, 0, 0, 0, 0, 0, 0, 0, 1][..];
        let second_document = &[0, 2, 0, 0, 0, 0][..];

        assert_eq!(table.glyph_data(GlyphId::new(0)).unwrap().map(|d| d.get()), None);
        assert_eq!(table.glyph_data(GlyphId::new(1)).unwrap().map(|d| d.get()), Some(first_document));
        assert_eq!(table.glyph_data(GlyphId::new(2)).unwrap().map(|d| d.get()), Some(first_document));
        assert_eq!(table.glyph_data(GlyphId::new(3)).unwrap().map(|d| d.get()), Some(first_document));
        assert_eq!(table.glyph_data(GlyphId::new(4)).unwrap().map(|d| d.get()), None);
        assert_eq!(table.glyph_data(GlyphId::new(5)).unwrap().map(|d| d.get()), None);
        assert_eq!(table.glyph_data(GlyphId::new(6)).unwrap().map(|d| d.get()), Some(second_document));
        assert_eq!(table.glyph_data(GlyphId::new(7)).unwrap().map(|d| d.get()), Some(second_document));
        assert_eq!(table.glyph_data(GlyphId::new(8)).unwrap().map(|d| d.get()), None);
        assert_eq!(table.glyph_data(GlyphId::new(9)).unwrap().map(|d| d.get()), Some(first_document));
        assert_eq!(table.glyph_data(GlyphId::new(10)).unwrap().map(|d| d.get()), None);

    }
}
