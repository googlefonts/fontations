//! Loads incremental font transfer <https://w3c.github.io/IFT/Overview.html> patch mappings.
//!
//! The IFT and IFTX tables encode mappings from subset definitions to URL's which host patches
//! that can be applied to the font to add support for the corresponding subset definition.

use std::collections::{BTreeSet, HashMap};

use crate::Tag;
use raw::types::GlyphId;
use read_fonts::{
    tables::ift::{Ift, PatchMapFormat1},
    TableProvider,
};

use read_fonts::collections::IntSet;

use crate::charmap::Charmap;

#[derive(Clone, Eq, PartialEq, Debug, Hash)]
pub enum PatchEncoding {
    Brotli,
    PerTableBrotli { fully_invalidating: bool },
    GlyphKeyed,
}

impl PatchEncoding {
    fn from_format_number(format: u8) -> Option<Self> {
        // Based on https://w3c.github.io/IFT/Overview.html#font-patch-formats-summary
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
            patch_uri: PatchUri {
                uri: String::new(),
                encoding: patch_encoding,
            },
            codepoints: IntSet::empty(),
            feature_tags: BTreeSet::new(),
            compatibility_id: mapping.get_compatibility_id(),
        };

        // Assume nearly all entries up to max entry index will be present, and preallocate storage for them.
        let mut entries = vec![];
        let mut present_entries = IntSet::<u16>::empty();
        entries.resize(mapping.entry_count() as usize, prototype);
        for (entry_index, entry) in entries.iter_mut().enumerate() {
            entry.patch_uri.uri = PatchMap::apply_uri_template(uri_template, entry_index);
        }

        let glyph_to_unicode = PatchMap::glyph_to_unicode_map(font);
        for (gid, entry_index) in mapping.gid_to_entry_iter() {
            let Some(entry) = entries.get_mut(entry_index as usize) else {
                // Table is invalid, entry_index is out of bounds.
                return;
            };

            present_entries.insert(entry_index);
            if let Some(codepoints) = glyph_to_unicode.get(&gid) {
                entry.codepoints.extend(codepoints.iter());
            };
        }

        Self::add_format_1_feature_entries(mapping, &mut entries, &mut present_entries);

        self.entry_list.extend(
            entries
                .into_iter()
                .enumerate()
                .filter(|(index, _)| !mapping.is_entry_applied(*index as u32))
                // Entries not referenced in the table do not exist, per the spec:
                // <https://w3c.github.io/IFT/Overview.html#interpreting-patch-map-format-1>
                .filter(|(index, _)| present_entries.contains(*index as u16))
                .map(|(_, entry)| entry),
        )
    }

    fn add_format_1_feature_entries(
        mapping: &PatchMapFormat1,
        entries: &mut [Entry],
        present_entries: &mut IntSet<u16>,
    ) {
        let mut new_entries = IntSet::<u16>::empty();
        for m in mapping.entry_map_records() {
            let mut mapped_codepoints = IntSet::<u32>::empty();
            for index in m.matched_entries {
                if !present_entries.contains(index) {
                    // The spec only allows entries produced by the glyph map to be referenced here.
                    // TODO(garretrieger): need to error out instead of ignoring.
                    continue;
                }
                let Some(entry) = entries.get(index as usize) else {
                    // TODO(garretrieger): need to error out instead of ignoring.
                    continue;
                };

                mapped_codepoints.union(&entry.codepoints);
            }

            if present_entries.contains(m.new_entry_index) {
                // TODO(garretrieger): update the spec to require new entry indices to by disjoint from the glyph map
                //                     entries.
                // TODO(garretrieger): need to error out instead of ignoring.
                continue;
            }

            let Some(new_entry) = entries.get_mut(m.new_entry_index as usize) else {
                // TODO(garretrieger): need to error out instead of ignoring.
                continue;
            };

            new_entry.codepoints.union(&mapped_codepoints);
            new_entry.feature_tags.insert(m.feature_tag);
            new_entries.insert(m.new_entry_index);
        }

        present_entries.union(&new_entries);
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
pub struct PatchUri {
    uri: String,
    encoding: PatchEncoding,
}

#[derive(Clone, Eq, PartialEq, Hash)]
pub struct Entry {
    pub patch_uri: PatchUri,
    pub codepoints: IntSet<u32>,
    pub feature_tags: BTreeSet<Tag>,
    pub compatibility_id: [u32; 4],
}

impl std::fmt::Debug for Entry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        let values: Vec<_> = self.codepoints.iter().collect();
        write!(f, "Entry({values:?} => {})", self.patch_uri.uri)
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
    // TODO(garretrieger): test with format 1 that has max entry = 0.

    #[test]
    fn format_1_patch_map_u8_entries() {
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
                // Entry 1 - contains gid 2 - is applied so not present.
                // Entry 2 - contains gid 1
                &Entry {
                    patch_uri: PatchUri {
                        uri: "ABCDEFɤ".to_string(),
                        encoding: PatchEncoding::GlyphKeyed,
                    },
                    codepoints: [0x101723].into_iter().collect(),
                    feature_tags: BTreeSet::new(),
                    compatibility_id: [1, 2, 3, 4],
                },
            ]
        );
    }

    #[test]
    fn format_1_patch_map_u16_entries() {
        let font_bytes = create_ift_font(
            FontRef::new(test_data::CMAP12_FONT1).unwrap(),
            Some(test_data::ift::U16_ENTRIES_FORMAT1),
            None,
        );
        let font = FontRef::new(&font_bytes).unwrap();

        let patch_map = PatchMap::new(&font);
        let entries: Vec<&Entry> = patch_map.iter().collect();

        assert_eq!(
            entries,
            vec![
                // Entry 0x50 - gid 2, 6
                &Entry {
                    patch_uri: PatchUri {
                        uri: "ABCDEFɤ".to_string(),
                        encoding: PatchEncoding::GlyphKeyed,
                    },
                    feature_tags: BTreeSet::new(),
                    codepoints: [0x101724, 0x102523].into_iter().collect(),
                    compatibility_id: [1, 2, 3, 4],
                },
                // Entry 0x51 - gid 3
                &Entry {
                    patch_uri: PatchUri {
                        uri: "ABCDEFɤ".to_string(),
                        encoding: PatchEncoding::GlyphKeyed,
                    },
                    feature_tags: BTreeSet::new(),
                    codepoints: [0x101725].into_iter().collect(),
                    compatibility_id: [1, 2, 3, 4],
                },
                // Entry 0x12c - gid 4, 5
                &Entry {
                    patch_uri: PatchUri {
                        uri: "ABCDEFɤ".to_string(),
                        encoding: PatchEncoding::GlyphKeyed,
                    },
                    feature_tags: BTreeSet::new(),
                    codepoints: [0x101726, 0x101727].into_iter().collect(),
                    compatibility_id: [1, 2, 3, 4],
                },
            ]
        );
    }

    #[test]
    fn format_1_patch_map_u16_entries_with_feature_mapping() {
        let font_bytes = create_ift_font(
            FontRef::new(test_data::CMAP12_FONT1).unwrap(),
            Some(test_data::ift::FEATURE_MAP_FORMAT1),
            None,
        );
        let font = FontRef::new(&font_bytes).unwrap();

        let patch_map = PatchMap::new(&font);
        let entries: Vec<&Entry> = patch_map.iter().collect();

        assert_eq!(
            entries,
            vec![
                // Entry 0x50 - gid 2, 6
                &Entry {
                    patch_uri: PatchUri {
                        uri: "ABCDEFɤ".to_string(),
                        encoding: PatchEncoding::GlyphKeyed,
                    },
                    feature_tags: BTreeSet::new(),
                    codepoints: [0x101724, 0x102523].into_iter().collect(),
                    compatibility_id: [1, 2, 3, 4],
                },
                // Entry 0x51 - gid 3
                &Entry {
                    patch_uri: PatchUri {
                        uri: "ABCDEFɤ".to_string(),
                        encoding: PatchEncoding::GlyphKeyed,
                    },
                    feature_tags: BTreeSet::new(),
                    codepoints: [0x101725].into_iter().collect(),
                    compatibility_id: [1, 2, 3, 4],
                },
                // Entry 0x70 (copy 0x50 U 0x51 + 'liga')
                &Entry {
                    patch_uri: PatchUri {
                        uri: "ABCDEFɤ".to_string(),
                        encoding: PatchEncoding::GlyphKeyed,
                    },
                    feature_tags: [Tag::new(&[b'l', b'i', b'g', b'a'])].into_iter().collect(),
                    codepoints: [0x101724, 0x101725, 0x102523].into_iter().collect(),
                    compatibility_id: [1, 2, 3, 4],
                },
                // Entry 0x71 (copy 0x12c + 'liga')
                &Entry {
                    patch_uri: PatchUri {
                        uri: "ABCDEFɤ".to_string(),
                        encoding: PatchEncoding::GlyphKeyed,
                    },
                    feature_tags: [Tag::new(&[b'l', b'i', b'g', b'a'])].into_iter().collect(),
                    codepoints: [0x101726, 0x101727].into_iter().collect(),
                    compatibility_id: [1, 2, 3, 4],
                },
                // Entry 0x12c - gid 4, 5
                &Entry {
                    patch_uri: PatchUri {
                        uri: "ABCDEFɤ".to_string(),
                        encoding: PatchEncoding::GlyphKeyed,
                    },
                    feature_tags: BTreeSet::new(),
                    codepoints: [0x101726, 0x101727].into_iter().collect(),
                    compatibility_id: [1, 2, 3, 4],
                },
                // Entry 0x190 (copy 0x51 + 'dlig')
                &Entry {
                    patch_uri: PatchUri {
                        uri: "ABCDEFɤ".to_string(),
                        encoding: PatchEncoding::GlyphKeyed,
                    },
                    feature_tags: [Tag::new(&[b'd', b'l', b'i', b'g'])].into_iter().collect(),
                    codepoints: [0x101725].into_iter().collect(),
                    compatibility_id: [1, 2, 3, 4],
                },
            ]
        );
    }
}
