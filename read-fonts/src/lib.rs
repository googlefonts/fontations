//! Parsing OpentType tables.

#![deny(rustdoc::broken_intra_doc_links)]
#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(any(feature = "std", test))]
#[macro_use]
extern crate std;

#[cfg(all(not(feature = "std"), not(test)))]
#[macro_use]
extern crate core as std;

pub mod array;
mod font_data;
pub mod layout;
mod offset;
mod read;
mod table_provider;
mod table_ref;
pub mod tables;
#[cfg(feature = "traversal")]
pub mod traversal;

#[cfg(any(test, feature = "test_data"))]
pub mod codegen_test;
#[cfg(any(test, feature = "test_data"))]
#[path = "tests/test_data.rs"]
pub mod test_data;
#[cfg(any(test, feature = "test_data"))]
#[path = "tests/test_helpers.rs"]
pub mod test_helpers;

pub use font_data::FontData;
pub use offset::{Offset, ResolveNullableOffset, ResolveOffset};
pub use read::{ComputeSize, FontRead, FontReadWithArgs, ReadArgs, ReadError, VarSize};
pub use table_provider::TableProvider;
pub use table_ref::TableRef;

/// Public re-export of the font-types crate.
pub use font_types as types;

/// All the types that may be referenced in auto-generated code.
#[doc(hidden)]
pub(crate) mod codegen_prelude {
    pub use crate::array::{ComputedArray, VarLenArray};
    pub use crate::font_data::{Cursor, FontData};
    pub use crate::offset::{Offset, ResolveNullableOffset, ResolveOffset};
    pub use crate::read::{
        ComputeSize, FontRead, FontReadWithArgs, Format, ReadArgs, ReadError, VarSize,
    };
    pub use crate::table_ref::TableRef;
    pub use font_types::*;
    pub use std::ops::Range;

    #[cfg(feature = "traversal")]
    pub use crate::traversal::{self, Field, FieldType, RecordResolver, SomeRecord, SomeTable};

    // used in generated traversal code to get type names of offset fields, which
    // may include generics
    #[cfg(feature = "traversal")]
    pub(crate) fn better_type_name<T>() -> &'static str {
        let raw_name = std::any::type_name::<T>();
        let last = raw_name.rsplit("::").next().unwrap_or(raw_name);
        // this happens if we end up getting a type name like TableRef<'a, module::SomeMarker>
        last.trim_end_matches("Marker>")
    }

    /// used in generated code
    pub fn minus_one(val: impl Into<usize>) -> usize {
        val.into().saturating_sub(1)
    }
}

include!("../generated/font.rs");

#[derive(Clone)]
/// Reference to the content of a font or font collection file.
pub enum FileRef<'a> {
    /// A single font.
    Font(FontRef<'a>),
    /// A collection of fonts.
    Collection(CollectionRef<'a>),
}

impl<'a> FileRef<'a> {
    /// Creates a new reference to a file representing a font or font collection.
    pub fn new(data: FontData<'a>) -> Result<Self, ReadError> {
        Ok(if let Ok(collection) = CollectionRef::new(data) {
            Self::Collection(collection)
        } else {
            Self::Font(FontRef::new(data)?)
        })
    }

    /// Returns an iterator over the fonts contained in the file.
    pub fn fonts(&self) -> impl Iterator<Item = Result<FontRef<'a>, ReadError>> + 'a + Clone {
        let (iter_one, iter_two) = match self {
            Self::Font(font) => (Some(Ok(font.clone())), None),
            Self::Collection(collection) => (None, Some(collection.iter())),
        };
        iter_two.into_iter().flatten().chain(iter_one)
    }
}

/// Reference to the content of a font collection file.
#[derive(Clone)]
pub struct CollectionRef<'a> {
    data: FontData<'a>,
    header: TTCHeader<'a>,
}

impl<'a> CollectionRef<'a> {
    /// Creates a new reference to a font collection.
    pub fn new(data: FontData<'a>) -> Result<Self, ReadError> {
        let header = TTCHeader::read(data)?;
        if header.ttc_tag() != TTC_HEADER_TAG {
            Err(ReadError::InvalidTtc(header.ttc_tag()))
        } else {
            Ok(Self { data, header })
        }
    }

    /// Returns the number of fonts in the collection.
    pub fn len(&self) -> u32 {
        self.header.num_fonts()
    }

    /// Returns true if the collection is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the font in the collection at the specified index.
    pub fn get(&self, index: u32) -> Result<FontRef<'a>, ReadError> {
        let offset = self
            .header
            .table_directory_offsets()
            .get(index as usize)
            .ok_or(ReadError::InvalidCollectionIndex(index))?
            .get() as usize;
        let table_dir_data = self.data.slice(offset..).ok_or(ReadError::OutOfBounds)?;
        FontRef::with_table_directory(self.data, TableDirectory::read(table_dir_data)?)
    }

    /// Returns an iterator over the fonts in the collection.
    pub fn iter(&self) -> impl Iterator<Item = Result<FontRef<'a>, ReadError>> + 'a + Clone {
        let copy = self.clone();
        (0..self.len()).map(move |ix| copy.get(ix))
    }
}

#[derive(Clone)]
/// A temporary type for accessing tables
pub struct FontRef<'a> {
    data: FontData<'a>,
    pub table_directory: TableDirectory<'a>,
}

impl<'a> FontRef<'a> {
    /// Creates a new reference to a font.
    pub fn new(data: FontData<'a>) -> Result<Self, ReadError> {
        Self::with_table_directory(data, TableDirectory::read(data)?)
    }

    /// Returns the data for the table with the specified tag, if present.
    pub fn table_data(&self, tag: Tag) -> Option<FontData<'a>> {
        self.table_directory
            .table_records()
            .binary_search_by(|rec| rec.tag.get().cmp(&tag))
            .ok()
            .and_then(|idx| self.table_directory.table_records().get(idx))
            .and_then(|record| {
                let start = record.offset().non_null()?;
                let len = record.length() as usize;
                self.data.slice(start..start + len)
            })
    }

    fn with_table_directory(
        data: FontData<'a>,
        table_directory: TableDirectory<'a>,
    ) -> Result<Self, ReadError> {
        if [TT_SFNT_VERSION, CFF_SFTN_VERSION].contains(&table_directory.sfnt_version()) {
            Ok(FontRef {
                data,
                table_directory,
            })
        } else {
            Err(ReadError::InvalidSfnt(table_directory.sfnt_version()))
        }
    }
}

impl<'a> TableProvider<'a> for FontRef<'a> {
    fn data_for_tag(&self, tag: Tag) -> Option<FontData<'a>> {
        self.table_data(tag)
    }
}
