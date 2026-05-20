//! Extension traits for `FontBuilder`

use font_types::Tag;
use read_fonts::TopLevelTable;

use crate::{validate::Validate, FontWrite};

/// An error returned when attempting to add a table to the builder.
///
/// This wraps a compilation error, adding the tag of the table where it was
/// encountered.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct BuilderError {
    /// The tag of the root table where the error occurred
    pub tag: Tag,
    /// The underlying error
    pub inner: crate::error::Error,
}

impl std::fmt::Display for BuilderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "failed to build '{}' table: '{}'", self.tag, self.inner)
    }
}

impl std::error::Error for BuilderError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.inner)
    }
}

/// Extension trait for `font_builder::FontBuilder` to support adding typed `write-fonts` tables.
pub trait FontBuilderExt {
    /// Add a table to the builder.
    ///
    /// The table can be any top-level table defined in this crate. This function
    /// will attempt to compile the table and then add it to the builder if
    /// successful, returning an error otherwise.
    fn add_table<T>(&mut self, table: &T) -> Result<&mut Self, BuilderError>
    where
        T: FontWrite + Validate + TopLevelTable;
}

impl<'a> FontBuilderExt for font_builder::FontBuilder<'a> {
    fn add_table<T>(&mut self, table: &T) -> Result<&mut Self, BuilderError>
    where
        T: FontWrite + Validate + TopLevelTable,
    {
        let tag = T::TAG;
        let bytes = crate::dump_table(table).map_err(|inner| BuilderError { inner, tag })?;
        self.add_raw(tag, bytes);
        Ok(self)
    }
}
