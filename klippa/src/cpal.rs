//! impl subset() for CPAL table

use crate::{NameIdClosure, Plan};
use write_fonts::read::{tables::cpal::Cpal, types::NameId};

impl NameIdClosure for Cpal<'_> {
    fn collect_name_ids(&self, plan: &mut Plan) {
        if self.version() == 0 {
            return;
        }

        if let Some(Ok(palette_labels)) = self.palette_labels_array() {
            plan.name_ids
                .extend_unsorted(palette_labels.iter().map(|x| NameId::from(x.get())));
        }

        if let Some(Ok(palette_entry_labels)) = self.palette_entry_labels_array() {
            plan.name_ids.extend_unsorted(
                palette_entry_labels
                    .iter()
                    .enumerate()
                    .filter(|x| plan.colr_palettes.contains_key(&(x.0 as u16)))
                    .map(|x| x.1.get()),
            );
        }
    }
}
