#![no_main]
use libfuzzer_sys::fuzz_target;

use read_fonts::{FontData, FontRef, TableProvider};

fuzz_target!(|data: &[u8]| {
    let data = FontData::new(&data);
    if let Ok(font) = FontRef::new(data) {
        for tag in font
            .table_directory
            .table_records()
            .iter()
            .map(|rec| rec.tag())
        {
            match tag {
                read_fonts::tables::gpos::TAG => font.gpos().map(|_| ()).unwrap_or(()),
                read_fonts::tables::gsub::TAG => font.gsub().map(|_| ()).unwrap_or(()),
                read_fonts::tables::cmap::TAG => font.cmap().map(|_| ()).unwrap_or(()),
                read_fonts::tables::gdef::TAG => font.gdef().map(|_| ()).unwrap_or(()),
                read_fonts::tables::glyf::TAG => font.glyf().map(|_| ()).unwrap_or(()),
                read_fonts::tables::head::TAG => font.head().map(|_| ()).unwrap_or(()),
                read_fonts::tables::hhea::TAG => font.hhea().map(|_| ()).unwrap_or(()),
                read_fonts::tables::hmtx::TAG => font.hmtx().map(|_| ()).unwrap_or(()),
                read_fonts::tables::loca::TAG => font.loca(None).map(|_| ()).unwrap_or(()),
                read_fonts::tables::maxp::TAG => font.maxp().map(|_| ()).unwrap_or(()),
                read_fonts::tables::name::TAG => font.name().map(|_| ()).unwrap_or(()),
                read_fonts::tables::post::TAG => font.post().map(|_| ()).unwrap_or(()),
                _ => (),
            }
        }
    };
});
