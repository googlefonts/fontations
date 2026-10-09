//! Character mappings for a font.

use crate::{
    model::{font::SharedFont, Font},
    ps::encoding::PredefinedEncoding,
    ps::type1::Type1Font,
    tables::{
        cmap::{Cmap14, CmapIterLimits, CmapSubtable, MapVariant, PlatformId},
        name::MacRomanMapping,
    },
    TableProvider,
};
use alloc::vec::Vec;
use types::GlyphId;
use yoke::Yokeable;

/// Character mappings for a font.
///
/// Obtain this facade with [`Font::charmap`]. It offers two ways to map
/// characters, depending on what the caller knows about its input.
///
/// # Unicode lookup
///
/// [`map_unicode`](Self::map_unicode) accepts a Unicode codepoint (or `char`).
/// For an SFNT font, it uses one selected `cmap` subtable, with a preference
/// for a Microsoft symbol subtable, then full Unicode, then BMP Unicode, and
/// finally Mac Roman. A symbol subtable retries codes in U+0000..U+00FF at
/// U+F000..U+F0FF when the direct lookup fails. A Mac Roman subtable converts
/// Unicode codepoints to Mac Roman character codes before lookup. For Type 1 and standalone CFF
/// fonts, the Unicode map is derived from glyph names when the `agl` feature is
/// enabled. Glyph ID 0 is treated as unmapped.
/// [`map_unicode_batched`](Self::map_unicode_batched) writes several results
/// while reusing the selected subtable and, for formats 4 and 12, the preceding
/// segment or group when possible.
///
/// [`map_unicode_variant`](Self::map_unicode_variant) uses the first SFNT
/// `cmap` format 14 subtable. A non-default variation sequence names its glyph
/// directly; a default sequence resolves through the selected Unicode map.
/// Type 1 and standalone CFF fonts have no variation-sequence map.
///
/// The corresponding iterators are [`iter_unicodes`](Self::iter_unicodes) and
/// [`iter_unicode_variants`](Self::iter_unicode_variants).
///
/// # Individual encodings
///
/// [`encodings`](Self::encodings) lists each supported SFNT `cmap` encoding
/// record separately, including multiple records with the same
/// [`EncodingKind`]. It also lists native and glyph-name-derived Unicode
/// encodings for Type 1 and standalone CFF fonts when available. Keep the complete
/// [`Encoding`] value to select that specific mapping with
/// [`map_code`](Self::map_code) or [`iter_codes`](Self::iter_codes).
/// These methods use character codes in the selected encoding as stored in
/// the font. They do not convert Mac Roman codes from Unicode or apply the
/// Microsoft symbol fallback. A mapping to glyph ID 0 is treated as unmapped.
///
/// The selected Unicode subtable and the individual encoding records are
/// parsed and cached separately, only when their respective methods need them.
#[derive(Clone, Copy)]
pub struct Charmap<'a> {
    font: &'a SharedFont,
}

impl<'a> Charmap<'a> {
    pub(crate) fn new(font: &'a Font) -> Self {
        Self {
            font: font.shared(),
        }
    }

    /// Returns whether this font has a selected Unicode mapping.
    ///
    /// For Type 1 fonts, this requires a nonempty glyph-name-derived map.
    pub fn has_unicode(&self) -> bool {
        match self.font.kind() {
            crate::model::Kind::Type1(font) => font.unicode_charmap().iter().next().is_some(),
            crate::model::Kind::Cff(_, _) => self
                .font
                .bare_cff_unicode_charmap()
                .is_some_and(|map| map.iter().next().is_some()),
            _ => self
                .font
                .unicode_charmap()
                .is_some_and(|map| map.subtable.is_some()),
        }
    }

    /// Returns whether Unicode lookup uses a Microsoft symbol subtable.
    pub fn unicode_is_symbol(&self) -> bool {
        self.font.unicode_charmap().is_some_and(|map| map.is_symbol)
    }

    /// Returns whether Unicode lookup uses a Mac Roman subtable.
    pub fn unicode_is_mac_roman(&self) -> bool {
        self.font
            .unicode_charmap()
            .is_some_and(|map| map.is_mac_roman)
    }

    /// Maps a Unicode codepoint to a glyph.
    ///
    /// Type 1 and standalone CFF fonts use a glyph-name-based map when the
    /// `agl` feature is enabled.
    /// A mapping to glyph 0 is treated as unmapped.
    pub fn map_unicode(&self, codepoint: impl Into<u32>) -> Option<GlyphId> {
        let codepoint = codepoint.into();
        let glyph = match self.font.kind() {
            crate::model::Kind::Type1(font) => font.unicode_charmap().map(codepoint),
            crate::model::Kind::Cff(_, _) => self.font.bare_cff_unicode_charmap()?.map(codepoint),
            _ => self.font.unicode_charmap()?.map(codepoint),
        };
        glyph.filter(|glyph| *glyph != GlyphId::NOTDEF)
    }

    /// Writes the Unicode mapping of each codepoint to its slot, in order.
    ///
    /// Unmapped codepoints, including mappings to glyph 0, receive
    /// [`GlyphId::NOTDEF`]. Returns the number of initial consecutive nonzero
    /// mappings, while still writing every output. This uses
    /// the same selected mapping, Mac Roman conversion, and Microsoft symbol
    /// fallback as [`map_unicode`](Self::map_unicode).
    pub fn map_unicode_batched<'o>(
        &self,
        codepoints: impl Iterator<Item = (u32, &'o mut GlyphId)>,
    ) -> usize {
        match self.font.kind() {
            crate::model::Kind::Type1(font) => {
                map_ps_batched(Some(font.unicode_charmap()), codepoints)
            }
            crate::model::Kind::Cff(_, _) => {
                map_ps_batched(self.font.bare_cff_unicode_charmap(), codepoints)
            }
            _ => {
                if let Some(map) = self.font.unicode_charmap() {
                    map.map_batched(codepoints)
                } else {
                    for (_, out) in codepoints {
                        *out = GlyphId::NOTDEF;
                    }
                    0
                }
            }
        }
    }

    /// Iterates over Unicode codepoints and their resolved glyph IDs.
    ///
    /// Type 1 and standalone CFF mappings are derived from glyph names when
    /// `agl` is enabled.
    pub fn iter_unicodes(&self) -> impl Iterator<Item = (u32, GlyphId)> + '_ {
        let type1 = match self.font.kind() {
            crate::model::Kind::Type1(font) => Some(font),
            _ => None,
        };
        let type1 = type1.into_iter().flat_map(iter_type1_unicodes);
        let bare_cff = self
            .font
            .bare_cff_unicode_charmap()
            .into_iter()
            .flat_map(iter_ps_unicodes);
        let sfnt = self.font.unicode_charmap().into_iter().flat_map(|map| {
            map.iter(CmapIterLimits {
                max_char: char::MAX as u32,
                glyph_count: self.font.num_glyphs(),
            })
        });
        type1
            .chain(bare_cff)
            .chain(sfnt)
            .filter(|(_, glyph)| *glyph != GlyphId::NOTDEF)
    }

    /// Returns whether this font has a Unicode variation-sequence subtable.
    pub fn has_unicode_variants(&self) -> bool {
        self.font
            .unicode_charmap()
            .is_some_and(|map| map.vs_subtable.is_some())
    }

    /// Maps a Unicode codepoint and variation selector to a glyph.
    ///
    /// A default UVS entry resolves through [`map_unicode`](Self::map_unicode).
    /// A mapping to glyph 0 is treated as unmapped.
    pub fn map_unicode_variant(
        &self,
        codepoint: impl Into<u32>,
        selector: impl Into<u32>,
    ) -> Option<GlyphId> {
        self.font
            .unicode_charmap()?
            .map_variant(codepoint.into(), selector.into())
            .filter(|glyph| *glyph != GlyphId::NOTDEF)
    }

    /// Iterates over Unicode variation sequences and their resolved glyph IDs.
    ///
    /// Default variation entries use the selected Unicode mapping, just as
    /// [`map_unicode_variant`](Self::map_unicode_variant) does.
    ///
    /// Yields `(codepoint, selector, glyph)` tuples, where `glyph` is the
    /// resolved glyph ID.
    pub fn iter_unicode_variants(&self) -> impl Iterator<Item = (u32, u32, GlyphId)> + '_ {
        self.font.unicode_charmap().into_iter().flat_map(|map| {
            map.vs_subtable
                .as_ref()
                .into_iter()
                .flat_map(|subtable| {
                    subtable.iter_with_limits(CmapIterLimits {
                        max_char: char::MAX as u32,
                        glyph_count: self.font.num_glyphs(),
                    })
                })
                .filter_map(move |(codepoint, selector, variant)| {
                    let glyph = match variant {
                        MapVariant::UseDefault => map.map(codepoint)?,
                        MapVariant::Variant(glyph) => glyph,
                    };
                    (glyph != GlyphId::NOTDEF).then_some((codepoint, selector, glyph))
                })
        })
    }

    /// Returns the selectable character mappings of this font.
    ///
    /// More than one mapping can have the same kind. Keep the full
    /// [`Encoding`] value to select a mapping with [`map_code`](Self::map_code).
    pub fn encodings(&self) -> impl Iterator<Item = Encoding> + '_ {
        let sfnt = self
            .font
            .encodings()
            .into_iter()
            .flat_map(EncodingTables::iter);
        let type1 = match self.font.kind() {
            crate::model::Kind::Type1(font) => Some(font),
            _ => None,
        };
        let unicode = type1
            .filter(|font| font.unicode_charmap().iter().next().is_some())
            .map(|_| Encoding::type1_unicode());
        let native = type1
            .and_then(|font| font.encoding())
            .map(|encoding| Encoding::type1_native(encoding.predefined()));
        let bare_cff = match self.font.kind() {
            crate::model::Kind::Cff(font, _) => Some(font),
            _ => None,
        };
        let bare_cff_unicode = bare_cff
            .as_ref()
            .filter(|_| self.has_unicode())
            .map(|_| Encoding::bare_cff_unicode());
        let bare_cff_native = bare_cff
            .as_ref()
            .filter(|font| !font.is_cid())
            .and_then(|font| font.encoding())
            .map(|encoding| Encoding::bare_cff_native(encoding.predefined()));
        sfnt.chain(unicode)
            .chain(native)
            .chain(bare_cff_unicode)
            .chain(bare_cff_native)
    }

    /// Maps a character code through a mapping returned by [`encodings`](Self::encodings).
    ///
    /// The code is interpreted in the selected encoding, without Unicode
    /// conversion. The encoding must come from this font. A mapping to glyph 0
    /// is treated as unmapped.
    pub fn map_code(&self, encoding: Encoding, code: u32) -> Option<GlyphId> {
        let glyph = match self.font.kind() {
            crate::model::Kind::Type1(font) if encoding == Encoding::type1_unicode() => {
                font.unicode_charmap().map(code)
            }
            crate::model::Kind::Type1(font) => {
                let native = font.encoding()?;
                if encoding != Encoding::type1_native(native.predefined()) {
                    return None;
                }
                native.map(u8::try_from(code).ok()?)
            }
            crate::model::Kind::Cff(_, _) if encoding == Encoding::bare_cff_unicode() => {
                self.font.bare_cff_unicode_charmap()?.map(code)
            }
            crate::model::Kind::Cff(font, _) => {
                if font.is_cid() {
                    return None;
                }
                let native = font.encoding()?;
                if encoding != Encoding::bare_cff_native(native.predefined()) {
                    return None;
                }
                native.map(u8::try_from(code).ok()?)
            }
            _ => self.font.encodings()?.map(encoding, code),
        };
        glyph.filter(|glyph| *glyph != GlyphId::NOTDEF)
    }

    /// Iterates over character codes and glyph IDs in the selected encoding.
    ///
    /// The `encoding` must come from this font's [`encodings`](Self::encodings).
    /// Codes are returned as stored in the encoding; Mac Roman conversion and
    /// symbol fallback are only used by the Unicode methods. Mappings to glyph
    /// 0 are omitted.
    pub fn iter_codes(&self, encoding: Encoding) -> impl Iterator<Item = (u32, GlyphId)> + '_ {
        let type1 = match self.font.kind() {
            crate::model::Kind::Type1(font) => Some(font),
            _ => None,
        };
        let type1_unicode = type1
            .filter(|_| encoding == Encoding::type1_unicode())
            .into_iter()
            .flat_map(iter_type1_unicodes);
        let type1_native = type1
            .and_then(|font| font.encoding())
            .filter(|native| encoding == Encoding::type1_native(native.predefined()))
            .into_iter()
            .flat_map(|native| {
                (0_u32..=255)
                    .filter_map(move |code| native.map(code as u8).map(|glyph| (code, glyph)))
            });
        let bare_cff = match self.font.kind() {
            crate::model::Kind::Cff(font, _) => Some(font),
            _ => None,
        };
        let bare_cff_unicode = (encoding == Encoding::bare_cff_unicode())
            .then(|| self.font.bare_cff_unicode_charmap())
            .flatten()
            .into_iter()
            .flat_map(iter_ps_unicodes);
        let bare_cff_native = bare_cff
            .as_ref()
            .filter(|font| !font.is_cid())
            .and_then(|font| font.encoding())
            .filter(|native| encoding == Encoding::bare_cff_native(native.predefined()))
            .into_iter()
            .flat_map(|native| {
                (0_u32..=255)
                    .filter_map(move |code| native.map(code as u8).map(|glyph| (code, glyph)))
            });
        let sfnt = self.font.encodings().into_iter().flat_map(move |tables| {
            tables.iter_codes(
                encoding,
                CmapIterLimits {
                    max_char: u32::MAX,
                    glyph_count: self.font.num_glyphs(),
                },
            )
        });
        type1_unicode
            .chain(type1_native)
            .chain(bare_cff_unicode)
            .chain(bare_cff_native)
            .chain(sfnt)
            .filter(|(_, glyph)| *glyph != GlyphId::NOTDEF)
    }
}

fn map_ps_batched<'o>(
    map: Option<&crate::ps::charmap::Charmap>,
    codepoints: impl Iterator<Item = (u32, &'o mut GlyphId)>,
) -> usize {
    let mut mapped = 0;
    let mut initial = true;
    for (codepoint, out) in codepoints {
        *out = map
            .and_then(|map| map.map(codepoint))
            .unwrap_or(GlyphId::NOTDEF);
        initial &= *out != GlyphId::NOTDEF;
        mapped += usize::from(initial);
    }
    mapped
}

fn iter_type1_unicodes(font: &Type1Font) -> impl Iterator<Item = (u32, GlyphId)> + '_ {
    iter_ps_unicodes(font.unicode_charmap())
}

fn iter_ps_unicodes(
    map: &crate::ps::charmap::Charmap,
) -> impl Iterator<Item = (u32, GlyphId)> + '_ {
    map.iter()
        .scan(None, move |previous, (codepoint, _)| {
            let codepoint = codepoint & 0x7fff_ffff;
            let result = if *previous == Some(codepoint) {
                None
            } else {
                map.map(codepoint).map(|glyph| (codepoint, glyph))
            };
            *previous = Some(codepoint);
            Some(result)
        })
        .flatten()
}

/// The character encoding of a font mapping.
///
/// Values match FreeType's `FT_Encoding` tags. Multiple mappings can have
/// the same kind, so use the full [`Encoding`] to select one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum EncodingKind {
    /// An unclassified mapping.
    None = 0,
    /// Microsoft symbol encoding.
    MsSymbol = 0x7379_6d62,
    /// Unicode encoding.
    Unicode = 0x756e_6963,
    /// Shift JIS encoding.
    Sjis = 0x736a_6973,
    /// Simplified Chinese encoding.
    Prc = 0x6762_2020,
    /// Traditional Chinese encoding.
    Big5 = 0x6269_6735,
    /// Wansung encoding.
    Wansung = 0x7761_6e73,
    /// Johab encoding.
    Johab = 0x6a6f_6861,
    /// Adobe Standard encoding.
    AdobeStandard = 0x4144_4f42,
    /// Adobe Expert encoding.
    AdobeExpert = 0x4144_4245,
    /// Adobe Custom encoding.
    AdobeCustom = 0x4144_4243,
    /// Adobe Latin 1 encoding.
    AdobeLatin1 = 0x6c61_7431,
    /// Apple Roman encoding.
    AppleRoman = 0x6172_6d6e,
}

/// A selectable character mapping of a font.
///
/// This fixed-size value contains no borrowed data. It is a key for the font
/// that produced it; passing it to another font has no defined mapping.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(C)]
pub struct Encoding {
    kind: EncodingKind,
    index: u32,
    platform_id: u16,
    encoding_id: u16,
}

impl Encoding {
    const BARE_CFF_UNICODE: u32 = u32::MAX - 3;
    const BARE_CFF_NATIVE: u32 = u32::MAX - 2;
    const TYPE1_UNICODE: u32 = u32::MAX - 1;
    const TYPE1_NATIVE: u32 = u32::MAX;

    /// Returns the FreeType-compatible encoding kind.
    pub fn kind(self) -> EncodingKind {
        self.kind
    }

    /// Returns the SFNT cmap record index, if this mapping came from SFNT.
    pub fn sfnt_index(self) -> Option<u32> {
        (self.index < Self::BARE_CFF_UNICODE).then_some(self.index)
    }

    /// Returns the SFNT platform and encoding identifiers, if applicable.
    pub fn sfnt_ids(self) -> Option<(u16, u16)> {
        self.sfnt_index()
            .map(|_| (self.platform_id, self.encoding_id))
    }

    pub(crate) fn type1_unicode() -> Self {
        Self {
            kind: EncodingKind::Unicode,
            index: Self::TYPE1_UNICODE,
            platform_id: 0,
            encoding_id: 0,
        }
    }

    pub(crate) fn type1_native(predefined: Option<PredefinedEncoding>) -> Self {
        let kind = match predefined {
            Some(PredefinedEncoding::Standard) => EncodingKind::AdobeStandard,
            Some(PredefinedEncoding::Expert) => EncodingKind::AdobeExpert,
            Some(PredefinedEncoding::IsoLatin1) => EncodingKind::AdobeLatin1,
            None => EncodingKind::AdobeCustom,
        };
        Self {
            kind,
            index: Self::TYPE1_NATIVE,
            platform_id: 0,
            encoding_id: 0,
        }
    }

    pub(crate) fn bare_cff_unicode() -> Self {
        Self {
            index: Self::BARE_CFF_UNICODE,
            ..Self::type1_unicode()
        }
    }

    pub(crate) fn bare_cff_native(predefined: Option<PredefinedEncoding>) -> Self {
        Self {
            index: Self::BARE_CFF_NATIVE,
            ..Self::type1_native(predefined)
        }
    }
}

/// Parsed, selectable SFNT character mappings, indexed by cmap record.
#[derive(Clone, Default, Yokeable)]
pub(crate) struct EncodingTables<'a>(Vec<Option<EncodingTable<'a>>>);

#[derive(Clone, Yokeable)]
struct EncodingTable<'a> {
    encoding: Encoding,
    subtable: CmapSubtable<'a>,
}

impl<'a> EncodingTables<'a> {
    pub(crate) fn read(font: &impl TableProvider<'a>) -> Self {
        let Ok(cmap) = font.cmap() else {
            return Self::default();
        };
        let tables = cmap
            .encoding_records()
            .iter()
            .enumerate()
            .map(|(index, record)| {
                let subtable = record.subtable(cmap.offset_data()).ok()?;
                match subtable {
                    CmapSubtable::Format0(_)
                    | CmapSubtable::Format4(_)
                    | CmapSubtable::Format6(_)
                    | CmapSubtable::Format10(_)
                    | CmapSubtable::Format12(_)
                    | CmapSubtable::Format13(_) => {}
                    _ => return None,
                }
                let platform = record.platform_id();
                let id = record.encoding_id();
                let kind = match platform {
                    PlatformId::Unicode | PlatformId::ISO => EncodingKind::Unicode,
                    PlatformId::Macintosh if id == 0 => EncodingKind::AppleRoman,
                    PlatformId::Windows => match id {
                        0 => EncodingKind::MsSymbol,
                        1 | 10 => EncodingKind::Unicode,
                        2 => EncodingKind::Sjis,
                        3 => EncodingKind::Prc,
                        4 => EncodingKind::Big5,
                        5 => EncodingKind::Wansung,
                        6 => EncodingKind::Johab,
                        _ => EncodingKind::None,
                    },
                    _ => EncodingKind::None,
                };
                Some(EncodingTable {
                    encoding: Encoding {
                        kind,
                        index: index as u32,
                        platform_id: platform as u16,
                        encoding_id: id,
                    },
                    subtable,
                })
            })
            .collect();
        Self(tables)
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = Encoding> + '_ {
        self.0.iter().flatten().map(|table| table.encoding)
    }

    pub(crate) fn map(&self, encoding: Encoding, code: u32) -> Option<GlyphId> {
        let table = self.0.get(encoding.sfnt_index()? as usize)?.as_ref()?;
        if table.encoding != encoding {
            return None;
        }
        table.subtable.map_codepoint(code)
    }

    fn iter_codes(
        &self,
        encoding: Encoding,
        limits: CmapIterLimits,
    ) -> impl Iterator<Item = (u32, GlyphId)> + '_ {
        self.0
            .get(encoding.sfnt_index().unwrap_or(u32::MAX) as usize)
            .and_then(Option::as_ref)
            .filter(|table| table.encoding == encoding)
            .into_iter()
            .flat_map(move |table| {
                let subtable = &table.subtable;
                let format0 = matches!(subtable, CmapSubtable::Format0(_));
                subtable
                    .iter_with_limits(limits)
                    .chain((0_u32..=255).filter_map(move |code| {
                        format0
                            .then(|| subtable.map_codepoint(code))
                            .flatten()
                            .map(|glyph| (code, glyph))
                    }))
            })
    }
}

/// The selected cmap subtables and the metadata needed to use them.
#[derive(Clone, Default, Yokeable)]
pub(crate) struct UnicodeCharmap<'a> {
    subtable: Option<CmapSubtable<'a>>,
    vs_subtable: Option<Cmap14<'a>>,
    is_symbol: bool,
    is_mac_roman: bool,
}

impl<'a> UnicodeCharmap<'a> {
    pub(crate) fn read(font: &impl TableProvider<'a>) -> Self {
        let Ok(cmap) = font.cmap() else {
            return Self::default();
        };
        let (subtable, is_symbol, is_mac_roman) = cmap
            .best_subtable()
            .map(|(_, record, subtable)| {
                (Some(subtable), record.is_symbol(), record.is_mac_roman())
            })
            .unwrap_or((None, false, false));
        Self {
            subtable,
            vs_subtable: cmap.uvs_subtable().map(|(_, subtable)| subtable),
            is_symbol,
            is_mac_roman,
        }
    }

    pub(crate) fn map(&self, codepoint: u32) -> Option<GlyphId> {
        let subtable = self.subtable.as_ref()?;
        self.map_with(codepoint, |codepoint| subtable.map_codepoint(codepoint))
    }

    fn map_with(
        &self,
        mut codepoint: u32,
        mut lookup: impl FnMut(u32) -> Option<GlyphId>,
    ) -> Option<GlyphId> {
        if self.is_mac_roman && codepoint > 0x7f {
            codepoint = char::from_u32(codepoint)
                .and_then(|value| MacRomanMapping.encode(value))
                .map(u32::from)
                .unwrap_or(0);
        }
        let result = lookup(codepoint);
        if result.is_none_or(|glyph| glyph == GlyphId::NOTDEF)
            && self.is_symbol
            && codepoint <= 0xff
        {
            return lookup(0xf000 + codepoint);
        }
        result
    }

    fn map_batched<'o>(&self, codepoints: impl Iterator<Item = (u32, &'o mut GlyphId)>) -> usize {
        let mut mapped = 0;
        let mut initial = true;
        match self.subtable.as_ref() {
            Some(CmapSubtable::Format4(subtable)) => {
                let mut cached = None;
                for (codepoint, out) in codepoints {
                    *out = self
                        .map_with(codepoint, |codepoint| {
                            subtable.map_codepoint_cached(codepoint, &mut cached)
                        })
                        .unwrap_or(GlyphId::NOTDEF);
                    initial &= *out != GlyphId::NOTDEF;
                    mapped += usize::from(initial);
                }
            }
            Some(CmapSubtable::Format12(subtable)) => {
                let mut cached = None;
                for (codepoint, out) in codepoints {
                    *out = self
                        .map_with(codepoint, |codepoint| {
                            subtable.map_codepoint_cached(codepoint, &mut cached)
                        })
                        .unwrap_or(GlyphId::NOTDEF);
                    initial &= *out != GlyphId::NOTDEF;
                    mapped += usize::from(initial);
                }
            }
            Some(subtable) => {
                for (codepoint, out) in codepoints {
                    *out = self
                        .map_with(codepoint, |codepoint| subtable.map_codepoint(codepoint))
                        .unwrap_or(GlyphId::NOTDEF);
                    initial &= *out != GlyphId::NOTDEF;
                    mapped += usize::from(initial);
                }
            }
            None => {
                for (_, out) in codepoints {
                    *out = GlyphId::NOTDEF;
                }
            }
        }
        mapped
    }

    pub(crate) fn map_variant(&self, codepoint: u32, selector: u32) -> Option<GlyphId> {
        match self
            .vs_subtable
            .as_ref()?
            .map_variant(codepoint, selector)?
        {
            MapVariant::UseDefault => self.map(codepoint),
            MapVariant::Variant(glyph) => Some(glyph),
        }
    }

    fn iter(&self, limits: CmapIterLimits) -> impl Iterator<Item = (u32, GlyphId)> + '_ {
        let roman = self
            .is_mac_roman
            .then_some(self)
            .into_iter()
            .flat_map(|map| {
                (u8::MIN..=u8::MAX)
                    .map(|code| MacRomanMapping.decode(code) as u32)
                    .filter_map(move |codepoint| map.map(codepoint).map(|glyph| (codepoint, glyph)))
            });
        let normal = (!self.is_mac_roman)
            .then_some(self)
            .into_iter()
            .flat_map(move |map| {
                let subtable = map.subtable.as_ref();
                let direct = subtable
                    .into_iter()
                    .flat_map(move |subtable| subtable.iter_with_limits(limits));
                let format0 = (0_u32..=255).filter_map(move |codepoint| {
                    let subtable = subtable?;
                    matches!(subtable, CmapSubtable::Format0(_))
                        .then(|| subtable.map_codepoint(codepoint))
                        .flatten()
                        .map(|glyph| (codepoint, glyph))
                });
                let symbol = (0_u32..=255).filter_map(move |codepoint| {
                    let subtable = subtable?;
                    (map.is_symbol
                        && subtable
                            .map_codepoint(codepoint)
                            .is_none_or(|glyph| glyph == GlyphId::NOTDEF))
                    .then(|| subtable.map_codepoint(0xf000 + codepoint))
                    .flatten()
                    .map(|glyph| (codepoint, glyph))
                });
                direct.chain(format0).chain(symbol)
            });
        roman.chain(normal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        model::{Blob, Font},
        FontRef,
    };
    use alloc::{sync::Arc, vec};
    use core::sync::atomic::{AtomicUsize, Ordering};
    use types::Tag;

    fn check_unicode_batch(data: &'static [u8], codepoints: &[u32]) {
        let font = Font::new(data, 0).unwrap();
        let charmap = font.charmap();
        let expected: Vec<_> = codepoints
            .iter()
            .map(|&codepoint| charmap.map_unicode(codepoint).unwrap_or(GlyphId::NOTDEF))
            .collect();
        let mut actual = vec![GlyphId::new(123); codepoints.len()];
        let mapped = charmap.map_unicode_batched(codepoints.iter().copied().zip(actual.iter_mut()));
        assert_eq!(actual, expected);
        assert_eq!(
            mapped,
            expected
                .iter()
                .take_while(|&&glyph| glyph != GlyphId::NOTDEF)
                .count()
        );
    }

    #[test]
    fn batched_unicode_matches_scalar_across_cmap_formats() {
        let codepoints = [
            0, 0x20, 0x41, 0x42, 0xe9, 0x400, 0xf000, 0xffff, 0x10000, 0x10ffff, 0x110000, 0x41,
            0x20, 0,
        ];
        for data in [
            font_test_data::TINOS_SUBSET,
            font_test_data::CMAP12_FONT1,
            font_test_data::CMAP4_SYMBOL_PUA,
            font_test_data::CMAP6,
            font_test_data::NOTO_SERIF_DISPLAY_TRIMMED,
            font_test_data::NOTO_COLOR_EMOJI_FLAGS,
        ] {
            check_unicode_batch(data, &codepoints);
        }
        check_unicode_batch(
            font_test_data::type1::NOTO_SERIF_REGULAR_SUBSET_PFB,
            &codepoints,
        );

        let font = Font::new(font_test_data::TINOS_SUBSET, 0).unwrap();
        let mut from_chars = [GlyphId::NOTDEF; 3];
        let mapped = font.charmap().map_unicode_batched(
            ['A', 'B', 'C']
                .into_iter()
                .map(u32::from)
                .zip(from_chars.iter_mut()),
        );
        assert_eq!(
            from_chars,
            ['A', 'B', 'C'].map(|ch| font.charmap().map_unicode(ch).unwrap_or(GlyphId::NOTDEF))
        );
        assert_eq!(
            mapped,
            from_chars
                .iter()
                .take_while(|&&glyph| glyph != GlyphId::NOTDEF)
                .count()
        );
    }

    #[test]
    fn batched_unicode_handles_large_ordered_and_reversed_inputs() {
        let codepoints: Vec<_> = (0..=0x11000).step_by(7).collect();
        for data in [font_test_data::TINOS_SUBSET, font_test_data::CMAP12_FONT1] {
            check_unicode_batch(data, &codepoints);
            check_unicode_batch(data, &codepoints.iter().copied().rev().collect::<Vec<_>>());
        }
    }

    #[test]
    fn standalone_cff_batch_matches_scalar_and_writes_after_a_miss() {
        let sfnt = FontRef::new(font_test_data::NOTO_SERIF_DISPLAY_TRIMMED).unwrap();
        let cff = sfnt.cff().unwrap().offset_data().as_bytes();
        let codepoints = ['i' as u32, 0x10ffff, 'i' as u32];
        check_unicode_batch(cff, &codepoints);

        let font = Font::new(cff, 0).unwrap();
        let mut glyphs = [GlyphId::new(123); 3];
        let mapped = font
            .charmap()
            .map_unicode_batched(codepoints.into_iter().zip(glyphs.iter_mut()));
        #[cfg(feature = "agl")]
        {
            assert_eq!(mapped, 1);
            assert_eq!(glyphs, [GlyphId::new(1), GlyphId::NOTDEF, GlyphId::new(1)]);
        }
        #[cfg(not(feature = "agl"))]
        {
            assert_eq!(mapped, 0);
            assert_eq!(glyphs, [GlyphId::NOTDEF; 3]);
        }
    }

    #[test]
    fn cid_cff_has_no_glyph_name_unicode_mapping() {
        let sfnt = FontRef::new(font_test_data::NOTO_SANS_JP_CFF).unwrap();
        let cff = sfnt.cff().unwrap().offset_data().as_bytes();
        let font = Font::new(cff, 0).unwrap();
        let charmap = font.charmap();
        assert!(!charmap.has_unicode());
        assert_eq!(charmap.map_unicode('A'), None);
        assert_eq!(charmap.encodings().count(), 0);
        assert_eq!(charmap.map_code(Encoding::bare_cff_native(None), 65), None);
        let mut glyphs = [GlyphId::new(123); 2];
        assert_eq!(
            charmap.map_unicode_batched([0x41, 0x42].into_iter().zip(glyphs.iter_mut())),
            0
        );
        assert_eq!(glyphs, [GlyphId::NOTDEF; 2]);
    }

    #[test]
    fn maps_a_selected_unicode_subtable() {
        let font = Font::new(font_test_data::VAZIRMATN_VAR, 0).unwrap();
        assert!(font.charmap().has_unicode());
        assert!(!font.charmap().unicode_is_symbol());
        assert!(!font.charmap().unicode_is_mac_roman());
        assert_eq!(font.charmap().map_unicode('A'), Some(GlyphId::new(1)));
        assert_eq!(
            font.charmap().map_unicode('A' as u32),
            Some(GlyphId::new(1))
        );
        assert_eq!(font.charmap().map_unicode('B'), None);
        assert_eq!(font.charmap().map_unicode(0xffff_u32), None);
        assert!(font
            .charmap()
            .iter_unicodes()
            .any(|entry| entry == ('A' as u32, GlyphId::new(1))));
    }

    #[test]
    fn selectable_mappings_are_fixed_size_and_keep_record_identity() {
        assert_eq!(core::mem::size_of::<Encoding>(), 12);
        assert_eq!(EncodingKind::Unicode as u32, u32::from_be_bytes(*b"unic"));
        assert_eq!(
            EncodingKind::AppleRoman as u32,
            u32::from_be_bytes(*b"armn")
        );

        let data = font_test_data::VORG;
        let font = Font::new(data, 0).unwrap();
        let direct = FontRef::new(data).unwrap();
        let cmap = direct.cmap().unwrap();
        let encodings: Vec<_> = font.charmap().encodings().collect();
        for encoding in &encodings {
            let index = encoding.sfnt_index().unwrap() as usize;
            let record = &cmap.encoding_records()[index];
            assert_eq!(
                encoding.sfnt_ids(),
                Some((record.platform_id() as u16, record.encoding_id()))
            );
            let subtable = record.subtable(cmap.offset_data()).unwrap();
            assert_eq!(
                font.charmap().map_code(*encoding, 'A' as u32),
                subtable.map_codepoint('A')
            );
            for (code, glyph) in font.charmap().iter_codes(*encoding).take(3) {
                assert_eq!(font.charmap().map_code(*encoding, code), Some(glyph));
            }
        }
        assert!(
            encodings
                .iter()
                .filter(|encoding| encoding.kind() == EncodingKind::Unicode)
                .count()
                > 1
        );
        assert!(encodings.windows(2).all(|pair| pair[0] != pair[1]));
    }

    #[test]
    #[cfg(feature = "agl")]
    fn type1_exposes_unicode_and_native_mappings() {
        let font = Font::new(font_test_data::type1::NOTO_SERIF_REGULAR_SUBSET_PFB, 0).unwrap();
        let encodings: Vec<_> = font.charmap().encodings().collect();
        let unicode = encodings
            .iter()
            .find(|encoding| encoding.kind() == EncodingKind::Unicode)
            .unwrap();
        assert_eq!(unicode.sfnt_index(), None);
        assert_eq!(
            font.charmap().map_code(*unicode, 'H' as u32),
            font.charmap().map_unicode('H')
        );
        let native = encodings
            .iter()
            .find(|encoding| encoding.kind() != EncodingKind::Unicode)
            .unwrap();
        assert_eq!(native.sfnt_index(), None);
        assert_eq!(
            font.charmap().map_code(*native, b'H' as u32),
            Some(GlyphId::new(1))
        );
        assert_eq!(font.charmap().map_code(*native, 256), None);
        assert!(font
            .charmap()
            .iter_unicodes()
            .any(|entry| entry == ('H' as u32, GlyphId::new(1))));
        assert!(font
            .charmap()
            .iter_codes(*native)
            .any(|entry| entry == (b'H' as u32, GlyphId::new(1))));
    }

    #[test]
    fn selected_subtables_are_cached_across_font_instances() {
        let cmap_reads = Arc::new(AtomicUsize::new(0));
        let count = cmap_reads.clone();
        let source: Arc<dyn Fn(Tag) -> Option<Blob> + Send + Sync> = Arc::new(move |tag| {
            if tag == Tag::new(b"cmap") {
                count.fetch_add(1, Ordering::Relaxed);
            }
            let font = FontRef::new(font_test_data::VAZIRMATN_VAR).ok()?;
            font.table_data(tag)
                .map(|data| Blob::from(data.as_bytes().to_vec()))
        });
        let font = Font::new(source, 0).unwrap();
        let charmap = font.charmap();
        assert_eq!(cmap_reads.load(Ordering::Relaxed), 0);
        assert_eq!(charmap.map_unicode('A'), Some(GlyphId::new(1)));
        let first_reads = cmap_reads.load(Ordering::Relaxed);
        assert!(first_reads > 0);
        assert_eq!(font.charmap().map_unicode('A'), Some(GlyphId::new(1)));
        assert_eq!(
            font.default_instance().charmap().map_unicode('A'),
            Some(GlyphId::new(1))
        );
        let varied = font
            .instance_builder()
            .variations([("wght", 500.0)])
            .build();
        assert!(!varied.normalized_coords().is_empty());
        assert_eq!(varied.charmap().map_unicode('A'), Some(GlyphId::new(1)));
        assert_eq!(cmap_reads.load(Ordering::Relaxed), first_reads);
    }

    #[test]
    fn resolves_default_and_non_default_variants() {
        let font = Font::new(font_test_data::CMAP14_FONT1, 0).unwrap();
        assert!(font.charmap().has_unicode_variants());
        let selector = '\u{e0100}';
        assert_eq!(
            font.charmap().map_unicode_variant('\u{4e00}', selector),
            font.charmap().map_unicode('\u{4e00}')
        );
        assert_eq!(
            font.charmap().map_unicode_variant('\u{4e08}', selector),
            Some(GlyphId::new(25))
        );
        assert_eq!(
            font.charmap()
                .map_unicode_variant(0x4e08_u32, selector as u32),
            Some(GlyphId::new(25))
        );
        assert_eq!(font.charmap().map_unicode_variant('a', selector), None);
        let variants: Vec<_> = font.charmap().iter_unicode_variants().collect();
        assert!(variants.contains(&(
            0x4e00,
            selector as u32,
            font.charmap().map_unicode(0x4e00_u32).unwrap()
        )));
        assert!(variants.contains(&(0x4e08, selector as u32, GlyphId::new(25))));
    }

    #[test]
    fn symbol_encoding_falls_back_to_the_private_use_page() {
        let data = font_test_data::CMAP4_SYMBOL_PUA;
        let direct = FontRef::new(data).unwrap();
        let (_, _, subtable) = direct.cmap().unwrap().best_subtable().unwrap();
        let (codepoint, glyph) = (0xf000..=0xf0ff)
            .find_map(|codepoint| {
                subtable
                    .map_codepoint(codepoint)
                    .map(|glyph| (codepoint, glyph))
            })
            .unwrap();
        let character = char::from_u32(codepoint - 0xf000).unwrap();
        assert_eq!(subtable.map_codepoint(character), None);
        let font = Font::new(data, 0).unwrap();
        assert!(font.charmap().has_unicode());
        assert!(font.charmap().unicode_is_symbol());
        assert!(!font.charmap().unicode_is_mac_roman());
        assert_eq!(font.charmap().map_unicode(character), Some(glyph));
        assert!(font
            .charmap()
            .iter_unicodes()
            .any(|entry| entry == (character as u32, glyph)));
        let encoding = font.charmap().encodings().next().unwrap();
        assert_eq!(font.charmap().map_code(encoding, codepoint), Some(glyph));
        assert!(font
            .charmap()
            .iter_codes(encoding)
            .any(|entry| entry == (codepoint, glyph)));
    }

    #[test]
    fn symbol_encoding_retries_when_direct_mapping_is_notdef() {
        let mut cmap = vec![0, 0, 0, 1, 0, 3, 0, 0, 0, 0, 0, 12];
        // Format 12: U+0041 maps to .notdef, while U+F041 maps to glyph 3.
        cmap.extend_from_slice(&[0, 12, 0, 0]);
        cmap.extend_from_slice(&40_u32.to_be_bytes());
        cmap.extend_from_slice(&0_u32.to_be_bytes());
        cmap.extend_from_slice(&2_u32.to_be_bytes());
        for (codepoint, glyph) in [(0x41_u32, 0_u32), (0xf041, 3)] {
            cmap.extend_from_slice(&codepoint.to_be_bytes());
            cmap.extend_from_slice(&codepoint.to_be_bytes());
            cmap.extend_from_slice(&glyph.to_be_bytes());
        }
        let source: Arc<dyn Fn(Tag) -> Option<Blob> + Send + Sync> = Arc::new(move |tag| {
            if tag == Tag::new(b"cmap") {
                return Some(Blob::from(cmap.clone()));
            }
            let font = FontRef::new(font_test_data::TINOS_SUBSET).ok()?;
            font.table_data(tag)
                .map(|data| Blob::from(data.as_bytes().to_vec()))
        });
        let font = Font::new(source, 0).unwrap();
        let charmap = font.charmap();
        assert_eq!(charmap.map_unicode('A'), Some(GlyphId::new(3)));
        let mut batched = [GlyphId::NOTDEF; 3];
        let mapped = charmap.map_unicode_batched(
            ['A', 'B', 'A']
                .into_iter()
                .map(u32::from)
                .zip(batched.iter_mut()),
        );
        assert_eq!(batched, [GlyphId::new(3), GlyphId::NOTDEF, GlyphId::new(3)]);
        assert_eq!(mapped, 1);
        assert!(charmap
            .iter_unicodes()
            .any(|entry| entry == (0x41, GlyphId::new(3))));
        let encoding = charmap.encodings().next().unwrap();
        assert_eq!(charmap.map_code(encoding, 0x41), None);
        assert!(!charmap.iter_codes(encoding).any(|(code, _)| code == 0x41));
    }

    #[test]
    fn mac_roman_encoding_translates_unicode() {
        let mut cmap = vec![0, 0, 0, 1, 0, 1, 0, 0, 0, 0, 0, 12];
        cmap.extend_from_slice(&[0, 0, 1, 6, 0, 0]);
        let mut glyphs = [0u8; 256];
        glyphs[0x8e] = 3; // U+00E9, encoded as Mac Roman 0x8E.
        cmap.extend_from_slice(&glyphs);
        let source: Arc<dyn Fn(Tag) -> Option<Blob> + Send + Sync> = Arc::new(move |tag| {
            if tag == Tag::new(b"cmap") {
                return Some(Blob::from(cmap.clone()));
            }
            let font = FontRef::new(font_test_data::TINOS_SUBSET).ok()?;
            font.table_data(tag)
                .map(|data| Blob::from(data.as_bytes().to_vec()))
        });
        let font = Font::new(source, 0).unwrap();
        assert!(font.charmap().has_unicode());
        assert!(!font.charmap().unicode_is_symbol());
        assert!(font.charmap().unicode_is_mac_roman());
        assert_eq!(font.charmap().map_unicode('é'), Some(GlyphId::new(3)));
        assert_eq!(font.charmap().map_unicode('A'), None);
        assert!(font
            .charmap()
            .iter_unicodes()
            .any(|entry| entry == ('é' as u32, GlyphId::new(3))));
        let encoding = font.charmap().encodings().next().unwrap();
        assert_eq!(
            font.charmap().map_code(encoding, 0x8e),
            Some(GlyphId::new(3))
        );
        assert!(font
            .charmap()
            .iter_codes(encoding)
            .any(|entry| entry == (0x8e, GlyphId::new(3))));
    }

    #[test]
    #[cfg(feature = "agl")]
    fn type1_uses_its_unicode_charmap() {
        let font = Font::new(font_test_data::type1::NOTO_SERIF_REGULAR_SUBSET_PFB, 0).unwrap();
        assert!(font.charmap().has_unicode());
        assert!(!font.charmap().unicode_is_symbol());
        assert!(!font.charmap().unicode_is_mac_roman());
        assert!(!font.charmap().has_unicode_variants());
        assert_eq!(font.charmap().map_unicode('H'), Some(GlyphId::new(1)));
        assert_eq!(font.charmap().map_unicode('a'), None);
        assert_eq!(font.charmap().map_unicode_variant('H', '\u{fe0f}'), None);
    }
}
