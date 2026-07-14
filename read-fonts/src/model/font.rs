//! Font representation.

mod blob;
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

use super::once::Once;
use crate::{
    ps::{
        cff::{CffFontAccel, CffFontRef},
        charmap::Charmap as PsCharmap,
        type1::Type1Font,
    },
    ReadError,
};
use alloc::{boxed::Box, sync::Arc};
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
            Some(FontKindRepr::Sfnt(tables, index))
        } else if let FontSource::Blob(blob) = &source {
            if let Ok(cff_accel) = CffFontAccel::new(blob, index, None) {
                cff_accel.materialize(blob).ok().map(|cff| {
                    #[cfg(feature = "agl")]
                    let charmap = if !cff.is_cid() {
                        cff.charset()
                            .map(|charset| {
                                PsCharmap::from_glyph_names(charset.iter().filter_map(
                                    |(gid, sid)| {
                                        Some((gid, core::str::from_utf8(cff.string(sid)?).ok()?))
                                    },
                                ))
                            })
                            .unwrap_or_default()
                    } else {
                        PsCharmap::default()
                    };
                    #[cfg(not(feature = "agl"))]
                    let charmap = PsCharmap::default();
                    // cff is unused when agl feature is disabled; silence the warning
                    let _ = cff;
                    FontKindRepr::Cff(blob.clone(), index, cff_accel, charmap)
                })
            } else {
                Type1Font::new(blob).ok().map(FontKindRepr::Type1)
            }
        } else {
            None
        };
        let kind = kind.ok_or(ReadError::MalformedData("Data isn't a font"))?;
        let repr = FontRepr {
            source,
            kind,
            shaping_data: Once::new(),
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
            FontKindRepr::Cff(blob, index, accel, _charmap) => {
                // Unwrap is safe because we materialized the font on creation
                FontKind::Cff(accel.materialize(blob).unwrap(), *index)
            }
        }
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
}

/// The underlying type of a font.
#[expect(clippy::large_enum_variant)]
#[derive(Clone)]
pub enum FontKind<'a> {
    /// An SFNT-based font represented by a set of tables and an index.
    Sfnt(&'a FontTables, u32),
    /// An Adobe Type1 font.
    Type1(&'a Type1Font),
    /// A CFF font with an associated index.
    Cff(CffFontRef<'a>, u32),
}

enum FontKindRepr {
    Sfnt(FontTables, u32),
    Type1(Type1Font),
    Cff(FontBlob, u32, CffFontAccel, PsCharmap),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FontRef, TableProvider};

    #[cfg(feature = "agl")]
    #[test]
    fn bare_cff_computed_charmap() {
        use types::GlyphId;
        let cff_data = FontRef::new(font_test_data::NOTO_SERIF_DISPLAY_TRIMMED)
            .unwrap()
            .cff()
            .unwrap()
            .offset_data()
            .as_bytes();
        let font = Font::new(cff_data, 0).unwrap();
        let FontKindRepr::Cff(_blob, _index, _accel, charmap) = &font.0.kind else {
            panic!("Expected CFF font");
        };
        let expected = [('i', 1), ('j', 2), ('k', 3), ('l', 4)]
            .map(|(ch, gid)| (ch as u32, GlyphId::new(gid)));
        let map = charmap.iter().collect::<Vec<_>>();
        assert_eq!(map, expected);
    }
}
