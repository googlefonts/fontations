//! impl subset() for hmtx

use crate::{Plan, Subset, SubsetError};
use write_fonts::read::TopLevelTable;
use write_fonts::tables::{hmtx::Hmtx, hmtx::LongMetric};

impl Subset for Hmtx {
    fn subset(&mut self, plan: &Plan) -> Result<bool, SubsetError> {
        let gids = &plan.glyphset;
        if gids.is_empty() {
            return Err(SubsetError::SubsetTableError(Hmtx::TAG));
        }

        let num_long_metrics = plan.num_h_metrics as usize;
        let mut new_metrics = Vec::with_capacity(num_long_metrics);
        let mut new_side_bearings = Vec::new();
        for gid in gids.iter() {
            let glyph_id = gid.to_u32() as usize;
            let side_bearing =
                get_gid_side_bearing(&self.h_metrics, &self.left_side_bearings, glyph_id);
            if glyph_id < num_long_metrics {
                let advance = get_gid_advance(&self.h_metrics, glyph_id);
                new_metrics.push(LongMetric {
                    advance,
                    side_bearing,
                });
            } else {
                new_side_bearings.push(side_bearing);
            }
        }

        self.h_metrics = new_metrics;
        self.left_side_bearings = new_side_bearings;

        Ok(true)
    }
}

fn get_gid_advance(metrics: &[LongMetric], gid: usize) -> u16 {
    metrics.get(gid).or_else(|| metrics.last()).unwrap().advance
}

fn get_gid_side_bearing(metrics: &[LongMetric], side_bearings: &[i16], gid: usize) -> i16 {
    match metrics.get(gid) {
        Some(long_metric) => long_metric.side_bearing,
        None => *side_bearings.get(gid - metrics.len()).unwrap(),
    }
}
