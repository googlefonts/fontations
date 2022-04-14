//! what does a builder for cmap look like?

use font_types::Offset32;

use super::{OffsetMarker, OffsetMarker2, Table, TableWriter};
use crate::tables::cmap::PlatformId;

#[derive(Default)]
struct CmapBuilder<'a> {
    //writer: TableWriter,
    records: Vec<CmapRecord<'a>>,
}

#[derive(Clone)]
struct CmapRecord<'a> {
    platform_id: PlatformId,
    encoding_id: u16,
    offset: OffsetMarker2<'a, Offset32>,
}

impl<'a> CmapBuilder<'a> {
    pub fn add_subtable(
        &mut self,
        platform_id: PlatformId,
        encoding_id: u16,
        table: &'a dyn Table,
    ) {
        let offset = OffsetMarker2::new(table);
        self.records.push(CmapRecord {
            platform_id,
            encoding_id,
            offset,
        });
    }
}

impl Table for CmapBuilder<'_> {
    fn describe(&self, writer: &mut TableWriter) {
        writer.write(&0u16.to_be_bytes());
        let len: u16 = self.records.len().try_into().unwrap();
        writer.write(&len.to_be_bytes());
        for record in &self.records {
            writer.write(&(record.platform_id as u16).to_be_bytes());
            writer.write(&record.encoding_id.to_be_bytes());
            writer.write_offset_marker2(record.offset);
        }
    }
}

struct FakeCmap0 {
    glyphs: Vec<u8>,
}

impl Table for FakeCmap0 {
    fn describe(&self, writer: &mut TableWriter) {
        writer.write(&0u16.to_be_bytes());
        let length = std::mem::size_of::<u16>() as u16 * 3 + 256;
        writer.write(&length.to_be_bytes());
        writer.write(&69u16.to_be_bytes());
        writer.write(&self.glyphs);
    }
}

#[cfg(test)]
mod tests {
    use font_types::{BigEndian, FontRead, OffsetHost};

    use crate::tables::cmap::Cmap;

    use super::*;

    fn make_fake_cmap() -> Vec<u8> {
        let glyphs = [0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff, 0x99, 0x88]
            .into_iter()
            .cycle()
            .take(256)
            .collect();
        let cmap0 = FakeCmap0 { glyphs };

        let mut builder = CmapBuilder::default();
        builder.add_subtable(PlatformId::Unicode, 5, &cmap0);
        builder.add_subtable(PlatformId::Macintosh, 2, &cmap0);
        super::super::dump_table(&builder)
    }

    fn print_cmap_info(cmap: &Cmap) {
        eprintln!(
            "\ncmap version {}, {} tables",
            cmap.version(),
            cmap.num_tables()
        );

        for record in cmap.encoding_records() {
            let platform_id = PlatformId::new(record.platform_id());
            let encoding_id = record.encoding_id();
            let offset = record.subtable_offset();
            let format: BigEndian<u16> = cmap
                .resolve_offset(record.subtable_offset())
                .expect("failed to resolve subtable");
            eprintln!(
                "  ({:?}, {}) {:?} format {}",
                platform_id, encoding_id, offset, format
            );
        }
    }

    #[test]
    fn does_it_cmap() {
        let cmap_data = make_fake_cmap();
        let cmap = Cmap::read(&cmap_data).unwrap();
        print_cmap_info(&cmap);
        panic!("ahhh");
    }
}
