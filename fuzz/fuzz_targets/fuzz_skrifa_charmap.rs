#![no_main]
use std::error::Error;

use libfuzzer_sys::fuzz_target;
use skrifa::{FontRef, MetadataProvider};

fn do_skrifa_things(data: &[u8]) -> Result<(), Box<dyn Error>> {
    let font = FontRef::new(data)?;
    let charmap = font.charmap();

    // we don't care about the result, just that we don't panic, hang, etc

    let _ = charmap.has_map();
    let _ = charmap.is_symbol();
    let _ = charmap.has_variant_map();

    let _ = charmap.mappings().count();
    let _ = charmap.variant_mappings().count();

    Ok(())
}

fuzz_target!(|data: &[u8]| {
    let _ = do_skrifa_things(data);
});
