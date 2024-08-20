//! impl subset() for hmtx

use crate::{Plan, SubsetError, SubsetError::SubsetTableError};
use write_fonts::types::{FWord, GlyphId, UfWord};
use write_fonts::{
    read::{
        collections::IntSet,
        tables::{hhea::Hhea, hmtx::Hmtx},
        FontRef, TableProvider, TopLevelTable,
    },
    FontBuilder,
};

// reference: subset() for hmtx/hhea in harfbuzz
// https://github.com/harfbuzz/harfbuzz/blob/main/src/hb-ot-hmtx-table.hh#L214
pub(crate) fn subset_hmtx_hhea(
    font: &FontRef,
    plan: &Plan,
    builder: &mut FontBuilder,
) -> Result<(), SubsetError> {
    let hmtx = font.hmtx().or(Err(SubsetTableError(Hmtx::TAG)))?;
    let h_metrics = hmtx.h_metrics();
    let side_bearings = hmtx.left_side_bearings();

    let last_gid = plan.num_output_glyphs - 1;
    if last_gid >= h_metrics.len() + side_bearings.len() {
        return Err(SubsetTableError(Hmtx::TAG));
    }

    let new_num_h_metrics =
        compute_new_num_h_metrics(&hmtx, &plan.glyphset, plan.num_output_glyphs);
    //subsetted hmtx table length
    let hmtx_cap = new_num_h_metrics * 4 + (plan.num_output_glyphs - new_num_h_metrics) * 2;
    let mut hmtx_out = vec![0; hmtx_cap];

    for (new_gid, old_gid) in &plan.new_to_old_gid_list {
        let new_gid = new_gid.to_u32() as usize;
        if new_gid < new_num_h_metrics {
            let idx = 4 * new_gid;
            let advance = UfWord::from(hmtx.advance(*old_gid).unwrap());
            hmtx_out
                .get_mut(idx..idx + 2)
                .unwrap()
                .copy_from_slice(&advance.to_be_bytes());

            let lsb = FWord::from(hmtx.side_bearing(*old_gid).unwrap());
            hmtx_out
                .get_mut(idx + 2..idx + 4)
                .unwrap()
                .copy_from_slice(&lsb.to_be_bytes());
        } else {
            let idx = 4 * new_num_h_metrics + (new_gid - new_num_h_metrics) * 2;
            let lsb = FWord::from(hmtx.side_bearing(*old_gid).unwrap());
            hmtx_out
                .get_mut(idx..idx + 2)
                .unwrap()
                .copy_from_slice(&lsb.to_be_bytes());
        }
    }

    let Ok(hhea) = font.hhea() else {
        builder.add_raw(Hmtx::TAG, hmtx_out);
        return Ok(());
    };

    let mut hhea_out = hhea.offset_data().as_bytes().to_owned();
    let new_num_h_metrics = (new_num_h_metrics as u16).to_be_bytes();
    hhea_out
        .get_mut(34..36)
        .unwrap()
        .copy_from_slice(&new_num_h_metrics);

    builder.add_raw(Hmtx::TAG, hmtx_out);
    builder.add_raw(Hhea::TAG, hhea_out);
    Ok(())
}

fn compute_new_num_h_metrics(
    hmtx: &Hmtx,
    gid_set: &IntSet<GlyphId>,
    num_output_glyphs: usize,
) -> usize {
    let mut num_long_metrics = num_output_glyphs.min(0xFFFF) as u32;
    let last_gid = num_long_metrics - 1;
    let last_advance = hmtx.advance(GlyphId::from(last_gid)).unwrap();

    while num_long_metrics > 1 {
        let gid = GlyphId::from(num_long_metrics - 2);
        let advance = gid_set
            .contains(gid)
            .then(|| hmtx.advance(gid).unwrap())
            .unwrap_or(0);
        if advance != last_advance {
            break;
        }
        num_long_metrics -= 1;
    }
    num_long_metrics as usize
}
