//! impl subset() for gvar table
use std::mem::size_of;

use crate::{
    glyf_loca::ContourPoints,
    serialize::{SerializeErrorFlags, Serializer},
    variations::solver::{Triple, TripleDistances},
    Plan, Subset, SubsetError, SubsetFlags,
};

use fnv::FnvHashMap;
use skrifa::{
    raw::tables::{
        gvar::GlyphDelta,
        variations::{TupleVariation, TupleVariationData},
    },
    Tag,
};
use write_fonts::{
    read::{tables::gvar::Gvar, types::GlyphId, FontRef, TopLevelTable},
    tables::gvar::{
        GlyphDelta as WriteGlyphDelta, GlyphDeltas, GlyphVariations, Gvar as WriteGvar,
    },
    types::{F2Dot14, Scalar},
    FontBuilder,
};

const FIXED_HEADER_SIZE: u32 = 20;

/// Newtype wrapper for Vec<Tent> that implements ordering matching Harfbuzz:
/// - Regular tuples (no intermediate) come before intermediate tuples
/// - Within each group, sort by peak value ascending
/// - Then sort by axis order (already maintained by iteration order)
///
/// Since we can't directly inspect Tent's intermediate field, we track whether
/// this tuple has intermediate coordinates separately.
#[derive(Debug, Clone, PartialEq, Eq)]
struct TupleSortKey {
    tents: Vec<write_fonts::tables::gvar::Tent>,
    has_intermediate: bool,
}

impl TupleSortKey {
    fn new(tents: Vec<write_fonts::tables::gvar::Tent>) -> Self {
        // We determine if this tuple has intermediate coordinates by examining
        // how many tents it has and their structure. For now, we'll use a simple
        // heuristic that we can refine later.
        // Actually, we don't have a way to check this without serialize/deserialize
        // Let's use a different approach - store this info when we create the tent.
        Self {
            tents,
            has_intermediate: false,
        }
    }

    fn with_intermediate_flag(mut self, has_intermediate: bool) -> Self {
        self.has_intermediate = has_intermediate;
        self
    }
}

impl Ord for TupleSortKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering;

        // Regular tuples (no intermediate) < intermediate tuples
        match (self.has_intermediate, other.has_intermediate) {
            (false, true) => return Ordering::Less,
            (true, false) => return Ordering::Greater,
            _ => {}
        }
        // Lowest order first
        if self.tents.len() != other.tents.len() {
            self.tents.len().cmp(&other.tents.len())
        } else {
            self.tents.cmp(&other.tents)
        }
    }
}

impl PartialOrd for TupleSortKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

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

    for (new_gid, old_gid) in plan.new_to_old_gid_list.iter() {
        if let Some(glyph_var) = gvar
            .glyph_variation_data(*old_gid)
            .map_err(|_| SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)?
        {
            if let Some(all_points) = plan.new_gid_contour_points_map.get(new_gid) {
                let new_var = instantiate_tuple(
                    glyph_var,
                    &plan.axes_location,
                    &plan.axes_triple_distances,
                    &plan.axes_old_index_tag_map,
                    &plan.axis_tags,
                    all_points,
                    plan.subset_flags
                        .contains(SubsetFlags::SUBSET_FLAGS_OPTIMIZE_IUP_DELTAS),
                );
                new_variations.push(GlyphVariations::new(*new_gid, new_var));
            } else {
                // Can't happen
                return Err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER);
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

fn instantiate_tuple(
    variation_data: TupleVariationData<'_, GlyphDelta>,
    normalized_axes_location: &FnvHashMap<Tag, Triple<f64>>,
    axes_triple_distances: &FnvHashMap<Tag, TripleDistances<f64>>,
    axes_old_index_tag_map: &FnvHashMap<usize, Tag>,
    axis_tags: &[Tag],
    all_points: &ContourPoints,
    _optimize: bool,
) -> Vec<GlyphDeltas> {
    if variation_data.tuples().count() == 0 {
        return vec![];
    }

    let tuples: Vec<_> = variation_data.tuples().collect();
    if tuples.is_empty() {
        return vec![];
    }

    // Point count includes all glyph points plus 4 phantom points
    let point_count = all_points.0.len();

    // First pass: collect all rebased tuples with their deltas
    // Key is tents (normalized for comparison), value is accumulated deltas
    // Use BTreeMap for deterministic ordering matching Harfbuzz
    use std::collections::BTreeMap;
    let mut tuple_map: BTreeMap<TupleSortKey, (Vec<f64>, Vec<f64>)> = BTreeMap::new();

    for tuple in tuples {
        // Change tuple variations axis limits (rebase using axis instantiation)
        let processed = change_tuple_variations_axis_limits(
            &tuple,
            normalized_axes_location,
            axes_triple_distances,
            axes_old_index_tag_map,
            axis_tags,
        );

        for (scalar, tents, has_intermediate) in processed {
            // If all axes are pinned (empty tents), skip this tuple
            // The deltas would be baked into the base glyph coordinates
            if tents.is_empty() {
                continue;
            }

            // Create deltas array and track which points have explicit deltas
            let mut deltas_x = vec![0.0f64; point_count];
            let mut deltas_y = vec![0.0f64; point_count];
            let mut has_delta = vec![false; point_count];

            // Apply explicit deltas from the tuple
            for delta in tuple.deltas() {
                let index = delta.position as usize;
                if index >= point_count {
                    continue;
                }
                deltas_x[index] = (delta.x_delta as f32 * scalar) as f64;
                deltas_y[index] = (delta.y_delta as f32 * scalar) as f64;
                has_delta[index] = true;
            }

            // Calculate inferred deltas for unreferenced points using IUP
            calc_inferred_deltas(&mut deltas_x, &mut deltas_y, &mut has_delta, all_points);

            // Merge with existing tuples that have the same tents
            tuple_map
                .entry(TupleSortKey {
                    tents,
                    has_intermediate,
                })
                .and_modify(|(accum_x, accum_y)| {
                    for i in 0..point_count {
                        accum_x[i] += deltas_x[i];
                        accum_y[i] += deltas_y[i];
                    }
                })
                .or_insert((deltas_x, deltas_y));
        }
    }

    // Second pass: convert accumulated deltas to GlyphDeltas
    let result_variations: Vec<GlyphDeltas> = tuple_map
        .into_iter()
        .map(|(sort_key, (deltas_x, deltas_y))| {
            let scaled_deltas: Vec<WriteGlyphDelta> = deltas_x
                .into_iter()
                .zip(deltas_y)
                .map(|(x, y)| WriteGlyphDelta::new(x.round() as i16, y.round() as i16, true))
                .collect();
            GlyphDeltas::new(sort_key.tents, scaled_deltas)
        })
        .collect();

    result_variations
}

/// Result of rebasing a tuple: (scalar, tents, has_intermediate_coords)
type RebasedTuples = Vec<(f32, Vec<write_fonts::tables::gvar::Tent>, bool)>;

/// Change tuple variations by instantiating at the given axis limits
fn change_tuple_variations_axis_limits(
    tuple: &TupleVariation<'_, GlyphDelta>,
    normalized_axes_location: &FnvHashMap<Tag, Triple<f64>>,
    axes_triple_distances: &FnvHashMap<Tag, TripleDistances<f64>>,
    axes_old_index_tag_map: &FnvHashMap<usize, Tag>,
    axis_tags: &[Tag],
) -> RebasedTuples {
    use crate::variations::solver::rebase_tent;

    let peak = tuple.peak();
    let inter_start = tuple.intermediate_start();
    let inter_end = tuple.intermediate_end();

    // If no axes being instanced, return single result
    if normalized_axes_location.is_empty() {
        let (tents, has_intermediate) = convert_peak_to_tents(&peak, &inter_start, &inter_end);
        return vec![(1.0, tents, has_intermediate)];
    }

    // Sort axes for deterministic processing
    let sorted_axes: Vec<Tag> = {
        let mut axes: Vec<Tag> = normalized_axes_location.keys().copied().collect();
        axes.sort();
        axes
    };

    // Start with the original tuple represented by one result
    let mut results: Vec<(f32, FnvHashMap<Tag, Triple<f64>>)> = vec![(1.0, {
        let mut m = FnvHashMap::default();
        for idx in 0..peak.len() {
            let axis_tag = match axes_old_index_tag_map.get(&idx) {
                Some(tag) => *tag,
                None => continue,
            };
            if let Some(peak_val) = peak.get(idx) {
                let min_val = inter_start
                    .as_ref()
                    .and_then(|t| t.get(idx))
                    .map(|v| v.to_f32())
                    .unwrap_or_else(|| {
                        let p = peak_val.to_f32();
                        if p > 0.0 {
                            0.0
                        } else {
                            p
                        }
                    });

                let max_val = inter_end
                    .as_ref()
                    .and_then(|t| t.get(idx))
                    .map(|v| v.to_f32())
                    .unwrap_or_else(|| {
                        let p = peak_val.to_f32();
                        if p < 0.0 {
                            0.0
                        } else {
                            p
                        }
                    });

                m.insert(
                    axis_tag,
                    Triple::new(min_val.into(), peak_val.to_f32().into(), max_val.into()),
                );
            }
        }
        m
    })];

    // Process each axis
    for axis_tag in &sorted_axes {
        let mut new_results = Vec::new();

        for (scalar, current_tents) in results {
            // First, remove any axes with zero peaks from current_tents
            let mut current_tents = current_tents;
            current_tents.retain(|_, tent| tent.middle.abs() >= 1e-6);

            if let Some(axis_limit) = normalized_axes_location.get(axis_tag) {
                let axis_distances = axes_triple_distances
                    .get(axis_tag)
                    .copied()
                    .unwrap_or(TripleDistances::new(1.0, 1.0));

                if let Some(tent) = current_tents.get(axis_tag).copied() {
                    if (tent.minimum < 0.0 && tent.maximum > 0.0)
                        || !(tent.minimum <= tent.middle && tent.middle <= tent.maximum)
                    {
                        continue;
                    }
                    if tent.middle == 0.0 {
                        new_results.push((scalar, current_tents));
                        continue;
                    }
                    // Call rebase_tent to transform this tent
                    let rebased = rebase_tent(tent, *axis_limit, axis_distances);
                    log::trace!(
                        "Got {} solutions for new limits: min {}, peak {}, max {}",
                        rebased.len(),
                        axis_limit.minimum,
                        axis_limit.middle,
                        axis_limit.maximum
                    );

                    for (sub_scalar, new_tent) in rebased {
                        log::trace!(
                            "Solution: scalar {}, min {}, peak {}, max {}",
                            sub_scalar,
                            new_tent.minimum,
                            new_tent.middle,
                            new_tent.maximum
                        );
                        let mut new_tents = current_tents.clone();
                        // Remove this axis if tent has zero peak (no variation)
                        // or if tent is exactly default (pinned)
                        if new_tent.middle.abs() < 1e-6 || new_tent == Triple::default() {
                            new_tents.remove(axis_tag);
                        } else {
                            new_tents.insert(*axis_tag, new_tent);
                        }
                        new_results.push((scalar * sub_scalar as f32, new_tents));
                    }
                } else {
                    // Axis not in this tuple, keep as is
                    new_results.push((scalar, current_tents));
                }
            } else {
                new_results.push((scalar, current_tents));
            }
        }

        results = new_results;
    }

    // Convert results to tents format
    results
        .into_iter()
        .map(|(scalar, tent_map)| {
            let (tents, has_intermediate) =
                convert_tent_map_to_tents_with_flag(axis_tags, tent_map);
            (scalar, tents, has_intermediate)
        })
        .collect()
}

/// Convert peak tuple and intermediate tuples to tents, and determine if it has intermediate coords
fn convert_peak_to_tents(
    peak: &skrifa::raw::tables::variations::Tuple<'_>,
    inter_start: &Option<skrifa::raw::tables::variations::Tuple<'_>>,
    inter_end: &Option<skrifa::raw::tables::variations::Tuple<'_>>,
) -> (Vec<write_fonts::tables::gvar::Tent>, bool) {
    let mut tents = Vec::new();
    let mut has_intermediate = false;

    for idx in 0..peak.len() {
        if let Some(peak_val) = peak.get(idx) {
            let peak_f32 = peak_val.to_f32();
            let peak_f2dot14 = F2Dot14::from_f32(peak_f32);

            let min_val = inter_start
                .as_ref()
                .and_then(|t| t.get(idx))
                .map(|v| v.to_f32())
                .unwrap_or_else(|| if peak_f32 > 0.0 { 0.0 } else { peak_f32 });

            let max_val = inter_end
                .as_ref()
                .and_then(|t| t.get(idx))
                .map(|v| v.to_f32())
                .unwrap_or_else(|| if peak_f32 < 0.0 { 0.0 } else { peak_f32 });

            let min_f2dot14 = F2Dot14::from_f32(min_val);
            let max_f2dot14 = F2Dot14::from_f32(max_val);

            // Check if we need explicit intermediate values
            let inferred_min = if peak_f32 > 0.0 { 0.0 } else { peak_f32 };
            let inferred_max = if peak_f32 < 0.0 { 0.0 } else { peak_f32 };

            let intermediate = if (min_val - inferred_min).abs() > 0.001
                || (max_val - inferred_max).abs() > 0.001
            {
                has_intermediate = true;
                Some((min_f2dot14, max_f2dot14))
            } else {
                None
            };

            tents.push(write_fonts::tables::gvar::Tent::new(
                peak_f2dot14,
                intermediate,
            ));
        }
    }

    (tents, has_intermediate)
}

/// Convert a tent map back to tents vector, and determine if it has intermediate coordinates
fn convert_tent_map_to_tents_with_flag(
    axis_tags: &[Tag],
    tent_map: FnvHashMap<Tag, Triple<f64>>,
) -> (Vec<write_fonts::tables::gvar::Tent>, bool) {
    let mut has_intermediate = false;

    let tents: Vec<_> = axis_tags
        .iter()
        .filter_map(|axis_tag| {
            if let Some(tent) = tent_map.get(axis_tag) {
                let peak = F2Dot14::from_f32(tent.middle as f32);
                let min = F2Dot14::from_f32(tent.minimum as f32);
                let max = F2Dot14::from_f32(tent.maximum as f32);

                // Check if we need explicit intermediate values
                let inferred_min = if tent.middle > 0.0 { 0.0 } else { tent.middle };
                let inferred_max = if tent.middle < 0.0 { 0.0 } else { tent.middle };

                let intermediate = if (tent.minimum - inferred_min).abs() > 0.001
                    || (tent.maximum - inferred_max).abs() > 0.001
                {
                    has_intermediate = true;
                    Some((min, max))
                } else {
                    None
                };

                Some(write_fonts::tables::gvar::Tent::new(peak, intermediate))
            } else {
                None
            }
        })
        .collect();

    (tents, has_intermediate)
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

/// Calculate inferred deltas for unreferenced points using IUP (Interpolate Unlisted Points).
/// This is a port of Harfbuzz's calc_inferred_deltas function.
fn calc_inferred_deltas(
    deltas_x: &mut [f64],
    deltas_y: &mut [f64],
    has_delta: &mut [bool],
    all_points: &ContourPoints,
) {
    let point_count = all_points.0.len();
    if point_count != deltas_x.len()
        || point_count != deltas_y.len()
        || point_count != has_delta.len()
    {
        return;
    }

    // Count referenced points
    let ref_count = has_delta.iter().filter(|&&x| x).count();

    // All points are referenced, nothing to do
    if ref_count == point_count {
        return;
    }

    // Extract end points (contour ends)
    let end_points: Vec<usize> = all_points
        .0
        .iter()
        .enumerate()
        .filter_map(|(i, p)| if p.is_end_point { Some(i) } else { None })
        .collect();

    // Process each contour
    let mut start_point = 0;
    for &end_point in &end_points {
        // Check the number of unreferenced points in this contour
        let mut unref_count = 0;
        for i in start_point..=end_point {
            if !has_delta[i] {
                unref_count += 1;
            }
        }

        // If no unreferenced points or all points unreferenced, skip this contour
        if unref_count == 0 || unref_count > end_point - start_point {
            start_point = end_point + 1;
            continue;
        }

        // Find gaps of unreferenced points between referenced points
        let mut j = start_point;
        loop {
            // Find start of gap (prev = last referenced point before gap)
            let mut i = j;
            let mut prev = 0;
            loop {
                i = j;
                j = next_index(i, start_point, end_point);
                if has_delta[i] && !has_delta[j] {
                    prev = i;
                    break;
                }
            }

            // Find end of gap (next = first referenced point after gap)
            let mut next = 0;
            loop {
                i = j;
                j = next_index(i, start_point, end_point);
                if !has_delta[i] && has_delta[j] {
                    next = j;
                    break;
                }
            }

            // Infer deltas for all unreferenced points in gap between prev and next
            i = prev;
            loop {
                i = next_index(i, start_point, end_point);
                if i == next {
                    break;
                }

                deltas_x[i] = infer_delta(
                    all_points.0[i].x as f64,
                    all_points.0[prev].x as f64,
                    all_points.0[next].x as f64,
                    deltas_x[prev],
                    deltas_x[next],
                );
                deltas_y[i] = infer_delta(
                    all_points.0[i].y as f64,
                    all_points.0[prev].y as f64,
                    all_points.0[next].y as f64,
                    deltas_y[prev],
                    deltas_y[next],
                );

                // Mark this point as having an inferred delta
                has_delta[i] = true;

                unref_count -= 1;
                if unref_count == 0 {
                    break;
                }
            }

            if unref_count == 0 {
                break;
            }
        }

        start_point = end_point + 1;
    }

    // Set remaining unreferenced points (those not inferred) to 0
    // and mark all points as referenced for gvar output
    for i in 0..point_count {
        if !has_delta[i] {
            deltas_x[i] = 0.0;
            deltas_y[i] = 0.0;
            has_delta[i] = true;
        }
    }
}

/// Infer a delta value for a point using linear interpolation between two reference points.
/// This is a port of Harfbuzz's infer_delta function.
fn infer_delta(
    target_val: f64,
    prev_val: f64,
    next_val: f64,
    prev_delta: f64,
    next_delta: f64,
) -> f64 {
    if prev_val == next_val {
        // Same position - use delta if they match, otherwise 0
        if prev_delta == next_delta {
            prev_delta
        } else {
            0.0
        }
    } else if target_val <= prev_val.min(next_val) {
        // Target is before/at both reference points
        if prev_val < next_val {
            prev_delta
        } else {
            next_delta
        }
    } else if target_val >= prev_val.max(next_val) {
        // Target is after/at both reference points
        if prev_val > next_val {
            prev_delta
        } else {
            next_delta
        }
    } else {
        // Target is between reference points - linear interpolation
        let r = (target_val - prev_val) / (next_val - prev_val);
        prev_delta + r * (next_delta - prev_delta)
    }
}

/// Get next index in circular contour (wraps around at end).
fn next_index(i: usize, start: usize, end: usize) -> usize {
    if i >= end {
        start
    } else {
        i + 1
    }
}
