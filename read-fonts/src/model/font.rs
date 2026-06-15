//! Font representation.

mod blob;
mod format;
mod instance;
mod tables;

pub use blob::FontBlob;
pub use format::FontFormat;
pub use instance::{
    FontFeatureVariations, FontInstance, FontInstanceBuilder, FontVariation, NormalizedCoord,
};
pub use tables::{FontTableFunction, FontTables};

// Do our best to not expose this to users through docs or rust-analyzer.
#[doc(hidden)]
#[rust_analyzer::completions(hidden_from_completion)]
pub mod interop;

use super::once::Once;
use crate::{ps::type1::Type1Font, types::Tag, ReadError};
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
pub struct Font(Arc<FontRepr>);

impl Font {
    /// Creates a new font from the given source and font index.
    ///
    /// The index parameter specifies the desired font in a font collection
    /// (ttc or otc) file. It is ignored if the data source is not a blob.
    pub fn new(source: impl Into<FontSource>, index: u32) -> Result<Self, ReadError> {
        let source = source.into();
        let kind = if let Ok(tables) = FontTables::new(source.clone(), index) {
            Some(FontKind::Sfnt(tables, index))
        } else if let FontSource::Blob(blob) = &source {
            match FontFormat::new(blob) {
                Some(FontFormat::Type1) => Type1Font::new(blob).ok().map(FontKind::Type1),
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
        };
        Ok(Self(Arc::new(repr)))
    }

    /// Returns the underlying source of font data.
    pub fn source(&self) -> &FontSource {
        &self.0.source
    }

    /// Returns the underlying kind of the font.
    pub fn kind(&self) -> &FontKind {
        &self.0.kind
    }

    /// Returns an object that provides access to individual font tables.
    ///
    /// For non-SFNT fonts, this will return an empty set of tables.
    pub fn tables(&self) -> &FontTables {
        if let FontKind::Sfnt(tables, _) = &self.0.kind {
            tables
        } else {
            &tables::EMPTY_FONT_TABLES
        }
    }
}

struct FontRepr {
    source: FontSource,
    kind: FontKind,
    // Storage cell for lazily loaded HarfRust shaping data.
    shaping_data: Once<Box<dyn Any + Send + Sync>>,
}

/// The underlying type of a font.
#[expect(clippy::large_enum_variant)]
pub enum FontKind {
    /// An SFNT-based font represented by a set of tables and an index.
    Sfnt(FontTables, u32),
    /// An Adobe Type1 font.
    Type1(Type1Font),
}
