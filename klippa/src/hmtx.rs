//! impl subset() for hmtx

use crate::{estimate_subset_table_size, Plan, SubsetError, SubsetError::SubsetTableError};
use write_fonts::types::{FWord, GlyphId, UfWord};
use write_fonts::{
    read::{
        collections::IntSet,
        tables::{hhea::Hhea, hmtx::Hmtx},
        FontRef, TableProvider, TopLevelTable,
    },
    FontBuilder,
};

pub fn subset_hmtx_hhea(
    font: &FontRef,
    plan: &Plan,
    builder: &mut FontBuilder,
) -> Result<(), SubsetError> {
    let Ok(hmtx) = font.hmtx() else {
        return Err(SubsetTableError(Hmtx::TAG));
    };

    let gids = &plan.glyphset;
    let Some(last_gid) = gids.last() else {
        return Err(SubsetTableError(Hmtx::TAG));
    };

    let h_metrics = hmtx.h_metrics();
    let side_bearings = hmtx.left_side_bearings();
    if last_gid.to_u32() as usize >= h_metrics.len() + side_bearings.len() {
        return Err(SubsetTableError(Hmtx::TAG));
    }

    let hmtx_cap = estimate_subset_table_size(font, Hmtx::TAG, plan);
    let mut hmtx_out = Vec::with_capacity(hmtx_cap);
    let new_num_h_metrics = compute_new_num_h_metrics(&hmtx, gids);

    for (new_gid, old_gid) in &plan.new_to_old_gid_list {
        if (new_gid.to_u32() as usize) < new_num_h_metrics {
            let advance = UfWord::from(hmtx.advance(*old_gid).unwrap());
            let lsb = FWord::from(hmtx.side_bearing(*old_gid).unwrap());
            hmtx_out.extend_from_slice(&advance.to_be_bytes());
            hmtx_out.extend_from_slice(&lsb.to_be_bytes());
        } else {
            let lsb = FWord::from(hmtx.side_bearing(*old_gid).unwrap());
            hmtx_out.extend_from_slice(&lsb.to_be_bytes());
        }
    }

    let Ok(hhea) = font.hhea() else {
        builder.add_raw(Hmtx::TAG, hmtx_out);
        return Ok(());
    };

    let mut hhea_out = Vec::with_capacity(hhea.offset_data().len());
    hhea_out.extend_from_slice(hhea.offset_data().as_bytes());
    let Some(index_num_h_metrics) = hhea_out.get_mut(34..36) else {
        return Err(SubsetTableError(Hhea::TAG));
    };
    let new_num_h_metrics = (new_num_h_metrics as u16).to_be_bytes();
    index_num_h_metrics.clone_from_slice(&new_num_h_metrics);

    builder.add_raw(Hmtx::TAG, hmtx_out);
    builder.add_raw(Hhea::TAG, hhea_out);
    Ok(())
}

fn compute_new_num_h_metrics(hmtx: &Hmtx, gids: &IntSet<GlyphId>) -> usize {
    let num_long_metrics = gids.len().min(0xFFFF);
    let last_gid = gids.last().unwrap();
    let last_advance = hmtx.advance(last_gid).unwrap();

    let num_skippable_glyphs = gids
        .iter()
        .rev()
        .take_while(|gid| hmtx.advance(*gid).unwrap() == last_advance)
        .count();
    (num_long_metrics - num_skippable_glyphs + 1).max(1)
}
