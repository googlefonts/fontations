#![no_main]
use std::error::Error;

use libfuzzer_sys::fuzz_target;
use skrifa::{
    charmap::{Charmap, MappingIndex},
    FontRef, MetadataProvider,
};

const CJK_VARIATION_1: u32 = 0xFE00;
const CJK_VARIATION_2: u32 = 0xFE01;

fn do_charmap_things(charmap: Charmap<'_>) {
    let _ = charmap.has_map();
    let _ = charmap.is_symbol();
    let _ = charmap.has_variant_map();

    for (cp, _) in charmap.mappings() {
        let _ = charmap.map(cp);
        let _ = charmap.map_variant(cp, CJK_VARIATION_1);
        let _ = charmap.map_variant(cp, CJK_VARIATION_2);
    }
    let _ = charmap.variant_mappings().count();
}

fn do_skrifa_things(data: &[u8]) -> Result<(), Box<dyn Error>> {
    let font = FontRef::new(data)?;

    // we don't care about the result, just that we don't panic, hang, etc
    do_charmap_things(font.charmap());
    do_charmap_things(MappingIndex::new(&font).charmap(&font));

    Ok(())
}

fuzz_target!(|data: &[u8]| {
    let _ = do_skrifa_things(data);
});
