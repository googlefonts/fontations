//! impl subset() for hmtx

use crate::{serialize::Serializer, Plan, Subset, SubsetError, SubsetError::SubsetTableError};
use skrifa::raw::tables::mvar::tags::{HCOF, HCRN, HCRS};
use write_fonts::{
    from_obj::ToOwnedTable,
    read::{tables::hmtx::Hmtx, FontRef, TableProvider, TopLevelTable},
    types::{FWord, GlyphId, UfWord},
    FontBuilder,
};

// reference: subset() for hmtx/hhea in harfbuzz
// https://github.com/harfbuzz/harfbuzz/blob/a070f9ebbe88dc71b248af9731dd49ec93f4e6e6/src/hb-ot-hmtx-table.hh#L214
impl Subset for Hmtx<'_> {
    fn subset(
        &self,
        plan: &Plan,
        font: &FontRef,
        s: &mut Serializer,
        builder: &mut FontBuilder,
    ) -> Result<(), SubsetError> {
        let h_metrics = self.h_metrics();
        let side_bearings = self.left_side_bearings();

        let last_gid = plan.num_output_glyphs - 1;
        if last_gid >= h_metrics.len() + side_bearings.len() {
            return Err(SubsetTableError(Hmtx::TAG));
        }

        let new_num_h_metrics = compute_new_num_h_metrics(self, plan);
        //subsetted hmtx table length
        let hmtx_cap = new_num_h_metrics * 4 + (plan.num_output_glyphs - new_num_h_metrics) * 2;
        s.allocate_size(hmtx_cap, false)
            .map_err(|_| SubsetError::SubsetTableError(Hmtx::TAG))?;

        for (new_gid, old_gid) in &plan.new_to_old_gid_list {
            let new_gid_usize = new_gid.to_u32() as usize;
            let old_lsb = self.side_bearing(*old_gid).unwrap_or(0);
            let lsb = if plan.normalized_coords.is_empty() {
                old_lsb
            } else {
                plan.hmtx_map
                    .borrow()
                    .get(new_gid)
                    .map(|(_, lsb)| *lsb)
                    .unwrap_or(old_lsb)
            };
            if new_gid_usize < new_num_h_metrics {
                let idx = 4 * new_gid_usize;
                let old_advance = self.advance(*old_gid).unwrap_or(0);
                let advance = if plan.normalized_coords.is_empty() {
                    old_advance
                } else {
                    plan.hmtx_map
                        .borrow()
                        .get(new_gid)
                        .map(|(aw, _)| *aw)
                        .unwrap_or(old_advance)
                };
                s.copy_assign(idx, UfWord::from(advance));

                s.copy_assign(idx + 2, FWord::from(lsb));
            } else {
                let idx = 4 * new_num_h_metrics + (new_gid_usize - new_num_h_metrics) * 2;
                s.copy_assign(idx, FWord::from(lsb));
            }
        }

        let Ok(hhea) = font.hhea() else {
            return Ok(());
        };

        let mut hhea_out: write_fonts::tables::hhea::Hhea = hhea.to_owned_table();
        hhea_out.number_of_h_metrics = new_num_h_metrics as u16;

        if !plan.normalized_coords.is_empty() {
            hhea_out.caret_slope_rise +=
                plan.mvar_entries.get(&HCRS).cloned().unwrap_or(0.0) as i16;
            hhea_out.caret_slope_run += plan.mvar_entries.get(&HCRN).cloned().unwrap_or(0.0) as i16;
            hhea_out.caret_offset += plan.mvar_entries.get(&HCOF).cloned().unwrap_or(0.0) as i16;

            let mut empty = true;
            let mut min_lsb = 0x7FFF;
            let mut min_rsb: i16 = 0x7FFF;
            let mut max_extent = -0x7FFF;
            let mut max_adv = 0;
            let bounds = plan.bounds_width_vec.borrow();
            for (gid, &(advance, lsb)) in plan.hmtx_map.borrow().iter() {
                max_adv = max_adv.max(advance);
                if let Some(&bound_width) = bounds.get(gid) {
                    empty = false;
                    let rsb: i16 = (advance as i16) - lsb - (bound_width as i16);
                    let extent = lsb + (bound_width as i16);
                    min_lsb = min_lsb.min(lsb);
                    min_rsb = min_rsb.min(rsb);
                    max_extent = max_extent.max(extent);
                }
            }
            hhea_out.advance_width_max = UfWord::new(max_adv);
            if !empty {
                hhea_out.min_left_side_bearing = FWord::new(min_lsb);
                hhea_out.min_right_side_bearing = FWord::new(min_rsb);
                hhea_out.x_max_extent = FWord::new(max_extent);
            }
        }
        builder
            .add_table(&hhea_out)
            .map_err(|_| SubsetTableError(Hmtx::TAG))?;
        Ok(())
    }
}

fn compute_new_num_h_metrics(hmtx: &Hmtx, plan: &Plan) -> usize {
    let mut num_long_metrics = plan.num_output_glyphs.min(0xFFFF);
    let last_advance = get_new_gid_advance(hmtx, GlyphId::from(num_long_metrics as u32 - 1), plan);

    while num_long_metrics > 1 {
        let advance = get_new_gid_advance(hmtx, GlyphId::from(num_long_metrics as u32 - 2), plan);
        if advance != last_advance {
            break;
        }
        num_long_metrics -= 1;
    }
    num_long_metrics
}

fn get_new_gid_advance(hmtx: &Hmtx, new_gid: GlyphId, plan: &Plan) -> u16 {
    let Some(old_gid) = plan.reverse_glyph_map.get(&new_gid) else {
        return 0;
    };
    hmtx.advance(*old_gid).unwrap_or(0)
}
