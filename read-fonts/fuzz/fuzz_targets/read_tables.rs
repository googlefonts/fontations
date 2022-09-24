#![no_main]
use libfuzzer_sys::fuzz_target;

use read_fonts::{FontData, FontRef, TableProvider};

fuzz_target!(|data: &[u8]| {
    let data = FontData::new(&data);
    if let Ok(font) = FontRef::new(data) {
        let _ = font.cmap();
        let _ = font.gdef();
        let _ = font.glyf();
        let _ = font.gpos();
        let _ = font.gsub();
        let _ = font.head();
        let _ = font.hhea();
        let _ = font.hmtx();
        let _ = font.loca(None);
        let _ = font.maxp();
        let _ = font.name();
        let _ = font.post();
    };
});
