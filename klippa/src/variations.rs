//! impl Subset for OpenType font variations common tables.
use crate::inc_bimap::IncBiMap;
use crate::serialize::{SerializeErrorFlags, Serializer};
use crate::SubsetFlags;
use crate::{Plan, Subset, SubsetError};
use skrifa::raw::tables::variations::ItemVariationData;
use write_fonts::types::Offset32;
use write_fonts::{
    read::{collections::IntSet, tables::variations::ItemVariationStore, FontRef, TopLevelTable},
    types::Tag,
    FontBuilder,
};

pub(crate) fn subset_item_varstore(
    var_store: &ItemVariationStore,
    s: &mut Serializer,
    inner_maps: &[IncBiMap],
    table_tag: Tag,
) -> Result<(), SubsetError> {
    s.embed(var_store.format())
        .map_err(|_| SubsetError::SubsetTableError(table_tag))?;

    let regions_offset_pos = s
        .embed(Offset32::new(0))
        .map_err(|_| SubsetError::SubsetTableError(table_tag))?;

    let vardata_count = inner_maps.iter().filter(|m| m.len() > 0).count() as u16;
    s.embed(vardata_count)
        .map_err(|_| SubsetError::SubsetTableError(table_tag))?;

    let regions = var_store
        .variation_region_list()
        .map_err(|_| SubsetError::SubsetTableError(table_tag))?;

    let var_data_array = var_store.item_variation_data();
    let mut region_indices = IntSet::empty();
    for (i, inc_bimap) in inner_maps.iter().enumerate() {
        match var_data_array.get(i) {
            Some(Ok(var_data)) => {
                collect_region_refs(&var_data, &inner_maps[i], &mut region_indices)
            }
            None => continue,
            Some(Err(e)) => {
                return Err(SubsetError::SubsetTableError(table_tag));
            }
        }
    }

    Ok(())
}

fn collect_region_refs(
    var_data: &ItemVariationData,
    inner_map: &IncBiMap,
    region_indices: &mut IntSet<u16>,
) {
}
