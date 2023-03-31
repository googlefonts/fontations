//! Mapping characters to nominal glyph identifiers.
//!
//! The functionality in this module provides a 1-to-1 mapping from Unicode
//! characters (or [Unicode variation sequences](http://unicode.org/faq/vs.html)) to
//! nominal or "default" internal glyph identifiers for a given font.
//! This is a necessary first step, but generally insufficient for proper layout of
//! [complex text](https://en.wikipedia.org/wiki/Complex_text_layout) or even
//! simple text containing diacritics and ligatures.
//!
//! Comprehensive mapping of characters to positioned glyphs requires a process called
//! shaping. For more detail, see: [Why do I need a shaping engine?](https://harfbuzz.github.io/why-do-i-need-a-shaping-engine.html)

use read_fonts::{
    tables::cmap::{self, Cmap, Cmap12, Cmap14, Cmap4, CmapSubtable, EncodingRecord, PlatformId},
    types::{GlyphId, Uint24},
    FontData, TableProvider,
};

pub use read_fonts::tables::cmap::MapVariant;

/// Mapping of characters to nominal glyph identifiers.
///
/// The mappings are derived from the [cmap](https://learn.microsoft.com/en-us/typography/opentype/spec/cmap)
/// table.
///
/// ## Supported formats
/// * Unicode characters: formats [4](https://learn.microsoft.com/en-us/typography/opentype/spec/cmap#format-4-segment-mapping-to-delta-values)
/// and [12](https://learn.microsoft.com/en-us/typography/opentype/spec/cmap#format-12-segmented-coverage)
/// * Unicode variation sequences: format [14](https://learn.microsoft.com/en-us/typography/opentype/spec/cmap#format-14-unicode-variation-sequences)
#[derive(Clone, Default)]
pub struct Charmap<'a> {
    map: Option<Map<'a>>,
    variant_map: Option<Cmap14<'a>>,
}

impl<'a> Charmap<'a> {
    /// Creates a new character map from the given font.
    pub fn new(font: &impl TableProvider<'a>) -> Self {
        if let Ok(cmap) = font.cmap() {
            let (maps, map, variant_map) = find_best_mappings(&cmap);
            Self {
                map: map.map(|subtable| Map {
                    subtable,
                    is_symbol: maps.is_symbol,
                }),
                variant_map,
            }
        } else {
            Self::default()
        }
    }

    /// Returns true if a suitable Unicode character mapping is available.
    pub fn has_map(&self) -> bool {
        self.map.is_some()
    }

    /// Returns true if a symbol mapping was selected.
    pub fn is_symbol(&self) -> bool {
        self.map.as_ref().map(|x| x.is_symbol).unwrap_or(false)
    }

    /// Returns true if a Unicode variation sequence mapping is available.
    pub fn has_variant_map(&self) -> bool {
        self.variant_map.is_some()
    }

    /// Maps a character to a nominal glyph identifier. Returns `None` if a mapping does
    /// not exist.
    pub fn map(&self, ch: impl Into<u32>) -> Option<GlyphId> {
        self.map.as_ref()?.map(ch.into())
    }

    /// Maps a character and variation selector to a nominal glyph identifier.
    pub fn map_variant(&self, ch: impl Into<u32>, selector: impl Into<u32>) -> Option<MapVariant> {
        self.variant_map.as_ref()?.map_variant(ch, selector)
    }
}

/// Cacheable indices of selected mapping tables for materializing a character map.
///
/// Since [`Charmap`] carries a lifetime, it is difficult to store in a cache. This
/// type serves as an acceleration structure that allows for construction of
/// a character map while skipping the search for the most suitable Unicode
/// mappings.
#[derive(Copy, Clone, Default, Debug)]
pub struct MappingIndex {
    /// Index of Unicode or symbol mapping subtable.
    map: Option<u16>,
    /// True if the above is a symbol mapping.
    is_symbol: bool,
    /// Index of Unicode variation selector sutable.
    variant_map: Option<u16>,
}

impl MappingIndex {
    /// Finds the indices of the most suitable Unicode mapping tables in the
    /// given font.
    pub fn new<'a>(font: &impl TableProvider<'a>) -> Self {
        if let Ok(cmap) = font.cmap() {
            find_best_mappings(&cmap).0
        } else {
            Default::default()
        }
    }

    /// Creates a new character map for the given font using the tables referenced by
    /// the precomputed indices.
    ///
    /// The font should be the same as the one used to construct this object.
    pub fn charmap<'a>(&self, font: &impl TableProvider<'a>) -> Charmap<'a> {
        if let Ok(cmap) = font.cmap() {
            let records = cmap.encoding_records();
            let data = cmap.offset_data();
            Charmap {
                map: self
                    .map
                    .and_then(|index| get_subtable(data, records, index))
                    .and_then(UnicodeSubtable::new)
                    .map(|subtable| Map {
                        subtable,
                        is_symbol: self.is_symbol,
                    }),
                variant_map: self
                    .variant_map
                    .and_then(|index| get_subtable(data, records, index))
                    .and_then(|subtable| match subtable {
                        CmapSubtable::Format14(cmap14) => Some(cmap14),
                        _ => None,
                    }),
            }
        } else {
            Default::default()
        }
    }
}

fn get_subtable<'a>(
    data: FontData<'a>,
    records: &[EncodingRecord],
    index: u16,
) -> Option<CmapSubtable<'a>> {
    records
        .get(index as usize)
        .and_then(|record| record.subtable(data).ok())
}

#[derive(Clone)]
struct Map<'a> {
    subtable: UnicodeSubtable<'a>,
    is_symbol: bool,
}

impl<'a> Map<'a> {
    fn map(&self, codepoint: u32) -> Option<GlyphId> {
        self.map_impl(codepoint).or_else(|| {
            if self.is_symbol {
                self.map_impl(Self::adjust_symbol_pua(codepoint)?)
            } else {
                None
            }
        })
    }

    fn map_impl(&self, codepoint: u32) -> Option<GlyphId> {
        match &self.subtable {
            UnicodeSubtable::Format4(subtable) => subtable.map_codepoint(codepoint),
            UnicodeSubtable::Format12(subtable) => subtable.map_codepoint(codepoint),
        }
    }

    fn adjust_symbol_pua(codepoint: u32) -> Option<u32> {
        // From HarfBuzz:
        // For symbol-encoded OpenType fonts, we duplicate the
        // U+F000..F0FF range at U+0000..U+00FF.  That's what
        // Windows seems to do, and that's hinted about at:
        // https://docs.microsoft.com/en-us/typography/opentype/spec/recom
        // under "Non-Standard (Symbol) Fonts".
        // See <https://github.com/harfbuzz/harfbuzz/blob/453ded05392af38bba9f89587edce465e86ffa6b/src/hb-ot-cmap-table.hh#L1595>
        if codepoint <= 0x00FF {
            Some(codepoint + 0xF000)
        } else {
            None
        }
    }
}

#[derive(Clone, Debug)]
enum UnicodeSubtable<'a> {
    Format4(Cmap4<'a>),
    Format12(Cmap12<'a>),
}

impl<'a> UnicodeSubtable<'a> {
    fn new(subtable: CmapSubtable<'a>) -> Option<Self> {
        Some(match subtable {
            CmapSubtable::Format4(cmap4) => Self::Format4(cmap4),
            CmapSubtable::Format12(cmap12) => Self::Format12(cmap12),
            _ => return None,
        })
    }

    fn from_cmap_record(cmap: &Cmap<'a>, record: &cmap::EncodingRecord) -> Option<Self> {
        Self::new(record.subtable(cmap.offset_data()).ok()?)
    }
}

/// The mapping kind of a cmap subtable.
///
/// The ordering is significant and determines the priority of subtable
/// selection (greater is better).
#[derive(Copy, Clone, PartialEq, PartialOrd)]
enum MapKind {
    None = 0,
    UnicodeBmp = 1,
    UnicodeFull = 2,
    Symbol = 3,
}

/// Searches the `cmap` table for the best Unicode subtables.
///
/// Returns the `MappingIndex` accelerator along with character and UVS
/// subtables.
fn find_best_mappings<'a>(
    cmap: &Cmap<'a>,
) -> (
    MappingIndex,
    Option<UnicodeSubtable<'a>>,
    Option<Cmap14<'a>>,
) {
    const ENCODING_MS_SYMBOL: u16 = 0;
    const ENCODING_MS_UNICODE_CS: u16 = 1;
    const ENCODING_APPLE_ID_UNICODE_32: u16 = 4;
    const ENCODING_APPLE_ID_VARIANT_SELECTOR: u16 = 5;
    const ENCODING_MS_ID_UCS_4: u16 = 10;
    let mut maps = MappingIndex::default();
    let mut map_kind = MapKind::None;
    let mut map = None;
    let mut cmap14 = None;
    let mut maybe_choose_subtable = |kind, index, subtable| {
        if kind > map_kind {
            map_kind = kind;
            maps.is_symbol = kind == MapKind::Symbol;
            maps.map = Some(index as u16);
            map = Some(subtable);
        }
    };
    // This generally follows the same strategy as FreeType, searching the encoding
    // records in reverse and prioritizing UCS-4 subtables over UCS-2.
    // See <https://github.com/freetype/freetype/blob/4d8db130ea4342317581bab65fc96365ce806b77/src/base/ftobjs.c#L1370>
    // The exception is that we prefer a symbol subtable over all others which matches the behavior
    // of HarfBuzz.
    // See <https://github.com/harfbuzz/harfbuzz/blob/453ded05392af38bba9f89587edce465e86ffa6b/src/hb-ot-cmap-table.hh#L1818>
    for (i, record) in cmap.encoding_records().iter().enumerate().rev() {
        match (record.platform_id(), record.encoding_id()) {
            (PlatformId::Unicode, ENCODING_APPLE_ID_VARIANT_SELECTOR) => {
                // Unicode variation sequences
                if let Ok(CmapSubtable::Format14(subtable)) = record.subtable(cmap.offset_data()) {
                    if cmap14.is_none() {
                        maps.variant_map = Some(i as u16);
                        cmap14 = Some(subtable);
                    }
                }
            }
            (PlatformId::Windows, ENCODING_MS_SYMBOL) => {
                // Symbol
                if let Some(subtable) = UnicodeSubtable::from_cmap_record(cmap, record) {
                    maybe_choose_subtable(MapKind::Symbol, i, subtable);
                }
            }
            (PlatformId::Windows, ENCODING_MS_ID_UCS_4)
            | (PlatformId::Unicode, ENCODING_APPLE_ID_UNICODE_32) => {
                // Unicode full repertoire
                if let Some(subtable) = UnicodeSubtable::from_cmap_record(cmap, record) {
                    maybe_choose_subtable(MapKind::UnicodeFull, i, subtable);
                }
            }
            (PlatformId::ISO, _)
            | (PlatformId::Unicode, _)
            | (PlatformId::Windows, ENCODING_MS_UNICODE_CS) => {
                // Unicode BMP only
                if let Some(subtable) = UnicodeSubtable::from_cmap_record(cmap, record) {
                    maybe_choose_subtable(MapKind::UnicodeBmp, i, subtable);
                }
            }
            _ => {}
        }
    }
    (maps, map, cmap14)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MetadataProvider;
    use read_fonts::FontRef;

    #[test]
    fn choose_format_12_over_4() {
        let font = FontRef::new(read_fonts::test_data::test_fonts::CMAP12_FONT1).unwrap();
        let charmap = font.charmap();
        assert!(matches!(
            charmap.map.unwrap().subtable,
            UnicodeSubtable::Format12(..)
        ));
    }

    #[test]
    fn choose_format_4() {
        let font = FontRef::new(read_fonts::test_data::test_fonts::VAZIRMATN_VAR).unwrap();
        let charmap = font.charmap();
        assert!(matches!(
            charmap.map.unwrap().subtable,
            UnicodeSubtable::Format4(..)
        ));
    }

    #[test]
    fn map_format_4() {
        let font = FontRef::new(read_fonts::test_data::test_fonts::VAZIRMATN_VAR).unwrap();
        let charmap = font.charmap();
        assert_eq!(charmap.map('A'), Some(GlyphId::new(1)));
        assert_eq!(charmap.map('À'), Some(GlyphId::new(2)));
        assert_eq!(charmap.map('`'), Some(GlyphId::new(3)));
        assert_eq!(charmap.map('B'), None);
    }

    #[test]
    fn map_format_12() {
        let font = FontRef::new(read_fonts::test_data::test_fonts::CMAP12_FONT1).unwrap();
        let charmap = font.charmap();
        assert_eq!(charmap.map(' '), None);
        assert_eq!(charmap.map(0x101723_u32), Some(GlyphId::new(23)));
        assert_eq!(charmap.map(0x101725_u32), Some(GlyphId::new(25)));
        assert_eq!(charmap.map(0x102523_u32), Some(GlyphId::new(53)));
        assert_eq!(charmap.map(0x102526_u32), Some(GlyphId::new(56)));
        assert_eq!(charmap.map(0x102527_u32), Some(GlyphId::new(57)));
    }

    #[test]
    fn map_symbol_pua() {
        let font = FontRef::new(read_fonts::test_data::test_fonts::CMAP4_SYMBOL_PUA).unwrap();
        let charmap = font.charmap();
        assert!(charmap.map.as_ref().unwrap().is_symbol);
        assert_eq!(charmap.map(0xF001_u32), Some(GlyphId::new(1)));
        assert_eq!(charmap.map(0xF002_u32), Some(GlyphId::new(2)));
        assert_eq!(charmap.map(0xF003_u32), Some(GlyphId::new(3)));
        assert_eq!(charmap.map(0xF0FE_u32), Some(GlyphId::new(4)));
        // The following are remapped into the U+F000..F0FF range for a symbol font
        assert_eq!(charmap.map(0x1_u32), Some(GlyphId::new(1)));
        assert_eq!(charmap.map(0x2_u32), Some(GlyphId::new(2)));
        assert_eq!(charmap.map(0x3_u32), Some(GlyphId::new(3)));
        assert_eq!(charmap.map(0xFE_u32), Some(GlyphId::new(4)));
    }

    #[test]
    fn map_variants() {
        use super::{CmapSubtable, MapVariant::*};
        let font = FontRef::new(read_fonts::test_data::test_fonts::CMAP14_FONT1).unwrap();
        let charmap = font.charmap();
        let selector = '\u{e0100}';
        assert_eq!(charmap.map_variant('a', selector), None);
        assert_eq!(charmap.map_variant('\u{4e00}', selector), Some(UseDefault));
        assert_eq!(charmap.map_variant('\u{4e06}', selector), Some(UseDefault));
        assert_eq!(
            charmap.map_variant('\u{4e08}', selector),
            Some(Variant(GlyphId::new(25)))
        );
        assert_eq!(
            charmap.map_variant('\u{4e09}', selector),
            Some(Variant(GlyphId::new(26)))
        );
    }
}
