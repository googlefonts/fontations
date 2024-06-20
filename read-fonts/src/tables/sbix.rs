//! The [sbix (Standard Bitmap Graphics)](https://docs.microsoft.com/en-us/typography/opentype/spec/sbix) table

include!("../../generated/generated_sbix.rs");

impl<'a> Strike<'a> {
    pub fn glyph_data(&self, glyph_id: GlyphId) -> Result<Option<GlyphData<'a>>, ReadError> {
        let offsets = self.glyph_data_offsets();
        let start_ix = glyph_id.to_u16() as usize;
        let start = offsets.get(start_ix).ok_or(ReadError::OutOfBounds)?.get() as usize;
        let end = offsets
            .get(start_ix + 1)
            .ok_or(ReadError::OutOfBounds)?
            .get() as usize;
        if start == end {
            // Empty glyphs are okay
            return Ok(None);
        }
        let data = self
            .offset_data()
            .slice(start..end)
            .ok_or(ReadError::OutOfBounds)?;
        Ok(Some(GlyphData::read(data)?))
    }
}

#[cfg(test)]
mod tests {
    use crate::{FontRef, TableProvider};

    // Test must not panic in 32bit build.
    // $ cargo test --target=i686-unknown-linux-gnu "sbix_strikes_count_overflow"
    // See https://github.com/googlefonts/fontations/issues/959
    #[test]
    fn sbix_strikes_count_overflow() {
        // Contains an invalid `num_strikes` values which would move the cursor outside the able.
        let test_case = &[
            0, 1, 0, 0, 0, 11, 0, 144, 0, 3, 0, 32, 79, 83, 47, 50, 0, 0, 0, 0, 0, 0, 0, 188, 0, 0,
            0, 96, 99, 109, 97, 112, 0, 0, 0, 0, 0, 0, 1, 28, 0, 0, 0, 44, 103, 108, 121, 102, 0,
            0, 0, 0, 0, 0, 1, 72, 0, 0, 0, 2, 104, 101, 97, 100, 0, 0, 0, 0, 0, 0, 1, 76, 0, 0, 0,
            54, 104, 104, 101, 97, 0, 0, 0, 0, 0, 0, 1, 132, 0, 0, 0, 36, 104, 109, 116, 120, 0,
            98, 0, 0, 0, 0, 1, 168, 0, 0, 0, 12, 108, 111, 99, 97, 0, 0, 0, 0, 0, 0, 1, 180, 0, 0,
            0, 8, 109, 97, 120, 112, 0, 2, 0, 0, 0, 0, 1, 188, 0, 0, 0, 32, 110, 97, 109, 101, 0,
            0, 0, 0, 0, 0, 1, 220, 0, 0, 0, 6, 112, 111, 115, 116, 0, 0, 0, 0, 0, 0, 1, 228, 0, 0,
            0, 40, 115, 98, 105, 120, 0, 103, 0, 0, 0, 0, 2, 12, 0, 0, 0, 57, 0, 4, 0, 0, 0, 94, 0,
            49, 0, 0, 2, 188, 2, 138, 0, 0, 0, 140, 2, 188, 2, 138, 0, 0, 1, 221, 0, 202, 255, 5,
            255, 255, 255, 255, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 32, 32, 32, 118, 32, 0, 64, 0, 67, 0, 6, 254, 65, 255, 0, 2, 0, 3, 32, 9, 18, 0, 0,
            0, 1, 0, 0, 0, 0, 0, 4, 0, 0, 0, 0, 0, 32, 0, 8, 0, 0, 0, 1, 0, 3, 0, 1, 0, 0, 0, 12,
            0, 4, 0, 32, 0, 0, 0, 4, 0, 4, 0, 1, 0, 0, 0, 67, 255, 255, 0, 0, 0, 204, 255, 255,
            255, 191, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 8, 1, 0, 0, 0, 0, 0, 0, 95, 15, 60,
            245, 0, 43, 3, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 4, 0, 0, 0, 0, 0, 3, 32,
            3, 32, 0, 0, 0, 8, 0, 2, 0, 0, 0, 0, 0, 250, 0, 1, 0, 0, 3, 223, 35, 6, 243, 0, 2, 32,
            255, 255, 0, 1, 3, 31, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 214, 0, 0, 0, 3, 3, 32,
            0, 90, 3, 32, 0, 90, 3, 32, 0, 90, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 254, 0, 3, 0, 8, 0,
            2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 6,
            222, 35, 0, 2, 0, 0, 0, 0, 0, 0, 255, 181, 0, 50, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 128, 0, 0, 0, 1, 0, 1, 255, 178, 0, 255, 250,
            255, 4, 0, 59, 255, 255, 119, 79, 70, 50, 116, 102, 116, 99, 0, 0, 0, 55, 255, 255,
            255, 0, 1, 247, 41, 1, 0, 0, 107, 97, 221, 55, 255, 0, 0, 59, 0, 3, 0, 0, 0, 0, 0, 0,
            6, 0, 0, 104, 2, 0, 101,
        ];
        let font = FontRef::new(test_case).unwrap();
        assert!(font.sbix().is_err());
    }
}
