//! impl subset() for MVAR

use crate::{
    offset::SerializeCopy, serialize::Serializer, variations::subset_itemvarstore_with_instancing,
    IncBiMap, Plan, Subset, SubsetError,
};
use fnv::FnvHashMap;
use font_types::Offset16;
use write_fonts::{
    read::{
        collections::IntSet,
        tables::{mvar::Mvar, variations::DeltaSetIndex},
        FontRef, TopLevelTable,
    },
    FontBuilder,
};

// reference: subset() for MVAR in harfbuzz
// <https://github.com/harfbuzz/harfbuzz/blob/bcd5aa368d3fd3ef741ea29df15d3d56011811c0/src/hb-ot-var-mvar-table.hh#L330>
impl Subset for Mvar<'_> {
    fn subset(
        &self,
        plan: &Plan,
        _font: &FontRef,
        s: &mut Serializer,
        _builder: &mut FontBuilder,
    ) -> Result<(), SubsetError> {
        if plan.all_axes_pinned {
            return Ok(());
        }
        s.embed(self.version())
            .map_err(|_| SubsetError::SubsetTableError(Mvar::TAG))?;
        s.embed(0_u16)
            .map_err(|_| SubsetError::SubsetTableError(Mvar::TAG))?; // reserved
        s.embed(self.value_record_size())
            .map_err(|_| SubsetError::SubsetTableError(Mvar::TAG))?;
        s.embed(self.value_record_count())
            .map_err(|_| SubsetError::SubsetTableError(Mvar::TAG))?;

        let var_store = self
            .item_variation_store()
            .transpose()
            .map_err(|_| SubsetError::SubsetTableError(Mvar::TAG))?;

        let var_store_offset_pos = s
            .embed(0_u16)
            .map_err(|_| SubsetError::SubsetTableError(Mvar::TAG))?;

        let mut inner_maps: Vec<IncBiMap> = Vec::new();
        let mut varidx_map: FnvHashMap<u32, u32> = FnvHashMap::default();
        if let Some(ref var_store) = var_store {
            let vardata_count = var_store.item_variation_data_count() as usize;
            let mut inner_sets = vec![IntSet::empty(); vardata_count];
            for value_record in self.value_records() {
                let outer = value_record.delta_set_outer_index() as usize;
                if outer >= inner_sets.len() {
                    continue;
                }
                inner_sets[outer].insert(value_record.delta_set_inner_index());
            }
            inner_maps = inner_sets
                .iter()
                .map(|set| set.iter().map(|idx| idx as u32).collect::<IncBiMap>())
                .collect();
        }

        if let Some(var_store) = var_store {
            if !plan.normalized_coords.is_empty() {
                let (bytes, new_varidx_map) = subset_itemvarstore_with_instancing(
                    var_store.clone(),
                    plan,
                    s,
                    &inner_maps,
                    true,
                )
                .map_err(|_| SubsetError::SubsetTableError(Mvar::TAG))?;
                varidx_map = new_varidx_map;
                Offset16::serialize_copy_from_bytes(&bytes, s, var_store_offset_pos)
                    .map_err(|_| SubsetError::SubsetTableError(Mvar::TAG))?;
            } else {
                Offset16::serialize_copy(&var_store, s, var_store_offset_pos)
                    .map_err(|_| SubsetError::SubsetTableError(Mvar::TAG))?;
            }
        }
        for value_record in self.value_records() {
            s.embed(value_record.value_tag())
                .map_err(|_| SubsetError::SubsetTableError(Mvar::TAG))?;

            let old_outer = value_record.delta_set_outer_index();
            let old_inner = value_record.delta_set_inner_index();
            let old_varidx = ((old_outer as u32) << 16) | old_inner as u32;
            let new_varidx = if varidx_map.is_empty() {
                old_varidx
            } else {
                varidx_map.get(&old_varidx).copied().unwrap_or(u32::MAX)
            };

            let (new_outer, new_inner) = if new_varidx == u32::MAX {
                (
                    DeltaSetIndex::NO_VARIATION_INDEX.outer,
                    DeltaSetIndex::NO_VARIATION_INDEX.inner,
                )
            } else {
                ((new_varidx >> 16) as u16, (new_varidx & 0xFFFF) as u16)
            };

            s.embed(new_outer)
                .map_err(|_| SubsetError::SubsetTableError(Mvar::TAG))?;
            s.embed(new_inner)
                .map_err(|_| SubsetError::SubsetTableError(Mvar::TAG))?;
        }

        Ok(())
    }
}
