//! Incremental Font Transfer [Patch Map](https://w3c.github.io/IFT/Overview.html#font-format-extensions)

include!("../../generated/generated_ift.rs");

use core::str::Utf8Error;
use std::str;

// TODO(garretrieger): fail validation if entry count is == 0.

impl<'a> PatchMapFormat1<'a> {
    pub fn get_compatibility_id(&self) -> [u32; 4] {
        [
            self.compatibility_id().first().unwrap().get(),
            self.compatibility_id().get(1).unwrap().get(),
            self.compatibility_id().get(2).unwrap().get(),
            self.compatibility_id().get(3).unwrap().get(),
        ]
    }

    pub fn gid_to_entry_iter(&'a self) -> impl Iterator<Item = (GlyphId, u32)> + 'a {
        GidToEntryIter {
            glyph_map: self.glyph_map().ok(),
            glyph_count: self.glyph_count(),
            gid: 0,
        }
    }

    pub fn uri_template_as_string(&self) -> Result<&str, Utf8Error> {
        str::from_utf8(self.uri_template())
    }

    pub fn is_entry_applied(&self, entry_index: u32) -> bool {
        let byte_index = entry_index / 8;
        let bit_mask = 1 << (entry_index % 8);
        self.applied_entries_bitmap()
            .get(byte_index as usize)
            .map(|byte| byte & bit_mask != 0)
            .unwrap_or(false)
    }
}

struct GidToEntryIter<'a> {
    glyph_map: Option<GlyphMap<'a>>,
    glyph_count: u32,
    gid: u32,
}

impl<'a> Iterator for GidToEntryIter<'a> {
    type Item = (GlyphId, u32);

    fn next(&mut self) -> Option<Self::Item> {
        let glyph_map = self.glyph_map.as_ref()?;

        let cur_gid = self.gid;
        self.gid += 1;

        if cur_gid >= self.glyph_count {
            return None;
        }

        if cur_gid < glyph_map.first_mapped_glyph().into() {
            // TODO(garretrieger): this cast may overflow, GlyphId is going to become u32
            //   in the near future (https://github.com/googlefonts/fontations/issues/784)
            //   once it is, this cast should be removed.
            return Some(((cur_gid as u16).into(), 0));
        }

        let index = cur_gid as usize - glyph_map.first_mapped_glyph() as usize;
        glyph_map
            .entry_index()
            .get(index)
            // TODO(garretrieger): this cast may overflow, GlyphId is going to become u32
            //   in the near future (https://github.com/googlefonts/fontations/issues/784)
            //   once it is, this cast should be removed.
            .map(|entry_index| ((cur_gid as u16).into(), *entry_index as u32))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use font_test_data::ift as test_data;

    // TODO(garretrieger) - more tests (as functionality is implemented):
    // - Test where entryIndex array has len 0 (eg. all glyphs map to 0)
    // - Test which appliedEntriesBitmap > 1 byte
    // - Test w/ feature map populated.
    // - Test enforced minimum entry count of > 0.
    // - Test where entryIndex is a u16.
    // - Invalid table (too short).
    // - Invalid UTF8 sequence in uri template.
    // - Compat ID is to short.

    #[test]
    fn format_1_gid_to_entry_iter() {
        let table = Ift::read(test_data::SIMPLE_FORMAT1.into()).unwrap();
        let Ift::Format1(map) = table else {
            panic!("Not format 1.");
        };
        let entries: Vec<(GlyphId, u32)> = map.gid_to_entry_iter().collect();

        assert_eq!(
            entries,
            vec![(0.into(), 0), (1.into(), 0), (2.into(), 1), (3.into(), 0),]
        );
    }

    #[test]
    fn compatibility_id() {
        let table = Ift::read(test_data::SIMPLE_FORMAT1.into()).unwrap();
        let Ift::Format1(map) = table else {
            panic!("Not format 1.");
        };

        assert_eq!(map.get_compatibility_id(), [1, 2, 3, 4]);
    }

    #[test]
    fn is_entry_applied() {
        let table = Ift::read(test_data::SIMPLE_FORMAT1.into()).unwrap();
        let Ift::Format1(map) = table else {
            panic!("Not format 1.");
        };
        assert!(!map.is_entry_applied(0));
        assert!(map.is_entry_applied(1));
        assert!(!map.is_entry_applied(2));
    }

    #[test]
    fn uri_template_as_string() {
        let table = Ift::read(test_data::SIMPLE_FORMAT1.into()).unwrap();
        let Ift::Format1(map) = table else {
            panic!("Not format 1.");
        };

        assert_eq!(Ok("ABCDEFɤ"), map.uri_template_as_string());
    }
}
