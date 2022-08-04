//! Reading OpentType tables

mod array;
mod font_data;
pub mod layout;
mod read;
mod table_provider;
mod table_ref;

#[cfg(any(test, feature = "test_data"))]
#[path = "tests/test_data.rs"]
pub mod test_data;
#[cfg(any(test, feature = "test_data"))]
#[path = "tests/test_helpers.rs"]
pub mod test_helpers;

pub use font_data::FontData;
pub use read::{FontRead, FontReadWithArgs, ReadError};
pub use table_provider::TableProvider;

pub mod parse_prelude {
    pub use crate::array::ComputedArray;
    pub use crate::font_data::{Cursor, FontData};
    pub use crate::read::{ComputeSize, FontRead, FontReadWithArgs, Format, ReadArgs, ReadError};
    pub use crate::table_ref::{ResolveOffset, TableInfo, TableInfoWithArgs, TableRef};
    pub use font_types::*;
    pub use std::ops::Range;
}

pub mod tables {
    pub use super::layout::gpos;
}

include!("../generated/font.rs");

/// A temporary type for accessing tables
pub struct FontRef<'a> {
    data: FontData<'a>,
    pub table_directory: TableDirectory<'a>,
}

const TT_MAGIC: u32 = 0x00010000;
const OT_MAGIC: u32 = 0x4F54544F;

impl<'a> FontRef<'a> {
    pub fn new(data: FontData<'a>) -> Result<Self, ReadError> {
        let table_directory = TableDirectory::read(data)?;
        if [TT_MAGIC, OT_MAGIC].contains(&table_directory.sfnt_version()) {
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
                let len = record.len() as usize;
                self.data.slice(start..start + len)
            })
    }
}

impl<'a> TableProvider for FontRef<'a> {
    fn data_for_tag(&self, tag: Tag) -> Option<FontData> {
        self.table_data(tag)
    }
}
