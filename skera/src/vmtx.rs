//! impl subset() for vmtx

use crate::serialize::Serializer;
use crate::{Plan, Subset, SubsetError, SubsetError::SubsetTableError};
use skrifa::raw::tables::mvar::tags::{VCOF, VCRN, VCRS};
use write_fonts::from_obj::ToOwnedTable;
use write_fonts::types::{FWord, GlyphId, UfWord};
use write_fonts::{
    read::{tables::vmtx::Vmtx, FontRef, TableProvider, TopLevelTable},
    FontBuilder,
};

// reference: subset() for vmtx/vhea in harfbuzz
// https://github.com/harfbuzz/harfbuzz/blob/a070f9ebbe88dc71b248af9731dd49ec93f4e6e6/src/hb-ot-hmtx-table.hh#L214
impl Subset for Vmtx<'_> {
    fn subset(
        &self,
        plan: &Plan,
        font: &FontRef,
        s: &mut Serializer,
        builder: &mut FontBuilder,
    ) -> Result<(), SubsetError> {
        let v_metrics = self.v_metrics();
        let side_bearings = self.top_side_bearings();

        let last_gid = plan.num_output_glyphs - 1;
        if last_gid >= v_metrics.len() + side_bearings.len() {
            return Err(SubsetTableError(Vmtx::TAG));
        }

        let new_num_v_metrics = compute_new_num_v_metrics(self, plan);
        //subsetted vmtx table length
        let vmtx_cap = new_num_v_metrics * 4 + (plan.num_output_glyphs - new_num_v_metrics) * 2;
        s.allocate_size(vmtx_cap, false)
            .map_err(|_| SubsetError::SubsetTableError(Vmtx::TAG))?;

        for (new_gid, old_gid) in &plan.new_to_old_gid_list {
            let new_gid_usize = new_gid.to_u32() as usize;
            let old_tsb = self.side_bearing(*old_gid).unwrap_or(0);
            let tsb = if plan.normalized_coords.is_empty() {
                old_tsb
            } else {
                plan.vmtx_map
                    .borrow()
                    .get(new_gid)
                    .map(|(_, tsb)| *tsb)
                    .unwrap_or(old_tsb)
            };
            if new_gid_usize < new_num_v_metrics {
                let idx = 4 * new_gid_usize;
                let old_advance = self.advance(*old_gid).unwrap_or(0);
                let advance = if plan.normalized_coords.is_empty() {
                    old_advance
                } else {
                    plan.vmtx_map
                        .borrow()
                        .get(new_gid)
                        .map(|(aw, _)| *aw)
                        .unwrap_or(old_advance)
                };
                s.copy_assign(idx, UfWord::from(advance));

                s.copy_assign(idx + 2, FWord::from(tsb));
            } else {
                let idx = 4 * new_num_v_metrics + (new_gid_usize - new_num_v_metrics) * 2;
                s.copy_assign(idx, FWord::from(tsb));
            }
        }

        let Ok(vhea) = font.vhea() else {
            return Ok(());
        };

        let mut vhea_out: write_fonts::tables::vhea::Vhea = vhea.to_owned_table();
        vhea_out.number_of_long_ver_metrics = new_num_v_metrics as u16;

        if !plan.normalized_coords.is_empty() {
            vhea_out.caret_slope_rise +=
                plan.mvar_entries.get(&VCRS).cloned().unwrap_or(0.0) as i16;
            vhea_out.caret_slope_run += plan.mvar_entries.get(&VCRN).cloned().unwrap_or(0.0) as i16;
            vhea_out.caret_offset += plan.mvar_entries.get(&VCOF).cloned().unwrap_or(0.0) as i16;

            let mut empty = true;
            let mut min_tsb = 0x7FFF;
            let mut min_bsb: i16 = 0x7FFF;
            let mut max_extent = -0x7FFF;
            let mut max_adv = 0;
            let bounds = plan.bounds_width_vec.borrow();
            for (gid, &(advance, tsb)) in plan.hmtx_map.borrow().iter() {
                max_adv = max_adv.max(advance);
                if let Some(&bound_width) = bounds.get(gid) {
                    empty = false;
                    let bsb: i16 = (advance as i16) - tsb - (bound_width as i16);
                    let extent = tsb + (bound_width as i16);
                    min_tsb = min_tsb.min(tsb);
                    min_bsb = min_bsb.min(bsb);
                    max_extent = max_extent.max(extent);
                }
            }
            vhea_out.advance_height_max = UfWord::new(max_adv);
            if !empty {
                vhea_out.min_top_side_bearing = FWord::new(min_tsb);
                vhea_out.min_bottom_side_bearing = FWord::new(min_bsb);
                vhea_out.y_max_extent = FWord::new(max_extent);
            }
        }
        builder
            .add_table(&vhea_out)
            .map_err(|_| SubsetTableError(Vmtx::TAG))?;
        Ok(())
    }
}

fn compute_new_num_v_metrics(vmtx: &Vmtx, plan: &Plan) -> usize {
    let mut num_long_metrics = plan.num_output_glyphs.min(0xFFFF);
    let last_advance = get_new_gid_advance(vmtx, GlyphId::from(num_long_metrics as u32 - 1), plan);

    while num_long_metrics > 1 {
        let advance = get_new_gid_advance(vmtx, GlyphId::from(num_long_metrics as u32 - 2), plan);
        if advance != last_advance {
            break;
        }
        num_long_metrics -= 1;
    }
    num_long_metrics
}

fn get_new_gid_advance(vmtx: &Vmtx, new_gid: GlyphId, plan: &Plan) -> u16 {
    let Some(old_gid) = plan.reverse_glyph_map.get(&new_gid) else {
        return 0;
    };
    vmtx.advance(*old_gid).unwrap_or(0)
}
