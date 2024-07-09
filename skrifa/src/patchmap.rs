//! Loads incremental font transfer (https://w3c.github.io/IFT/Overview.html) patch mappings.
//!
//! The IFT and IFTX tables encode mappings from subset definitions to URL's which host patches
//! that can be applied to the font to add support for the corresponding subset definition.

use std::collections::HashMap;

use raw::types::GlyphId;
use read_fonts::{
    tables::ift::{Ift, PatchMapFormat1},
    TableProvider,
};

use int_set::IntSet;

use crate::charmap::Charmap;

#[derive(Clone, Eq, PartialEq, Debug)]
pub enum PatchEncoding {
    Brotli,
    PerTableBrotli { fully_invalidating: bool },
    GlyphKeyed,
}

impl PatchEncoding {
    fn from_format_number(format: u8) -> Option<Self> {
        match format {
            1 => Some(Self::Brotli),
            2 => Some(Self::PerTableBrotli {
                fully_invalidating: true,
            }),
            3 => Some(Self::PerTableBrotli {
                fully_invalidating: false,
            }),
            4 => Some(Self::GlyphKeyed),
            _ => None,
        }
    }
}

#[derive(Default)]
pub struct PatchMap {
    entry_list: Vec<Entry>,
    uri_format: HashMap<String, PatchEncoding>,
    // TODO(garretrieger): store an index from URI to the bit location that can be set to mark the entry as ignored.
}

impl PatchMap {
    pub fn new<'a>(font: &impl TableProvider<'a>) -> Self {
        // TODO(garretrieger): propagate errors up, or silently drop malformed mappings?
        //   - spec probably requires the error to be recognized and acted on, but need to double check.
        let mut map = PatchMap::default();
        if let Ok(ift) = font.ift() {
            map.add_entries(&ift, font);
        };
        if let Ok(iftx) = font.iftx() {
            map.add_entries(&iftx, font);
        };
        map
    }

    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &Entry> {
        self.entry_list.iter()
    }

    pub fn patch_encoding(&self, patch_uri: &str) -> Option<PatchEncoding> {
        self.uri_format.get(patch_uri).cloned()
    }

    // TODO(garretrieger): add method that can be used to actuate removal of entries given a URI.

    fn add_entries<'a>(&mut self, mapping: &Ift, font: &impl TableProvider<'a>) {
        match mapping {
            Ift::Format1(format_1) => self.add_format_1_entries(format_1, font),
            Ift::Format2(_) => todo!(),
        }
    }

    fn add_format_1_entries<'a>(
        &mut self,
        mapping: &PatchMapFormat1,
        font: &impl TableProvider<'a>,
    ) {
        let Ok(uri_template) = mapping.uri_template_as_string() else {
            return;
        };

        let Some(patch_encoding) = PatchEncoding::from_format_number(mapping.patch_encoding())
        else {
            return;
        };

        let prototype = Entry {
            patch_uri: String::new(),
            codepoints: IntSet::empty(),
            compatibility_id: mapping.get_compatibility_id(),
        };

        let mut entries = vec![];
        entries.resize(mapping.entry_count() as usize, prototype);
        for (entry_index, entry) in entries.iter_mut().enumerate() {
            entry.patch_uri = PatchMap::apply_uri_template(uri_template, entry_index);
            self.uri_format
                .insert(entry.patch_uri.clone(), patch_encoding.clone());
        }

        let glyph_to_unicode = PatchMap::glyph_to_unicode_map(font);
        for (gid, entry_index) in mapping.gid_to_entry_iter() {
            let Some(entry) = entries.get_mut(entry_index as usize) else {
                continue;
            };

            if let Some(codepoints) = glyph_to_unicode.get(&gid) {
                entry.codepoints.extend(codepoints.iter());
            };
        }

        // TODO(garretrieger): some entries may not have had an codepoints added and will have empty codepoint sets
        //                     (matching all). We should clarify in the spec text whether this is allowed or not.
        self.entry_list.extend(
            entries
                .into_iter()
                .enumerate()
                .filter(|(index, _)| !mapping.is_entry_applied(*index as u32))
                .map(|(_, entry)| entry),
        )
    }

    /// Produce a mapping from each glyph id to the codepoint(s) that map to that glyph.
    fn glyph_to_unicode_map<'a>(font: &impl TableProvider<'a>) -> HashMap<GlyphId, IntSet<u32>> {
        let charmap = Charmap::new(font);
        let mut glyph_to_unicode = HashMap::<GlyphId, IntSet<u32>>::new();
        for (cp, glyph) in charmap.mappings() {
            glyph_to_unicode
                .entry(glyph)
                .or_insert_with(IntSet::<u32>::empty)
                .insert(cp);
        }
        glyph_to_unicode
    }

    fn apply_uri_template(uri_template: &str, _entry_index: usize) -> String {
        // TODO(garretrieger): properly implement this, may deserve to go into it's own module.
        uri_template.to_string()
    }

    // TODO(garretrieger): add template application method that takes a string entry id (will be needed for format 2)
}

#[derive(Clone, Eq, PartialEq, Hash)]
pub struct Entry {
    pub patch_uri: String,
    pub codepoints: IntSet<u32>,
    pub compatibility_id: [u32; 4],
}

impl std::fmt::Debug for Entry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        let values: Vec<_> = self.codepoints.iter().collect();
        write!(f, "Entry({values:?} => {})", self.patch_uri)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use font_test_data as test_data;
    use read_fonts::{
        tables::ift::{IFT, IFTX},
        FontRef, TopLevelTable,
    };
    use write_fonts::FontBuilder;

    fn create_ift_font(font: FontRef, ift: Option<&[u8]>, iftx: Option<&[u8]>) -> Vec<u8> {
        let mut builder = FontBuilder::default();

        if let Some(bytes) = ift {
            builder.add_raw(IFT::TAG, bytes);
        }

        if let Some(bytes) = iftx {
            builder.add_raw(IFTX::TAG, bytes);
        }

        builder.copy_missing_tables(font);
        builder.build()
    }

    // TODO(garretrieger): test w/ multi codepoints mapping to the same glyph.
    // TODO(garretrieger): test w/ IFT + IFTX both populated tables.
    // TODO(garretrieger): test which has entry that has empty codepoint array.
    // TODO(garretrieger): test which has requires URI substitution.
    // TODO(garretrieger): patch encoding lookup + URI substitution.

    #[test]
    fn patch_encoding() {
        let font_bytes = create_ift_font(
            FontRef::new(test_data::CMAP12_FONT1).unwrap(),
            Some(test_data::ift::SIMPLE_FORMAT1),
            None,
        );
        let font = FontRef::new(&font_bytes).unwrap();
        let patch_map = PatchMap::new(&font);

        assert_eq!(
            patch_map.patch_encoding("ABCDEFɤ"),
            Some(PatchEncoding::GlyphKeyed)
        );

        assert_eq!(patch_map.patch_encoding("ABCDEFG"), None);
    }

    #[test]
    fn format_1_patch_map() {
        let font_bytes = create_ift_font(
            FontRef::new(test_data::CMAP12_FONT1).unwrap(),
            Some(test_data::ift::SIMPLE_FORMAT1),
            None,
        );
        let font = FontRef::new(&font_bytes).unwrap();

        let patch_map = PatchMap::new(&font);
        let entries: Vec<&Entry> = patch_map.iter().collect();

        assert_eq!(
            entries,
            vec![
                // Entry 0
                &Entry {
                    patch_uri: "ABCDEFɤ".to_string(),
                    codepoints: [0x101723, 0x101725].into_iter().collect(),
                    compatibility_id: [1, 2, 3, 4],
                },
            ]
        );
    }
}
