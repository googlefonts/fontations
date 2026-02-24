//! impl subset() for gvar table
use std::mem::size_of;

use crate::{
    serialize::{SerializeErrorFlags, Serializer},
    variations::TupleVariations,
    Plan, Subset, SubsetError, SubsetFlags,
};

use write_fonts::{
    read::{tables::gvar::Gvar, types::GlyphId, FontRef, TopLevelTable},
    tables::gvar::{GlyphVariations, Gvar as WriteGvar},
    types::Scalar,
    FontBuilder,
};

const FIXED_HEADER_SIZE: u32 = 20;

// reference: subset() for gvar table in harfbuzz
// https://github.com/harfbuzz/harfbuzz/blob/63d09dbefcf7ad9f794ca96445d37b6d8c3c9124/src/hb-ot-var-gvar-table.hh#L411
impl Subset for Gvar<'_> {
    fn subset(
        &self,
        plan: &Plan,
        _font: &FontRef,
        s: &mut Serializer,
        builder: &mut FontBuilder,
    ) -> Result<(), SubsetError> {
        if plan.all_axes_pinned {
            // it's not an error, we just don't need it
            log::trace!("Removing gvar, because all axes are pinned and there is no variation data to subset.");
            return Err(SubsetError::SubsetTableError(Gvar::TAG));
        }
        if !plan.normalized_coords.is_empty() {
            // Instantiate instead
            return instantiate_gvar(self, plan, builder)
                .map_err(|_| SubsetError::SubsetTableError(Gvar::TAG));
        }
        //table header: from version to sharedTuplesOffset
        s.embed_bytes(self.offset_data().as_bytes().get(0..12).unwrap())
            .map_err(|_| SubsetError::SubsetTableError(Gvar::TAG))?;

        // glyphCount
        let num_glyphs = plan.num_output_glyphs.min(0xFFFF) as u16;
        s.embed(num_glyphs)
            .map_err(|_| SubsetError::SubsetTableError(Gvar::TAG))?;

        let subset_data_size: usize = plan
            .new_to_old_gid_list
            .iter()
            .filter_map(|x| {
                if x.0 == GlyphId::NOTDEF
                    && !plan
                        .subset_flags
                        .contains(SubsetFlags::SUBSET_FLAGS_NOTDEF_OUTLINE)
                {
                    return None;
                }
                self.data_for_gid(x.0)
                    .ok()
                    .flatten()
                    .map(|data| data.len() + data.len() % 2)
            })
            .sum();

        // According to the spec: If the short format (Offset16) is used for offsets, the value stored is the offset divided by 2.
        // So the maximum subset data size that could use short format should be 2 * 0xFFFFu, which is 0x1FFFE
        let long_offset = if subset_data_size > 0x1FFFE_usize {
            1_u16
        } else {
            0_u16
        };
        // flags
        s.embed(long_offset)
            .map_err(|_| SubsetError::SubsetTableError(Gvar::TAG))?;

        if long_offset > 0 {
            subset_with_offset_type::<u32>(self, plan, num_glyphs, s)?;
        } else {
            subset_with_offset_type::<u16>(self, plan, num_glyphs, s)?;
        }

        Ok(())
    }
}

fn instantiate_gvar(
    gvar: &Gvar<'_>,
    plan: &Plan,
    builder: &mut FontBuilder,
) -> Result<(), SerializeErrorFlags> {
    let mut new_variations = vec![];
    let new_axis_count = plan.axes_index_map.len() as u16;
    let optimize = plan
        .subset_flags
        .contains(SubsetFlags::SUBSET_FLAGS_OPTIMIZE_IUP_DELTAS);
    log::debug!(
        "Instantiating gvar with normalized coords {:?}, axes_location {:?} and axes_triple_distances: {:?}",
        plan.normalized_coords,
        plan.axes_location,
        plan.axes_triple_distances
    );
    for (new_gid, old_gid) in plan.new_to_old_gid_list.iter() {
        if let Some(glyph_var) = gvar
            .glyph_variation_data(*old_gid)
            .map_err(|_| SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)?
        {
            if new_gid == &GlyphId::new(0)
                && !(plan
                    .subset_flags
                    .contains(SubsetFlags::SUBSET_FLAGS_NOTDEF_OUTLINE))
            {
                // Special handling for .notdef glyph
                new_variations.push(GlyphVariations::new(*new_gid, vec![]));
            } else if let Some(all_points) = plan.new_gid_contour_points_map.get(new_gid) {
                log::trace!(
                    "Instantiating gvar for gid {:?} with {} points",
                    new_gid,
                    all_points.0.len()
                );
                let mut tuple_variations: TupleVariations = TupleVariations::from_gvar(
                    glyph_var,
                    all_points.0.len(),
                    &plan.axes_old_index_tag_map,
                )?;

                tuple_variations.instantiate(
                    &plan.axes_location,
                    &plan.axes_triple_distances,
                    Some(all_points), // I don't think we need to instantiate the contour here, we do it in the plan already
                    optimize,
                )?;
                // Normalize axes: ensure all tuples have the same set of axes
                tuple_variations.normalize_axes();
                new_variations.push(GlyphVariations::new(
                    *new_gid,
                    tuple_variations.to_glyph_deltas(&plan.axis_tags),
                ));
            } else {
                // Can't happen
                panic!("Can't find contour points for gid {:?} in plan, but it should be there as it's used in gvar", new_gid);
            }
        } else {
            // No variations for this glyph
            new_variations.push(GlyphVariations::new(*new_gid, vec![]));
        }
    }
    let new_gvar = WriteGvar::new(new_variations, new_axis_count)
        .expect("Can't write gvar table with new variations"); // This should never fail, as we're not doing any complex serialization here
                                                               // .map_err(|_| SerializeErrorFlags::SERIALIZE_ERROR_OTHER)?;

    builder
        .add_table(&new_gvar)
        .expect("Can't add gvar table to font builder"); // This should never fail, as we're not doing any complex serialization here
                                                         // .map_err(|_| SerializeErrorFlags::SERIALIZE_ERROR_OTHER)?;
    Ok(())
}

fn subset_with_offset_type<OffsetType: GvarOffset>(
    gvar: &Gvar<'_>,
    plan: &Plan,
    num_glyphs: u16,
    s: &mut Serializer,
) -> Result<(), SubsetError> {
    // calculate sharedTuplesOffset
    // shared tuples array follow the GlyphVariationData offsets array at the end of the 'gvar' header.
    let off_size = size_of::<OffsetType>();

    let glyph_var_data_offset_array_size = (num_glyphs as u32 + 1) * off_size as u32;
    let shared_tuples_offset =
        if gvar.shared_tuple_count() == 0 || gvar.shared_tuples_offset().is_null() {
            0_u32
        } else {
            FIXED_HEADER_SIZE + glyph_var_data_offset_array_size
        };

    //update sharedTuplesOffset, which is of Offset32 type and byte position in gvar is 8..12
    s.copy_assign(
        gvar.shape().shared_tuples_offset_byte_range().start,
        shared_tuples_offset,
    );

    // calculate glyphVariationDataArrayOffset: put the glyphVariationData at last in the table
    let shared_tuples_size = 2 * gvar.axis_count() * gvar.shared_tuple_count();
    let glyph_var_data_offset =
        FIXED_HEADER_SIZE + glyph_var_data_offset_array_size + shared_tuples_size as u32;
    s.embed(glyph_var_data_offset)
        .map_err(|_| SubsetError::SubsetTableError(Gvar::TAG))?;

    //pre-allocate glyphVariationDataOffsets array
    let mut start_idx = s
        .allocate_size(glyph_var_data_offset_array_size as usize, false)
        .map_err(|_| SubsetError::SubsetTableError(Gvar::TAG))?;

    // shared tuples array
    if shared_tuples_offset > 0 {
        let offset = gvar.shared_tuples_offset().to_u32() as usize;
        let shared_tuples_data = gvar
            .offset_data()
            .as_bytes()
            .get(offset..offset + shared_tuples_size as usize)
            .ok_or(SubsetError::SubsetTableError(Gvar::TAG))?;
        s.embed_bytes(shared_tuples_data)
            .map_err(|_| SubsetError::SubsetTableError(Gvar::TAG))?;
    }

    // GlyphVariationData table array, also update glyphVariationDataOffsets
    start_idx += off_size;

    let mut glyph_offset = 0_u32;
    let mut last = 0;
    for (new_gid, old_gid) in plan.new_to_old_gid_list.iter().filter(|x| {
        x.0 != GlyphId::NOTDEF
            || plan
                .subset_flags
                .contains(SubsetFlags::SUBSET_FLAGS_NOTDEF_OUTLINE)
    }) {
        let last_gid = last;
        for _ in last_gid..new_gid.to_u32() {
            s.copy_assign(start_idx, OffsetType::stored_value(glyph_offset));
            start_idx += off_size;
            last += 1;
        }

        if let Some(glyph_var_data) = gvar
            .data_for_gid(*old_gid)
            .map_err(|_| SubsetError::SubsetTableError(Gvar::TAG))?
        {
            s.embed_bytes(glyph_var_data.as_bytes())
                .map_err(|_| SubsetError::SubsetTableError(Gvar::TAG))?;

            let len = glyph_var_data.len();
            glyph_offset += len as u32;
            // padding when short offset format is used
            if off_size == 2 && len % 2 != 0 {
                s.embed(1_u8)
                    .map_err(|_| SubsetError::SubsetTableError(Gvar::TAG))?;
                glyph_offset += 1;
            }
        };

        s.copy_assign(start_idx, OffsetType::stored_value(glyph_offset));
        start_idx += off_size;

        last += 1;
    }

    for _ in last..plan.num_output_glyphs as u32 {
        s.copy_assign(start_idx, OffsetType::stored_value(glyph_offset));
        start_idx += off_size;
    }

    Ok(())
}

trait GvarOffset: Scalar {
    fn stored_value(val: u32) -> Self;
}

impl GvarOffset for u16 {
    fn stored_value(val: u32) -> u16 {
        (val / 2) as u16
    }
}

impl GvarOffset for u32 {
    fn stored_value(val: u32) -> u32 {
        val
    }
}
