//! Character mappings for a font.

use crate::{
    model::Font,
    ps::encoding::PredefinedEncoding,
    ps::type1::Type1Font,
    tables::{
        cmap::{
            Cmap, CmapIterLimits, CmapSubtable, EncodingRecord, MapVariant, PlatformId,
            VariationSubtable,
        },
        name::MacRomanMapping,
    },
    FontData, TableProvider,
};
use alloc::vec::Vec;
use types::{GlyphId, Tag};
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
/// Unicode codepoints to Mac Roman character codes before lookup. For a Type 1
/// font, the Unicode map is derived from glyph names when the `agl` feature is
/// enabled. Glyph ID 0 is treated as unmapped.
/// An optional `DMAP` table takes precedence, with missing mappings falling
/// back to `cmap` (ISO/IEC 14496-22:2026, 5.6.15).
///
/// [`map_unicode_variant`](Self::map_unicode_variant) uses the first SFNT
/// `cmap` format 14 subtable. A non-default variation sequence names its glyph
/// directly; a default sequence resolves through the selected Unicode map.
/// Type 1 fonts have no variation-sequence map.
///
/// The corresponding iterators are [`iter_unicodes`](Self::iter_unicodes) and
/// [`iter_unicode_variants`](Self::iter_unicode_variants).
///
/// # Individual encodings
///
/// [`encodings`](Self::encodings) lists each supported SFNT `cmap` encoding
/// record separately, including multiple records with the same
/// [`EncodingKind`]. It also lists a Type 1 font's native encoding and its
/// glyph-name-derived Unicode encoding when available. Keep the complete
/// [`Encoding`] value to select that specific mapping with
/// [`map_code`](Self::map_code) or [`iter_codes`](Self::iter_codes).
/// These methods use character codes in the selected encoding as stored in
/// the font. They do not convert Mac Roman codes from Unicode or apply the
/// Microsoft symbol fallback. A mapping to glyph ID 0 is treated as unmapped.
/// DMAP overrides are applied to matching encodings, and encodings supplied
/// only by DMAP are also available.
///
/// The selected Unicode subtable and the individual encoding records are
/// parsed and cached separately, only when their respective methods need them.
#[derive(Clone, Copy)]
pub struct Charmap<'a> {
    font: &'a Font,
}

impl<'a> Charmap<'a> {
    pub(crate) fn new(font: &'a Font) -> Self {
        Self { font }
    }

    /// Returns whether this font has a selected Unicode mapping.
    ///
    /// For Type 1 fonts, this requires a nonempty glyph-name-derived map.
    pub fn has_unicode(&self) -> bool {
        match self.font.kind() {
            crate::model::Kind::Type1(font) => font.unicode_charmap().iter().next().is_some(),
            _ => self
                .font
                .unicode_charmap()
                .is_some_and(|map| map.dmap.subtable.is_some() || map.cmap.subtable.is_some()),
        }
    }

    /// Returns whether Unicode lookup uses a Microsoft symbol subtable.
    pub fn unicode_is_symbol(&self) -> bool {
        self.font
            .unicode_charmap()
            .is_some_and(|map| map.dmap.is_symbol || map.cmap.is_symbol)
    }

    /// Returns whether Unicode lookup uses a Mac Roman subtable.
    pub fn unicode_is_mac_roman(&self) -> bool {
        self.font
            .unicode_charmap()
            .is_some_and(|map| map.dmap.is_mac_roman || map.cmap.is_mac_roman)
    }

    /// Maps a Unicode codepoint to a glyph.
    ///
    /// Type 1 fonts use a glyph-name-based map when the `agl` feature is enabled.
    /// A mapping to glyph 0 is treated as unmapped.
    pub fn map_unicode(&self, codepoint: impl Into<u32>) -> Option<GlyphId> {
        let codepoint = codepoint.into();
        let glyph = match self.font.kind() {
            crate::model::Kind::Type1(font) => font.unicode_charmap().map(codepoint),
            _ => self.font.unicode_charmap()?.map(codepoint),
        };
        glyph.filter(|glyph| *glyph != GlyphId::NOTDEF)
    }

    /// Iterates over Unicode codepoints and their resolved glyph IDs.
    ///
    /// Type 1 mappings are derived from glyph names when `agl` is enabled.
    pub fn iter_unicodes(&self) -> impl Iterator<Item = (u32, GlyphId)> + '_ {
        let type1 = match self.font.kind() {
            crate::model::Kind::Type1(font) => Some(font),
            _ => None,
        };
        let type1 = type1.into_iter().flat_map(iter_type1_unicodes);
        let sfnt = self.font.unicode_charmap().into_iter().flat_map(|map| {
            map.iter(CmapIterLimits {
                max_char: char::MAX as u32,
                glyph_count: self.font.num_glyphs(),
            })
        });
        type1
            .chain(sfnt)
            .filter(|(_, glyph)| *glyph != GlyphId::NOTDEF)
    }

    /// Returns whether this font has a Unicode variation-sequence subtable.
    pub fn has_unicode_variants(&self) -> bool {
        self.font
            .unicode_charmap()
            .is_some_and(|map| map.dmap.vs_subtable.is_some() || map.cmap.vs_subtable.is_some())
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
            map.variant_mappings(CmapIterLimits {
                max_char: char::MAX as u32,
                glyph_count: self.font.num_glyphs(),
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
            .encoding_tables()
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
        sfnt.chain(unicode).chain(native)
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
            _ => self.font.encoding_tables()?.map(encoding, code),
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
        let sfnt = self
            .font
            .encoding_tables()
            .into_iter()
            .flat_map(move |tables| {
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
            .chain(sfnt)
            .filter(|(_, glyph)| *glyph != GlyphId::NOTDEF)
    }
}

fn iter_type1_unicodes(font: &Type1Font) -> impl Iterator<Item = (u32, GlyphId)> + '_ {
    let map = font.unicode_charmap();
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
    const DMAP: u32 = 1 << 31;
    const TYPE1_UNICODE: u32 = u32::MAX - 1;
    const TYPE1_NATIVE: u32 = u32::MAX;

    /// Returns the FreeType-compatible encoding kind.
    pub fn kind(self) -> EncodingKind {
        self.kind
    }

    /// Returns the encoding record index within [`sfnt_tag`](Self::sfnt_tag).
    pub fn sfnt_index(self) -> Option<u32> {
        (self.index < Self::TYPE1_UNICODE).then_some(self.index & !Self::DMAP)
    }

    /// Returns the SFNT mapping table tag (`cmap` or `DMAP`), if applicable.
    pub fn sfnt_tag(self) -> Option<Tag> {
        self.sfnt_index().map(|_| {
            if self.index & Self::DMAP != 0 {
                Tag::new(b"DMAP")
            } else {
                Tag::new(b"cmap")
            }
        })
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
}

/// Parsed, selectable SFNT character mappings, indexed by table and encoding record.
#[derive(Clone, Default, Yokeable)]
pub(crate) struct EncodingTables<'a> {
    cmap: Vec<Option<EncodingTable<'a>>>,
    dmap: Vec<Option<EncodingTable<'a>>>,
}

#[derive(Clone, Yokeable)]
struct EncodingTable<'a> {
    encoding: Encoding,
    subtable: CmapSubtable<'a>,
    delta: Option<CmapSubtable<'a>>,
}

impl<'a> EncodingTables<'a> {
    pub(crate) fn read(font: &impl TableProvider<'a>) -> Self {
        let read = |cmap: Option<Cmap<'a>>, source| {
            cmap.into_iter()
                .flat_map(|cmap| {
                    cmap.encoding_records()
                        .iter()
                        .enumerate()
                        .map(move |(index, record)| {
                            EncodingTable::read(index as u32 | source, record, cmap.offset_data())
                        })
                })
                .collect::<Vec<_>>()
        };
        let dmap = font.dmap().ok().map(|table| table.as_cmap());
        let mut tables = Self {
            cmap: read(font.cmap().ok(), 0),
            dmap: read(dmap.clone(), Encoding::DMAP),
        };
        let unicode_delta = dmap
            .and_then(|table| table.best_subtable())
            .and_then(|(index, _, _)| tables.dmap.get(index as usize)?.as_ref())
            .filter(|table| table.encoding.kind == EncodingKind::Unicode);
        for base in tables.cmap.iter_mut().flatten() {
            let matches = |delta: &&EncodingTable<'_>| {
                delta.encoding.kind == base.encoding.kind
                    && (base.encoding.kind != EncodingKind::None
                        || delta.encoding.sfnt_ids() == base.encoding.sfnt_ids())
            };
            let delta = (base.encoding.kind == EncodingKind::Unicode)
                .then_some(unicode_delta)
                .flatten()
                .or_else(|| {
                    tables
                        .dmap
                        .iter()
                        .flatten()
                        .find(|delta| delta.encoding.sfnt_ids() == base.encoding.sfnt_ids())
                })
                .or_else(|| tables.dmap.iter().flatten().find(matches));
            base.delta = delta.map(|table| table.subtable.clone());
        }
        tables
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = Encoding> + '_ {
        self.cmap
            .iter()
            .flatten()
            .chain(self.dmap.iter().flatten().filter(|delta| {
                !self.cmap.iter().flatten().any(|base| {
                    base.encoding.kind == delta.encoding.kind
                        && (base.encoding.kind != EncodingKind::None
                            || base.encoding.sfnt_ids() == delta.encoding.sfnt_ids())
                })
            }))
            .map(|table| table.encoding)
    }

    fn table(&self, encoding: Encoding) -> Option<&EncodingTable<'a>> {
        let tables = if encoding.index & Encoding::DMAP != 0 {
            &self.dmap
        } else {
            &self.cmap
        };
        tables
            .get(encoding.sfnt_index()? as usize)?
            .as_ref()
            .filter(|table| table.encoding == encoding)
    }

    pub(crate) fn map(&self, encoding: Encoding, code: u32) -> Option<GlyphId> {
        let table = self.table(encoding)?;
        table
            .delta
            .as_ref()
            .and_then(|delta| delta.map_codepoint(code))
            .filter(|glyph| *glyph != GlyphId::NOTDEF)
            .or_else(|| table.subtable.map_codepoint(code))
    }

    fn iter_codes(
        &self,
        encoding: Encoding,
        limits: CmapIterLimits,
    ) -> impl Iterator<Item = (u32, GlyphId)> + '_ {
        self.table(encoding).into_iter().flat_map(move |table| {
            table
                .delta
                .as_ref()
                .into_iter()
                .flat_map(move |delta| iter_subtable(delta, limits))
                .filter(|(_, glyph)| *glyph != GlyphId::NOTDEF)
                .chain(iter_subtable(&table.subtable, limits).filter(|(code, _)| {
                    table
                        .delta
                        .as_ref()
                        .and_then(|delta| delta.map_codepoint(*code))
                        .is_none_or(|glyph| glyph == GlyphId::NOTDEF)
                }))
        })
    }
}

fn iter_subtable<'a>(
    subtable: &'a CmapSubtable<'a>,
    limits: CmapIterLimits,
) -> impl Iterator<Item = (u32, GlyphId)> + 'a {
    subtable
        .iter_with_limits(limits)
        .chain((0_u32..=255).filter_map(move |code| {
            matches!(subtable, CmapSubtable::Format0(_))
                .then(|| subtable.map_codepoint(code))
                .flatten()
                .map(|glyph| (code, glyph))
        }))
}

impl<'a> EncodingTable<'a> {
    fn read(index: u32, record: &EncodingRecord, data: FontData<'a>) -> Option<Self> {
        let subtable = record.subtable(data).ok()?;
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
                index,
                platform_id: platform as u16,
                encoding_id: id,
            },
            subtable,
            delta: None,
        })
    }
}

/// The selected cmap subtables and the metadata needed to use them.
#[derive(Clone, Default, Yokeable)]
pub(crate) struct UnicodeCharmap<'a> {
    cmap: UnicodeSubtables<'a>,
    dmap: UnicodeSubtables<'a>,
}

#[derive(Clone, Default, Yokeable)]
struct UnicodeSubtables<'a> {
    subtable: Option<CmapSubtable<'a>>,
    vs_subtable: Option<VariationSubtable<'a>>,
    is_symbol: bool,
    is_mac_roman: bool,
}

impl<'a> UnicodeCharmap<'a> {
    pub(crate) fn read(font: &impl TableProvider<'a>) -> Self {
        Self {
            cmap: UnicodeSubtables::read(font.cmap().ok()),
            dmap: UnicodeSubtables::read(font.dmap().ok().map(|table| table.as_cmap())),
        }
    }

    pub(crate) fn map(&self, codepoint: u32) -> Option<GlyphId> {
        self.dmap
            .map(codepoint)
            .filter(|glyph| *glyph != GlyphId::NOTDEF)
            .or_else(|| self.cmap.map(codepoint))
    }

    pub(crate) fn map_variant(&self, codepoint: u32, selector: u32) -> Option<GlyphId> {
        let variant = self
            .dmap
            .vs_subtable
            .as_ref()
            .and_then(|table| table.map_variant(codepoint, selector))
            .or_else(|| {
                self.cmap
                    .vs_subtable
                    .as_ref()?
                    .map_variant(codepoint, selector)
            })?;
        match variant {
            MapVariant::UseDefault => self.map(codepoint),
            MapVariant::Variant(glyph) => Some(glyph),
        }
    }

    fn iter(&self, limits: CmapIterLimits) -> impl Iterator<Item = (u32, GlyphId)> + '_ {
        self.dmap
            .iter(limits)
            .filter(|(_, glyph)| *glyph != GlyphId::NOTDEF)
            .chain(self.cmap.iter(limits).filter(|(codepoint, _)| {
                self.dmap
                    .map(*codepoint)
                    .is_none_or(|glyph| glyph == GlyphId::NOTDEF)
            }))
    }

    fn variant_mappings(
        &self,
        limits: CmapIterLimits,
    ) -> impl Iterator<Item = (u32, u32, MapVariant)> + '_ {
        self.dmap
            .vs_subtable
            .as_ref()
            .into_iter()
            .flat_map(move |table| table.iter_with_limits(limits))
            .chain(
                self.cmap
                    .vs_subtable
                    .as_ref()
                    .into_iter()
                    .flat_map(move |table| table.iter_with_limits(limits))
                    .filter(|(codepoint, selector, _)| {
                        self.dmap
                            .vs_subtable
                            .as_ref()
                            .and_then(|table| table.map_variant(*codepoint, *selector))
                            .is_none()
                    }),
            )
    }
}

impl<'a> UnicodeSubtables<'a> {
    fn read(cmap: Option<Cmap<'a>>) -> Self {
        let Some(cmap) = cmap else {
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
            vs_subtable: cmap.variation_subtable().map(|(_, subtable)| subtable),
            is_symbol,
            is_mac_roman,
        }
    }

    pub(crate) fn map(&self, mut codepoint: u32) -> Option<GlyphId> {
        let subtable = self.subtable.as_ref()?;
        if self.is_mac_roman && codepoint > 0x7f {
            codepoint = char::from_u32(codepoint)
                .and_then(|value| MacRomanMapping.encode(value))
                .map(u32::from)
                .unwrap_or(0);
        }
        let result = subtable.map_codepoint(codepoint);
        if result.is_none_or(|glyph| glyph == GlyphId::NOTDEF)
            && self.is_symbol
            && codepoint <= 0xff
        {
            return subtable.map_codepoint(0xf000 + codepoint);
        }
        result
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

    #[test]
    fn cmap15_selection_and_dmap_variants() {
        use font_test_data::cmap::{font_with_cmaps, format12, format14, format15, table};
        let nominal = format12(&[(65, 1)]);
        let old = format14(0xfe0f, &[], &[(65, 2), (67, 3)]);
        let new = format15(0xfe0f, &[65], &[(66, 70000), (68, 0xffffff)]);
        for records in [
            vec![
                (3, 10, nominal.as_slice()),
                (0, 5, old.as_slice()),
                (0, 5, new.as_slice()),
            ],
            vec![
                (3, 10, nominal.as_slice()),
                (0, 5, new.as_slice()),
                (0, 5, old.as_slice()),
            ],
        ] {
            let base = table(&records);
            let delta = table(&[(0, 5, old.as_slice())]);
            for (dmap, expected) in [
                (None, vec![(65, 1), (66, 70000), (68, 0xffffff)]),
                (
                    Some(delta.as_slice()),
                    vec![(65, 2), (66, 70000), (67, 3), (68, 0xffffff)],
                ),
            ] {
                let data = font_with_cmaps(Some(&base), dmap);
                let font = Font::new(data, 0).unwrap();
                let charmap = font.charmap();
                assert!(charmap.has_unicode_variants());
                let mut mappings: Vec<_> = charmap.iter_unicode_variants().collect();
                mappings.sort_unstable();
                assert_eq!(
                    mappings,
                    expected
                        .iter()
                        .map(|&(ch, glyph)| (ch, 0xfe0f, GlyphId::new(glyph)))
                        .collect::<Vec<_>>()
                );
                for (ch, selector, glyph) in mappings {
                    assert_eq!(charmap.map_unicode_variant(ch, selector), Some(glyph));
                }
            }
        }
        let delta = table(&[(0, 5, new.as_slice())]);
        let data = font_with_cmaps(None, Some(&delta));
        let font = Font::new(data, 0).unwrap();
        assert_eq!(
            font.charmap().map_unicode_variant(66u32, 0xfe0fu32),
            Some(GlyphId::new(70000))
        );
    }

    #[test]
    fn dmap_unicode_lookups_and_iterators() {
        use font_test_data::cmap::{font_with_cmaps, format12, format14, table};
        let base = table(&[
            (3, 10, &format12(&[(65, 1), (66, 2), (68, 5)])),
            (0, 5, &format14(0xfe0f, &[66], &[(65, 5), (67, 6)])),
        ]);
        let delta = table(&[
            (0, 4, &format12(&[(65, 3), (66, 0), (67, 4)])),
            (0, 5, &format14(0xfe0f, &[65], &[(66, 4), (68, 7)])),
        ]);
        let data = font_with_cmaps(Some(&base), Some(&delta));
        let font = Font::new(data, 0).unwrap();
        let charmap = font.charmap();
        assert!(charmap.has_unicode());
        assert!(charmap.has_unicode_variants());
        let mut mappings: Vec<_> = charmap.iter_unicodes().collect();
        mappings.sort_unstable();
        assert_eq!(
            mappings,
            vec![
                (65, GlyphId::new(3)),
                (66, GlyphId::new(2)),
                (67, GlyphId::new(4)),
                (68, GlyphId::new(5))
            ]
        );
        for (ch, glyph) in mappings {
            assert_eq!(charmap.map_unicode(ch), Some(glyph));
        }
        for encoding in charmap.encodings() {
            assert_eq!(encoding.sfnt_tag(), Some(Tag::new(b"cmap")));
            assert_eq!(charmap.map_code(encoding, 65), Some(GlyphId::new(3)));
            let mut codes: Vec<_> = charmap.iter_codes(encoding).collect();
            codes.sort_unstable();
            assert_eq!(
                codes,
                vec![
                    (65, GlyphId::new(3)),
                    (66, GlyphId::new(2)),
                    (67, GlyphId::new(4)),
                    (68, GlyphId::new(5))
                ]
            );
        }
        let mut variants: Vec<_> = charmap.iter_unicode_variants().collect();
        variants.sort_unstable();
        assert_eq!(
            variants,
            vec![
                (65, 0xfe0f, GlyphId::new(3)),
                (66, 0xfe0f, GlyphId::new(4)),
                (67, 0xfe0f, GlyphId::new(6)),
                (68, 0xfe0f, GlyphId::new(7))
            ]
        );
        for (ch, selector, glyph) in variants {
            assert_eq!(charmap.map_unicode_variant(ch, selector), Some(glyph));
        }
        let data = font_with_cmaps(None, Some(&delta));
        let font = Font::new(data, 0).unwrap();
        assert!(font.charmap().has_unicode());
        assert_eq!(font.charmap().map_unicode('A'), Some(GlyphId::new(3)));
        assert_eq!(
            font.charmap().map_unicode_variant('A', 0xfe0fu32),
            Some(GlyphId::new(3))
        );
        let encoding = font.charmap().encodings().next().unwrap();
        assert_eq!(encoding.sfnt_tag(), Some(Tag::new(b"DMAP")));
        assert_eq!(encoding.sfnt_index(), Some(0));
        assert_eq!(font.charmap().map_code(encoding, 65), Some(GlyphId::new(3)));
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
        assert_eq!(font.charmap().map_unicode('A'), Some(GlyphId::new(1)));
        let first_reads = cmap_reads.load(Ordering::Relaxed);
        assert!(first_reads > 0);
        assert_eq!(font.charmap().map_unicode('A'), Some(GlyphId::new(1)));
        assert_eq!(
            font.default_instance().charmap().map_unicode('A'),
            Some(GlyphId::new(1))
        );
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
