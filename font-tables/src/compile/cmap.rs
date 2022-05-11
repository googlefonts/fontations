//! what does a builder for cmap look like?

#![allow(dead_code)]

use font_types::Offset32;

use super::{FontWrite, OffsetMarker, TableWriter};
use crate::tables::cmap::PlatformId;

/// currently just for my edification
#[derive(Default)]
struct CmapBuilder {
    writer: TableWriter,
    records: Vec<CmapRecord>,
}

#[derive(Clone)]
struct CmapRecord {
    platform_id: PlatformId,
    encoding_id: u16,
    offset: OffsetMarker<Offset32>,
}

impl CmapBuilder {
    pub fn add_subtable(
        &mut self,
        platform_id: PlatformId,
        encoding_id: u16,
        table: &dyn FontWrite,
    ) {
        let obj_id = self.writer.add_table(table);
        self.records.push(CmapRecord {
            platform_id,
            encoding_id,
            offset: OffsetMarker::new(obj_id),
        });
    }

    pub fn build(mut self) -> Vec<u8> {
        0u16.write_into(&mut self.writer);
        let len: u16 = self.records.len().try_into().unwrap();
        len.write_into(&mut self.writer);
        for record in &self.records {
            (record.platform_id as u16).write_into(&mut self.writer);
            record.encoding_id.write_into(&mut self.writer);
            self.writer.write_offset_marker(record.offset);
        }

        self.writer.dump()
    }
}

struct FakeCmap0 {
    glyphs: Vec<u8>,
}

impl FontWrite for FakeCmap0 {
    fn write_into(&self, writer: &mut TableWriter) {
        0u16.write_into(writer);
        let length = std::mem::size_of::<u16>() as u16 * 3 + 256;
        length.write_into(writer);
        69u16.write_into(writer);
        writer.write_slice(self.glyphs.as_slice());
    }
}

#[cfg(test)]
mod tests {
    use font_types::{FontRead, OffsetHost};

    use crate::tables::cmap::{Cmap, Cmap0};

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
        builder.build()
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
            let subtable: Cmap0 = cmap
                .resolve_offset(record.subtable_offset())
                .expect("failed to resolve subtable");
            eprintln!("{:x?}", subtable.glyph_id_array());
            eprintln!(
                "  ({:?}, {}) {:?} format {}",
                platform_id,
                encoding_id,
                offset,
                subtable.format(),
            );
        }
    }

    #[test]
    fn does_it_cmap() {
        let cmap_data = make_fake_cmap();
        let cmap = Cmap::read(&cmap_data).unwrap();
        print_cmap_info(&cmap);
        //panic!("ahhh");
    }
}
