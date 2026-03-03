//! impl subset() for COLR
use crate::{
    offset::{SerializeCopy, SerializeSubset},
    offset_array::SubsetOffsetArray,
    serialize::{SerializeErrorFlags, Serializer},
    variations::{subset_itemvarstore_with_instancing, DeltaSetIndexMapSerializePlan},
    Plan, Subset, SubsetError, SubsetTable,
};
use fnv::FnvHashMap;
use font_types::F2Dot14;
use skrifa::raw::{tables::colr::Affine2x3, ResolveOffset};
use write_fonts::{
    read::{
        collections::IntSet,
        tables::{
            colr::{
                BaseGlyph, BaseGlyphList, BaseGlyphPaint, ClipBox, ClipBoxFormat1, ClipBoxFormat2,
                ClipList, ColorLine, ColorStop, Colr, Layer, LayerList, Paint, PaintColrGlyph,
                PaintColrLayers, PaintComposite, PaintGlyph, PaintLinearGradient,
                PaintRadialGradient, PaintRotate, PaintRotateAroundCenter, PaintScale,
                PaintScaleAroundCenter, PaintScaleUniform, PaintScaleUniformAroundCenter,
                PaintSkew, PaintSkewAroundCenter, PaintSolid, PaintSweepGradient, PaintTransform,
                PaintTranslate, PaintVarLinearGradient, PaintVarRadialGradient, PaintVarRotate,
                PaintVarRotateAroundCenter, PaintVarScale, PaintVarScaleAroundCenter,
                PaintVarScaleUniform, PaintVarScaleUniformAroundCenter, PaintVarSkew,
                PaintVarSkewAroundCenter, PaintVarSolid, PaintVarSweepGradient, PaintVarTransform,
                PaintVarTranslate, VarAffine2x3, VarColorLine, VarColorStop,
            },
            variations::NO_VARIATION_INDEX,
        },
        FontRef, TopLevelTable,
    },
    types::{GlyphId, Offset24, Offset32},
    FontBuilder,
};

/// Helper for applying deltas during COLR instantiation
#[derive(Clone, Copy)]
pub struct ColrInstancer<'a> {
    delta_map: &'a FnvHashMap<u32, (u32, i32)>,
    has_variations: bool,
    all_axes_pinned: bool,
}

impl<'a> ColrInstancer<'a> {
    pub fn new(
        delta_map: &'a FnvHashMap<u32, (u32, i32)>,
        has_variations: bool,
        all_axes_pinned: bool,
    ) -> Self {
        Self {
            delta_map,
            has_variations,
            all_axes_pinned,
        }
    }

    pub fn remap_varidx(&self, old_varidx: u32) -> u32 {
        if old_varidx == NO_VARIATION_INDEX {
            return NO_VARIATION_INDEX;
        }

        // The delta_map stores pre-filter indices (not old paint varidx values).
        // For paint VarIdxBase values, we need to remap them through the varidx_map.
        // Since we don't have direct access to varidx_map here, we check if the index
        // appears in our delta_map. If it does, use the mapped value; otherwise keep original.
        // This works when ColorStop.varidx_base values match those collected during planning.
        if let Some((new_varidx, _)) = self.delta_map.get(&old_varidx) {
            *new_varidx
        } else {
            // Not found in collected indices - may be because not all indices were collected,
            // or this is a different subsetting context
            old_varidx
        }
    }

    /// Get delta for a given variation index and field index.
    /// Scales the delta appropriately for the field type.
    pub fn get_delta(&self, var_idx: u32, field_idx: usize) -> f32 {
        if !self.has_variations || var_idx == NO_VARIATION_INDEX || field_idx > 15 {
            return 0.0;
        }

        // Calculate the actual variation index: base + field offset
        let actual_idx = var_idx.wrapping_add(field_idx as u32);

        let raw_delta = self.delta_map
            .get(&actual_idx)
            .map(|(_, delta)| *delta)
            .unwrap_or(0);
        
        // Scale the delta: F2Dot14 values use 16384 units per 1.0
        // so delta in raw units should be divided by 16384
        (raw_delta as f32) / 16384.0
    }
}

// reference: subset() for COLR in Harfbuzz:
// <https://github.com/harfbuzz/harfbuzz/blob/043980a60eb2fe93dd65b8c2f5eaa021fd8653f2/src/OT/Color/COLR/COLR.hh#L2414>
impl Subset for Colr<'_> {
    fn subset(
        &self,
        plan: &Plan,
        _font: &FontRef,
        s: &mut Serializer,
        _builder: &mut FontBuilder,
    ) -> Result<(), SubsetError> {
        let base_glyph_list = self
            .base_glyph_list()
            .transpose()
            .map_err(|_| SubsetError::SubsetTableError(Colr::TAG))?;
        let subset_to_v0 = downgrade_to_v0(base_glyph_list.as_ref(), plan);

        serialize_v0(self, plan, s, subset_to_v0)
            .map_err(|_| SubsetError::SubsetTableError(Colr::TAG))?;

        if subset_to_v0 {
            return Ok(());
        }

        // set version to 1, format pos = 0
        s.copy_assign(0, 1_u16);

        // Handle instantiation of ItemVariationStore if needed
        let (var_store_bytes, final_delta_map, post_filter_varidx_map) =
            if !plan.normalized_coords.is_empty() {
                if let Some(var_store) = self
                    .item_variation_store()
                    .transpose()
                    .map_err(|_| SubsetError::SubsetTableError(Colr::TAG))?
                {
                    let (bytes, post_filter_map) = subset_itemvarstore_with_instancing(
                        var_store.clone(),
                        plan,
                        s,
                        &plan.colr_varstore_inner_maps,
                        true,
                        self.var_index_map()
                            .transpose()
                            .map_err(|_| SubsetError::SubsetTableError(Colr::TAG))?
                            .is_some(),
                        true,
                    )
                    .map_err(|_| SubsetError::SubsetTableError(Colr::TAG))?;

                    log::warn!("COLR instancing: original delta_map has {} entries", plan.colr_varidx_delta_map.len());
                    log::warn!("COLR instancing: post_filter_map has {} entries", post_filter_map.len());

                    // When post_filter_map is empty, all delta rows were filtered out.
                    // The ItemVariationStore is now empty, so VarIndexBase values are meaningless.
                    // However, we keep them with their deltas applied - don't try to remap them.
                    // Overwrite each entry's first value (new_varidx) with the old_varidx itself,
                    // so remap_varidx() returns it unchanged. This effectively disables remapping.
                    if post_filter_map.is_empty() {
                        // Keep original indices unchanged (they point to non-existent ItemVariationStore)
                        let mut composed_delta_map = FnvHashMap::default();
                        for (old_idx, (_pre_filter_idx, delta)) in plan.colr_varidx_delta_map.iter() {
                            // Use old_idx as the "new" index so remap returns it unchanged
                            composed_delta_map.insert(*old_idx, (*old_idx, *delta));
                        }
                        (Some(bytes), composed_delta_map, Some(post_filter_map))
                    } else {
                        // Compose maps: old_idx -> (pre_filter_idx, delta) + pre_filter_idx -> post_filter_idx
                        // Result: old_idx -> (post_filter_idx, delta)
                        let mut composed_delta_map = FnvHashMap::default();
                        for (old_idx, (pre_filter_idx, delta)) in plan.colr_varidx_delta_map.iter() {
                            let post_filter_idx = post_filter_map
                                .get(pre_filter_idx)
                                .copied()
                                .unwrap_or(NO_VARIATION_INDEX);
                            composed_delta_map.insert(*old_idx, (post_filter_idx, *delta));
                        }
                        log::warn!("COLR instancing: composed_delta_map has {} entries", composed_delta_map.len());
                        (Some(bytes), composed_delta_map, Some(post_filter_map))
                    }
                } else {
                    // No ItemVariationStore in original COLR, so don't compose maps.
                    // The var paints will use their embedded deltas; plan.colr_varidx_delta_map
                    // will have NO entries anyway since there was nothing to collect.
                    log::info!("COLR instancing: no ItemVariationStore in original, using original delta_map");
                    (None, plan.colr_varidx_delta_map.clone(), None)
                }
            } else {
                (None, plan.colr_varidx_delta_map.clone(), None)
            };

        // Create instancer for applying deltas to Paint structures
        let instancer = if !plan.normalized_coords.is_empty() {
            ColrInstancer::new(&final_delta_map, true, plan.all_axes_pinned)
        } else {
            ColrInstancer::new(&final_delta_map, false, false)
        };

        // BaseGlyphList offset pos = 14
        Offset32::serialize_subset(&base_glyph_list.unwrap(), s, plan, instancer, 14)
            .map_err(|_| SubsetError::SubsetTableError(Colr::TAG))?;

        //LayerList offset pos = 18
        if let Some(layer_list) = self
            .layer_list()
            .transpose()
            .map_err(|_| SubsetError::SubsetTableError(Colr::TAG))?
        {
            match Offset32::serialize_subset(&layer_list, s, plan, instancer, 18) {
                Ok(()) | Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY) => (),
                Err(_) => {
                    return Err(SubsetError::SubsetTableError(Colr::TAG));
                }
            }
        }

        //ClipList offset pos = 22
        if let Some(clip_list) = self
            .clip_list()
            .transpose()
            .map_err(|_| SubsetError::SubsetTableError(Colr::TAG))?
        {
            // cliplist could be empty after subsetting
            match Offset32::serialize_subset(&clip_list, s, plan, instancer, 22) {
                Ok(()) | Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY) => (),
                Err(_) => {
                    return Err(SubsetError::SubsetTableError(Colr::TAG));
                }
            }
        }

        // Omit var machinery only when all axes are pinned and COLR is fully materialized.
        if plan.normalized_coords.is_empty() || !plan.all_axes_pinned {
            let mut remapped_deltaset_idx_varidx_map = None;
            if let Some(remap) = post_filter_varidx_map.as_ref() {
                let mut remapped = FnvHashMap::default();
                for (idx, old_varidx) in plan.colr_new_deltaset_idx_varidx_map.iter() {
                    let mapped = remap.get(old_varidx).copied().unwrap_or(NO_VARIATION_INDEX);
                    remapped.insert(*idx, mapped);
                }
                remapped_deltaset_idx_varidx_map = Some(remapped);
            }

            //varIndexMap offset pos = 26
            if let Some(var_index_map) = self
                .var_index_map()
                .transpose()
                .map_err(|_| SubsetError::SubsetTableError(Colr::TAG))?
            {
                let deltaset_plan =
                    if let Some(remapped) = remapped_deltaset_idx_varidx_map.as_ref() {
                        create_deltaset_index_map_subset_plan_from_map(remapped)
                    } else {
                        create_deltaset_index_map_subset_plan(plan)
                    };

                if let Some(deltaset_index_map_subset_plan) = deltaset_plan {
                    Offset32::serialize_subset(
                        &var_index_map,
                        s,
                        plan,
                        &deltaset_index_map_subset_plan,
                        26,
                    )
                    .map_err(|_| SubsetError::SubsetTableError(Colr::TAG))?;
                }
            }

            // var_store offset pos = 30
            if let Some(bytes) = var_store_bytes {
                if !bytes.is_empty() {
                    Offset32::serialize_copy_from_bytes(&bytes, s, 30)
                        .map_err(|_| SubsetError::SubsetTableError(Colr::TAG))?;
                }
            } else if let Some(var_store) = self
                .item_variation_store()
                .transpose()
                .map_err(|_| SubsetError::SubsetTableError(Colr::TAG))?
            {
                match Offset32::serialize_subset(
                    &var_store,
                    s,
                    plan,
                    (&plan.colr_varstore_inner_maps, false, true, true),
                    30,
                ) {
                    Ok(()) | Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY) => (),
                    Err(_) => return Err(SubsetError::SubsetTableError(Colr::TAG)),
                }
            }
        }

        Ok(())
    }
}

// serialize header and V0 tables, format is decided by subset_to_v0 flag
//ref: <https://github.com/harfbuzz/harfbuzz/blob/bda5b832b0bc0090f7db0577ef501c5ca56f20c8/src/OT/Color/COLR/COLR.hh#L2353>
fn serialize_v0(
    colr: &Colr,
    plan: &Plan,
    s: &mut Serializer,
    subset_to_v0: bool,
) -> Result<(), SerializeErrorFlags> {
    let num_records = colr.num_base_glyph_records();
    if num_records == 0 && subset_to_v0 {
        return Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY);
    }

    let base_glyph_records = colr
        .base_glyph_records()
        .transpose()
        .map_err(|_| SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)?;

    // allocate V0 header: min byte size = 14
    s.allocate_size(14, false)?;

    // if needed, allocate additional V1 header size = 20
    if !subset_to_v0 {
        s.allocate_size(20, false)?;
    }

    // all v0 fields are 0, return
    if base_glyph_records.is_none() {
        return Ok(());
    }

    let base_glyph_records = base_glyph_records.unwrap();
    let num_bit_storage = 16 - num_records.leading_zeros() as usize;

    let glyph_set = &plan.glyphset_colred;
    let retained_record_idxes: Vec<usize> =
        if num_records as usize > glyph_set.len() as usize * num_bit_storage {
            glyph_set
                .iter()
                .filter_map(|g| {
                    base_glyph_records
                        .binary_search_by_key(&g.to_u32(), |record| record.glyph_id().to_u32())
                        .ok()
                })
                .collect()
        } else {
            base_glyph_records
                .iter()
                .enumerate()
                .filter_map(|(idx, record)| {
                    if !glyph_set.contains(GlyphId::from(record.glyph_id())) {
                        None
                    } else {
                        Some(idx)
                    }
                })
                .collect()
        };

    if retained_record_idxes.is_empty() {
        if subset_to_v0 {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY);
        }
        return Ok(());
    }

    // serialize base glyph records, offset_pos = 4
    let mut num_layers = 0;
    Offset32::serialize_subset(
        &base_glyph_records,
        s,
        plan,
        (&retained_record_idxes, &mut num_layers),
        4,
    )?;

    //update num base glyph records
    s.copy_assign(2, retained_record_idxes.len() as u16);
    //update num layer records
    s.copy_assign(12, num_layers);

    //serialize layer records, offset_pos = 8
    let layer_records = colr
        .layer_records()
        .transpose()
        .map_err(|_| SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)?
        .unwrap();

    Offset32::serialize_subset(
        &layer_records,
        s,
        plan,
        (base_glyph_records, &retained_record_idxes),
        8,
    )?;

    Ok(())
}

impl<'a> SubsetTable<'a> for &[BaseGlyph] {
    type ArgsForSubset = (&'a [usize], &'a mut u16);
    type Output = ();
    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        args: Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        let (retained_record_idxes, num_layers) = args;
        let glyph_map = &plan.glyph_map;
        for idx in retained_record_idxes {
            let record = self.get(*idx).unwrap();
            let old_gid = GlyphId::from(record.glyph_id());
            let Some(new_gid) = glyph_map.get(&old_gid) else {
                return Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER));
            };
            s.embed(new_gid.to_u32() as u16)?;
            s.embed(*num_layers)?;

            let record_num_layers = record.num_layers();
            s.embed(record_num_layers)?;

            *num_layers += record_num_layers;
        }
        Ok(())
    }
}

impl<'a> SubsetTable<'a> for &[Layer] {
    type ArgsForSubset = (&'a [BaseGlyph], &'a [usize]);
    type Output = ();
    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        args: Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        let (base_glyph_records, retained_record_idxes) = args;
        let glyph_map = &plan.glyph_map;
        let palettes_map = &plan.colr_palettes;
        for idx in retained_record_idxes {
            let record = base_glyph_records.get(*idx).unwrap();
            let layer_idx = record.first_layer_index() as usize;
            let record_num_layers = record.num_layers() as usize;

            for i in layer_idx..layer_idx + record_num_layers {
                let Some(layer_record) = self.get(i) else {
                    return Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER));
                };

                let old_gid = GlyphId::from(layer_record.glyph_id());
                let Some(new_gid) = glyph_map.get(&old_gid) else {
                    return Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER));
                };
                let palette_idx = layer_record.palette_index();
                let Some(new_palette_idx) = palettes_map.get(&palette_idx) else {
                    return Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER));
                };

                s.embed(new_gid.to_u32() as u16)?;
                s.embed(*new_palette_idx)?;
            }
        }
        Ok(())
    }
}

impl<'a> SubsetTable<'a> for BaseGlyphList<'_> {
    type ArgsForSubset = ColrInstancer<'a>;
    type Output = ();

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        instancer: Self::ArgsForSubset,
    ) -> Result<Self::Output, SerializeErrorFlags> {
        // num BaseGlyphPaint initialized to 0
        let num_pos = s.embed(0_u32)?;

        let mut num = 0_u32;
        for paint_record in self.base_glyph_paint_records() {
            if !plan
                .glyphset_colred
                .contains(GlyphId::from(paint_record.glyph_id()))
            {
                continue;
            }
            paint_record.subset(plan, s, (self, instancer))?;
            num += 1;
        }

        s.copy_assign(num_pos, num);
        Ok(())
    }
}

impl<'a> SubsetTable<'a> for BaseGlyphPaint {
    type ArgsForSubset = (&'a BaseGlyphList<'a>, ColrInstancer<'a>);
    type Output = ();

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        args: Self::ArgsForSubset,
    ) -> Result<Self::Output, SerializeErrorFlags> {
        let (base_glyph_list, instancer) = args;
        let old_gid = GlyphId::from(self.glyph_id());
        let Some(new_gid) = plan.glyph_map.get(&old_gid) else {
            return Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER));
        };

        s.embed(new_gid.to_u32() as u16)?;

        let offset_pos = s.embed(0_u32)?;
        let Ok(paint) = self.paint(base_glyph_list.offset_data()) else {
            return Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR));
        };
        Offset32::serialize_subset(&paint, s, plan, instancer, offset_pos)
    }
}

impl<'a> SubsetTable<'a> for LayerList<'_> {
    type ArgsForSubset = ColrInstancer<'a>;
    type Output = ();

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        instancer: Self::ArgsForSubset,
    ) -> Result<Self::Output, SerializeErrorFlags> {
        let layers_map = &plan.colrv1_layers;
        if layers_map.is_empty() {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY);
        }
        s.embed(layers_map.len() as u32)?;

        let paint_offsets = self.paints();
        let num_layers = self.num_layers();
        for idx in 0..num_layers {
            if !layers_map.contains_key(&idx) {
                continue;
            }
            paint_offsets.subset_offset(idx as usize, s, plan, instancer)?;
        }
        Ok(())
    }
}

impl<'a> SubsetTable<'a> for ClipList<'_> {
    type ArgsForSubset = ColrInstancer<'a>;
    type Output = ();

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        instancer: Self::ArgsForSubset,
    ) -> Result<Self::Output, SerializeErrorFlags> {
        let glyph_set = &plan.glyphset_colred;
        let glyph_map = &plan.glyph_map;
        let retained_first_gid = glyph_set.first().unwrap().to_u32();
        let retained_last_gid = glyph_set.last().unwrap().to_u32();

        let mut new_gids_set = IntSet::empty();
        let mut new_gids_offset_map = FnvHashMap::default();
        for clip in self.clips() {
            let start_gid = clip.start_glyph_id().to_u32();
            let end_gid = clip.end_glyph_id().to_u32();
            if end_gid < retained_first_gid || start_gid > retained_last_gid {
                continue;
            }
            let offset = clip.clip_box_offset();
            for gid in start_gid..=end_gid {
                let g = GlyphId::from(gid);
                if !glyph_set.contains(g) {
                    continue;
                }

                let Some(new_gid) = glyph_map.get(&g) else {
                    continue;
                };

                let new_gid = new_gid.to_u32() as u16;
                new_gids_set.insert(new_gid);
                new_gids_offset_map.insert(new_gid, offset);
            }
        }

        if new_gids_set.is_empty() {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY);
        }

        s.embed(self.format())?;
        let num_clips_pos = s.embed(0_u32)?;
        let num_clips = serialize_clips(
            self,
            s,
            plan,
            &new_gids_set,
            &new_gids_offset_map,
            instancer,
        )?;
        s.copy_assign(num_clips_pos, num_clips);
        Ok(())
    }
}

fn serialize_clips(
    clip_list: &ClipList,
    s: &mut Serializer,
    plan: &Plan,
    gids_set: &IntSet<u16>,
    gids_offset_map: &FnvHashMap<u16, Offset24>,
    args: ColrInstancer<'_>,
) -> Result<u32, SerializeErrorFlags> {
    let mut count = 0;

    let mut start_gid = gids_set.first().unwrap();
    let mut prev_gid = start_gid;

    let mut prev_offset = gids_offset_map.get(&start_gid).unwrap();

    for g in gids_set.iter().skip(1) {
        let offset = gids_offset_map.get(&g).unwrap();
        if g == prev_gid + 1 && offset == prev_offset {
            prev_gid = g;
            continue;
        }

        let clip_box: ClipBox = prev_offset
            .resolve(clip_list.offset_data())
            .map_err(|_| SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)?;

        serialize_clip(s, plan, start_gid, prev_gid, &clip_box, args)?;
        count += 1;

        start_gid = g;
        prev_gid = g;
        prev_offset = offset;
    }

    // last one
    let clip_box: ClipBox = prev_offset
        .resolve(clip_list.offset_data())
        .map_err(|_| SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)?;

    serialize_clip(s, plan, start_gid, prev_gid, &clip_box, args)?;
    count += 1;

    Ok(count)
}

fn serialize_clip(
    s: &mut Serializer,
    plan: &Plan,
    start: u16,
    end: u16,
    clip_box: &ClipBox,
    args: ColrInstancer,
) -> Result<(), SerializeErrorFlags> {
    s.embed(start)?;
    s.embed(end)?;
    let offset_pos = s.embed_bytes(&[0_u8; 3])?;
    Offset24::serialize_subset(clip_box, s, plan, args, offset_pos)
}

impl<'a> SubsetTable<'a> for ClipBox<'_> {
    type ArgsForSubset = ColrInstancer<'a>;
    type Output = ();

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        args: Self::ArgsForSubset,
    ) -> Result<Self::Output, SerializeErrorFlags> {
        match self {
            Self::Format1(item) => item.subset(plan, s, args),
            Self::Format2(item) => item.subset(plan, s, args),
        }
    }
}

impl<'a> SubsetTable<'a> for ClipBoxFormat1<'_> {
    type ArgsForSubset = ColrInstancer<'a>;
    type Output = ();

    fn subset(
        &self,
        _plan: &Plan,
        s: &mut Serializer,
        _args: Self::ArgsForSubset,
    ) -> Result<Self::Output, SerializeErrorFlags> {
        s.embed_bytes(self.min_table_bytes()).map(|_| ())
    }
}

impl<'a> SubsetTable<'a> for ClipBoxFormat2<'_> {
    type ArgsForSubset = ColrInstancer<'a>;
    type Output = ();

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        instancer: Self::ArgsForSubset,
    ) -> Result<Self::Output, SerializeErrorFlags> {
        let start_pos = s.embed_bytes(self.min_table_bytes())?;

        let varidx_base = self.var_index_base();
        if varidx_base != NO_VARIATION_INDEX {
            let new_varidx = instancer.remap_varidx(varidx_base);
            // update VarIdxBase
            s.copy_assign(
                start_pos + self.shape().var_index_base_byte_range().start,
                new_varidx,
            );
        }
        Ok(())
    }
}

fn create_deltaset_index_map_subset_plan(plan: &Plan) -> Option<DeltaSetIndexMapSerializePlan<'_>> {
    create_deltaset_index_map_subset_plan_from_map(&plan.colr_new_deltaset_idx_varidx_map)
}

fn create_deltaset_index_map_subset_plan_from_map(
    deltaset_idx_varidx_map: &FnvHashMap<u32, u32>,
) -> Option<DeltaSetIndexMapSerializePlan<'_>> {
    let count = deltaset_idx_varidx_map.len();
    if count == 0 {
        return None;
    }

    let mut last_idx = count as u32 - 1;
    let last_varidx = deltaset_idx_varidx_map.get(&last_idx).unwrap();

    for i in (0..last_idx).rev() {
        let var_idx = deltaset_idx_varidx_map.get(&i).unwrap();
        if var_idx != last_varidx {
            break;
        }
        last_idx = i;
    }
    let map_count = last_idx + 1;
    let mut outer_bit_count = 1;
    let mut inner_bit_count = 1;

    for idx in 0..map_count {
        let var_idx = deltaset_idx_varidx_map.get(&idx).unwrap();

        let outer = var_idx >> 16;
        let bit_count = 32 - outer.leading_zeros();
        outer_bit_count = outer_bit_count.max(bit_count);

        let inner = var_idx & 0xFFFF;
        let bit_count = 32 - inner.leading_zeros();
        inner_bit_count = inner_bit_count.max(bit_count);
    }
    Some(DeltaSetIndexMapSerializePlan::new(
        outer_bit_count as u8,
        inner_bit_count as u8,
        deltaset_idx_varidx_map,
        map_count,
    ))
}

impl<'a> SubsetTable<'a> for ColorStop {
    type ArgsForSubset = ColrInstancer<'a>;
    type Output = ();

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        _instancer: Self::ArgsForSubset,
    ) -> Result<Self::Output, SerializeErrorFlags> {
        s.embed(self.stop_offset())?;
        let palette_idx = self.palette_index();
        let Some(new_idx) = plan.colr_palettes.get(&palette_idx) else {
            return Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER));
        };
        s.embed(*new_idx)?;
        s.embed(self.alpha()).map(|_| ())
    }
}

impl<'a> SubsetTable<'a> for VarColorStop {
    type ArgsForSubset = ColrInstancer<'a>;
    type Output = ();

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        instancer: Self::ArgsForSubset,
    ) -> Result<Self::Output, SerializeErrorFlags> {
        let varidx_base = self.var_index_base();
        
        // Apply deltas if this is a variable ColorStop with active instancer
        let stop_offset = if instancer.has_variations && varidx_base != NO_VARIATION_INDEX {
            self.stop_offset().to_f32() + instancer.get_delta(varidx_base, 0)
        } else {
            self.stop_offset().to_f32()
        };
        s.embed(F2Dot14::from_f32(stop_offset))?;
        
        let palette_idx = self.palette_index();
        let Some(new_idx) = plan.colr_palettes.get(&palette_idx) else {
            return Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER));
        };
        s.embed(*new_idx)?;

        let alpha = if instancer.has_variations && varidx_base != NO_VARIATION_INDEX {
            self.alpha().to_f32() + instancer.get_delta(varidx_base, 1)
        } else {
            self.alpha().to_f32()
        };
        s.embed(F2Dot14::from_f32(alpha))?;

        // Emit as non-var ColorStop only when all axes are pinned.
        if instancer.all_axes_pinned {
            return Ok(());
        }

        if varidx_base != NO_VARIATION_INDEX {
            let new_varidx = instancer.remap_varidx(varidx_base);
            s.embed(new_varidx).map(|_| ())
        } else {
            s.embed(varidx_base).map(|_| ())
        }
    }
}

impl<'a> SubsetTable<'a> for ColorLine<'_> {
    type ArgsForSubset = ColrInstancer<'a>;
    type Output = ();

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        instancer: Self::ArgsForSubset,
    ) -> Result<Self::Output, SerializeErrorFlags> {
        s.embed(self.extend())?;
        s.embed(self.num_stops())?;

        for stop in self.color_stops() {
            stop.subset(plan, s, instancer)?;
        }
        Ok(())
    }
}

impl<'a> SubsetTable<'a> for VarColorLine<'_> {
    type ArgsForSubset = ColrInstancer<'a>;
    type Output = ();

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        instancer: Self::ArgsForSubset,
    ) -> Result<Self::Output, SerializeErrorFlags> {
        s.embed(self.extend())?;
        s.embed(self.num_stops())?;

        for stop in self.color_stops() {
            stop.subset(plan, s, instancer)?;
        }
        Ok(())
    }
}

impl<'a> SubsetTable<'a> for Paint<'_> {
    type ArgsForSubset = ColrInstancer<'a>;
    type Output = ();

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        args: Self::ArgsForSubset,
    ) -> Result<Self::Output, SerializeErrorFlags> {
        match self {
            Self::ColrLayers(item) => item.subset(plan, s, args),
            Self::Solid(item) => item.subset(plan, s, args),
            Self::VarSolid(item) => item.subset(plan, s, args),
            Self::LinearGradient(item) => item.subset(plan, s, args),
            Self::VarLinearGradient(item) => item.subset(plan, s, args),
            Self::RadialGradient(item) => item.subset(plan, s, args),
            Self::VarRadialGradient(item) => item.subset(plan, s, args),
            Self::SweepGradient(item) => item.subset(plan, s, args),
            Self::VarSweepGradient(item) => item.subset(plan, s, args),
            Self::Glyph(item) => item.subset(plan, s, args),
            Self::ColrGlyph(item) => item.subset(plan, s, args),
            Self::Transform(item) => item.subset(plan, s, args),
            Self::VarTransform(item) => item.subset(plan, s, args),
            Self::Translate(item) => item.subset(plan, s, args),
            Self::VarTranslate(item) => item.subset(plan, s, args),
            Self::Scale(item) => item.subset(plan, s, args),
            Self::VarScale(item) => item.subset(plan, s, args),
            Self::ScaleAroundCenter(item) => item.subset(plan, s, args),
            Self::VarScaleAroundCenter(item) => item.subset(plan, s, args),
            Self::ScaleUniform(item) => item.subset(plan, s, args),
            Self::VarScaleUniform(item) => item.subset(plan, s, args),
            Self::ScaleUniformAroundCenter(item) => item.subset(plan, s, args),
            Self::VarScaleUniformAroundCenter(item) => item.subset(plan, s, args),
            Self::Rotate(item) => item.subset(plan, s, args),
            Self::VarRotate(item) => item.subset(plan, s, args),
            Self::RotateAroundCenter(item) => item.subset(plan, s, args),
            Self::VarRotateAroundCenter(item) => item.subset(plan, s, args),
            Self::Skew(item) => item.subset(plan, s, args),
            Self::VarSkew(item) => item.subset(plan, s, args),
            Self::SkewAroundCenter(item) => item.subset(plan, s, args),
            Self::VarSkewAroundCenter(item) => item.subset(plan, s, args),
            Self::Composite(item) => item.subset(plan, s, args),
        }
    }
}

impl<'a> SubsetTable<'a> for PaintColrLayers<'_> {
    type ArgsForSubset = ColrInstancer<'a>;
    type Output = ();

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        _instancer: Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        let start_pos = s.embed_bytes(self.min_table_bytes())?;

        let old_layer_idx = self.first_layer_index();
        let new_layer_idx = if self.num_layers() == 0 {
            0
        } else {
            let new_idx = plan
                .colrv1_layers
                .get(&old_layer_idx)
                .ok_or_else(|| s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER))?;
            *new_idx
        };
        s.copy_assign(start_pos + 2, new_layer_idx);
        Ok(())
    }
}

impl<'a> SubsetTable<'a> for PaintSolid<'_> {
    type ArgsForSubset = ColrInstancer<'a>;
    type Output = ();

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        _instancer: Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        let start_pos = s.embed_bytes(self.min_table_bytes())?;
        let palette_idx = self.palette_index();
        let Some(new_idx) = plan.colr_palettes.get(&palette_idx) else {
            return Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER));
        };
        s.copy_assign(start_pos + 1, *new_idx);
        Ok(())
    }
}

impl<'a> SubsetTable<'a> for PaintVarSolid<'_> {
    type ArgsForSubset = ColrInstancer<'a>;
    type Output = ();

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        _instancer: Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        let start_pos = s.embed_bytes(self.min_table_bytes())?;
        let palette_idx = self.palette_index();
        let Some(new_idx) = plan.colr_palettes.get(&palette_idx) else {
            return Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER));
        };
        s.copy_assign(start_pos + 1, *new_idx);

        let varidx_base = self.var_index_base();
        if varidx_base != NO_VARIATION_INDEX {
            s.copy_assign(start_pos + 5, _instancer.remap_varidx(varidx_base));
        }
        Ok(())
    }
}

impl<'a> SubsetTable<'a> for PaintLinearGradient<'_> {
    type ArgsForSubset = ColrInstancer<'a>;
    type Output = ();

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        instancer: Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        let start_pos = s.embed_bytes(self.min_table_bytes())?;

        let Ok(color_line) = self.color_line() else {
            return Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR));
        };
        //colorline offset pos = 1
        Offset24::serialize_subset(&color_line, s, plan, instancer, start_pos + 1)
    }
}

impl<'a> SubsetTable<'a> for PaintVarLinearGradient<'_> {
    type ArgsForSubset = ColrInstancer<'a>;
    type Output = ();

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        instancer: Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        let start_pos = s.embed_bytes(self.min_table_bytes())?;

        let Ok(color_line) = self.color_line() else {
            return Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR));
        };
        //colorline offset pos = 1
        Offset24::serialize_subset(&color_line, s, plan, instancer, start_pos + 1)?;

        let varidx_base = self.var_index_base();
        if instancer.has_variations && varidx_base != NO_VARIATION_INDEX {
            let x0 = self.x0().to_i16() as f32 + instancer.get_delta(varidx_base, 0);
            let y0 = self.y0().to_i16() as f32 + instancer.get_delta(varidx_base, 1);
            let x1 = self.x1().to_i16() as f32 + instancer.get_delta(varidx_base, 2);
            let y1 = self.y1().to_i16() as f32 + instancer.get_delta(varidx_base, 3);
            let x2 = self.x2().to_i16() as f32 + instancer.get_delta(varidx_base, 4);
            let y2 = self.y2().to_i16() as f32 + instancer.get_delta(varidx_base, 5);

            s.copy_assign(
                start_pos + self.shape().x0_byte_range().start,
                x0.round() as i16,
            );
            s.copy_assign(
                start_pos + self.shape().y0_byte_range().start,
                y0.round() as i16,
            );
            s.copy_assign(
                start_pos + self.shape().x1_byte_range().start,
                x1.round() as i16,
            );
            s.copy_assign(
                start_pos + self.shape().y1_byte_range().start,
                y1.round() as i16,
            );
            s.copy_assign(
                start_pos + self.shape().x2_byte_range().start,
                x2.round() as i16,
            );
            s.copy_assign(
                start_pos + self.shape().y2_byte_range().start,
                y2.round() as i16,
            );

            let pos = start_pos + self.shape().var_index_base_byte_range().start;
            if instancer.all_axes_pinned {
                s.copy_assign(start_pos, 4_u8);
                s.copy_assign(pos, NO_VARIATION_INDEX);
            } else {
                let new_varidx = instancer.remap_varidx(varidx_base);
                s.copy_assign(pos, new_varidx);
            }
        } else if varidx_base != NO_VARIATION_INDEX {
            let new_varidx = instancer.remap_varidx(varidx_base);
            let pos = start_pos + self.shape().var_index_base_byte_range().start;
            s.copy_assign(pos, new_varidx);
        }
        Ok(())
    }
}

impl<'a> SubsetTable<'a> for PaintRadialGradient<'_> {
    type ArgsForSubset = ColrInstancer<'a>;
    type Output = ();

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        instancer: Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        let start_pos = s.embed_bytes(self.min_table_bytes())?;

        let Ok(color_line) = self.color_line() else {
            return Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR));
        };
        //colorline offset pos = 1
        Offset24::serialize_subset(&color_line, s, plan, instancer, start_pos + 1)
    }
}

impl<'a> SubsetTable<'a> for PaintVarRadialGradient<'_> {
    type ArgsForSubset = ColrInstancer<'a>;
    type Output = ();

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        instancer: Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        let start_pos = s.embed_bytes(self.min_table_bytes())?;

        let Ok(color_line) = self.color_line() else {
            return Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR));
        };
        //colorline offset pos = 1
        Offset24::serialize_subset(&color_line, s, plan, instancer, start_pos + 1)?;

        let varidx_base = self.var_index_base();
        if instancer.has_variations && varidx_base != NO_VARIATION_INDEX {
            let x0 = self.x0().to_i16() as f32 + instancer.get_delta(varidx_base, 0);
            let y0 = self.y0().to_i16() as f32 + instancer.get_delta(varidx_base, 1);
            let radius0 = self.radius0().to_u16() as f32 + instancer.get_delta(varidx_base, 2);
            let x1 = self.x1().to_i16() as f32 + instancer.get_delta(varidx_base, 3);
            let y1 = self.y1().to_i16() as f32 + instancer.get_delta(varidx_base, 4);
            let radius1 = self.radius1().to_u16() as f32 + instancer.get_delta(varidx_base, 5);

            s.copy_assign(
                start_pos + self.shape().x0_byte_range().start,
                x0.round() as i16,
            );
            s.copy_assign(
                start_pos + self.shape().y0_byte_range().start,
                y0.round() as i16,
            );
            s.copy_assign(
                start_pos + self.shape().radius0_byte_range().start,
                radius0.round() as u16,
            );
            s.copy_assign(
                start_pos + self.shape().x1_byte_range().start,
                x1.round() as i16,
            );
            s.copy_assign(
                start_pos + self.shape().y1_byte_range().start,
                y1.round() as i16,
            );
            s.copy_assign(
                start_pos + self.shape().radius1_byte_range().start,
                radius1.round() as u16,
            );

            let pos = start_pos + self.shape().var_index_base_byte_range().start;
            if instancer.all_axes_pinned {
                s.copy_assign(start_pos, 6_u8);
                s.copy_assign(pos, NO_VARIATION_INDEX);
            } else {
                let new_varidx = instancer.remap_varidx(varidx_base);
                s.copy_assign(pos, new_varidx);
            }
        } else if varidx_base != NO_VARIATION_INDEX {
            let new_varidx = instancer.remap_varidx(varidx_base);
            let pos = start_pos + self.shape().var_index_base_byte_range().start;
            s.copy_assign(pos, new_varidx);
        }
        Ok(())
    }
}

impl<'a> SubsetTable<'a> for PaintSweepGradient<'_> {
    type ArgsForSubset = ColrInstancer<'a>;
    type Output = ();

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        instancer: Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        let start_pos = s.embed_bytes(self.min_table_bytes())?;

        let Ok(color_line) = self.color_line() else {
            return Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR));
        };
        //colorline offset pos = 1
        Offset24::serialize_subset(&color_line, s, plan, instancer, start_pos + 1)
    }
}

impl<'a> SubsetTable<'a> for PaintVarSweepGradient<'_> {
    type ArgsForSubset = ColrInstancer<'a>;
    type Output = ();

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        instancer: Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        let start_pos = s.embed_bytes(self.min_table_bytes())?;

        let Ok(color_line) = self.color_line() else {
            return Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR));
        };
        //colorline offset pos = 1
        Offset24::serialize_subset(&color_line, s, plan, instancer, start_pos + 1)?;

        let varidx_base = self.var_index_base();
        if varidx_base != NO_VARIATION_INDEX {
            let new_varidx = instancer.remap_varidx(varidx_base);
            // update VarIdxBase
            let pos = start_pos + self.shape().var_index_base_byte_range().start;
            s.copy_assign(pos, new_varidx);
        }
        Ok(())
    }
}

impl<'a> SubsetTable<'a> for PaintGlyph<'_> {
    type ArgsForSubset = ColrInstancer<'a>;
    type Output = ();

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        _args: Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        s.embed(self.format())?;
        let offset_pos = s.embed_bytes(&[0_u8; 3])?;

        let old_gid = GlyphId::from(self.glyph_id());
        let Some(new_gid) = plan.glyph_map.get(&old_gid) else {
            return Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER));
        };
        s.embed(new_gid.to_u32() as u16)?;

        let Ok(paint) = self.paint() else {
            return Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR));
        };
        Offset24::serialize_subset(&paint, s, plan, _args, offset_pos)
    }
}

impl<'a> SubsetTable<'a> for PaintColrGlyph<'_> {
    type ArgsForSubset = ColrInstancer<'a>;
    type Output = ();

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        _args: Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        s.embed(self.format())?;

        let old_gid = GlyphId::from(self.glyph_id());
        let Some(new_gid) = plan.glyph_map.get(&old_gid) else {
            return Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER));
        };
        s.embed(new_gid.to_u32() as u16).map(|_| ())
    }
}

impl<'a> SubsetTable<'a> for Affine2x3<'_> {
    type ArgsForSubset = ColrInstancer<'a>;
    type Output = ();

    fn subset(
        &self,
        _plan: &Plan,
        s: &mut Serializer,
        _args: Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        s.embed_bytes(self.min_table_bytes()).map(|_| ())
    }
}

impl<'a> SubsetTable<'a> for VarAffine2x3<'_> {
    type ArgsForSubset = ColrInstancer<'a>;
    type Output = ();

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        instancer: Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        let start_pos = s.embed_bytes(self.min_table_bytes())?;
        let varidx_base = self.var_index_base();
        if varidx_base != NO_VARIATION_INDEX {
            let new_varidx = instancer.remap_varidx(varidx_base);
            // update VarIdxBase
            let pos = start_pos + self.shape().var_index_base_byte_range().start;
            s.copy_assign(pos, new_varidx);
        }
        Ok(())
    }
}

impl<'a> SubsetTable<'a> for PaintTransform<'_> {
    type ArgsForSubset = ColrInstancer<'a>;
    type Output = ();

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        _args: Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        s.embed(self.format())?;

        let paint_pos = s.embed_bytes(&[0_u8; 3])?;
        let transform_pos = s.embed_bytes(&[0_u8; 3])?;

        let Ok(paint) = self.paint() else {
            return Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR));
        };
        Offset24::serialize_subset(&paint, s, plan, _args, paint_pos)?;

        let Ok(affine) = self.transform() else {
            return Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR));
        };
        Offset24::serialize_subset(&affine, s, plan, _args, transform_pos)
    }
}

impl<'a> SubsetTable<'a> for PaintVarTransform<'_> {
    type ArgsForSubset = ColrInstancer<'a>;
    type Output = ();

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        _args: Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        s.embed(self.format())?;

        let paint_pos = s.embed_bytes(&[0_u8; 3])?;
        let transform_pos = s.embed_bytes(&[0_u8; 3])?;

        let Ok(paint) = self.paint() else {
            return Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR));
        };
        Offset24::serialize_subset(&paint, s, plan, _args, paint_pos)?;

        let Ok(affine) = self.transform() else {
            return Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR));
        };
        Offset24::serialize_subset(&affine, s, plan, _args, transform_pos)
    }
}

impl<'a> SubsetTable<'a> for PaintTranslate<'_> {
    type ArgsForSubset = ColrInstancer<'a>;
    type Output = ();

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        _args: Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        let start_pos = s.embed_bytes(self.min_table_bytes())?;
        let Ok(paint) = self.paint() else {
            return Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR));
        };
        Offset24::serialize_subset(&paint, s, plan, _args, start_pos + 1)
    }
}

impl<'a> SubsetTable<'a> for PaintVarTranslate<'_> {
    type ArgsForSubset = ColrInstancer<'a>;
    type Output = ();

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        instancer: Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        let start_pos = s.embed_bytes(self.min_table_bytes())?;
        let Ok(paint) = self.paint() else {
            return Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR));
        };
        Offset24::serialize_subset(&paint, s, plan, instancer, start_pos + 1)?;

        let varidx_base = self.var_index_base();
        if varidx_base != NO_VARIATION_INDEX {
            let new_varidx = instancer.remap_varidx(varidx_base);
            // update VarIdxBase
            let pos = start_pos + self.shape().var_index_base_byte_range().start;
            s.copy_assign(pos, new_varidx);
        }
        Ok(())
    }
}

impl<'a> SubsetTable<'a> for PaintScale<'_> {
    type ArgsForSubset = ColrInstancer<'a>;
    type Output = ();

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        _args: Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        let start_pos = s.embed_bytes(self.min_table_bytes())?;
        let Ok(paint) = self.paint() else {
            return Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR));
        };
        Offset24::serialize_subset(&paint, s, plan, _args, start_pos + 1)
    }
}

impl<'a> SubsetTable<'a> for PaintVarScale<'_> {
    type ArgsForSubset = ColrInstancer<'a>;
    type Output = ();

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        instancer: Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        let start_pos = s.embed_bytes(self.min_table_bytes())?;
        let Ok(paint) = self.paint() else {
            return Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR));
        };
        Offset24::serialize_subset(&paint, s, plan, instancer, start_pos + 1)?;

        let varidx_base = self.var_index_base();
        if varidx_base != NO_VARIATION_INDEX {
            let new_varidx = instancer.remap_varidx(varidx_base);
            // update VarIdxBase
            let pos = start_pos + self.shape().var_index_base_byte_range().start;
            s.copy_assign(pos, new_varidx);
        }
        Ok(())
    }
}

impl<'a> SubsetTable<'a> for PaintScaleAroundCenter<'_> {
    type ArgsForSubset = ColrInstancer<'a>;
    type Output = ();

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        _args: Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        let start_pos = s.embed_bytes(self.min_table_bytes())?;
        let Ok(paint) = self.paint() else {
            return Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR));
        };
        Offset24::serialize_subset(&paint, s, plan, _args, start_pos + 1)
    }
}

impl<'a> SubsetTable<'a> for PaintVarScaleAroundCenter<'_> {
    type ArgsForSubset = ColrInstancer<'a>;
    type Output = ();

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        instancer: Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        let start_pos = s.embed_bytes(self.min_table_bytes())?;
        let Ok(paint) = self.paint() else {
            return Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR));
        };
        Offset24::serialize_subset(&paint, s, plan, instancer, start_pos + 1)?;

        let varidx_base = self.var_index_base();
        if varidx_base != NO_VARIATION_INDEX {
            let new_varidx = instancer.remap_varidx(varidx_base);
            // update VarIdxBase
            let pos = start_pos + self.shape().var_index_base_byte_range().start;
            s.copy_assign(pos, new_varidx);
        }
        Ok(())
    }
}

impl<'a> SubsetTable<'a> for PaintScaleUniform<'_> {
    type ArgsForSubset = ColrInstancer<'a>;
    type Output = ();

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        _args: Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        let start_pos = s.embed_bytes(self.min_table_bytes())?;
        let Ok(paint) = self.paint() else {
            return Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR));
        };
        Offset24::serialize_subset(&paint, s, plan, _args, start_pos + 1)
    }
}

impl<'a> SubsetTable<'a> for PaintVarScaleUniform<'_> {
    type ArgsForSubset = ColrInstancer<'a>;
    type Output = ();

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        instancer: Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        let start_pos = s.embed_bytes(self.min_table_bytes())?;
        let Ok(paint) = self.paint() else {
            return Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR));
        };
        Offset24::serialize_subset(&paint, s, plan, instancer, start_pos + 1)?;

        let varidx_base = self.var_index_base();
        if varidx_base != NO_VARIATION_INDEX {
            let new_varidx = instancer.remap_varidx(varidx_base);
            // update VarIdxBase
            let pos = start_pos + self.shape().var_index_base_byte_range().start;
            s.copy_assign(pos, new_varidx);
        }
        Ok(())
    }
}

impl<'a> SubsetTable<'a> for PaintScaleUniformAroundCenter<'_> {
    type ArgsForSubset = ColrInstancer<'a>;
    type Output = ();

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        _args: Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        let start_pos = s.embed_bytes(self.min_table_bytes())?;
        let Ok(paint) = self.paint() else {
            return Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR));
        };
        Offset24::serialize_subset(&paint, s, plan, _args, start_pos + 1)
    }
}

impl<'a> SubsetTable<'a> for PaintVarScaleUniformAroundCenter<'_> {
    type ArgsForSubset = ColrInstancer<'a>;
    type Output = ();

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        instancer: Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        let start_pos = s.embed_bytes(self.min_table_bytes())?;
        let Ok(paint) = self.paint() else {
            return Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR));
        };
        Offset24::serialize_subset(&paint, s, plan, instancer, start_pos + 1)?;

        let varidx_base = self.var_index_base();
        if varidx_base != NO_VARIATION_INDEX {
            let new_varidx = instancer.remap_varidx(varidx_base);
            // update VarIdxBase
            let pos = start_pos + self.shape().var_index_base_byte_range().start;
            s.copy_assign(pos, new_varidx);
        }
        Ok(())
    }
}

impl<'a> SubsetTable<'a> for PaintRotate<'_> {
    type ArgsForSubset = ColrInstancer<'a>;
    type Output = ();

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        _args: Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        let start_pos = s.embed_bytes(self.min_table_bytes())?;
        let Ok(paint) = self.paint() else {
            return Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR));
        };
        Offset24::serialize_subset(&paint, s, plan, _args, start_pos + 1)
    }
}

impl<'a> SubsetTable<'a> for PaintVarRotate<'_> {
    type ArgsForSubset = ColrInstancer<'a>;
    type Output = ();

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        instancer: Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        let start_pos = s.embed_bytes(self.min_table_bytes())?;
        let Ok(paint) = self.paint() else {
            return Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR));
        };
        Offset24::serialize_subset(&paint, s, plan, instancer, start_pos + 1)?;

        let varidx_base = self.var_index_base();
        if varidx_base != NO_VARIATION_INDEX {
            let new_varidx = instancer.remap_varidx(varidx_base);
            // update VarIdxBase
            let pos = start_pos + self.shape().var_index_base_byte_range().start;
            s.copy_assign(pos, new_varidx);
        }
        Ok(())
    }
}

impl<'a> SubsetTable<'a> for PaintRotateAroundCenter<'_> {
    type ArgsForSubset = ColrInstancer<'a>;
    type Output = ();

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        _args: Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        let start_pos = s.embed_bytes(self.min_table_bytes())?;
        let Ok(paint) = self.paint() else {
            return Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR));
        };
        Offset24::serialize_subset(&paint, s, plan, _args, start_pos + 1)
    }
}

impl<'a> SubsetTable<'a> for PaintVarRotateAroundCenter<'_> {
    type ArgsForSubset = ColrInstancer<'a>;
    type Output = ();

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        instancer: Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        let start_pos = s.embed_bytes(self.min_table_bytes())?;
        let Ok(paint) = self.paint() else {
            return Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR));
        };
        Offset24::serialize_subset(&paint, s, plan, instancer, start_pos + 1)?;

        let varidx_base = self.var_index_base();
        if varidx_base != NO_VARIATION_INDEX {
            let new_varidx = instancer.remap_varidx(varidx_base);
            // update VarIdxBase
            let pos = start_pos + self.shape().var_index_base_byte_range().start;
            s.copy_assign(pos, new_varidx);
        }
        Ok(())
    }
}

impl<'a> SubsetTable<'a> for PaintSkew<'_> {
    type ArgsForSubset = ColrInstancer<'a>;
    type Output = ();

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        _args: Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        let start_pos = s.embed_bytes(self.min_table_bytes())?;
        let Ok(paint) = self.paint() else {
            return Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR));
        };
        Offset24::serialize_subset(&paint, s, plan, _args, start_pos + 1)
    }
}

impl<'a> SubsetTable<'a> for PaintVarSkew<'_> {
    type ArgsForSubset = ColrInstancer<'a>;
    type Output = ();

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        instancer: Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        let start_pos = s.embed_bytes(self.min_table_bytes())?;
        let Ok(paint) = self.paint() else {
            return Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR));
        };
        Offset24::serialize_subset(&paint, s, plan, instancer, start_pos + 1)?;

        let varidx_base = self.var_index_base();
        if varidx_base != NO_VARIATION_INDEX {
            let new_varidx = instancer.remap_varidx(varidx_base);
            // update VarIdxBase
            let pos = start_pos + self.shape().var_index_base_byte_range().start;
            s.copy_assign(pos, new_varidx);
        }
        Ok(())
    }
}

impl<'a> SubsetTable<'a> for PaintSkewAroundCenter<'_> {
    type ArgsForSubset = ColrInstancer<'a>;
    type Output = ();

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        _args: Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        let start_pos = s.embed_bytes(self.min_table_bytes())?;
        let Ok(paint) = self.paint() else {
            return Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR));
        };
        Offset24::serialize_subset(&paint, s, plan, _args, start_pos + 1)
    }
}

impl<'a> SubsetTable<'a> for PaintVarSkewAroundCenter<'_> {
    type ArgsForSubset = ColrInstancer<'a>;
    type Output = ();

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        instancer: Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        let start_pos = s.embed_bytes(self.min_table_bytes())?;
        let Ok(paint) = self.paint() else {
            return Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR));
        };
        Offset24::serialize_subset(&paint, s, plan, instancer, start_pos + 1)?;

        let varidx_base = self.var_index_base();
        if varidx_base != NO_VARIATION_INDEX {
            let new_varidx = instancer.remap_varidx(varidx_base);
            // update VarIdxBase
            let pos = start_pos + self.shape().var_index_base_byte_range().start;
            s.copy_assign(pos, new_varidx);
        }
        Ok(())
    }
}

impl<'a> SubsetTable<'a> for PaintComposite<'_> {
    type ArgsForSubset = ColrInstancer<'a>;
    type Output = ();

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        _args: Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        s.embed(self.format())?;
        let src_paint_pos = s.embed_bytes(&[0_u8; 3])?;
        s.embed(self.composite_mode())?;
        let backdrop_paint_pos = s.embed_bytes(&[0_u8; 3])?;

        let Ok(src_paint) = self.source_paint() else {
            return Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR));
        };
        Offset24::serialize_subset(&src_paint, s, plan, _args, src_paint_pos)?;

        let Ok(backdrop_paint) = self.backdrop_paint() else {
            return Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR));
        };
        Offset24::serialize_subset(&backdrop_paint, s, plan, _args, backdrop_paint_pos)
    }
}

// downgrade to v0 if we have no v1 glyphs to retain
fn downgrade_to_v0(base_glyph_list: Option<&BaseGlyphList>, plan: &Plan) -> bool {
    if base_glyph_list.is_none() {
        return true;
    }

    for paint_record in base_glyph_list.unwrap().base_glyph_paint_records() {
        if plan
            .glyphset_colred
            .contains(GlyphId::from(paint_record.glyph_id()))
        {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod test {
    use super::*;
    use write_fonts::{read::TableProvider, types::GlyphId};
    #[test]
    fn test_subset_colr_retain_all() {
        let ttf: &[u8] = include_bytes!("../test-data/fonts/TwemojiMozilla.subset.ttf");
        let font = FontRef::new(ttf).unwrap();
        let colr = font.colr().unwrap();

        let mut builder = FontBuilder::new();

        let mut plan = Plan::default();

        plan.glyphset_colred
            .insert_range(GlyphId::NOTDEF..=GlyphId::from(6_u32));

        plan.glyph_map.insert(GlyphId::NOTDEF, GlyphId::NOTDEF);
        plan.glyph_map
            .insert(GlyphId::from(1_u32), GlyphId::from(1_u32));
        plan.glyph_map
            .insert(GlyphId::from(2_u32), GlyphId::from(2_u32));
        plan.glyph_map
            .insert(GlyphId::from(3_u32), GlyphId::from(3_u32));
        plan.glyph_map
            .insert(GlyphId::from(4_u32), GlyphId::from(4_u32));
        plan.glyph_map
            .insert(GlyphId::from(5_u32), GlyphId::from(5_u32));
        plan.glyph_map
            .insert(GlyphId::from(6_u32), GlyphId::from(6_u32));

        plan.colr_palettes.insert(2, 0);
        plan.colr_palettes.insert(11, 1);

        let mut s = Serializer::new(1024);
        assert_eq!(s.start_serialize(), Ok(()));
        let ret = colr.subset(&plan, &font, &mut s, &mut builder);
        assert!(ret.is_ok());
        assert!(!s.in_error());
        s.end_serialize();

        let subsetted_data = s.copy_bytes();
        let expected_data: [u8; 42] = [
            0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x1e, 0x00, 0x00, 0x00, 0x0e, 0x00, 0x04,
            0x00, 0x04, 0x00, 0x00, 0x00, 0x05, 0x00, 0x01, 0x00, 0x04, 0x00, 0x00, 0x00, 0x06,
            0x00, 0x01, 0x00, 0x02, 0x00, 0x00, 0x00, 0x02, 0x00, 0x03, 0x00, 0x02, 0x00, 0x02,
        ];
        assert_eq!(subsetted_data, expected_data);
    }

    #[test]
    fn test_subset_colr_keep_one_colr_glyph() {
        let ttf: &[u8] = include_bytes!("../test-data/fonts/TwemojiMozilla.subset.ttf");
        let font = FontRef::new(ttf).unwrap();
        let colr = font.colr().unwrap();

        let mut builder = FontBuilder::new();

        let mut plan = Plan::default();

        plan.glyphset_colred.insert(GlyphId::NOTDEF);
        plan.glyphset_colred.insert(GlyphId::from(2_u32));
        plan.glyphset_colred.insert(GlyphId::from(4_u32));
        plan.glyphset_colred.insert(GlyphId::from(5_u32));

        plan.glyph_map.insert(GlyphId::NOTDEF, GlyphId::NOTDEF);
        plan.glyph_map
            .insert(GlyphId::from(2_u32), GlyphId::from(1_u32));
        plan.glyph_map
            .insert(GlyphId::from(4_u32), GlyphId::from(2_u32));
        plan.glyph_map
            .insert(GlyphId::from(5_u32), GlyphId::from(3_u32));

        plan.colr_palettes.insert(2, 0);
        plan.colr_palettes.insert(11, 1);

        let mut s = Serializer::new(1024);
        assert_eq!(s.start_serialize(), Ok(()));
        let ret = colr.subset(&plan, &font, &mut s, &mut builder);
        assert!(ret.is_ok());
        assert!(!s.in_error());
        s.end_serialize();

        let subsetted_data = s.copy_bytes();
        let expected_data: [u8; 28] = [
            0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x16, 0x00, 0x00, 0x00, 0x0e, 0x00, 0x02,
            0x00, 0x02, 0x00, 0x00, 0x00, 0x03, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x02,
        ];
        assert_eq!(subsetted_data, expected_data);
    }

    #[test]
    fn test_subset_colr_keep_mixed_glyph() {
        let ttf: &[u8] = include_bytes!("../test-data/fonts/TwemojiMozilla.subset.ttf");
        let font = FontRef::new(ttf).unwrap();
        let colr = font.colr().unwrap();

        let mut builder = FontBuilder::new();

        let mut plan = Plan::default();

        plan.glyphset_colred.insert(GlyphId::NOTDEF);
        plan.glyphset_colred.insert(GlyphId::from(1_u32));
        plan.glyphset_colred.insert(GlyphId::from(3_u32));
        plan.glyphset_colred.insert(GlyphId::from(4_u32));
        plan.glyphset_colred.insert(GlyphId::from(6_u32));

        plan.glyph_map.insert(GlyphId::NOTDEF, GlyphId::NOTDEF);
        plan.glyph_map
            .insert(GlyphId::from(1_u32), GlyphId::from(1_u32));
        plan.glyph_map
            .insert(GlyphId::from(3_u32), GlyphId::from(2_u32));
        plan.glyph_map
            .insert(GlyphId::from(4_u32), GlyphId::from(3_u32));
        plan.glyph_map
            .insert(GlyphId::from(6_u32), GlyphId::from(4_u32));

        plan.colr_palettes.insert(2, 0);
        plan.colr_palettes.insert(11, 1);

        let mut s = Serializer::new(1024);
        assert_eq!(s.start_serialize(), Ok(()));
        let ret = colr.subset(&plan, &font, &mut s, &mut builder);
        assert!(ret.is_ok());
        assert!(!s.in_error());
        s.end_serialize();

        let subsetted_data = s.copy_bytes();
        let expected_data: [u8; 28] = [
            0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x16, 0x00, 0x00, 0x00, 0x0e, 0x00, 0x02,
            0x00, 0x03, 0x00, 0x00, 0x00, 0x04, 0x00, 0x01, 0x00, 0x02, 0x00, 0x00, 0x00, 0x02,
        ];
        assert_eq!(subsetted_data, expected_data);
    }

    #[test]
    fn test_subset_colr_keep_no_colr_glyph() {
        let ttf: &[u8] = include_bytes!("../test-data/fonts/TwemojiMozilla.subset.ttf");
        let font = FontRef::new(ttf).unwrap();
        let colr = font.colr().unwrap();

        let mut builder = FontBuilder::new();

        let mut plan = Plan::default();

        plan.glyphset_colred.insert(GlyphId::NOTDEF);
        plan.glyphset_colred.insert(GlyphId::from(1_u32));

        plan.glyph_map.insert(GlyphId::NOTDEF, GlyphId::NOTDEF);
        plan.glyph_map
            .insert(GlyphId::from(1_u32), GlyphId::from(1_u32));

        let mut s = Serializer::new(1024);
        assert_eq!(s.start_serialize(), Ok(()));
        let ret = colr.subset(&plan, &font, &mut s, &mut builder);
        assert!(ret.is_err());
        assert!(!s.in_error());
        s.end_serialize();
    }
}
