//!  A builder for top-level font objects

use std::borrow::Cow;
use std::collections::BTreeMap;

use font_types::Tag;

use super::{FontWrite, TableWriter};

include!("../../generated/generated_font_compile.rs");

/// Build a font from some set of tables.
#[derive(Debug, Clone, Default)]
pub struct FontBuilder<'a> {
    tables: BTreeMap<Tag, Cow<'a, [u8]>>,
}

impl<'a> FontBuilder<'a> {
    pub fn add_table(&mut self, tag: Tag, data: impl Into<Cow<'a, [u8]>>) -> &mut Self {
        self.tables.insert(tag, data.into());
        self
    }

    pub fn build(&mut self) -> Vec<u8> {
        let header_len = std::mem::size_of::<u32>() // sfnt
            + std::mem::size_of::<u16>() * 4 // num_tables to range_shift
            + self.tables.len() * std::mem::size_of::<crate::TableRecord>();

        let mut position = header_len as u32;
        let table_records = self
            .tables
            .iter_mut()
            .map(|(tag, data)| {
                let offset = position;
                let len = data.len() as u32;
                position += len;
                let (checksum, padding) = checksum_and_padding(&data);
                position += padding;
                TableRecord {
                    tag: *tag,
                    checksum,
                    offset,
                    len,
                }
            })
            .collect();

        let directory = TableDirectory {
            sfnt_version: crate::TT_MAGIC,
            search_range: 0,
            entry_selector: 0,
            range_shift: 0,
            table_records,
        };

        let mut writer = TableWriter::default();
        directory.write_into(&mut writer);
        let mut data = writer.into_data();
        for table in self.tables.values() {
            data.extend_from_slice(&table);
            let rem = table.len() % 4;
            let padding = [0u8; 4];
            data.extend_from_slice(&padding[..rem]);
        }
        data
    }
}

fn checksum_and_padding(table: &[u8]) -> (u32, u32) {
    let padding = table.len() % 4;
    let mut sum = 0u32;
    let mut iter = table.chunks_exact(4);
    for quad in &mut iter {
        // this can't fail, and we trust the compiler to avoid a branch
        let array: [u8; 4] = quad.try_into().unwrap_or_default();
        sum = sum.wrapping_add(u32::from_be_bytes(array));
    }

    let rem = match iter.remainder() {
        &[a] => u32::from_be_bytes([a, 0, 0, 0]),
        &[a, b] => u32::from_be_bytes([a, b, 0, 0]),
        &[a, b, c] => u32::from_be_bytes([a, b, c, 0]),
        _ => 0,
    };

    (sum.wrapping_add(rem), padding as u32)
}
