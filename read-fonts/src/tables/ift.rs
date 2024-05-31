//! Incremental Font Transfer [Patch Map](https://w3c.github.io/IFT/Overview.html#font-format-extensions)

include!("../../generated/generated_ift.rs");

use core::str::Utf8Error;
use std::str;

// TODO(garretrieger): fail validation if entry count is == 0.

impl<'a> PatchMapFormat1<'a> {
    pub fn gid_to_entry_iter(&'a self) -> impl Iterator<Item = (u32, u32)> + 'a {
        GidToEntryIter {
            glyph_map: self.glyph_map().ok(),
            glyph_count: self.glyph_count(),
            gid: 0,
        }
    }

    pub fn uri_template_as_string(&self) -> Result<&str, Utf8Error> {
        str::from_utf8(self.uri_template())
    }

    pub fn is_entry_applied(&self, entry_index: usize) -> bool {
        let byte_index = entry_index / 8;
        let bit_mask = 1 << (entry_index % 8);
        self.applied_entries_bitmap()
            .get(byte_index)
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
    type Item = (u32, u32);

    fn next(&mut self) -> Option<Self::Item> {
        let Some(glyph_map) = &self.glyph_map else {
            return None;
        };

        let cur_gid = self.gid;
        self.gid += 1;

        if cur_gid >= self.glyph_count {
            return None;
        }

        if cur_gid < glyph_map.first_mapped_glyph().into() {
            return Some((cur_gid, 0));
        }

        let index = cur_gid as usize - glyph_map.first_mapped_glyph() as usize;

        glyph_map
            .entry_index()
            .get(index)
            .map(|entry_index| (cur_gid, *entry_index as u32))
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

    #[test]
    fn format_1_gid_to_entry_iter() {
        let table = Ift::read(test_data::SIMPLE_FORMAT1.into()).unwrap();
        let Ift::Format1(map) = table else {
            panic!("Not format 1.");
        };
        let entries: Vec<(u32, u32)> = map.gid_to_entry_iter().collect();

        assert_eq!(entries, vec![(0, 0), (1, 0), (2, 1), (3, 0),]);
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
