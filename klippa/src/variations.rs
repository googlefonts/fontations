//! impl Subset for OpenType font variations common tables.
use crate::inc_bimap::IncBiMap;
use crate::{
    serialize::{OffsetWhence, SerializeErrorFlags, Serializer},
    SubsetError,
};
use write_fonts::{
    read::{
        collections::IntSet,
        tables::variations::{ItemVariationData, ItemVariationStore, VariationRegionList},
    },
    types::{BigEndian, F2Dot14, FixedSize, Offset32, Tag},
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
    if vardata_count == 0 {
        return Err(SubsetError::SubsetTableError(table_tag));
    }

    s.embed(vardata_count)
        .map_err(|_| SubsetError::SubsetTableError(table_tag))?;

    let regions = var_store
        .variation_region_list()
        .map_err(|_| SubsetError::SubsetTableError(table_tag))?;

    let var_data_array = var_store.item_variation_data();
    let mut region_indices = IntSet::empty();
    for i in 0..inner_maps.iter().len() {
        match var_data_array.get(i) {
            Some(Ok(var_data)) => {
                collect_region_refs(&var_data, &inner_maps[i], &mut region_indices);
            }
            None => continue,
            Some(Err(_e)) => {
                return Err(SubsetError::SubsetTableError(table_tag));
            }
        }
    }

    let max_region_count = regions.region_count();
    region_indices.remove_range(max_region_count..=u16::MAX);

    let mut region_map = IncBiMap::new(region_indices.len().try_into().unwrap());
    for region in region_indices.iter() {
        region_map.add(region as u32);
    }

    let Ok(var_region_list) = var_store.variation_region_list() else {
        return Err(SubsetError::SubsetTableError(table_tag));
    };

    // var_region_list
    s.push()
        .map_err(|_| SubsetError::SubsetTableError(table_tag))?;
    serialize_var_region_list(&var_region_list, s, &region_map)
        .map_err(|_| SubsetError::SubsetTableError(table_tag))?;
    let obj_idx = s
        .pop_pack(true)
        .ok_or(SubsetError::SubsetTableError(table_tag))?;
    s.add_link(
        regions_offset_pos..regions_offset_pos + 4,
        obj_idx,
        OffsetWhence::Head,
        0,
        false,
    )
    .map_err(|_| SubsetError::SubsetTableError(table_tag))?;

    serialize_var_data_offset_array(var_store, s, inner_maps, &region_map)
        .map_err(|_| SubsetError::SubsetTableError(table_tag))
}

fn serialize_var_region_list(
    region_list: &VariationRegionList,
    s: &mut Serializer,
    region_map: &IncBiMap,
) -> Result<(), SerializeErrorFlags> {
    let axis_count = region_list.axis_count();
    s.embed(axis_count)?;

    let region_count = region_map.len() as u16;
    s.embed(region_count)?;

    //Fixed size of a VariationRegion
    let var_region_size = 3 * axis_count as usize * F2Dot14::RAW_BYTE_LEN;
    if var_region_size.checked_mul(region_count as usize).is_none() {
        return Err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER);
    }

    let subset_var_regions_size = var_region_size * region_count as usize;
    let var_regions_pos = s.allocate_size(subset_var_regions_size, false)?;

    let src_region_count = region_list.region_count() as u32;
    let Some(src_var_regions_bytes) = region_list
        .offset_data()
        .as_bytes()
        .get(region_list.shape().variation_regions_byte_range())
    else {
        return Err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER);
    };

    for r in 0..region_count {
        let Some(backward) = region_map.backward(r as u32) else {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER);
        };
        if *backward >= src_region_count {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER);
        }

        let src_pos = (*backward as usize) * var_region_size;
        let Some(src_bytes) = src_var_regions_bytes.get(src_pos..src_pos + var_region_size) else {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER);
        };
        s.copy_assign_from_bytes(var_regions_pos + r as usize * var_region_size, src_bytes);
    }
    Ok(())
}

fn serialize_var_data_offset_array(
    var_store: &ItemVariationStore,
    s: &mut Serializer,
    inner_maps: &[IncBiMap],
    region_map: &IncBiMap,
) -> Result<(), SerializeErrorFlags> {
    let var_data_array = var_store.item_variation_data();
    for i in 0..inner_maps.iter().len() {
        let inner_map = &inner_maps[i];
        if inner_map.len() == 0 {
            continue;
        }
        match var_data_array.get(i) {
            Some(Ok(var_data)) => {
                let offset_pos = s.embed(0_u32)?;
                s.push()?;
                serialize_var_data(&var_data, s, inner_map, region_map)?;
                let obj_idx = s.pop_pack(true).ok_or(s.error())?;
                s.add_link(
                    offset_pos..offset_pos + 4,
                    obj_idx,
                    OffsetWhence::Head,
                    0,
                    false,
                )?;
            }
            _ => {
                return Err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER);
            }
        }
    }
    Ok(())
}

fn serialize_var_data(
    var_data: &ItemVariationData,
    s: &mut Serializer,
    inner_map: &IncBiMap,
    region_map: &IncBiMap,
) -> Result<(), SerializeErrorFlags> {
    let new_item_count = inner_map.len() as u16;
    s.embed(new_item_count)?;

    // optimize word count
    let ri_count = var_data.region_index_count() as usize;

    #[derive(Clone, Copy, PartialEq)]
    enum DeltaSize {
        Zero,
        NonWord,
        Word,
    }

    let mut delta_sz = Vec::new();
    delta_sz.resize(ri_count, DeltaSize::Zero);
    // maps new index to old index
    let mut ri_map = Vec::new();
    ri_map.resize(ri_count, 0);

    let mut new_word_count: u16 = 0;

    let src_delta_bytes = var_data.delta_sets();
    let src_row_size = var_data.get_delta_row_len();

    let src_word_delta_count = var_data.word_delta_count();
    let src_word_count = (src_word_delta_count & 0x7FFF) as usize;
    let src_long_words = src_word_count & 0x8000 != 0;

    let mut has_long = false;
    if src_long_words {
        for r in 0..src_word_count {
            for item in inner_map.keys() {
                let delta =
                    get_item_delta(var_data, *item as usize, r, src_row_size, src_delta_bytes);
                if delta < -65536 || delta > 65535 {
                    has_long = true;
                    break;
                }
            }
        }
    }

    let min_threshold = if has_long { -65536 } else { -128 };
    let max_threshold = if has_long { 65535 } else { 127 };

    for r in 0..ri_count {
        let short_circuit = if src_long_words == has_long && src_word_count <= r {
            true
        } else {
            false
        };
        for item in inner_map.keys() {
            let delta = get_item_delta(var_data, *item as usize, r, src_row_size, src_delta_bytes);
            if delta < min_threshold || delta > max_threshold {
                delta_sz[r] = DeltaSize::Word;
                new_word_count += 1;
                break;
            } else if delta != 0 {
                delta_sz[r] = DeltaSize::NonWord;
                if short_circuit {
                    break;
                }
            }
        }
    }

    let mut word_index: u16 = 0;
    let mut non_word_index = new_word_count;
    let mut new_ri_count: u16 = 0;

    for r in 0..ri_count {
        let delta_type = delta_sz[r];
        if delta_type == DeltaSize::Zero {
            continue;
        }

        if delta_type == DeltaSize::Word {
            let new_r = word_index as usize;
            word_index += 1;
            ri_map[new_r] = r;
        } else {
            let new_r = non_word_index as usize;
            non_word_index += 1;
            ri_map[new_r] = r;
        }
        new_ri_count += 1;
    }

    let new_word_delta_count = if has_long {
        new_word_count | 0x8000
    } else {
        new_word_count
    };
    s.embed(new_word_delta_count)?;
    s.embed(new_ri_count)?;

    let region_indices_pos = s.allocate_size(new_ri_count as usize * 2, false)?;
    let src_region_indices = var_data.region_indexes();
    for r in 0..new_ri_count as usize {
        let Some(old_r) = src_region_indices.get(ri_map[r]) else {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER);
        };

        let Some(region) = region_map.get(old_r.get() as u32) else {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER);
        };
        s.copy_assign(region_indices_pos + r * 2, *region as u16);
    }

    let new_row_size =
        ItemVariationData::delta_sets_len(new_item_count, new_word_delta_count, new_ri_count);
    let new_delta_bytes_len = new_item_count as usize * new_row_size;

    let delta_bytes_pos = s.allocate_size(new_delta_bytes_len, false)?;
    for i in 0..new_item_count as usize {
        let Some(old_i) = inner_map.backward(i as u32) else {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER);
        };

        let old_i = *old_i as usize;
        for r in 0..new_ri_count as usize {
            set_item_delta(
                s,
                delta_bytes_pos,
                i,
                r,
                get_item_delta(var_data, old_i, ri_map[r], src_row_size, src_delta_bytes),
                new_row_size,
                has_long,
                new_word_count as usize,
            )?;
        }
    }

    Ok(())
}

fn get_item_delta(
    var_data: &ItemVariationData,
    item: usize,
    region: usize,
    row_size: usize,
    delta_bytes: &[u8],
) -> i32 {
    if item >= var_data.item_count() as usize || region >= var_data.region_index_count() as usize {
        return 0;
    }

    let p = item * row_size;
    let word_delta_count = var_data.word_delta_count();
    let word_count = (word_delta_count & 0x7FFF) as usize;
    let is_long = word_delta_count & 0x8000 != 0;

    if is_long {
        if region < word_count {
            let pos = p + region * 4;
            let Some(delta_bytes) = delta_bytes.get(pos..pos + 4) else {
                return 0;
            };
            let Some(delta) = BigEndian::<i32>::from_slice(delta_bytes) else {
                return 0;
            };
            delta.get()
        } else {
            let pos = p + 4 * word_count + 2 * (region - word_count);
            let Some(delta_bytes) = delta_bytes.get(pos..pos + 2) else {
                return 0;
            };
            let Some(delta) = BigEndian::<i16>::from_slice(delta_bytes) else {
                return 0;
            };
            delta.get() as i32
        }
    } else {
        if region < word_count {
            let pos = p + region * 2;
            let Some(delta_bytes) = delta_bytes.get(pos..pos + 2) else {
                return 0;
            };
            let Some(delta) = BigEndian::<i16>::from_slice(delta_bytes) else {
                return 0;
            };
            delta.get() as i32
        } else {
            let pos = p + 2 * word_count + (region - word_count);
            let Some(delta_bytes) = delta_bytes.get(pos..pos + 1) else {
                return 0;
            };
            let Some(delta) = BigEndian::<i8>::from_slice(delta_bytes) else {
                return 0;
            };
            delta.get() as i32
        }
    }
}

fn set_item_delta(
    s: &mut Serializer,
    pos: usize,
    item: usize,
    region: usize,
    delta: i32,
    row_size: usize,
    has_long: bool,
    word_count: usize,
) -> Result<(), SerializeErrorFlags> {
    let p = pos + item * row_size;
    if has_long {
        if region < word_count {
            let pos = p + region * 4;
            s.copy_assign(pos, delta);
        } else {
            let pos = p + 4 * word_count + 2 * (region - word_count);
            let Ok(delta) = i16::try_from(delta) else {
                return Err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER);
            };
            s.copy_assign(pos, delta);
        }
    } else {
        if region < word_count {
            let pos = p + region * 2;
            let Ok(delta) = i16::try_from(delta) else {
                return Err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER);
            };
            s.copy_assign(pos, delta);
        } else {
            let pos = p + 2 * word_count + (region - word_count);
            let Ok(delta) = i8::try_from(delta) else {
                return Err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER);
            };
            s.copy_assign(pos, delta);
        }
    }
    Ok(())
}

fn collect_region_refs(
    var_data: &ItemVariationData,
    inner_map: &IncBiMap,
    region_indices: &mut IntSet<u16>,
) {
    if inner_map.len() == 0 {
        return;
    }
    let delta_bytes = var_data.delta_sets();
    let row_size = var_data.get_delta_row_len();

    let regions = var_data.region_indexes();
    for (i, region) in regions.iter().enumerate() {
        let region = region.get();
        if region_indices.contains(region) {
            continue;
        }

        for item in inner_map.keys() {
            if get_item_delta(var_data, *item as usize, i, row_size, delta_bytes) != 0 {
                region_indices.insert(region);
                break;
            }
        }
    }
}
