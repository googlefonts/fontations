//! The [cmap](https://docs.microsoft.com/en-us/typography/opentype/spec/cmap) table

include!("../../generated/generated_cmap.rs");

/// Result of the mapping a codepoint with a variation selector.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum MapVariant {
    /// The variation selector should be ignored and the default mapping
    /// of the character should be used.
    UseDefault,
    /// The variant glyph mapped by a codepoint and associated variation
    /// selector.
    Variant(GlyphId),
}

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

impl<'a> Cmap14<'a> {
    /// Maps a codepoint and variation selector to a nominal glyph identifier.
    pub fn map_variant(
        &self,
        codepoint: impl Into<u32>,
        selector: impl Into<u32>,
    ) -> Option<MapVariant> {
        let codepoint = codepoint.into();
        let selector = selector.into();
        let selector_records = self.var_selector();
        // Variation selector records are sorted in order of var_selector. Binary search to find
        // the appropriate record.
        let selector_record = match selector_records.binary_search_by(|rec| {
            let rec_selector: u32 = rec.var_selector().into();
            rec_selector.cmp(&selector)
        }) {
            Ok(idx) => selector_records.get(idx)?,
            _ => return None,
        };
        // If a default UVS table is present in this selector record, binary search on the ranges
        // (start_unicode_value, start_unicode_value + additional_count) to find the requested codepoint.
        // If found, ignore the selector and return a value indicating that the default cmap mapping
        // should be used.
        if let Some(Ok(default_uvs)) = selector_record.default_uvs(self.offset_data()) {
            use core::cmp::Ordering;
            let found_default_uvs = default_uvs
                .ranges()
                .binary_search_by(|range| {
                    let start = range.start_unicode_value().into();
                    if codepoint < start {
                        Ordering::Greater
                    } else if codepoint > (start + range.additional_count() as u32) {
                        Ordering::Less
                    } else {
                        Ordering::Equal
                    }
                })
                .is_ok();
            if found_default_uvs {
                return Some(MapVariant::UseDefault);
            }
        }
        // Binary search the non-default UVS table if present. This maps codepoint+selector to a variant glyph.
        let non_default_uvs = selector_record.non_default_uvs(self.offset_data())?.ok()?;
        let mapping = non_default_uvs.uvs_mapping();
        let ix = mapping
            .binary_search_by(|map| {
                let map_codepoint: u32 = map.unicode_value().into();
                map_codepoint.cmp(&codepoint)
            })
            .ok()?;
        Some(MapVariant::Variant(GlyphId::new(
            mapping.get(ix)?.glyph_id(),
        )))
    }
}

#[cfg(test)]
mod tests {
    use crate::{FontRef, GlyphId, TableProvider};

    #[test]
    fn map_codepoints() {
        let font = FontRef::new(font_test_data::VAZIRMATN_VAR).unwrap();
        let cmap = font.cmap().unwrap();
        assert_eq!(cmap.map_codepoint('A'), Some(GlyphId::new(1)));
        assert_eq!(cmap.map_codepoint('À'), Some(GlyphId::new(2)));
        assert_eq!(cmap.map_codepoint('`'), Some(GlyphId::new(3)));
        assert_eq!(cmap.map_codepoint('B'), None);

        let font = FontRef::new(font_test_data::SIMPLE_GLYF).unwrap();
        let cmap = font.cmap().unwrap();
        assert_eq!(cmap.map_codepoint(' '), Some(GlyphId::new(1)));
        assert_eq!(cmap.map_codepoint(0xE_u32), Some(GlyphId::new(2)));
        assert_eq!(cmap.map_codepoint('B'), None);
    }

    #[test]
    fn map_variants() {
        use super::{CmapSubtable, MapVariant::*};
        let font = FontRef::new(font_test_data::CMAP14_FONT1).unwrap();
        let cmap = font.cmap().unwrap();
        let cmap14 = cmap
            .encoding_records()
            .iter()
            .filter_map(|record| record.subtable(cmap.offset_data()).ok())
            .find_map(|subtable| match subtable {
                CmapSubtable::Format14(cmap14) => Some(cmap14),
                _ => None,
            })
            .unwrap();
        let selector = '\u{e0100}';
        assert_eq!(cmap14.map_variant('a', selector), None);
        assert_eq!(cmap14.map_variant('\u{4e00}', selector), Some(UseDefault));
        assert_eq!(cmap14.map_variant('\u{4e06}', selector), Some(UseDefault));
        assert_eq!(
            cmap14.map_variant('\u{4e08}', selector),
            Some(Variant(GlyphId::new(25)))
        );
        assert_eq!(
            cmap14.map_variant('\u{4e09}', selector),
            Some(Variant(GlyphId::new(26)))
        );
    }
}
