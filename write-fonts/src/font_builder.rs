//!  A builder for top-level font objects

use std::collections::BTreeMap;
use std::{borrow::Cow, sync::Arc};

use types::{Tag, TT_SFNT_VERSION};

use crate::dump_table;

include!("../generated/generated_font.rs");

const TABLE_RECORD_LEN: usize = 16;

/// Build a font from some set of tables.
#[derive(Clone, Default)]
pub struct FontBuilder<'a> {
    tables: BTreeMap<Tag, Table<'a>>,
}

/// A trait for types that represent compilable top-level tables.
///
/// (We need a new trait for this so that we can use dyn Trait)
trait TopLevelWriteTable: FontWrite + Validate {
    fn compile(&self) -> Result<Vec<u8>, crate::error::Error>;
}

impl<T: TopLevelTable + FontWrite + Validate> TopLevelWriteTable for T {
    fn compile(&self) -> Result<Vec<u8>, crate::error::Error> {
        dump_table(self)
    }
}

/// An internal type representing a table that may or may not be precompiled.
#[derive(Clone)]
enum Table<'a> {
    Raw(Arc<dyn TopLevelWriteTable>),
    Precompiled(Cow<'a, [u8]>),
}

impl TableDirectory {
    pub fn from_table_records(table_records: Vec<TableRecord>) -> TableDirectory {
        assert!(table_records.len() <= u16::MAX as usize);

        // See https://learn.microsoft.com/en-us/typography/opentype/spec/otff#table-directory
        // Computation works at the largest allowable num tables so don't stress the as u16's
        let entry_selector = (table_records.len() as f64).log2().floor() as u16;
        let search_range = (2.0_f64.powi(entry_selector as i32) * 16.0) as u16;
        // The result doesn't really make sense with 0 tables but ... let's at least not fail
        let range_shift = (table_records.len() * 16).saturating_sub(search_range as usize) as u16;

        TableDirectory::new(
            TT_SFNT_VERSION,
            search_range,
            entry_selector,
            range_shift,
            table_records,
        )
    }
}

impl<'a> FontBuilder<'a> {
    /// Add a table to the font.
    pub fn add_table<T: TopLevelTable + FontWrite + Validate + 'static>(
        &mut self,
        table: T,
    ) -> &mut Self {
        let tag = T::TAG;
        self.tables.insert(tag, Table::Raw(Arc::new(table)));
        self
    }

    /// Add a pre-compiled table to the font.
    ///
    /// This data can be either a borrowed slice or a vec.
    pub fn add_bytes(&mut self, tag: Tag, data: impl Into<Cow<'a, [u8]>>) -> &mut Self {
        self.tables.insert(tag, Table::Precompiled(data.into()));
        self
    }

    /// Returns `true` if the builder contains a table with this tag.
    pub fn contains(&self, tag: Tag) -> bool {
        self.tables.contains_key(&tag)
    }

    pub fn build(&mut self) -> Result<Vec<u8>, crate::error::Error> {
        let header_len = std::mem::size_of::<u32>() // sfnt
            + std::mem::size_of::<u16>() * 4 // num_tables to range_shift
            + self.tables.len() * TABLE_RECORD_LEN;

        // first compile everything, as needed
        //TODO rayon me
        let compiled = self
            .tables
            .iter()
            .map(|(tag, table)| table.to_bytes().map(|table| (*tag, table)))
            .collect::<Result<Vec<_>, _>>()?;

        let mut position = header_len as u32;
        let table_records: Vec<_> = compiled
            .iter()
            .map(|(tag, data)| {
                let offset = position;
                let length = data.len() as u32;
                position += length;
                let (checksum, padding) = checksum_and_padding(data);
                position += padding;
                TableRecord::new(*tag, checksum, offset, length)
            })
            .collect();

        let directory = TableDirectory::from_table_records(table_records);

        let mut writer = TableWriter::default();
        directory.write_into(&mut writer);
        let mut data = writer.into_data();
        for (_, table) in &compiled {
            data.extend_from_slice(table);
            let rem = round4(table.len()) - table.len();
            let padding = [0u8; 4];
            data.extend_from_slice(&padding[..rem]);
        }
        Ok(data)
    }
}

impl<'a> Table<'a> {
    /// Compile the table if necessary, returning the final bytes
    fn to_bytes(&self) -> Result<Cow<[u8]>, crate::error::Error> {
        match self {
            Table::Raw(table) => table.compile().map(Cow::Owned),
            Table::Precompiled(bytes) => Ok(Cow::Borrowed(bytes)),
        }
    }
}

/// <https://github.com/google/woff2/blob/a0d0ed7da27b708c0a4e96ad7a998bddc933c06e/src/round.h#L19>
fn round4(sz: usize) -> usize {
    (sz + 3) & !3
}

fn checksum_and_padding(table: &[u8]) -> (u32, u32) {
    let padding = round4(table.len()) - table.len();
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

    use crate::{font_builder::checksum_and_padding, FontBuilder};

    #[test]
    fn sets_binary_search_assists() {
        // Based on Roboto's num tables
        let data = b"doesn't matter".to_vec();
        let mut builder = FontBuilder::default();
        (0..0x16u32).for_each(|i| {
            builder.add_bytes(Tag::from_be_bytes(i.to_ne_bytes()), &data);
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
    fn survives_no_tables() {
        FontBuilder::default().build().unwrap();
    }

    #[test]
    fn pad4() {
        for i in 0..10 {
            let pad = checksum_and_padding(&vec![0; i]).1;
            assert!(pad < 4);
            assert!((i + pad as usize) % 4 == 0, "pad {i} +{pad} bytes");
        }
    }
}
