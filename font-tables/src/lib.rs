//! font tables, records, etc.

#![cfg_attr(not(feature = "std"), no_std)]
// we autogenerate len methods in some places
#![allow(clippy::len_without_is_empty)]

#[cfg(any(feature = "std", test))]
#[allow(unused_imports)]
#[macro_use]
extern crate std;

#[cfg(all(not(feature = "std"), not(test)))]
#[macro_use]
extern crate core as std;

use font_types::{FontRead, Offset, Tag};

#[cfg(feature = "compile")]
pub mod compile;
pub mod layout;
pub mod tables;

#[cfg(any(test, feature = "diagnostics"))]
#[cfg_attr(any(test, feature = "diagnostics"), doc(hidden))]
pub mod hex_diff;

#[path = "../generated/generated_font.rs"]
mod generated;

pub use generated::*;

/// A temporary type for accessing tables
pub struct FontRef<'a> {
    data: &'a [u8],
    pub table_directory: TableDirectory<'a>,
}

const TT_MAGIC: u32 = 0x00010000;
const OT_MAGIC: u32 = 0x4F54544F;

impl<'a> FontRef<'a> {
    pub fn new(data: &'a [u8]) -> Result<Self, u32> {
        let table_directory = TableDirectory::read(data).ok_or(0x_dead_beef_u32)?;
        if [TT_MAGIC, OT_MAGIC].contains(&table_directory.sfnt_version()) {
            Ok(FontRef {
                data,
                table_directory,
            })
        } else {
            Err(table_directory.sfnt_version())
        }
    }

    pub fn table_data(&self, tag: Tag) -> Option<&'a [u8]> {
        self.table_directory
            .table_records()
            .binary_search_by(|rec| rec.tag.get().cmp(&tag))
            .ok()
            .and_then(|idx| self.table_directory.table_records().get(idx))
            .and_then(|record| {
                let start = record.offset.get().non_null()?;
                self.data.get(start..start + record.len.get() as usize)
            })
    }
}

impl tables::TableProvider for FontRef<'_> {
    fn data_for_tag(&self, tag: Tag) -> Option<&[u8]> {
        self.table_data(tag)
    }
}
