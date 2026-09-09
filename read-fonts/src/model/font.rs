//! Font representation.

mod blob;
mod cache;
mod format;
mod instance;
mod source;
mod tables;

pub use blob::FontBlob;
pub use format::FontFormat;
pub use instance::{
    FontFeatureVariations, FontInstance, FontInstanceBuilder, FontVariation, NormalizedCoord,
};
pub use source::FontSource;
pub use tables::{FontTableFunction, FontTables};

// Do our best to not expose this to users through docs or rust-analyzer.
#[doc(hidden)]
#[rust_analyzer::completions(hidden_from_completion)]
pub mod interop;

use super::metrics::{empty_glyph_metrics, GlobalMetrics, GlyphMetrics, RawGlyphMetrics};
use super::once::Once;
use crate::tables::{glyf::Glyf, gvar::Gvar, hvar::Hvar, loca::Loca, vvar::Vvar};
use crate::{
    ps::{cff::CffFontRef, type1::Type1Font},
    ReadError,
};
use alloc::{boxed::Box, sync::Arc};
use cache::{GlyfLoca, GvarTable, HvarTable, TableCache, VvarTable};
use core::any::Any;

/// An OpenType or PostScript font.
///
/// This type is internally reference counted, cheaply cloneable and thread
/// safe.
#[derive(Clone)]
pub struct Font(Arc<FontRepr>);

impl Font {
    /// Creates a new font from the given source and font index.
    ///
    /// The index parameter specifies the desired font in a font collection
    /// (ttc or otc) file. It is ignored if the data source is not a blob.
    pub fn new(source: impl Into<FontSource>, index: u32) -> Result<Self, ReadError> {
        let source = source.into();
        let kind = if let Ok(tables) = FontTables::new(source.clone(), index) {
            Some(FontKindRepr::Sfnt(Arc::new(tables), index))
        } else if let FontSource::Blob(blob) = &source {
            match FontFormat::new(blob) {
                Some(FontFormat::Type1) => Type1Font::new(blob)
                    .ok()
                    .map(|font| FontKindRepr::Type1(Box::new(font))),
                // TODO: pure CFF fonts
                _ => None,
            }
        } else {
            None
        };
        let kind = kind.ok_or(ReadError::MalformedData("Data isn't a font"))?;
        let repr = FontRepr {
            source,
            kind,
            shaping_data: Once::new(),
            global_metrics: Once::new(),
            h_metrics: Once::new(),
            v_metrics: Once::new(),
            glyf_loca: Once::new(),
            hvar: Once::new(),
            vvar: Once::new(),
            gvar: Once::new(),
        };
        Ok(Self(Arc::new(repr)))
    }

    /// Returns the underlying source of font data.
    pub fn source(&self) -> &FontSource {
        &self.0.source
    }

    /// Returns the underlying kind of the font.
    pub fn kind(&self) -> FontKind<'_> {
        match &self.0.kind {
            FontKindRepr::Sfnt(tables, index) => FontKind::Sfnt(tables, *index),
            FontKindRepr::Type1(font) => FontKind::Type1(font),
        }
    }

    /// Returns the metrics describing the font as a whole, at its default
    /// location.
    #[inline]
    pub fn global_metrics(&self) -> &GlobalMetrics {
        self.0.global_metrics.get_or_init(|| match self.kind() {
            FontKind::Type1(font) => GlobalMetrics::from_type1(font),
            _ => GlobalMetrics::from_sfnt(&self.tables(), &[]),
        })
    }

    /// Returns the number of glyphs in the font.
    ///
    /// Fixed for the font: no location varies it.
    #[inline]
    pub fn num_glyphs(&self) -> u32 {
        self.global_metrics().num_glyphs
    }

    /// Returns the size of the em square, in design units.
    ///
    /// Fixed for the font: no location varies it.
    #[inline]
    pub fn units_per_em(&self) -> u16 {
        self.global_metrics().units_per_em
    }

    /// Returns measurements of individual glyphs, at the font's default
    /// location.
    ///
    /// Use [`FontInstance::glyph_metrics`] to measure elsewhere in the
    /// design space.
    #[inline]
    pub fn glyph_metrics(&self) -> GlyphMetrics<'_> {
        GlyphMetrics::new(self, self.global_metrics(), &[])
    }

    /// Returns the tables behind this font, for a cache that holds them.
    pub(crate) fn tables_arc(&self) -> Option<&Arc<FontTables>> {
        match &self.0.kind {
            FontKindRepr::Sfnt(tables, _) => Some(tables),
            _ => None,
        }
    }

    /// Returns what `hmtx` states, parsed once for the font.
    #[inline]
    pub(crate) fn h_metrics(&self) -> &RawGlyphMetrics<'_> {
        let Some(tables) = self.tables_arc() else {
            return empty_glyph_metrics();
        };
        self.0
            .h_metrics
            .get_or_init(|| {
                TableCache::read(tables.clone(), |tables| RawGlyphMetrics::from_hmtx(&tables))
            })
            .get()
    }

    /// Returns what `vmtx` states, parsed once for the font.
    ///
    /// Unlike `hmtx` this is read on demand: most text is horizontal and
    /// never asks.
    #[inline]
    pub(crate) fn v_metrics(&self) -> &RawGlyphMetrics<'_> {
        let Some(tables) = self.tables_arc() else {
            return empty_glyph_metrics();
        };
        self.0
            .v_metrics
            .get_or_init(|| {
                TableCache::read(tables.clone(), |tables| RawGlyphMetrics::from_vmtx(&tables))
            })
            .get()
    }

    /// Returns the outline tables, parsed once for the font.
    ///
    /// Outlines and metrics in both directions read these, so they are
    /// shared rather than parsed by each.
    #[inline]
    pub(crate) fn glyf_loca(&self) -> Option<&(Glyf<'_>, Loca<'_>)> {
        let tables = self.tables_arc()?;
        self.0
            .glyf_loca
            .get_or_init(|| TableCache::read(tables.clone(), |tables| GlyfLoca::read(&tables)))
            .get()
            .0
            .as_ref()
    }

    /// Returns `HVAR`, parsed once for the font.
    #[inline]
    pub(crate) fn hvar(&self) -> Option<&Hvar<'_>> {
        let tables = self.tables_arc()?;
        self.0
            .hvar
            .get_or_init(|| TableCache::read(tables.clone(), |tables| HvarTable::read(&tables)))
            .get()
            .0
            .as_ref()
    }

    /// Returns `VVAR`, parsed once for the font.
    #[inline]
    pub(crate) fn vvar(&self) -> Option<&Vvar<'_>> {
        let tables = self.tables_arc()?;
        self.0
            .vvar
            .get_or_init(|| TableCache::read(tables.clone(), |tables| VvarTable::read(&tables)))
            .get()
            .0
            .as_ref()
    }

    /// Returns `gvar`, parsed once for the font.
    ///
    /// Typically about half a variable font, so nothing should ask for this
    /// that another table can answer.
    #[inline]
    pub(crate) fn gvar(&self) -> Option<&Gvar<'_>> {
        let tables = self.tables_arc()?;
        self.0
            .gvar
            .get_or_init(|| TableCache::read(tables.clone(), |tables| GvarTable::read(&tables)))
            .get()
            .0
            .as_ref()
    }

    /// Returns this font as an instance at its default location.
    #[inline]
    pub fn default_instance(&self) -> FontInstance {
        FontInstance::from(self)
    }

    /// Returns an object that provides access to individual font tables.
    ///
    /// For non-SFNT fonts, this will return an empty set of tables.
    pub fn tables(&self) -> &FontTables {
        if let FontKindRepr::Sfnt(tables, _) = &self.0.kind {
            tables
        } else {
            &tables::EMPTY_FONT_TABLES
        }
    }
}

struct FontRepr {
    source: FontSource,
    kind: FontKindRepr,
    // Storage cell for lazily loaded HarfRust shaping data.
    shaping_data: Once<Box<dyn Any + Send + Sync>>,
    // Metrics that describe the font as a whole, at the default location,
    // read once rather than per query. Kept apart from `shaping_data`, which
    // holds one thing for one owner.
    global_metrics: Once<GlobalMetrics>,
    // What `hmtx` states, parsed once for the font. Held beside the tables
    // it borrows, which is what lets it live here at all.
    h_metrics: Once<TableCache<RawGlyphMetrics<'static>>>,
    /// `vmtx`, read only by a caller measuring vertically, which is the
    /// minority of them.
    v_metrics: Once<TableCache<RawGlyphMetrics<'static>>>,
    // `HVAR` states the deltas a location makes to a metric outright, and
    // `gvar` states them as phantom points on an outline, which `glyf` and
    // `loca` are read to reach and which outlines will read for their own
    // sake. None of the three depends on a location to parse, so every
    // instance shares them, and only an instance asks for them: a font read
    // at its default location touches none of them.
    glyf_loca: Once<TableCache<GlyfLoca<'static>>>,
    hvar: Once<TableCache<HvarTable<'static>>>,
    vvar: Once<TableCache<VvarTable<'static>>>,
    gvar: Once<TableCache<GvarTable<'static>>>,
}

/// The underlying type of a font.
#[derive(Clone)]
pub enum FontKind<'a> {
    /// An SFNT-based font represented by a set of tables and an index.
    Sfnt(&'a FontTables, u32),
    /// An Adobe Type1 font.
    Type1(&'a Type1Font),
    /// A CFF font with an associated index.
    Cff(CffFontRef<'a>, u32),
}

/// The underlying type of a font.
enum FontKindRepr {
    Sfnt(Arc<FontTables>, u32),
    // Boxed: a `Type1Font` is an order of magnitude larger than the sfnt
    // variant, and inline it would be paid by every font that is not one.
    Type1(Box<Type1Font>),
}
