#![no_main]
use std::error::Error;

use libfuzzer_sys::fuzz_target;
use skrifa::{raw::TableProvider, FontRef};

fn do_name_things(data: &[u8]) -> Result<(), Box<dyn Error>> {
    let font = FontRef::new(data)?;
    let name = font.name()?;

    for nr in name.name_record() {
        let _ = nr.string(name.string_data()).map(|ns| ns.to_string());
    }

    let _ = name.lang_tag_record();

    Ok(())
}

fuzz_target!(|data: &[u8]| {
    let _ = do_name_things(data);
});
