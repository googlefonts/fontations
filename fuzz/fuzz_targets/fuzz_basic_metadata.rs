#![no_main]
use std::error::Error;

use libfuzzer_sys::fuzz_target;
use skrifa::raw::TableProvider;

mod helpers;
use helpers::*;

fn do_metadata_things(data: &[u8]) -> Result<(), Box<dyn Error>> {
    let font = select_font(data)?;
    if let Ok(avar) = font.avar() {
        let _ = avar.version();
        let _ = avar.axis_count();
        for seg_map in avar.axis_segment_maps().iter().filter_map(Result::ok) {
            let _ = seg_map.position_map_count.get();
            let _ = seg_map.axis_value_maps();
        }
    }

    if let Ok(os2) = font.os2() {
        let _ = os2.fs_selection();
        let _ = os2.fs_type();
    }
    if let Ok(post) = font.post() {
        let _ = post.is_fixed_pitch();
    }

    if let Ok(hhea) = font.hhea() {
        let _ = hhea.number_of_h_metrics();
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
