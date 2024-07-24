//! Incremental Font Transfer [Patch Map](https://w3c.github.io/IFT/Overview.html#font-format-extensions)

include!("../../generated/generated_ift.rs");

use core::str::Utf8Error;
use std::str;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct U8Or16(u16);

impl ReadArgs for U8Or16 {
    type Args = u16;
}

impl ComputeSize for U8Or16 {
    fn compute_size(max_entry_index: &u16) -> Result<usize, ReadError> {
        Ok(if *max_entry_index < 256 { 1 } else { 2 })
    }
}

impl FontReadWithArgs<'_> for U8Or16 {
    fn read_with_args(data: FontData<'_>, max_entry_index: &Self::Args) -> Result<Self, ReadError> {
        if *max_entry_index < 256 {
            data.read_at::<u8>(0).map(|v| Self(v as u16))
        } else {
            data.read_at::<u16>(0).map(Self)
        }
    }
}

impl U8Or16 {
    #[inline]
    pub fn get(self) -> u16 {
        self.0
    }
}

impl<'a> PatchMapFormat1<'a> {
    pub fn get_compatibility_id(&self) -> [u32; 4] {
        [
            self.compatibility_id().first().unwrap().get(),
            self.compatibility_id().get(1).unwrap().get(),
            self.compatibility_id().get(2).unwrap().get(),
            self.compatibility_id().get(3).unwrap().get(),
        ]
    }

    pub fn gid_to_entry_iter(&'a self) -> impl Iterator<Item = (GlyphId, u16)> + 'a {
        GidToEntryIter {
            glyph_map: self.glyph_map().ok(),
            glyph_count: self.glyph_count(),
            gid: self
                .glyph_map()
                .map(|glyph_map| glyph_map.first_mapped_glyph() as u32)
                .unwrap_or(0),
        }
        .filter(|(_, entry_index)| *entry_index > 0)
    }

    pub fn entry_count(&self) -> u32 {
        self.max_entry_index() as u32 + 1
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
    type Item = (GlyphId, u16);

    fn next(&mut self) -> Option<Self::Item> {
        let glyph_map = self.glyph_map.as_ref()?;

        let cur_gid = self.gid;
        self.gid += 1;

        if cur_gid >= self.glyph_count {
            return None;
        }

        let index = cur_gid as usize - glyph_map.first_mapped_glyph() as usize;
        glyph_map
            .entry_index()
            .get(index)
            .ok()
            .map(|entry_index| (cur_gid.into(), entry_index.0))
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
    fn format_1_gid_to_u8_entry_iter() {
        let table = Ift::read(test_data::SIMPLE_FORMAT1.into()).unwrap();
        let Ift::Format1(map) = table else {
            panic!("Not format 1.");
        };
        let entries: Vec<(GlyphId, u16)> = map.gid_to_entry_iter().collect();

        assert_eq!(entries, vec![(1u32.into(), 2), (2u32.into(), 1),]);
    }

    #[test]
    fn format_1_gid_to_u16_entry_iter() {
        let table = Ift::read(test_data::U16_ENTRIES_FORMAT1.into()).unwrap();
        let Ift::Format1(map) = table else {
            panic!("Not format 1.");
        };
        let entries: Vec<(GlyphId, u16)> = map.gid_to_entry_iter().collect();

        assert_eq!(
            entries,
            vec![
                (2u32.into(), 0x50),
                (3u32.into(), 0x51),
                (4u32.into(), 0x12c),
                (5u32.into(), 0x12c),
                (6u32.into(), 0x50)
            ]
        );
    }

    #[test]
    fn format_1_feature_map() {
        let table = Ift::read(test_data::FEATURE_MAP_FORMAT1.into()).unwrap();
        let Ift::Format1(map) = table else {
            panic!("Not format 1.");
        };

        let Some(feature_map_result) = map.feature_map() else {
            panic!("should have a non null feature map.");
        };

        let Ok(feature_map) = feature_map_result else {
            panic!("should have a valid feature map.");
        };

        assert_eq!(feature_map.feature_records().len(), 2);

        let fr0 = feature_map.feature_records().get(0).unwrap();
        assert_eq!(fr0.feature_tag(), Tag::new(&[b'l', b'i', b'g', b'a']));
        assert_eq!(*fr0.first_new_entry_index(), U8Or16(0x70));
        assert_eq!(*fr0.entry_map_count(), U8Or16(0x02));

        let fr1 = feature_map.feature_records().get(1).unwrap();
        assert_eq!(fr1.feature_tag(), Tag::new(&[b'd', b'l', b'i', b'g']));
        assert_eq!(*fr1.first_new_entry_index(), U8Or16(0x190));
        assert_eq!(*fr1.entry_map_count(), U8Or16(0x01));
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
