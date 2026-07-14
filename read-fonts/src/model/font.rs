//! Font representation.

mod blob;
mod format;
mod tables;

pub use blob::FontBlob;
pub use format::FontFormat;
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
    types::Tag,
    ReadError,
};
use alloc::{boxed::Box, sync::Arc};
use core::any::Any;

/// Source for font data.
#[derive(Clone)]
pub enum FontSource {
    /// A nice flat buffer.
    Blob(FontBlob),
    /// Lazy loader with per-table data provided by a function.
    TableFunction(FontTableFunction),
}

impl<T: Into<FontBlob>> From<T> for FontSource {
    fn from(value: T) -> Self {
        Self::Blob(value.into())
    }
}

impl From<Arc<dyn Fn(Tag) -> Option<FontBlob> + Send + Sync>> for FontSource {
    fn from(value: Arc<dyn Fn(Tag) -> Option<FontBlob> + Send + Sync>) -> Self {
        Self::TableFunction(FontTableFunction::new(value))
    }
}

impl From<FontTableFunction> for FontSource {
    fn from(value: FontTableFunction) -> Self {
        Self::TableFunction(value)
    }
}

/// An OpenType or PostScript font.
#[derive(Clone)]
pub struct Font(Arc<FontInner>);

impl Font {
    /// Creates a new font from the given source and font index.
    ///
    /// The index parameter specifies the desired font in a font collection
    /// (ttc or otc) file. It is ignored if the data source is not a blob.
    pub fn new(source: impl Into<FontSource>, index: u32) -> Result<Self, ReadError> {
        let source = source.into();
        let kind = if let Ok(tables) = FontTables::new(source.clone(), index) {
            Some(FontKindRepr::OpenType(tables, index))
        } else if let FontSource::Blob(blob) = &source {
            match FontFormat::new(blob) {
                Some(FontFormat::Type1) => Type1Font::new(blob).ok().map(FontKindRepr::Type1),
                Some(FontFormat::Cff(_)) => CffFontAccel::new(blob, index, None)
                    .ok()
                    .and_then(|accel| Some((accel.clone(), accel.materialize(&blob).ok()?)))
                    .map(|(accel, _cff)| {
                        #[cfg(feature = "agl")]
                        let charmap = Some(PsCharmap::from_glyph_names(charset.iter().filter_map(
                            |(gid, sid)| Some((gid, core::str::from_utf8(_cff.string(sid)?).ok()?)),
                        )));
                        #[cfg(not(feature = "agl"))]
                        let charmap = PsCharmap::default();
                        FontKindRepr::Cff(blob.clone(), index, accel, charmap)
                    }),
                _ => None,
            }
        } else {
            None
        };
        let kind = kind.ok_or(ReadError::MalformedData("Data isn't a font"))?;
        let inner = FontInner {
            source,
            kind,
            shaping_data: Once::new(),
        };
        Ok(Self(Arc::new(inner)))
    }

    /// Returns the underlying source of font data.
    pub fn source(&self) -> &FontSource {
        &self.0.source
    }

    /// Returns the underlying kind of the font.
    pub fn kind(&self) -> FontKind<'_> {
        match &self.0.kind {
            FontKindRepr::OpenType(tables, index) => FontKind::OpenType(tables, *index),
            FontKindRepr::Type1(font) => FontKind::Type1(font),
            FontKindRepr::Cff(blob, index, accel, _charmap) => {
                // Unwrap is safe because we materialized the font on creation
                FontKind::Cff(accel.materialize(blob).unwrap(), *index)
            }
        }
    }

    /// If this is a table based (i.e. OpenType) font, then returns an object
    /// that provides access to the individual tables.
    pub fn tables(&self) -> Option<&FontTables> {
        if let FontKindRepr::OpenType(tables, _) = &self.0.kind {
            Some(tables)
        } else {
            None
        }
    }
}

struct FontInner {
    source: FontSource,
    kind: FontKindRepr,
    // Storage cell for lazily loaded HarfRust shaping data.
    shaping_data: Once<Box<dyn Any + Send + Sync>>,
}

#[allow(clippy::large_enum_variant)]
enum FontKindRepr {
    OpenType(FontTables, u32),
    Type1(Type1Font),
    Cff(FontBlob, u32, CffFontAccel, PsCharmap),
}

/// The underlying type of a font.
pub enum FontKind<'a> {
    /// An OpenType font represented by a set of tables and an index.
    OpenType(&'a FontTables, u32),
    /// An Adobe Type1 font.
    Type1(&'a Type1Font),
    /// A CFF font with an associated index.
    Cff(CffFontRef<'a>, u32),
}
