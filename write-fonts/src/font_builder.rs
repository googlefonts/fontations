//!  A builder for top-level font objects

use std::collections::BTreeMap;
use std::{borrow::Cow, fmt::Display};

use read_fonts::{FontRef, TableProvider};
use types::{Tag, TT_SFNT_VERSION};

use crate::util::SearchRange;

include!("../generated/generated_font.rs");

const TABLE_RECORD_LEN: usize = 16;

/// Build a font from some set of tables.
#[derive(Debug, Clone, Default)]
pub struct FontBuilder<'a> {
    tables: BTreeMap<Tag, Cow<'a, [u8]>>,
}

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

impl TableDirectory {
    pub fn from_table_records(table_records: Vec<TableRecord>) -> TableDirectory {
        assert!(table_records.len() <= u16::MAX as usize);
        // See https://learn.microsoft.com/en-us/typography/opentype/spec/otff#table-directory
        let computed = SearchRange::compute(table_records.len(), TABLE_RECORD_LEN);

        TableDirectory::new(
            TT_SFNT_VERSION,
            computed.search_range,
            computed.entry_selector,
            computed.range_shift,
            table_records,
        )
    }
}

impl<'a> FontBuilder<'a> {
    /// Create a new builder to compile a binary font
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a table to the builder.
    ///
    /// The table can be any top-level table defined in this crate. This function
    /// will attempt to compile the table and then add it to the builder if
    /// successful, returning an error otherwise.
    pub fn add_table<T>(&mut self, table: &T) -> Result<&mut Self, BuilderError>
    where
        T: FontWrite + Validate + TopLevelTable,
    {
        let tag = T::TAG;
        let bytes = crate::dump_table(table).map_err(|inner| BuilderError { inner, tag })?;
        Ok(self.add_raw(tag, bytes))
    }

    /// A builder method to add raw data for the provided tag
    pub fn add_raw(&mut self, tag: Tag, data: impl Into<Cow<'a, [u8]>>) -> &mut Self {
        self.tables.insert(tag, data.into());
        self
    }

    /// Copy each table from the source font if it does not already exist
    pub fn copy_missing_tables(&mut self, font: FontRef<'a>) -> &mut Self {
        for record in font.table_directory.table_records() {
            let tag = record.tag();
            if !self.tables.contains_key(&tag) {
                if let Some(data) = font.data_for_tag(tag) {
                    self.add_raw(tag, data);
                } else {
                    log::warn!("data for '{tag}' is malformed");
                }
            }
        }
        self
    }

    /// Returns `true` if the builder contains a table with this tag.
    pub fn contains(&self, tag: Tag) -> bool {
        self.tables.contains_key(&tag)
    }

    /// Assemble all the tables into a binary font file with a [Table Directory].
    ///
    /// [Table Directory]: https://learn.microsoft.com/en-us/typography/opentype/spec/otff#table-directory
    pub fn build(&mut self) -> Vec<u8> {
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

        let directory = TableDirectory::from_table_records(table_records);

        let mut writer = TableWriter::default();
        directory.write_into(&mut writer);
        let mut data = writer.into_data().bytes;
        for table in self.tables.values() {
            data.extend_from_slice(table);
            let rem = round4(table.len()) - table.len();
            let padding = [0u8; 4];
            data.extend_from_slice(&padding[..rem]);
        }
        data
    }
}

/// <https://github.com/google/woff2/blob/a0d0ed7da27b708c0a4e96ad7a998bddc933c06e/src/round.h#L19>
fn round4(sz: usize) -> usize {
    (sz + 3) & !3
}

fn checksum_and_padding(table: &[u8]) -> (u32, u32) {
    let checksum = read_fonts::tables::compute_checksum(table);
    let padding = round4(table.len()) - table.len();
    (checksum, padding as u32)
}

impl TTCHeader {
    fn compute_version(&self) -> MajorMinor {
        panic!("TTCHeader writing not supported (yet)")
    }
}

impl Display for BuilderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "failed to build '{}' table: '{}'", self.tag, self.inner)
    }
}

impl std::error::Error for BuilderError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.inner)
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
            builder.add_raw(Tag::from_be_bytes(i.to_ne_bytes()), &data);
        });
        let bytes = builder.build();
        let font = FontRef::new(&bytes).unwrap();
        let td = font.table_directory;
        assert_eq!(
            (256, 4, 96),
            (td.search_range(), td.entry_selector(), td.range_shift())
        );
    }

    #[test]
    fn survives_no_tables() {
        FontBuilder::default().build();
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
