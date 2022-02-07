mod cmap;
mod glyf;
mod head;
mod loca;
mod maxp;

use crate::*;
pub use cmap::{Cmap, Cmap4, Cmap4Zero, Cmap4ZeroChecked, CmapSubtable};
pub use glyf::{Glyf, GlyphHeader};
pub use head::{Head, HeadZero};
pub use loca::Loca;
pub use maxp::Maxp05;

pub struct FontRef<'a> {
    data: Blob<'a>,
    pub table_directory: TableDirectory<'a>,
}

const TT_MAGIC: u32 = 0x00010000;
const OT_MAGIC: u32 = 0x4F54544F;

#[derive(Clone, Debug, FontThing)]
pub struct TableDirectory<'a> {
    pub sfnt_version: uint32,
    pub num_tables: uint16,
    pub search_range: uint16,
    pub entry_selector: uint16,
    pub range_shift: uint16,
    #[font_thing(count = "num_tables")]
    pub table_records: Array<'a, TableRecord>,
}

/// Record for a table in a font.
#[derive(Clone, Debug, FontThing)]
pub struct TableRecord {
    /// Table identifier.
    pub tag: Tag,
    /// Checksum for the table.
    pub checksum: u32,
    /// Offset from the beginning of the font data.
    pub offset: Offset32,
    /// Length of the table.
    pub len: u32,
}

impl<'a> FontRef<'a> {
    pub fn new(bytes: &'a [u8]) -> Result<Self, u32> {
        let data = Blob::new(bytes);
        let table_directory = TableDirectory::read(data.clone()).ok_or(0x_dead_beef_u32)?;
        if [TT_MAGIC, OT_MAGIC].contains(&table_directory.sfnt_version) {
            Ok(FontRef {
                data,
                table_directory,
            })
        } else {
            Err(table_directory.sfnt_version)
        }
    }

    pub fn table_data(&self, tag: Tag) -> Option<Blob<'a>> {
        self.table_directory
            .table_records
            .binary_search_by(|rec| rec.tag.cmp(&tag))
            .ok()
            .and_then(|idx| self.table_directory.table_records.get(idx))
            .and_then(|record| {
                assert!(record.offset != 0); // that would be confusing
                let start = record.offset as usize;
                self.data.get(start..start + record.len as usize)
            })
    }

    // this isn't in trait just because it's slightly annoying
    pub fn head_zero(&self) -> Option<&'a head::HeadZero> {
        let data = self.table_data(HEAD_TAG)?;
        FontRead::read(data)
    }

    pub fn head_zero_copy(&self) -> Option<head::HeadZero> {
        self.head_zero().cloned()
    }
}

const HEAD_TAG: Tag = [b'h', b'e', b'a', b'd'];
const MAXP_TAG: Tag = [b'm', b'a', b'x', b'p'];
const LOCA_TAG: Tag = [b'l', b'o', b'c', b'a'];
const GLYF_TAG: Tag = [b'g', b'l', b'y', b'f'];
const CMAP_TAG: Tag = [b'c', b'm', b'a', b'p'];

pub trait TableProvider {
    fn head(&self) -> Option<head::Head>;
    fn maxp(&self) -> Option<maxp::Maxp05>;
    fn loca(&self, is_32_bit: bool) -> Option<Loca>;
    fn glyf(&self) -> Option<Glyf>;
    fn cmap(&self) -> Option<Cmap>;
}

pub trait TableProviderRef {
    fn head_ref(&self) -> Option<head::HeadDerivedView>;
    fn maxp_ref(&self) -> Option<maxp::Maxp05DerivedView>;
    //NOTE: These tables are already always views
    //fn loca(&self, is_32_bit: bool) -> Option<Loca>;
    //fn glyf(&self) -> Option<Glyf>;
    //fn cmap(&self) -> Option<Cmap>;
}

impl TableProvider for FontRef<'_> {
    fn head(&self) -> Option<Head> {
        let data = self.table_data(HEAD_TAG);
        data.and_then(Head::read)
    }

    fn maxp(&self) -> Option<Maxp05> {
        let data = self.table_data(MAXP_TAG);
        data.and_then(Maxp05::read)
    }

    fn loca(&self, is_32_bit: bool) -> Option<Loca> {
        let data = self.table_data(LOCA_TAG)?;
        Loca::new(data, is_32_bit)
    }

    fn glyf(&self) -> Option<Glyf> {
        self.table_data(GLYF_TAG).and_then(Glyf::new)
    }
    fn cmap(&self) -> Option<Cmap<'_>> {
        self.table_data(CMAP_TAG).and_then(Cmap::read)
    }
}

impl<'a> TableProviderRef for FontRef<'a> {
    fn head_ref(&self) -> Option<head::HeadDerivedView<'a>> {
        self.table_data(HEAD_TAG)
            .and_then(head::HeadDerivedView::read)
    }

    fn maxp_ref(&self) -> Option<maxp::Maxp05DerivedView<'a>> {
        todo!()
    }
}
