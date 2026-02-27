//! impl subset() for cvar table
use crate::{
    serialize::{SerializeErrorFlags, Serializer},
    variations::TupleVariations,
    Plan, Subset, SubsetError, CVT,
};

use write_fonts::{
    read::{tables::cvar::Cvar, FontRef, TableProvider, TopLevelTable},
    types::FWord,
    FontBuilder,
};

// reference: subset() for cvar table in harfbuzz
// https://github.com/harfbuzz/harfbuzz/blob/main/src/hb-ot-var-cvar-table.hh
impl Subset for Cvar<'_> {
    fn subset(
        &self,
        plan: &Plan,
        font: &FontRef,
        _s: &mut Serializer,
        builder: &mut FontBuilder,
    ) -> Result<(), SubsetError> {
        if plan.all_axes_pinned {
            // All axes pinned - need to instantiate fully and apply deltas to CVT
            log::trace!("Fully instantiating cvar and applying deltas to CVT table.");
            return instantiate_cvar_fully(self, plan, font, builder)
                .map_err(|_| SubsetError::SubsetTableError(Cvar::TAG));
        }

        if !plan.normalized_coords.is_empty() {
            // Partial instantiation - create new cvar with updated tuples
            log::trace!("Partially instantiating cvar table.");
            return instantiate_cvar_partially(self, plan, font, builder)
                .map_err(|_| SubsetError::SubsetTableError(Cvar::TAG));
        }

        // No instantiation needed - passthrough
        // This would be handled by the passthrough logic in the main subset function
        log::trace!("No instantiation needed for cvar, should be passed through.");
        Err(SubsetError::SubsetTableError(Cvar::TAG))
    }
}

fn instantiate_cvar_fully(
    cvar: &Cvar<'_>,
    plan: &Plan,
    font: &FontRef,
    builder: &mut FontBuilder,
) -> Result<(), SerializeErrorFlags> {
    // When all axes are pinned, we apply the deltas to the CVT table
    // and drop the cvar table entirely

    // Get the CVT table
    let cvt_data = font
        .data_for_tag(CVT)
        .ok_or(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)?;

    let cvt_blob = cvt_data.as_bytes();
    let num_cvt_items = cvt_blob.len() / std::mem::size_of::<FWord>();

    if num_cvt_items == 0 {
        log::trace!("No CVT items to instantiate");
        return Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY);
    }

    // Parse the CVT table values
    let mut cvt_values: Vec<i16> = Vec::with_capacity(num_cvt_items);
    for chunk in cvt_blob.chunks_exact(2) {
        let value = i16::from_be_bytes([chunk[0], chunk[1]]);
        cvt_values.push(value);
    }

    // Get the cvar variation data
    let axis_count = plan.axis_tags.len() as u16;
    if let Some(tuple_var_data) = cvar.variation_data(axis_count).ok() {
        let mut tuple_variations: TupleVariations = TupleVariations::from_cvar(
            tuple_var_data,
            num_cvt_items,
            &plan.axes_old_index_tag_map,
        )?;

        log::debug!(
            "Instantiating cvar with normalized coords {:?}, axes_location {:?} and axes_triple_distances: {:?}",
            plan.normalized_coords,
            plan.axes_location,
            plan.axes_triple_distances
        );

        // For cvar, we pass None for contour_points since CVT values are 1D
        // and we don't need IUP optimization
        tuple_variations.instantiate(
            &plan.axes_location,
            &plan.axes_triple_distances,
            None,  // No contour points for CVT
            false, // No IUP optimization for CVT
        )?;

        // Apply the deltas to the CVT values
        tuple_variations.apply_cvt_deltas(&mut cvt_values);
    }

    // Write the new CVT table
    add_cvt_table(builder, &cvt_values)?;

    // Don't add cvar table - it's fully instantiated
    log::trace!("Dropping cvar table after full instantiation");
    Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY)
}

fn instantiate_cvar_partially(
    cvar: &Cvar<'_>,
    plan: &Plan,
    font: &FontRef,
    _builder: &mut FontBuilder,
) -> Result<(), SerializeErrorFlags> {
    // Partial instantiation - some axes pinned, others remain variable

    // Get the CVT table to determine point count
    let cvt_data = font
        .data_for_tag(CVT)
        .ok_or(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)?;

    let cvt_blob = cvt_data.as_bytes();
    let num_cvt_items = cvt_blob.len() / std::mem::size_of::<FWord>();

    if num_cvt_items == 0 {
        log::trace!("No CVT items to instantiate");
        return Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY);
    }

    // Get the cvar variation data
    let axis_count = plan.axis_tags.len() as u16;
    let tuple_var_data = cvar
        .variation_data(axis_count)
        .map_err(|_| SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)?;

    let mut tuple_variations: TupleVariations =
        TupleVariations::from_cvar(tuple_var_data, num_cvt_items, &plan.axes_old_index_tag_map)?;

    log::debug!(
        "Partially instantiating cvar with normalized coords {:?}, axes_location {:?} and axes_triple_distances: {:?}",
        plan.normalized_coords,
        plan.axes_location,
        plan.axes_triple_distances
    );

    // For cvar, we pass None for contour_points since CVT values are 1D
    tuple_variations.instantiate(
        &plan.axes_location,
        &plan.axes_triple_distances,
        None,  // No contour points for CVT
        false, // No IUP optimization for CVT
    )?;

    // Get retained axis tags for normalization
    let retained_axis_tags = plan
        .axis_tags
        .iter()
        .enumerate()
        .filter(|(ix, _)| plan.axes_index_map.contains_key(ix))
        .map(|(_, tag)| *tag)
        .collect::<Vec<_>>();

    // Normalize axes: ensure all tuples have the same set of axes
    tuple_variations.normalize_axes(&retained_axis_tags);

    if tuple_variations.is_empty() {
        log::trace!("No variations remain after partial instantiation, dropping cvar");
        return Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY);
    }

    // Serialize the new cvar table
    // TODO: Use write-fonts to create a proper Cvar table
    // For now, this is a placeholder - we need to implement CvtDeltas conversion
    // similar to how gvar uses GlyphDeltas

    log::warn!("Partial cvar instantiation serialization not yet fully implemented");

    // This is where we would create a new Cvar table with the instantiated variations
    // using write_fonts::tables::cvar::Cvar or similar
    // For now, return an error to indicate this needs implementation

    Err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER)
}

fn add_cvt_table(builder: &mut FontBuilder, cvt_values: &[i16]) -> Result<(), SerializeErrorFlags> {
    // Serialize CVT values as big-endian i16 values
    let mut cvt_data = Vec::with_capacity(cvt_values.len() * 2);
    for &value in cvt_values {
        cvt_data.extend_from_slice(&value.to_be_bytes());
    }

    builder.add_raw(CVT, cvt_data);
    Ok(())
}
