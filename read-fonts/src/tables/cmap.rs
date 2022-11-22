//! The [cmap](https://docs.microsoft.com/en-us/typography/opentype/spec/cmap) table

use types::Tag;

/// 'cmap'
pub const TAG: Tag = Tag::new(b"cmap");

include!("../../generated/generated_cmap.rs");

impl<'a> Cmap<'a> {
    /// Maps a codepoint to a nominal glyph identifier using the first
    /// available subtable that provides a valid mapping.
    pub fn map_codepoint(&self, codepoint: impl Into<u32>) -> Option<GlyphId> {
        let codepoint = codepoint.into();
        for record in self.encoding_records() {
            if let Ok(subtable) = record.subtable(self.offset_data()) {
                if let Some(gid) = match subtable {
                    CmapSubtable::Format4(format4) => format4.map_codepoint(codepoint),
                    CmapSubtable::Format12(format12) => format12.map_codepoint(codepoint),
                    _ => None,
                } {
                    return Some(gid);
                }
            }
        }
        None
    }
}

impl<'a> Cmap4<'a> {
    /// Maps a codepoint to a nominal glyph identifier.
    pub fn map_codepoint(&self, codepoint: impl Into<u32>) -> Option<GlyphId> {
        let codepoint = codepoint.into();
        if codepoint > 0xFFFF {
            return None;
        }
        let codepoint = codepoint as u16;
        let mut lo = 0;
        let mut hi = self.seg_count_x2() as usize / 2;
        let start_codes = self.start_code();
        let end_codes = self.end_code();
        while lo < hi {
            let i = (lo + hi) / 2;
            let start_code = start_codes.get(i)?.get();
            if codepoint < start_code {
                hi = i;
            } else if codepoint > end_codes.get(i)?.get() {
                lo = i + 1;
            } else {
                let deltas = self.id_delta();
                let range_offsets = self.id_range_offsets();
                let delta = deltas.get(i)?.get() as i32;
                let range_offset = range_offsets.get(i)?.get() as usize;
                if range_offset == 0 {
                    return Some(GlyphId::new((codepoint as i32 + delta) as u16));
                }
                // sigh
                let mut offset = range_offset / 2 + (codepoint - start_code) as usize;
                offset = offset.saturating_sub(range_offsets.len() - i);
                let gid = self.glyph_id_array().get(offset)?.get();
                if gid != 0 {
                    return Some(GlyphId::new((gid as i32 + delta) as u16));
                } else {
                    return None;
                }
            }
        }
        None
    }
}

impl<'a> Cmap12<'a> {
    /// Maps a codepoint to a nominal glyph identifier.
    pub fn map_codepoint(&self, codepoint: impl Into<u32>) -> Option<GlyphId> {
        let codepoint = codepoint.into();
        let groups = self.groups();
        let mut lo = 0;
        let mut hi = groups.len();
        while lo < hi {
            let i = (lo + hi) / 2;
            let group = groups.get(i)?;
            if codepoint < group.start_char_code() {
                hi = i;
            } else if codepoint > group.end_char_code() {
                lo = i + 1;
            } else {
                return Some(GlyphId::new(
                    (group
                        .start_glyph_id()
                        .wrapping_add(codepoint.wrapping_sub(group.start_char_code())))
                        as u16,
                ));
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use crate::test_data;
    use crate::{FontRef, GlyphId, TableProvider};

    #[test]
    fn map_codepoints() {
        let font = FontRef::new(test_data::test_fonts::VAZIRMATN_VAR).unwrap();
        let cmap = font.cmap().unwrap();
        assert_eq!(cmap.map_codepoint('A'), Some(GlyphId::new(1)));
        assert_eq!(cmap.map_codepoint('À'), Some(GlyphId::new(2)));
        assert_eq!(cmap.map_codepoint('`'), Some(GlyphId::new(3)));
        assert_eq!(cmap.map_codepoint('B'), None);

        let font = FontRef::new(test_data::test_fonts::SIMPLE_GLYF).unwrap();
        let cmap = font.cmap().unwrap();
        assert_eq!(cmap.map_codepoint(' '), Some(GlyphId::new(1)));
        assert_eq!(cmap.map_codepoint(0xE_u32), Some(GlyphId::new(2)));
        assert_eq!(cmap.map_codepoint('B'), None);
    }
}
