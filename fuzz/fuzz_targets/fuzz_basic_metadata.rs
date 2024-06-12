#![no_main]
use std::error::Error;

use libfuzzer_sys::fuzz_target;
use skrifa::{raw::TableProvider, FontRef};

fn do_metadata_things(data: &[u8]) -> Result<(), Box<dyn Error>> {
    let font = FontRef::new(data)?;

    if let Ok(os2) = font.os2() {
        let _ = os2.fs_selection();
        let _ = os2.fs_type();
    }
    if let Ok(post) = font.post() {
        let _ = post.is_fixed_pitch();
    }

    if let Ok(hhea) = font.hhea() {
        let _ = hhea.number_of_long_metrics();
        if let Ok(hmtx) = font.hmtx() {
            let _ = hmtx.h_metrics().iter().count();
            let _ = hmtx.left_side_bearings().iter().count();
        }
    }

    Ok(())
}

fuzz_target!(|data: &[u8]| {
    let _ = do_metadata_things(data);
});
