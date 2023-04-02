//!  A builder for top-level font objects

use std::borrow::Cow;
use std::collections::BTreeMap;

use types::{Tag, TT_SFNT_VERSION};

include!("../generated/generated_font.rs");

const TABLE_RECORD_LEN: usize = 16;

/// Build a font from some set of tables.
#[derive(Debug, Clone, Default)]
pub struct FontBuilder<'a> {
    tables: BTreeMap<Tag, Cow<'a, [u8]>>,
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum BuildFontError {
    TooManyTables,
}

impl TableDirectory {
    pub fn from_table_records(
        table_records: Vec<TableRecord>,
    ) -> Result<TableDirectory, BuildFontError> {
        if table_records.len() > u16::MAX as usize {
            return Err(BuildFontError::TooManyTables);
        }

        // See https://learn.microsoft.com/en-us/typography/opentype/spec/otff#table-directory
        // Computation works at the largest allowable num tables so don't stress the as u16's
        let entry_selector = (table_records.len() as f64).log2().floor() as u16;
        let search_range = (2.0_f64.powi(entry_selector as i32) * 16.0) as u16;
        // The result doesn't really make sense with 0 tables but ... let's at least not fail
        let range_shift = (table_records.len() * 16).saturating_sub(search_range as usize) as u16;

        Ok(TableDirectory::new(
            TT_SFNT_VERSION,
            search_range,
            entry_selector,
            range_shift,
            table_records,
        ))
    }
}

impl<'a> FontBuilder<'a> {
    pub fn add_table(&mut self, tag: Tag, data: impl Into<Cow<'a, [u8]>>) -> &mut Self {
        self.tables.insert(tag, data.into());
        self
    }

    /// Returns `true` if the builder contains a table with this tag.
    pub fn contains(&self, tag: Tag) -> bool {
        self.tables.contains_key(&tag)
    }

    pub fn build(&mut self) -> Result<Vec<u8>, BuildFontError> {
        let header_len = std::mem::size_of::<u32>() // sfnt
            + std::mem::size_of::<u16>() * 4 // num_tables to range_shift
            + self.tables.len() * TABLE_RECORD_LEN;

        let mut position = header_len as u32;
        let table_records: Vec<_> = self
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

        let directory = TableDirectory::from_table_records(table_records)?;

        let mut writer = TableWriter::default();
        directory.write_into(&mut writer);
        let mut data = writer.into_data();
        for table in self.tables.values() {
            data.extend_from_slice(table);
            let rem = table.len() % 4;
            let padding = [0u8; 4];
            data.extend_from_slice(&padding[..rem]);
        }
        Ok(data)
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

#[cfg(test)]
mod tests {
    use font_types::Tag;
    use read_fonts::FontRef;

    use crate::{font_builder::BuildFontError, FontBuilder};

    #[test]
    fn sets_binary_search_assists() {
        // Based on Roboto's num tables
        let data = b"doesn't matter".to_vec();
        let mut builder = FontBuilder::default();
        (0..0x16u32).for_each(|i| {
            builder.add_table(Tag::from_be_bytes(i.to_ne_bytes()), &data);
        });
        let bytes = builder.build().unwrap();
        let font = FontRef::new(&bytes).unwrap();
        let td = font.table_directory;
        assert_eq!(
            (256, 4, 96),
            (td.search_range(), td.entry_selector(), td.range_shift())
        );
    }

    #[test]
    fn rejects_too_many_tables() {
        let data = b"doesn't matter".to_vec();
        let mut builder = FontBuilder::default();
        (0..=u16::MAX as u32).for_each(|i| {
            builder.add_table(Tag::from_be_bytes(i.to_ne_bytes()), &data);
        });
        assert_eq!(Err(BuildFontError::TooManyTables), builder.build());
    }

    #[test]
    fn survives_no_tables() {
        FontBuilder::default().build().unwrap();
    }
}
