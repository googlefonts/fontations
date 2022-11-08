//!  A builder for top-level font objects

use std::borrow::Cow;
use std::collections::BTreeMap;

use font_types::Tag;

include!("../generated/generated_font.rs");

const TABLE_RECORD_LEN: usize = 16;

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
            + self.tables.len() * TABLE_RECORD_LEN;

        let mut position = header_len as u32;
        let table_records = self
            .tables
            .iter_mut()
            .map(|(tag, data)| {
                let offset = position;
                let length = data.len() as u32;
                position += length;
                let (checksum, padding) = checksum_and_padding(data);
                position += padding;
                TableRecord::new(*tag, checksum, offset, length)
            })
            .collect();

        let directory = TableDirectory::new(font_types::TT_SFNT_VERSION, 0, 0, 0, table_records);

        let mut writer = TableWriter::default();
        directory.write_into(&mut writer);
        let mut data = writer.into_data();
        for table in self.tables.values() {
            data.extend_from_slice(table);
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

    let rem = match *iter.remainder() {
        [a] => u32::from_be_bytes([a, 0, 0, 0]),
        [a, b] => u32::from_be_bytes([a, b, 0, 0]),
        [a, b, c] => u32::from_be_bytes([a, b, c, 0]),
        _ => 0,
    };

    (sum.wrapping_add(rem), padding as u32)
}

impl TTCHeader {
    fn compute_version(&self) -> MajorMinor {
        panic!("TTCHeader writing not supported (yet)")
    }
}
