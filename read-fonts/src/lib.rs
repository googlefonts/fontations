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
pub use read::{ComputeSize, FontRead, FontReadWithArgs, ReadArgs, ReadError};
pub use table_provider::TableProvider;
pub use table_ref::TableRef;

/// All the types that may be referenced in auto-generated code.
#[doc(hidden)]
pub(crate) mod codegen_prelude {
    pub use crate::array::{ComputedArray, VarLenArray};
    pub use crate::font_data::{Cursor, FontData};
    pub use crate::offset::{Offset, ResolveNullableOffset, ResolveOffset};
    pub use crate::read::{ComputeSize, FontRead, FontReadWithArgs, Format, ReadArgs, ReadError};
    pub use crate::table_ref::{TableInfo, TableInfoWithArgs, TableRef};
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

/// A temporary type for accessing tables
pub struct FontRef<'a> {
    data: FontData<'a>,
    pub table_directory: TableDirectory<'a>,
}

impl<'a> FontRef<'a> {
    pub fn new(data: FontData<'a>) -> Result<Self, ReadError> {
        let table_directory = TableDirectory::read(data)?;
        if [TT_SFNT_VERSION, CFF_SFTN_VERSION].contains(&table_directory.sfnt_version()) {
            Ok(FontRef {
                data,
                table_directory,
            })
        } else {
            Err(ReadError::InvalidSfnt(table_directory.sfnt_version()))
        }
    }

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
}

impl<'a> TableProvider<'a> for FontRef<'a> {
    fn data_for_tag(&self, tag: Tag) -> Option<FontData<'a>> {
        self.table_data(tag)
    }
}
