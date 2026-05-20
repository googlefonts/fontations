//! OpenType Table Directory and related structures.

use types::{Tag, CFF_SFNT_VERSION, TT_SFNT_VERSION};

use crate::util::SearchRange;

include!("../generated/generated_font.rs");

const TABLE_RECORD_LEN: usize = 16;
const CFF: Tag = Tag::new(b"CFF ");
const CFF2: Tag = Tag::new(b"CFF2");

impl TableDirectory {
    pub fn from_table_records(table_records: Vec<TableRecord>) -> TableDirectory {
        assert!(table_records.len() <= u16::MAX as usize);
        // See https://learn.microsoft.com/en-us/typography/opentype/spec/otff#table-directory
        let computed = SearchRange::compute(table_records.len(), TABLE_RECORD_LEN);

        let is_cff = table_records
            .iter()
            .any(|rec| [CFF, CFF2].contains(&rec.tag));
        let sfnt = if is_cff {
            CFF_SFNT_VERSION
        } else {
            TT_SFNT_VERSION
        };

        TableDirectory::new(
            sfnt,
            computed.search_range,
            computed.entry_selector,
            computed.range_shift,
            table_records,
        )
    }
}

impl TTCHeader {
    fn compute_version(&self) -> MajorMinor {
        panic!("TTCHeader writing not supported (yet)")
    }
}
