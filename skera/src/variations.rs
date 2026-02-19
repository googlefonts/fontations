//! impl Subset for OpenType font variations common tables.
use crate::{
    glyf_loca::{ContourPoint, ContourPoints},
    inc_bimap::IncBiMap,
    offset::SerializeSubset,
    serialize::{SerializeErrorFlags, Serializer},
    variations::solver::{rebase_tent, Triple, TripleDistances},
    Plan, SubsetTable,
};
use fnv::FnvHashMap;
use font_types::{FixedSize, Point};
use skrifa::{
    raw::tables::{
        cvar::CvtDelta,
        gvar::GlyphDelta,
        variations::{RegionAxisCoordinates, TupleVariation, TupleVariationData},
    },
    Tag,
};
use std::{
    cmp::Ordering,
    collections::{BTreeMap, HashMap},
    hash::{Hash, Hasher},
    ops::{AddAssign, MulAssign},
    vec,
};
use write_fonts::{
    read::{
        collections::IntSet,
        tables::variations::{
            DeltaSetIndexMap, ItemVariationData, ItemVariationStore, VariationRegionList,
        },
    },
    tables::gvar::{GlyphDelta as WriteGlyphDelta, GlyphDeltas},
    types::{BigEndian, F2Dot14, Offset32},
    OtRound,
};

pub(crate) mod solver;

/// Hashable wrapper around a region (axis coordinates map).
/// Implements Hash for use as a HashMap key by hashing entries in sorted order.
#[derive(Clone, Debug, Default)]
struct Region(FnvHashMap<Tag, Triple<f64>>);

impl Hash for Region {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Hash entries in sorted order for deterministic hashing
        let mut entries: Vec<_> = self.0.iter().collect();
        entries.sort_by_key(|&(tag, _)| tag);
        for (tag, triple) in entries {
            tag.hash(state);
            triple.minimum.to_bits().hash(state);
            triple.middle.to_bits().hash(state);
            triple.maximum.to_bits().hash(state);
        }
    }
}

impl Region {
    fn new() -> Self {
        Region(FnvHashMap::default())
    }

    fn insert(&mut self, tag: Tag, triple: Triple<f64>) {
        self.0.insert(tag, triple);
    }

    fn get(&self, tag: &Tag) -> Option<&Triple<f64>> {
        self.0.get(tag)
    }

    fn contains_key(&self, tag: &Tag) -> bool {
        self.0.contains_key(tag)
    }

    fn remove(&mut self, tag: &Tag) -> Option<Triple<f64>> {
        self.0.remove(tag)
    }

    fn len(&self) -> usize {
        self.0.len()
    }

    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn iter(&self) -> impl Iterator<Item = (&Tag, &Triple<f64>)> {
        self.0.iter()
    }

    /// Maps axis indices to tags using the axes_old_index_tag_map
    fn from_readfonts_tuple<T: skrifa::raw::tables::variations::TupleDelta>(
        gvar_tuple: TupleVariation<'_, T>,
        axes_old_index_tag_map: &FnvHashMap<usize, Tag>,
    ) -> Result<Self, SerializeErrorFlags> {
        let region = (0..axes_old_index_tag_map.len())
            .map(|axis_index| {
                let tag = axes_old_index_tag_map
                    .get(&axis_index)
                    .ok_or(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)?;
                let peak = gvar_tuple
                    .peak()
                    .values
                    .get(axis_index)
                    .ok_or(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)?
                    .get()
                    .to_f32() as f64;
                let min_value = if let Some(start_tuple) = gvar_tuple.intermediate_start() {
                    start_tuple
                        .values
                        .get(axis_index)
                        .ok_or(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)?
                        .get()
                        .to_f32() as f64
                } else {
                    // For positive peaks, inferred min is 0; for negative peaks, it's the peak itself
                    if peak > 0.0 {
                        0.0
                    } else {
                        peak
                    }
                };
                let max_value = if let Some(end_tuple) = gvar_tuple.intermediate_end() {
                    end_tuple
                        .values
                        .get(axis_index)
                        .ok_or(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)?
                        .get()
                        .to_f32() as f64
                } else {
                    // For negative peaks, inferred max is 0; for positive peaks, it's the peak itself
                    if peak < 0.0 {
                        0.0
                    } else {
                        peak
                    }
                };
                Ok((
                    *tag,
                    Triple {
                        minimum: min_value,
                        middle: peak,
                        maximum: max_value,
                    },
                ))
            })
            .collect::<Result<FnvHashMap<_, _>, SerializeErrorFlags>>()?;
        Ok(Region(region))
    }

    fn to_tents(&self, axis_order: &[Tag]) -> Vec<write_fonts::tables::gvar::Tent> {
        axis_order
            .iter()
            .filter_map(|axis_tag| {
                if let Some(tent) = self.0.get(axis_tag) {
                    let peak = F2Dot14::from_f32(tent.middle as f32);
                    let min = F2Dot14::from_f32(tent.minimum as f32);
                    let max = F2Dot14::from_f32(tent.maximum as f32);

                    // Check if we need explicit intermediate values
                    let inferred_min = if tent.middle > 0.0 { 0.0 } else { tent.middle };
                    let inferred_max = if tent.middle < 0.0 { 0.0 } else { tent.middle };

                    let intermediate = if (tent.minimum - inferred_min).abs() > 0.001
                        || (tent.maximum - inferred_max).abs() > 0.001
                    {
                        Some((min, max))
                    } else {
                        None
                    };

                    Some(write_fonts::tables::gvar::Tent::new(peak, intermediate))
                } else {
                    None
                }
            })
            .collect()
    }
}

impl PartialEq for Region {
    fn eq(&self, other: &Self) -> bool {
        if self.0.len() != other.0.len() {
            return false;
        }
        for (tag, triple) in &self.0 {
            match other.0.get(tag) {
                Some(other_triple) if triple == other_triple => continue,
                _ => return false,
            }
        }
        true
    }
}

impl Eq for Region {}

/// Represents a single tuple variation: region coordinates + deltas.
/// Corresponds to Harfbuzz's tuple_delta_t.
#[derive(Debug, Clone, Default)]
struct TupleDelta {
    axis_tuples: Region,
    indices: Vec<bool>,
    deltas_x: Vec<f32>,
    deltas_y: Vec<f32>,
}
impl TupleDelta {
    // Corresponds to create_from_tuple_var_data in hb-ot-var-common.hh
    fn from_gvar_tuple(
        gvar_tuple: TupleVariation<'_, GlyphDelta>,
        point_count: usize,
        axes_old_index_tag_map: &FnvHashMap<usize, Tag>,
    ) -> Result<Self, SerializeErrorFlags> {
        let mut deltas = vec![Point::new(0.0, 0.0); point_count];
        let mut indices = vec![true; point_count];
        if gvar_tuple.has_deltas_for_all_points() {
            indices = vec![true; point_count];
            deltas = gvar_tuple
                .deltas()
                .map(|delta| Point::new(delta.x_delta as f32, delta.y_delta as f32))
                .collect();
        } else {
            indices = vec![false; point_count];
            deltas = vec![Point::new(0.0, 0.0); point_count];
            for delta in gvar_tuple.deltas() {
                let idx = delta.position as usize;
                if idx >= point_count {
                    return Err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR);
                }
                indices[idx] = true;
                deltas[idx] = Point::new(delta.x_delta as f32, delta.y_delta as f32);
            }
        }
        let tuple = TupleDelta {
            axis_tuples: Region::from_readfonts_tuple(gvar_tuple, axes_old_index_tag_map)?,
            indices,
            deltas_x: deltas.iter().map(|p| p.x).collect(),
            deltas_y: deltas.iter().map(|p| p.y).collect(),
        };
        Ok(tuple)
    }

    fn from_cvar_tuple(
        cvar_tuple: TupleVariation<'_, CvtDelta>,
        point_count: usize,
        axes_old_index_tag_map: &FnvHashMap<usize, Tag>,
    ) -> Result<Self, SerializeErrorFlags> {
        let orig_deltas = cvar_tuple.deltas().collect::<Vec<_>>();

        let (deltas, indices) = if cvar_tuple.has_deltas_for_all_points() {
            (
                orig_deltas.iter().map(|p| p.value as f32).collect(),
                vec![true; orig_deltas.len()],
            )
        } else {
            let mut indices = vec![false; point_count];
            let mut deltas = vec![0.0; point_count];
            for delta in orig_deltas {
                let idx = delta.position as usize;
                if idx >= point_count {
                    return Err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR);
                }
                indices[idx] = true;
                deltas[idx] = delta.value as f32;
            }
            (deltas, indices)
        };
        Ok(TupleDelta {
            axis_tuples: Region::from_readfonts_tuple(cvar_tuple, axes_old_index_tag_map)?,
            indices,
            deltas_x: deltas,
            deltas_y: vec![],
        })
    }

    fn to_glyph_deltas(&self, axis_tags: &[Tag]) -> GlyphDeltas {
        let mut deltas = vec![];
        for i in 0..self.deltas_x.len() {
            deltas.push(WriteGlyphDelta::new(
                self.deltas_x[i].ot_round(),
                self.deltas_y[i].ot_round(),
                self.indices[i],
            ));
        }
        let tents = self.axis_tuples.to_tents(axis_tags);
        GlyphDeltas::new(tents, deltas)
    }

    // Ported directly from harfbuzz
    fn change_tuple_var_axis_limit(
        self,
        axis_tag: Tag,
        axis_limit: Triple<f64>,
        axis_triple_distances: &TripleDistances<f64>,
    ) -> Option<Vec<TupleDelta>> {
        let mut out = vec![];
        let Some(tent) = self.axis_tuples.get(&axis_tag) else {
            return Some(vec![self]);
        };

        if (tent.minimum < 0.0 && tent.maximum > 0.0)
            || !(tent.minimum <= tent.middle && tent.middle <= tent.maximum)
        {
            return None;
        }

        if tent.middle == 0.0 {
            return Some(vec![self]);
        }

        let solutions = rebase_tent(*tent, axis_limit, *axis_triple_distances);
        for (scalar, triple) in solutions.into_iter() {
            let mut new_var = self.clone();
            if triple == Triple::default() {
                new_var.remove_axis(axis_tag);
            } else {
                new_var.axis_tuples.insert(axis_tag, triple);
            }
            new_var *= scalar;
            out.push(new_var);
        }
        Some(out)
    }

    fn remove_axis(&mut self, axis_tag: Tag) {
        self.axis_tuples.remove(&axis_tag);
    }

    // Ported directly from harfbuzz
    fn calc_inferred_deltas(
        &mut self,
        orig_points: &[ContourPoint],
    ) -> Result<(), SerializeErrorFlags> {
        let point_count = orig_points.len();
        if point_count != self.indices.len() {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER);
        }
        let mut ref_count = 0;
        let mut end_points = vec![];
        let mut inferred_indices = IntSet::empty();
        #[allow(clippy::indexing_slicing)] // We check bounds above
        for (i, point) in orig_points.iter().enumerate() {
            ref_count += self.indices[i] as usize;
            if point.is_end_point {
                end_points.push(i);
            }
        }
        if ref_count == point_count {
            // All points are referenced, nothing to do
            return Ok(());
        }

        let mut start_point = 0;
        for end_point in end_points {
            // Check the number of unreferenced points in a contour.
            // If no unref points or no ref points, nothing to do.
            let mut unref_count = self.indices[start_point..end_point + 1]
                .iter()
                .filter(|&&is_ref| !is_ref)
                .count();
            let mut j = start_point;

            if !(unref_count == 0 || unref_count > end_point - start_point) {
                // Outer loop to process multiple gaps in this contour
                loop {
                    let mut i;
                    loop {
                        i = j;
                        j = self.next_index(i, start_point, end_point);
                        if self.indices[i] && !self.indices[j] {
                            break;
                        }
                    }
                    let prev = i;
                    j = i;
                    loop {
                        i = j;
                        j = self.next_index(i, start_point, end_point);
                        if !self.indices[i] && self.indices[j] {
                            break;
                        }
                    }
                    let next = j;
                    // Infer deltas for all unref points in the gap between prev and next
                    i = prev;
                    loop {
                        i = self.next_index(i, start_point, end_point);
                        if i == next {
                            break;
                        }
                        self.deltas_x[i] = infer_delta(
                            orig_points[i].x,
                            orig_points[prev].x,
                            orig_points[next].x,
                            self.deltas_x[prev],
                            self.deltas_x[next],
                        );
                        self.deltas_y[i] = infer_delta(
                            orig_points[i].y,
                            orig_points[prev].y,
                            orig_points[next].y,
                            self.deltas_y[prev],
                            self.deltas_y[next],
                        );
                        inferred_indices.insert(i as u32);
                        unref_count -= 1;
                        if unref_count == 0 {
                            break;
                        }
                    }
                    if unref_count == 0 {
                        break;
                    }
                }
            }
            start_point = end_point + 1;
        }
        for i in 0..point_count {
            if !self.indices[i] {
                if !inferred_indices.contains(i as u32) {
                    // Unreferenced point that we couldn't infer, set delta to 0
                    self.deltas_x[i] = 0.0;
                    self.deltas_y[i] = 0.0;
                }
                self.indices[i] = true;
            }
        }
        Ok(())
    }

    fn next_index(&self, i: usize, start_point: usize, end_point: usize) -> usize {
        if i >= end_point {
            start_point
        } else {
            i + 1
        }
    }
}

impl AddAssign<&TupleDelta> for TupleDelta {
    fn add_assign(&mut self, rhs: &TupleDelta) {
        for i in 0..self.indices.len() {
            if self.indices[i] {
                if rhs.indices[i] {
                    self.deltas_x[i] += rhs.deltas_x[i];
                    if !self.deltas_y.is_empty() && !rhs.deltas_y.is_empty() {
                        self.deltas_y[i] += rhs.deltas_y[i];
                    }
                }
            } else {
                if !rhs.indices[i] {
                    continue;
                }
                self.indices[i] = true;
                self.deltas_x[i] = rhs.deltas_x[i];
                if !self.deltas_y.is_empty() && !rhs.deltas_y.is_empty() {
                    self.deltas_y[i] = rhs.deltas_y[i];
                }
            }
        }
    }
}

impl MulAssign<f64> for TupleDelta {
    fn mul_assign(&mut self, scalar: f64) {
        for delta in &mut self.deltas_x {
            *delta = (*delta as f64 * scalar) as f32;
        }
        for delta in &mut self.deltas_y {
            *delta = (*delta as f64 * scalar) as f32;
        }
    }
}

/// Collection of tuple variations for a VarData subtable.
/// Corresponds to Harfbuzz's tuple_variations_t.
#[derive(Debug, Clone)]
pub(crate) struct TupleVariations {
    tuple_vars: Vec<TupleDelta>,
}
impl TupleVariations {
    // Corresponds to harfbuzz decompile_tuple_variations
    pub fn from_gvar(
        value: TupleVariationData<'_, GlyphDelta>,
        point_count: usize,
        axes_old_index_tag_map: &FnvHashMap<usize, Tag>,
    ) -> Result<Self, SerializeErrorFlags> {
        Ok(TupleVariations {
            tuple_vars: value
                .tuples()
                .map(|gvar_tuple| {
                    TupleDelta::from_gvar_tuple(gvar_tuple, point_count, axes_old_index_tag_map)
                })
                .collect::<Result<Vec<_>, SerializeErrorFlags>>()?,
        })
    }

    pub fn from_cvar(
        value: TupleVariationData<'_, CvtDelta>,
        point_count: usize,
        axes_old_index_tag_map: &FnvHashMap<usize, Tag>,
    ) -> Result<Self, SerializeErrorFlags> {
        Ok(TupleVariations {
            tuple_vars: value
                .tuples()
                .map(|cvar_tuple| {
                    TupleDelta::from_cvar_tuple(cvar_tuple, point_count, axes_old_index_tag_map)
                })
                .collect::<Result<Vec<_>, SerializeErrorFlags>>()?,
        })
    }
    // Ported directly from harfbuzz
    pub fn instantiate(
        &mut self,
        normalized_axes_location: &FnvHashMap<Tag, Triple<f64>>,
        axes_triple_distances: &FnvHashMap<Tag, TripleDistances<f64>>,
        contour_points: Option<&ContourPoints>,
        optimize: bool,
    ) -> Result<(), SerializeErrorFlags> {
        if self.tuple_vars.is_empty() {
            return Ok(());
        }
        log::debug!(
            "TupleVariations::instantiate: {} tuples before rebasing",
            self.tuple_vars.len()
        );
        self.change_tuple_variations_axis_limits(normalized_axes_location, axes_triple_distances)?;
        log::debug!(
            "TupleVariations::instantiate: {} tuples after rebasing/axis limit changes",
            self.tuple_vars.len()
        );

        // compute inferred deltas only for gvar
        if let Some(ref cp) = contour_points {
            self.calc_inferred_deltas(&cp.0)?;
        } else if optimize {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER);
        }

        self.merge_tuple_variations(None)?;
        log::debug!(
            "TupleVariations::instantiate: {} tuples after merge",
            self.tuple_vars.len()
        );
        // self.merge_tuple_variations(if optimize { contour_points } else { None })?;

        // if optimize {
        //     iup_optimize(contour_points)?;
        // }
        Ok(())
    }

    fn merge_tuple_variations(
        &mut self,
        mut contour_points: Option<&mut ContourPoints>,
    ) -> Result<(), SerializeErrorFlags> {
        let mut new_vars: Vec<TupleDelta> = Vec::with_capacity(self.tuple_vars.len());
        // The pre-allocation is essential for address stability of pointers
        // we store in the hashmap.
        let mut m: HashMap<Region, usize> = HashMap::with_capacity(self.tuple_vars.len());
        for mut var in self.tuple_vars.iter().cloned() {
            // if all axes are pinned, drop the tuple variation
            if var.axis_tuples.is_empty() {
                // if iup_delta_optimize is enabled, add deltas to contour coords
                if let Some(ref mut cp) = contour_points {
                    cp.add_deltas_with_indices(&var.deltas_x, &var.deltas_y, &var.indices);
                }
                continue;
            }

            // Clean up pinned axes (triple=(0,0,0)) from the region.
            // These don't contribute any variation and should be removed.
            let tags_to_remove: Vec<_> = var
                .axis_tuples
                .0
                .iter()
                .filter(|(_, triple)| {
                    triple.minimum == 0.0 && triple.middle == 0.0 && triple.maximum == 0.0
                })
                .map(|(tag, _)| *tag)
                .collect();

            for tag in tags_to_remove {
                var.axis_tuples.0.remove(&tag);
            }

            // After cleaning, check if any axes  remain
            if var.axis_tuples.is_empty() {
                // Fully pinned region after cleaning, drop it
                continue;
            }

            if let Some(idx) = m.get(&var.axis_tuples) {
                new_vars[*idx] += &var;
            } else {
                new_vars.push(var.clone());
                let new_idx = new_vars.len() - 1;
                m.insert(var.axis_tuples.clone(), new_idx);
            }
        }
        self.tuple_vars = new_vars;

        Ok(())
    }

    // Ported directly from harfbuzz
    fn calc_inferred_deltas(
        &mut self,
        contour_points: &[ContourPoint],
    ) -> Result<(), SerializeErrorFlags> {
        for var in &mut self.tuple_vars {
            var.calc_inferred_deltas(contour_points)?;
        }
        Ok(())
    }

    // Ported directly from harfbuzz
    fn change_tuple_variations_axis_limits(
        &mut self,
        normalized_axes_location: &FnvHashMap<Tag, Triple<f64>>,
        axes_triple_distances: &FnvHashMap<Tag, TripleDistances<f64>>,
    ) -> Result<(), SerializeErrorFlags> {
        // sort axis_tag/axis_limits, make result deterministic
        let mut axis_tags = normalized_axes_location.keys().copied().collect::<Vec<_>>();
        axis_tags.sort();

        for axis_tag in axis_tags {
            let Some(axis_limit) = normalized_axes_location.get(&axis_tag) else {
                return Err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER);
            };
            let axis_triple_distances = axes_triple_distances
                .get(&axis_tag)
                .copied()
                .unwrap_or(TripleDistances::new(1.0, 1.0));
            let mut new_vars = vec![];
            for var in self.tuple_vars.drain(..) {
                let Some(out) =
                    var.change_tuple_var_axis_limit(axis_tag, *axis_limit, &axis_triple_distances)
                else {
                    continue;
                };
                new_vars.extend(out);
            }
            self.tuple_vars = new_vars;
        }
        Ok(())
    }

    // Ported directly from harfbuzz
    fn create_from_item_var_data(
        var_data: ItemVariationData,
        regions: &[Region],
        _axes_old_index_tag_map: &FnvHashMap<usize, Tag>,
        inner_map: Option<&IncBiMap>,
        // Returns self and new item count
    ) -> Result<(Self, usize), SerializeErrorFlags> {
        // Convert VarData to tuple format
        let mut tuple_vars = Vec::new();
        let num_regions = var_data.region_index_count() as usize;
        let item_count = if let Some(inner_map) = inner_map {
            inner_map.len()
        } else {
            var_data.item_count() as usize
        };
        if item_count == 0 {
            return Ok((TupleVariations { tuple_vars }, 0));
        }

        for r in 0..num_regions {
            /* In VarData, deltas are organized in rows, convert them into
             * column(region) based tuples, resize deltas_x first */
            let mut tuple = TupleDelta {
                indices: Vec::with_capacity(item_count),
                deltas_x: Vec::with_capacity(item_count),
                ..Default::default()
            };
            for i in 0..item_count {
                tuple.indices.push(true);
                tuple.deltas_x.push(
                    var_data
                        .delta_set(
                            inner_map
                                .and_then(|m| m.get_backward(i as u32))
                                .copied()
                                .unwrap_or(i as u32) as u16,
                        )
                        .nth(r)
                        .ok_or(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)?
                        as f32,
                );
            }
            let region_index: u16 = var_data
                .region_indexes()
                .get(r)
                .map(|&idx| idx.get())
                .ok_or(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)?;
            if region_index as usize >= regions.len() {
                return Err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER);
            }
            tuple.axis_tuples = regions[region_index as usize].clone();
            tuple_vars.push(tuple);
        }
        Ok((TupleVariations { tuple_vars }, item_count))
    }

    pub fn to_glyph_deltas(&self, axis_tags: &[Tag]) -> Vec<GlyphDeltas> {
        self.tuple_vars
            .iter()
            .map(|t| t.to_glyph_deltas(axis_tags))
            .collect()
    }

    /// Normalize all tuples to have the same set of axes.
    /// For axes present in some tuples but not others, add neutral regions (min=-1, peak=0, max=1).
    /// This is required for gvar serialization.
    pub fn normalize_axes(&mut self) {
        if self.tuple_vars.is_empty() {
            return;
        }

        // Collect all axes that appear in any tuple
        let mut all_axes = std::collections::BTreeSet::new();
        for tuple in &self.tuple_vars {
            for (tag, _) in tuple.axis_tuples.iter() {
                all_axes.insert(*tag);
            }
        }

        // Ensure every tuple has all axes
        for tuple in &mut self.tuple_vars {
            for axis_tag in &all_axes {
                if !tuple.axis_tuples.contains_key(axis_tag) {
                    // Add neutral region: doesn't affect this tuple
                    tuple.axis_tuples.insert(
                        *axis_tag,
                        Triple {
                            minimum: 0.0,
                            middle: 0.0,
                            maximum: 0.0,
                        },
                    );
                }
            }
        }
    }
}

// Ported directly from harfbuzz
fn infer_delta(
    target_val: f32,
    prev_val: f32,
    next_val: f32,
    prev_delta: f32,
    next_delta: f32,
) -> f32 {
    if prev_val == next_val {
        return if prev_delta == next_delta {
            prev_delta
        } else {
            0.0
        };
    } else if target_val <= prev_val.min(next_val) {
        return if prev_val < next_val {
            prev_delta
        } else {
            next_delta
        };
    } else if target_val >= prev_val.max(next_val) {
        return if prev_val > next_val {
            prev_delta
        } else {
            next_delta
        };
    }

    let r = (target_val - prev_val) / (next_val - prev_val);
    prev_delta + r * (next_delta - prev_delta)
}

/* ported from fonttools (class _Encoding) */
#[derive(Debug, Clone)]
struct DeltaRowEncoding {
    /* each byte represents a region, value is one of 0/1/2/4, which means bytes
     * needed for this region */
    chars: Vec<u8>,
    width: usize,
    overhead: usize,
    items: Vec<Vec<i32>>,
}

impl DeltaRowEncoding {
    fn new(rows: Vec<Vec<i32>>, num_cols: usize) -> Self {
        let mut encoding = DeltaRowEncoding {
            chars: vec![0; num_cols],
            width: 0,
            overhead: 0,
            items: rows,
        };
        encoding.calculate_chars();
        encoding
    }

    fn calculate_chars(&mut self) {
        let mut long_words = false;

        for row in &self.items {
            /* 0/1/2 byte encoding */
            for (i, &v) in row.iter().enumerate() {
                if v == 0 {
                    continue;
                } else if !(-32768..=32767).contains(&v) {
                    long_words = true;
                    self.chars[i] = self.chars[i].max(4);
                } else if !(-128..=127).contains(&v) {
                    self.chars[i] = self.chars[i].max(2);
                } else {
                    self.chars[i] = self.chars[i].max(1);
                }
            }
        }

        if long_words {
            // Convert 1s to 2s
            for v in &mut self.chars {
                if *v == 1 {
                    *v = 2;
                }
            }
        }

        self.chars_changed();
    }

    fn chars_changed(&mut self) {
        let (width, columns) = self.get_width();
        self.width = width;
        self.overhead = Self::get_chars_overhead(columns);
    }

    fn get_width(&self) -> (usize, usize) {
        let mut width = 0;
        let mut columns = 0;
        for &v in &self.chars {
            width += v as usize;
            columns += (v != 0) as usize;
        }
        (width, columns)
    }

    fn combine_width(&self, other: &DeltaRowEncoding) -> (usize, usize) {
        let mut combined_width = 0;
        let mut combined_columns = 0;
        for i in 0..self.chars.len() {
            let v = self.chars[i].max(other.chars[i]);
            combined_width += v as usize;
            combined_columns += (v != 0) as usize;
        }
        (combined_width, combined_columns)
    }

    fn get_chars_overhead(num_columns: usize) -> usize {
        let c = 4 + 6; // 4 bytes for LOffset, 6 bytes for VarData header
        c + num_columns * 2
    }

    fn get_gain(&self, additional_bytes_per_row: usize) -> usize {
        let count = self.items.len();
        self.overhead
            .saturating_sub(count * additional_bytes_per_row)
    }

    fn gain_from_merging(&self, other_encoding: &DeltaRowEncoding) -> i32 {
        // Back of the envelope calculations to reject early.
        let additional_bytes_per_rows = other_encoding.width as i32 - self.width as i32;
        if additional_bytes_per_rows > 0 {
            if self.get_gain(additional_bytes_per_rows as usize) == 0 {
                return 0;
            }
        } else if other_encoding.get_gain((-additional_bytes_per_rows) as usize) == 0 {
            return 0;
        }

        let (combined_width, combined_columns) = self.combine_width(other_encoding);

        let mut combined_gain = self.overhead as i32 + other_encoding.overhead as i32;
        combined_gain -= (combined_width as i32 - self.width as i32) * self.items.len() as i32;
        combined_gain -= (combined_width as i32 - other_encoding.width as i32)
            * other_encoding.items.len() as i32;
        combined_gain -= Self::get_chars_overhead(combined_columns) as i32;

        combined_gain
    }

    fn merge(&mut self, other: &DeltaRowEncoding) {
        for row in &other.items {
            self.add_row(row.clone());
        }

        // Merge chars
        for i in 0..self.chars.len() {
            self.chars[i] = self.chars[i].max(other.chars[i]);
        }
        self.chars_changed();
    }

    fn add_row(&mut self, row: Vec<i32>) {
        self.items.push(row);
    }

    fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

impl PartialEq for DeltaRowEncoding {
    fn eq(&self, other: &Self) -> bool {
        self.width == other.width && self.chars == other.chars
    }
}

impl Eq for DeltaRowEncoding {}

impl PartialOrd for DeltaRowEncoding {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for DeltaRowEncoding {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.width.cmp(&other.width) {
            Ordering::Equal => other.chars.cmp(&self.chars),
            other => other,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct CombinedGainIdxTuple {
    encoded: u64,
}

impl CombinedGainIdxTuple {
    fn new(gain: u32, i: u32, j: u32) -> Self {
        let encoded = ((0xFFFFFF - gain as u64) << 40) | ((i as u64) << 20) | (j as u64);
        CombinedGainIdxTuple { encoded }
    }

    fn idx_1(&self) -> usize {
        ((self.encoded >> 20) & 0xFFFFF) as usize
    }

    fn idx_2(&self) -> usize {
        (self.encoded & 0xFFFFF) as usize
    }
}

/// Intermediate representation for ItemVariationStore during instancing.
/// Corresponds to Harfbuzz's item_variations_t.
#[derive(Debug)]
struct ItemVariations {
    /// All tuple variations, one per VarData subtable
    vars: Vec<TupleVariations>,
    /// Number of items (rows) in each VarData
    var_data_num_rows: Vec<usize>,
    ///  original region list, decompiled from item varstore, used when rebuilding
    /// region list after instantiation
    orig_region_list: Vec<Region>,
    /// List of unique regions for the output
    region_list: Vec<Region>,
    /// Map from region coordinates to column index
    region_map: FnvHashMap<Region, usize>,
    /// all delta rows after instantiation
    delta_rows: Vec<Vec<i32>>,
    /// final optimized vector of encoding objects used to assemble the varstore
    encodings: Vec<DeltaRowEncoding>,
    /// old varidxes -> new var_idxes map
    varidx_map: FnvHashMap<u32, u32>,
    /// Whether we have long (32-bit) deltas
    has_long: bool,
}

impl ItemVariations {
    /// Convert ItemVariationStore to tuple representation.
    /// Corresponds to Harfbuzz's create_from_item_varstore.
    fn create_from_item_varstore(
        var_store: &ItemVariationStore,
        axes_old_index_tag_map: &FnvHashMap<usize, Tag>,
        inner_maps: &[IncBiMap],
    ) -> Result<Self, SerializeErrorFlags> {
        let region_list = var_store
            .variation_region_list()
            .map_err(|_| SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)?;
        // Get var regions and store them in orig_region_list here
        // regionList.get_var_regions (axes_old_index_tag_map, orig_region_list)
        let orig_region_list = {
            let region_records = region_list.variation_regions();
            region_records
                .iter()
                .flatten()
                .map(|record| {
                    record
                        .region_axes()
                        .iter()
                        .enumerate()
                        .filter_map(|(axis_index, axis)| {
                            let axis_tag = axes_old_index_tag_map.get(&axis_index)?;
                            Some((
                                *axis_tag,
                                Triple {
                                    minimum: axis.start_coord().to_f32().into(),
                                    middle: axis.peak_coord().to_f32().into(),
                                    maximum: axis.end_coord().to_f32().into(),
                                },
                            ))
                        })
                        .collect::<FnvHashMap<_, _>>()
                })
                .map(Region)
                .collect::<Vec<_>>()
        };

        let var_data_array = var_store.item_variation_data();
        let num_var_data = var_data_array.len();

        let mut vars = Vec::with_capacity(num_var_data);
        let mut var_data_num_rows = Vec::with_capacity(num_var_data);

        if inner_maps.is_empty() {
            // If no inner_maps provided, process all VarData with all items
            for major_idx in 0..num_var_data {
                let Some(Ok(var_data)) = var_data_array.get(major_idx) else {
                    continue;
                };
                let (var_data_tuples, item_count) = TupleVariations::create_from_item_var_data(
                    var_data,
                    &orig_region_list,
                    axes_old_index_tag_map,
                    None,
                )?;
                var_data_num_rows.push(item_count);
                vars.push(var_data_tuples);
            }
        } else {
            // Process only the VarData corresponding to provided inner_maps
            for (major_idx, inner_map) in inner_maps.iter().enumerate() {
                if inner_map.len() == 0 {
                    continue;
                }

                let Some(Ok(var_data)) = var_data_array.get(major_idx) else {
                    continue;
                };
                let (var_data_tuples, item_count) = TupleVariations::create_from_item_var_data(
                    var_data,
                    &orig_region_list,
                    axes_old_index_tag_map,
                    Some(inner_map),
                )?;
                var_data_num_rows.push(item_count);
                vars.push(var_data_tuples);
            }
        }
        Ok(ItemVariations {
            vars,
            var_data_num_rows,
            orig_region_list: orig_region_list.clone(),
            region_list: Vec::new(),
            region_map: FnvHashMap::default(),
            delta_rows: Vec::new(),
            encodings: Vec::new(),
            varidx_map: FnvHashMap::default(),
            has_long: false,
        })
    }

    /// Apply instancing: evaluate regions at pinned coordinates and transform deltas.
    /// Corresponds to Harfbuzz's instantiate_tuple_vars.
    fn instantiate_tuple_vars(
        &mut self,
        normalized_axes_location: &FnvHashMap<Tag, Triple<f64>>,
        axes_triple_distances: &FnvHashMap<Tag, TripleDistances<f64>>,
    ) -> Result<(), SerializeErrorFlags> {
        for tuple_variations in &mut self.vars {
            tuple_variations.instantiate(
                normalized_axes_location,
                axes_triple_distances,
                None,
                false,
            )?;
        }
        self.build_region_list()
    }

    /// Build region list
    /// Ported directly from harfbuzz
    fn build_region_list(&mut self) -> Result<(), SerializeErrorFlags> {
        /* scan all tuples and collect all unique regions, prune unused regions */
        let mut all_regions = FnvHashMap::default();
        let mut used_regions = FnvHashMap::default();

        /* use a vector when inserting new regions, make result deterministic */
        let mut all_unique_regions = Vec::new();
        for tuple_variations in &self.vars {
            for tuple_var in &tuple_variations.tuple_vars {
                let r = &tuple_var.axis_tuples;
                if !used_regions.contains_key(r) {
                    let all_zeros = tuple_var.deltas_x.iter().all(|&d| d.round() == 0.0);
                    if !all_zeros {
                        used_regions.insert(r, 1);
                    }
                }
                if all_regions.contains_key(r) {
                    continue;
                }
                all_regions.insert(r, 1);
                all_unique_regions.push(r);
            }
        }

        /* regions are empty means no variation data, return true */
        if all_regions.is_empty() || all_unique_regions.is_empty() {
            return Ok(());
        }

        // Allocate all_region.len() in the region list.
        self.region_list.reserve(all_regions.len());

        let mut idx = 0;
        /* append the original regions that pre-existed */
        for r in self.orig_region_list.iter() {
            if !all_regions.contains_key(&r) || !used_regions.contains_key(&r) {
                continue;
            }

            self.region_list.push(r.clone());
            self.region_map.insert(r.clone(), idx);
            all_regions.remove(&r);
            idx += 1;
        }

        /* append the new regions at the end */
        for r in all_unique_regions {
            if !all_regions.contains_key(r) || !used_regions.contains_key(r) {
                continue;
            }
            self.region_list.push(r.clone());
            self.region_map.insert(r.clone(), idx);
            all_regions.remove(&r);
            idx += 1;
        }
        Ok(())
    }

    /// Convert back to ItemVariationStore format with deduplication.
    /// Corresponds to Harfbuzz's as_item_varstore.
    fn as_item_varstore(
        &mut self,
        optimize: bool,
        use_no_variation_idx: bool,
    ) -> Result<(), SerializeErrorFlags> {
        /* return true if no variation data */
        if self.region_list.is_empty() {
            return Ok(());
        }
        let num_cols = self.region_list.len();

        /* pre-alloc a 2D vector for all sub_table's VarData rows */
        let mut total_rows = 0;
        for major in 0..self.var_data_num_rows.len() {
            total_rows += self.var_data_num_rows[major];
        }

        self.delta_rows.resize(total_rows, vec![0; num_cols]);
        /* init all rows to [0]*num_cols */
        for i in 0..total_rows {
            self.delta_rows[i].resize(num_cols, 0);
        }

        /* old VarIdxes -> full encoding_row mapping */
        let mut front_mapping: FnvHashMap<u32, Vec<i32>> = FnvHashMap::default();
        let mut start_row = 0;
        let mut encoding_objs = Vec::new();

        /* delta_rows map, used for filtering out duplicate rows */
        let mut delta_rows_map: FnvHashMap<Vec<i32>, bool> = FnvHashMap::default();

        for major in 0..self.vars.len() {
            /* deltas are stored in tuples(column based), convert them back into items
             * (row based) delta */
            let tuple_variations = &self.vars[major];
            let num_rows = self.var_data_num_rows[major];

            if num_rows == 0 {
                continue;
            }

            for tuple_var in &tuple_variations.tuple_vars {
                if tuple_var.deltas_x.len() != num_rows {
                    return Err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER);
                }

                /* skip unused regions */
                let Some(&col_idx) = self.region_map.get(&tuple_var.axis_tuples) else {
                    continue;
                };

                for i in 0..num_rows {
                    let rounded_delta = tuple_var.deltas_x[i].round() as i32;
                    self.delta_rows[start_row + i][col_idx] += rounded_delta;
                    self.has_long |= !(-65536..=65535).contains(&rounded_delta);
                }
            }

            let mut major_rows = Vec::new();
            for minor in 0..num_rows {
                let row = &self.delta_rows[start_row + minor];

                if use_no_variation_idx {
                    let mut all_zeros = true;
                    for &delta in row {
                        if delta != 0 {
                            all_zeros = false;
                            break;
                        }
                    }
                    if all_zeros {
                        continue;
                    }
                }

                front_mapping.insert(((major as u32) << 16) + minor as u32, row.clone());

                if delta_rows_map.contains_key(row) {
                    continue;
                }

                delta_rows_map.insert(row.clone(), true);
                major_rows.push(row.clone());
            }

            if !major_rows.is_empty() {
                encoding_objs.push(DeltaRowEncoding::new(major_rows, num_cols));
            }

            start_row += num_rows;
        }

        /* return directly if no optimization, maintain original VariationIndex so
         * varidx_map would be empty */
        if !optimize {
            self.encodings = encoding_objs;
            return Ok(());
        }

        /* NOTE: Fonttools instancer always optimizes VarStore from scratch. This
         * is too costly for large fonts. So, instead, we retain the encodings of
         * the original VarStore, and just try to combine them if possible. This
         * is a compromise between optimization and performance and practically
         * works very well. */

        // This produces slightly smaller results in some cases.
        encoding_objs.sort();

        /* main algorithm: repeatedly pick 2 best encodings to combine, and combine them */
        let mut queue_items = BTreeMap::new();
        let num_todos = encoding_objs.len();
        for i in 0..num_todos {
            for j in (i + 1)..num_todos {
                let combining_gain = encoding_objs[i].gain_from_merging(&encoding_objs[j]);
                if combining_gain > 0 {
                    let tuple =
                        CombinedGainIdxTuple::new(combining_gain as u32, i as u32, j as u32);
                    queue_items.insert(tuple, ());
                }
            }
        }

        let mut removed_todo_idxes = FnvHashMap::default();
        while let Some((t, _)) = queue_items.pop_first() {
            let i = t.idx_1();
            let j = t.idx_2();

            if removed_todo_idxes.contains_key(&i) || removed_todo_idxes.contains_key(&j) {
                continue;
            }

            let other_encoding = encoding_objs[j].clone();
            encoding_objs[i].merge(&other_encoding);

            removed_todo_idxes.insert(i, true);
            removed_todo_idxes.insert(j, true);

            for idx in 0..encoding_objs.len() {
                if removed_todo_idxes.contains_key(&idx) {
                    continue;
                }

                let obj = &encoding_objs[idx];
                // In the unlikely event that the same encoding exists already, combine it.
                if obj.width == encoding_objs[i].width && obj.chars == encoding_objs[i].chars {
                    // This is straight port from fonttools algorithm. I added this branch there
                    // because I thought it can happen. But looks like we never get in here in
                    // practice. I'm not confident enough to remove it though; in theory it can
                    // happen. I think it's just that our tests are not extensive enough to hit
                    // this path.

                    let items_to_add = obj.items.clone();
                    for row in &items_to_add {
                        encoding_objs[i].add_row(row.clone());
                    }

                    removed_todo_idxes.insert(idx, true);
                    continue;
                }

                let combined_gain = encoding_objs[i].gain_from_merging(obj);
                if combined_gain > 0 {
                    let tuple = CombinedGainIdxTuple::new(
                        combined_gain as u32,
                        idx as u32,
                        encoding_objs.len() as u32,
                    );
                    queue_items.insert(tuple, ());
                }
            }

            let moved_encoding = encoding_objs[i].clone();
            encoding_objs.push(moved_encoding);
        }

        let num_final_encodings = encoding_objs.len() - removed_todo_idxes.len();
        if num_final_encodings == 0 {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER);
        }

        self.encodings.reserve(num_final_encodings);
        for i in 0..encoding_objs.len() {
            if removed_todo_idxes.contains_key(&i) {
                continue;
            }
            self.encodings.push(encoding_objs[i].clone());
        }

        self.compile_varidx_map(front_mapping)
    }

    /* compile varidx_map for one VarData subtable (index specified by major) */
    fn compile_varidx_map(
        &mut self,
        front_mapping: FnvHashMap<u32, Vec<i32>>,
    ) -> Result<(), SerializeErrorFlags> {
        /* full encoding_row -> new VarIdxes mapping */
        let mut back_mapping: FnvHashMap<Vec<i32>, u32> = FnvHashMap::default();

        for major in 0..self.encodings.len() {
            let encoding = &mut self.encodings[major];
            /* just sanity check, this shouldn't happen */
            if encoding.is_empty() {
                return Err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER);
            }

            let num_rows = encoding.items.len();

            /* sort rows, make result deterministic */
            encoding.items.sort_by(_cmp_row);

            /* compile old to new var_idxes mapping */
            for minor in 0..num_rows {
                let new_varidx = ((major as u32) << 16) + minor as u32;
                back_mapping.insert(encoding.items[minor].clone(), new_varidx);
            }
        }

        for (old_varidx, row) in front_mapping.iter() {
            if let Some(&new_varidx) = back_mapping.get(row) {
                self.varidx_map.insert(*old_varidx, new_varidx);
            } else {
                self.varidx_map.insert(*old_varidx, 0xFFFFFFFF);
            }
        }
        Ok(())
    }

    fn get_region_list(&self) -> &Vec<Region> {
        &self.region_list
    }
}

fn _cmp_row(a: &Vec<i32>, b: &Vec<i32>) -> Ordering {
    /* compare pointers of vectors(const hb_vector_t<int>*) that represent a row */
    for i in 0..b.len() {
        let va = a[i];
        let vb = b[i];
        if va != vb {
            return va.cmp(&vb);
        }
    }
    Ordering::Equal
}

/// Convert ItemVariations (after instancing) into a write_fonts ItemVariationStore,
/// then serialize it to bytes that can be parsed back as a read_fonts ItemVariationStore.
///
/// Returns both the serialized bytes and the variation index remapping that maps
/// old variation indices to new ones after optimization.
///
/// This approach properly uses write_fonts infrastructure to handle the complex
/// binary layout rather than manually writing bytes, which was causing corruption.
fn itemvariations_to_varstore_bytes(
    item_vars: &ItemVariations,
    axis_order: &[Tag],
) -> Result<(Vec<u8>, FnvHashMap<u32, u32>), SerializeErrorFlags> {
    use write_fonts::{
        dump_table,
        tables::variations::{ivs_builder::VariationStoreBuilder, VariationRegion},
    };

    // If no regions, return early
    if item_vars.region_list.is_empty() {
        return Ok((Vec::new(), item_vars.varidx_map.clone()));
    }

    let axis_count = axis_order.len() as u16;
    let mut builder = VariationStoreBuilder::new(axis_count);

    // Collect all rows that are actually in the encodings
    // Skip all-zero rows that were filtered out during encoding
    let mut rows_to_serialize = std::collections::HashSet::new();
    for encoding in &item_vars.encodings {
        for row in &encoding.items {
            // Find the index of this row in delta_rows
            for (idx, delta_row) in item_vars.delta_rows.iter().enumerate() {
                if delta_row == row {
                    rows_to_serialize.insert(idx);
                    break;
                }
            }
        }
    }

    // Add only the rows that are in the encodings
    // Each row represents the deltas for one item across all regions
    for (row_idx, row) in item_vars.delta_rows.iter().enumerate() {
        if !rows_to_serialize.contains(&row_idx) {
            continue; // Skip rows that aren't in the encodings (all-zero rows)
        }

        let mut deltas = Vec::new();

        // For each region, collect the delta for this row
        for (region_idx, &delta) in row.iter().enumerate() {
            if region_idx < item_vars.region_list.len() {
                let region = &item_vars.region_list[region_idx];
                let axis_coords: Vec<_> = axis_order
                    .iter()
                    .map(|tag| {
                        let triple = region.get(tag).copied().unwrap_or_default();
                        write_fonts::tables::variations::RegionAxisCoordinates {
                            start_coord: F2Dot14::from_f32(triple.minimum as f32),
                            peak_coord: F2Dot14::from_f32(triple.middle as f32),
                            end_coord: F2Dot14::from_f32(triple.maximum as f32),
                        }
                    })
                    .collect();

                let var_region = VariationRegion::new(axis_coords);

                deltas.push((var_region, delta));
            }
        }

        // Add this delta set to the builder (with deduplication)
        if !deltas.is_empty() {
            builder.add_deltas(deltas);
        }
    }

    // Build the ItemVariationStore and capture the remapping
    let (store, _remapping) = builder.build();

    // Serialize using write_fonts
    let bytes = dump_table(&store).map_err(|_| SerializeErrorFlags::SERIALIZE_ERROR_OTHER)?;

    // Return bytes alongside the varidx_map for remapping
    Ok((bytes, item_vars.varidx_map.clone()))
}

/// Apply variation index remapping after instantiation.
/// Corresponds to Harfbuzz's remap_varidx_after_instantiation function.
///
/// This updates the layout_varidx_delta_map to use new variation indices after instantiation.
/// For each old variation index in the map, we look it up in the varidx_map to get the new index.
/// If the old index is not in varidx_map (meaning it was filtered out), we use NO_VARIATIONS_INDEX.
fn remap_varidx_after_instantiation(
    varidx_map: &FnvHashMap<u32, u32>,
    layout_varidx_delta_map: &mut FnvHashMap<u32, (u32, i32)>,
) {
    // varidx_map is empty which means varstore is empty after instantiation,
    // no variations, map all varidx to NO_VARIATIONS_INDEX (typically 0xFFFFFFFF or similar).
    // varidx_map doesn't have original varidx, indicating delta row is all zeros,
    // map varidx to NO_VARIATIONS_INDEX

    // NO_VARIATIONS_INDEX is typically defined as u32::MAX or similar sentinel value
    // In Harfbuzz it's HB_OT_LAYOUT_NO_VARIATIONS_INDEX
    const NO_VARIATIONS_INDEX: u32 = u32::MAX;

    // Update each entry in the delta map
    let mut entries_to_update = Vec::new();
    for (old_varidx, (old_new_varidx, delta)) in layout_varidx_delta_map.iter() {
        // The old_new_varidx is from the initial subsetting pass
        // We need to map it through the instantiation varidx_map
        let remapped_varidx = if let Some(&new_varidx) = varidx_map.get(&old_new_varidx) {
            new_varidx
        } else {
            NO_VARIATIONS_INDEX
        };
        entries_to_update.push((*old_varidx, (remapped_varidx, *delta)));
    }

    // Apply the updates
    for (old_varidx, (new_varidx, delta)) in entries_to_update {
        layout_varidx_delta_map.insert(old_varidx, (new_varidx, delta));
    }
}

/// Evaluate a variation region at given normalized coordinates.
/// Returns the scalar multiplier for deltas in this region.
/// Corresponds to Harfbuzz's VarRegionList::evaluate.
fn evaluate_region(axis_tuples: &[RegionAxisCoordinates], normalized_coords: &[F2Dot14]) -> f32 {
    let mut scalar = 1.0f32;

    for (axis_idx, coords) in axis_tuples.iter().enumerate() {
        if axis_idx >= normalized_coords.len() {
            break;
        }

        let coord = normalized_coords[axis_idx].to_f32();
        let start = coords.start_coord.get().to_f32();
        let peak = coords.peak_coord.get().to_f32();
        let end = coords.end_coord.get().to_f32();

        // Harfbuzz's VarRegionAxis::evaluate logic
        if start > peak || peak > end {
            continue;
        }
        if start < 0.0 && end > 0.0 && peak != 0.0 {
            continue;
        }

        if peak == 0.0 || coord == peak {
            continue;
        }

        if coord < start || coord > end {
            scalar = 0.0;
            break;
        }

        if coord < peak {
            if start != peak {
                scalar *= (coord - start) / (peak - start);
            }
        } else {
            if peak != end {
                scalar *= (end - coord) / (end - peak);
            }
        }
    }

    scalar
}

impl<'a> SubsetTable<'a> for ItemVariationStore<'a> {
    type ArgsForSubset = (&'a [IncBiMap], bool);
    type Output = ();

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        args: Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        let (inner_maps, keep_empty) = args;
        if !keep_empty && inner_maps.is_empty() {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY);
        }

        // When we have instancing (normalized_coords), use the instancing path
        // which handles instantiation and optimization all at once
        if !plan.normalized_coords.is_empty() {
            let (bytes, varidx_map) =
                subset_itemvarstore_with_instancing(self.clone(), plan, s, inner_maps, true)?;

            // Apply the variation index remapping to the plan's layout_varidx_delta_map
            // This remaps all variation indices used by layout tables (GPOS, GSUB, etc.)
            // to account for the changes made during ItemVariationStore instantiation and optimization.
            if !varidx_map.is_empty() {
                remap_varidx_after_instantiation(
                    &varidx_map,
                    &mut plan.layout_varidx_delta_map.borrow_mut(),
                );
                log::debug!(
                    "Applied variation index remapping with {} entries",
                    varidx_map.len()
                );
            }

            // The instancing path already serialized the bytes fully via itemvariations_to_varstore_bytes
            // Just write them directly (they include format version)
            log::debug!(
                "Returning early from instancing path with fully-serialized ItemVariationStore"
            );
            s.embed_bytes(&bytes)?;
            return Ok(());
        }

        // Non-instancing path: standard subsetting without instancing
        s.embed(self.format())?;

        let var_region_list = self
            .variation_region_list()
            .expect("Couldn't read var region list");

        let var_data_array = self.item_variation_data();
        let mut region_indices = IntSet::empty();
        for (i, inner_map) in inner_maps.iter().enumerate() {
            if inner_map.len() == 0 {
                continue;
            }
            match var_data_array.get(i) {
                Some(Ok(var_data)) => {
                    collect_region_refs(&var_data, inner_map, &mut region_indices);
                }
                None => continue,
                Some(Err(_e)) => {
                    log::warn!("Error reading VarData at index {}: {:?}", i, _e);
                    return Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR));
                }
            }
        }

        if region_indices.is_empty() && !keep_empty {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY);
        }

        // varRegionList
        let max_region_count = var_region_list.region_count();
        region_indices.remove_range(max_region_count..=u16::MAX);

        // Create region map directly from referenced indices (matching Harfbuzz behavior)
        let mut region_map: IncBiMap = IncBiMap::with_capacity(region_indices.len() as usize);

        for region in region_indices.iter() {
            region_map.add(region as u32);
        }

        let region_list_offset_pos = s.embed(Offset32::new(0))?;
        Offset32::serialize_subset(
            &var_region_list,
            s,
            plan,
            &region_map,
            region_list_offset_pos,
        )?;

        serialize_var_data_offset_array(self, s, plan, inner_maps, &region_map, keep_empty)
    }
}

/// Instancing path for ItemVariationStore.
/// Corresponds to Harfbuzz's item_variations_t::instantiate + serialize.
/// Returns both the serialized bytes and the variation index mapping.
pub fn subset_itemvarstore_with_instancing(
    var_store: ItemVariationStore,
    plan: &Plan,
    _s: &mut Serializer,
    inner_maps: &[IncBiMap],
    keep_empty: bool,
) -> Result<(Vec<u8>, FnvHashMap<u32, u32>), SerializeErrorFlags> {
    log::warn!(
        "Instancing ItemVariationStore with location: {:?}",
        plan.axes_location
    );
    // Create intermediate tuple representation
    let mut item_vars = ItemVariations::create_from_item_varstore(
        &var_store,
        &plan.axes_old_index_tag_map,
        inner_maps,
    )?;

    // Apply instancing transformation
    item_vars.instantiate_tuple_vars(&plan.axes_location, &plan.axes_triple_distances)?;

    // Convert back to ItemVariationStore format with deduplication and optimization
    item_vars.as_item_varstore(true, true)?;

    // Check if we have any data left after instancing
    if item_vars.encodings.is_empty() && !keep_empty {
        return Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY);
    }

    // Use write_fonts infrastructure to properly serialize the ItemVariationStore
    // instead of manually writing binary data - also get the remapping
    let (varstore_bytes, varidx_map) =
        itemvariations_to_varstore_bytes(&item_vars, &plan.axis_tags)?;

    if varstore_bytes.is_empty() && !keep_empty {
        return Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY);
    }

    Ok((varstore_bytes, varidx_map))
}

impl<'a> SubsetTable<'a> for VariationRegionList<'a> {
    type ArgsForSubset = &'a IncBiMap;
    type Output = ();

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        region_map: &IncBiMap,
    ) -> Result<(), SerializeErrorFlags> {
        let new_axis_count = if plan.normalized_coords.is_empty() {
            self.axis_count()
        } else {
            plan.axis_tags.len() as u16
        };
        s.embed(new_axis_count)?;

        let region_count = region_map.len() as u16;
        s.embed(region_count)?;

        if region_count == 0 {
            return Ok(());
        }
        //Fixed size of a VariationRegion
        let var_region_size = 3 * new_axis_count as usize * F2Dot14::RAW_BYTE_LEN;
        if var_region_size.checked_mul(region_count as usize).is_none() {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER);
        }

        let subset_var_regions_size = var_region_size * region_count as usize;
        let var_regions_pos = s.allocate_size(subset_var_regions_size, false)?;

        let src_region_count = self.region_count() as u32;
        let Some(src_var_regions_bytes) = self
            .offset_data()
            .as_bytes()
            .get(self.shape().variation_regions_byte_range())
        else {
            return Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR));
        };

        for r in 0..region_count {
            let Some(backward) = region_map.get_backward(r as u32) else {
                return Err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER);
            };
            if *backward >= src_region_count {
                return Err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER);
            }

            let src_pos = (*backward as usize) * var_region_size;
            let Some(src_bytes) = src_var_regions_bytes.get(src_pos..src_pos + var_region_size)
            else {
                return Err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER);
            };
            s.copy_assign_from_bytes(var_regions_pos + r as usize * var_region_size, src_bytes);
        }
        Ok(())
    }
}

fn serialize_var_data_offset_array(
    var_store: &ItemVariationStore,
    s: &mut Serializer,
    plan: &Plan,
    inner_maps: &[IncBiMap],
    region_map: &IncBiMap,
    keep_empty: bool,
) -> Result<(), SerializeErrorFlags> {
    let mut vardata_count = 0_u16;
    let count_pos = s.embed(vardata_count)?;

    let var_data_array = var_store.item_variation_data();
    for (i, inner_map) in inner_maps.iter().enumerate() {
        if inner_map.len() == 0 {
            continue;
        }
        match var_data_array.get(i) {
            Some(Ok(var_data)) => {
                let offset_pos = s.embed(0_u32)?;
                Offset32::serialize_subset(
                    &var_data,
                    s,
                    plan,
                    (inner_map, region_map),
                    offset_pos,
                )?;
                vardata_count += 1;
            }
            _ => {
                return Err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER);
            }
        }
    }
    if vardata_count == 0 {
        if !keep_empty {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY);
        } else {
            return Ok(());
        }
    }
    s.copy_assign(count_pos, vardata_count);
    Ok(())
}

impl<'a> SubsetTable<'a> for ItemVariationData<'_> {
    type ArgsForSubset = (&'a IncBiMap, &'a IncBiMap);
    type Output = ();

    fn subset(
        &self,
        _plan: &Plan,
        s: &mut Serializer,
        args: (&IncBiMap, &IncBiMap),
    ) -> Result<(), SerializeErrorFlags> {
        let (inner_map, region_map) = args;
        let new_item_count = inner_map.len() as u16;
        s.embed(new_item_count)?;

        // optimize word count
        let ri_count = self.region_index_count() as usize;

        #[derive(Clone, Copy, PartialEq)]
        enum DeltaSize {
            Zero,
            NonWord,
            Word,
        }

        let mut delta_sz = Vec::new();
        delta_sz.resize(ri_count, DeltaSize::Zero);
        // maps new index to old index
        let mut ri_map = vec![0; ri_count];

        let mut new_word_count: u16 = 0;

        let src_delta_bytes = self.delta_sets();
        let src_row_size = self.get_delta_row_len();

        let src_word_delta_count = self.word_delta_count();
        let src_word_count = (src_word_delta_count & 0x7FFF) as usize;
        let src_long_words = src_word_count & 0x8000 != 0;

        let mut has_long = false;
        if src_long_words {
            for r in 0..src_word_count {
                for item in inner_map.keys() {
                    let delta =
                        get_item_delta(self, *item as usize, r, src_row_size, src_delta_bytes);
                    if !(-65536..=65535).contains(&delta) {
                        has_long = true;
                        break;
                    }
                }
            }
        }

        let min_threshold = if has_long { -65536 } else { -128 };
        let max_threshold = if has_long { 65535 } else { 127 };

        for (r, delta_size) in delta_sz.iter_mut().enumerate() {
            let short_circuit = src_long_words == has_long && src_word_count <= r;
            for item in inner_map.keys() {
                let delta = get_item_delta(self, *item as usize, r, src_row_size, src_delta_bytes);
                if delta < min_threshold || delta > max_threshold {
                    *delta_size = DeltaSize::Word;
                    new_word_count += 1;
                    break;
                } else if delta != 0 {
                    *delta_size = DeltaSize::NonWord;
                    if short_circuit {
                        break;
                    }
                }
            }
        }

        let mut word_index: u16 = 0;
        let mut non_word_index = new_word_count;
        let mut new_ri_count: u16 = 0;

        for (r, delta_type) in delta_sz.iter().enumerate() {
            if *delta_type == DeltaSize::Zero {
                continue;
            }

            if *delta_type == DeltaSize::Word {
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
        let src_region_indices = self.region_indexes();
        for (idx, src_idx) in ri_map.iter().enumerate().take(new_ri_count as usize) {
            let Some(old_r) = src_region_indices.get(*src_idx) else {
                return Err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER);
            };

            let Some(region) = region_map.get(old_r.get() as u32) else {
                return Err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER);
            };
            s.copy_assign(region_indices_pos + idx * 2, *region as u16);
        }

        let new_row_size = ItemVariationData::delta_row_len(new_word_delta_count, new_ri_count);
        let new_delta_bytes_len = new_item_count as usize * new_row_size;

        let delta_bytes_pos = s.allocate_size(new_delta_bytes_len, false)?;
        for i in 0..new_item_count as usize {
            let Some(old_i) = inner_map.get_backward(i as u32) else {
                return Err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER);
            };

            let old_i = *old_i as usize;
            for (r, old_r) in ri_map.iter().enumerate().take(new_ri_count as usize) {
                set_item_delta(
                    s,
                    delta_bytes_pos,
                    i,
                    r,
                    get_item_delta(self, old_i, *old_r, src_row_size, src_delta_bytes),
                    new_row_size,
                    has_long,
                    new_word_count as usize,
                )?;
            }
        }

        Ok(())
    }
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

    // direct port from Harfbuzz: <https://github.com/harfbuzz/harfbuzz/blob/22fbc7568828b9acfd116be44b2d77d56d2d448b/src/hb-ot-layout-common.hh#L3061>
    // ignore the lint here
    #[allow(clippy::collapsible_else_if)]
    if is_long {
        if region < word_count {
            let pos = p + region * 4;
            let Some(delta_bytes) = delta_bytes.get(pos..pos + 4) else {
                return 0;
            };
            BigEndian::<i32>::from_slice(delta_bytes).unwrap().get()
        } else {
            let pos = p + 4 * word_count + 2 * (region - word_count);
            let Some(delta_bytes) = delta_bytes.get(pos..pos + 2) else {
                return 0;
            };
            BigEndian::<i16>::from_slice(delta_bytes).unwrap().get() as i32
        }
    } else {
        if region < word_count {
            let pos = p + region * 2;
            let Some(delta_bytes) = delta_bytes.get(pos..pos + 2) else {
                return 0;
            };
            BigEndian::<i16>::from_slice(delta_bytes).unwrap().get() as i32
        } else {
            let pos = p + 2 * word_count + (region - word_count);
            let Some(delta_bytes) = delta_bytes.get(pos..pos + 1) else {
                return 0;
            };
            BigEndian::<i8>::from_slice(delta_bytes).unwrap().get() as i32
        }
    }
}

// direct port from Harfbuzz: <https://github.com/harfbuzz/harfbuzz/blob/22fbc7568828b9acfd116be44b2d77d56d2d448b/src/hb-ot-layout-common.hh#L3090>
// ignore the lint here
#[allow(clippy::too_many_arguments)]
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
    #[allow(clippy::collapsible_else_if)]
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

pub(crate) struct DeltaSetIndexMapSerializePlan<'a> {
    outer_bit_count: u8,
    inner_bit_count: u8,
    output_map: &'a FnvHashMap<u32, u32>,
    map_count: u32,
}

impl<'a> DeltaSetIndexMapSerializePlan<'a> {
    pub(crate) fn new(
        outer_bit_count: u8,
        inner_bit_count: u8,
        output_map: &'a FnvHashMap<u32, u32>,
        map_count: u32,
    ) -> Self {
        Self {
            outer_bit_count,
            inner_bit_count,
            output_map,
            map_count,
        }
    }

    pub(crate) fn width(&self) -> u8 {
        (self.outer_bit_count + self.inner_bit_count).div_ceil(8)
    }

    pub(crate) fn inner_bit_count(&self) -> u8 {
        self.inner_bit_count
    }

    pub(crate) fn output_map(&self) -> &'a FnvHashMap<u32, u32> {
        self.output_map
    }

    pub(crate) fn map_count(&self) -> u32 {
        self.map_count
    }
}

impl<'a> SubsetTable<'a> for DeltaSetIndexMap<'a> {
    type ArgsForSubset = &'a DeltaSetIndexMapSerializePlan<'a>;
    type Output = ();

    fn subset(
        &self,
        _plan: &Plan,
        s: &mut Serializer,
        index_map_subset_plan: &'a DeltaSetIndexMapSerializePlan<'a>,
    ) -> Result<(), SerializeErrorFlags> {
        let output_map = index_map_subset_plan.output_map();
        let width = index_map_subset_plan.width();
        let inner_bit_count = index_map_subset_plan.inner_bit_count();

        let map_count = index_map_subset_plan.map_count();
        // sanity check
        if map_count > 0 && (((inner_bit_count - 1) & (!0xF)) != 0 || (((width - 1) & (!0x3)) != 0))
        {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER);
        }

        let format: u8 = if map_count <= 0xFFFF { 0 } else { 1 };
        s.embed(format)?;

        let entry_format = ((width - 1) << 4) | (inner_bit_count - 1);
        s.embed(entry_format)?;

        if format == 0 {
            s.embed(map_count as u16)?;
        } else {
            s.embed(map_count)?;
        }

        let num_data_bytes = width as usize * map_count as usize;
        let mapdata_pos = s.allocate_size(num_data_bytes, true)?;

        let be_byte_index_start = 4 - width as usize;
        for i in 0..map_count {
            let Some(v) = output_map.get(&i) else {
                continue;
            };
            if *v == 0 {
                continue;
            }

            let outer = v >> 16;
            let inner = v & 0xFFFF;
            let u = (outer << inner_bit_count) | inner;
            let data_bytes = u.to_be_bytes();
            let data_pos = mapdata_pos + (i as usize) * width as usize;
            s.copy_assign_from_bytes(data_pos, data_bytes.get(be_byte_index_start..4).unwrap());
        }

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use skrifa::raw::{
        tables::{cvar::Cvar, hvar::Hvar},
        FontData, FontRead,
    };
    #[test]
    fn test_subset_item_varstore() {
        use crate::DEFAULT_LAYOUT_FEATURES;
        let raw_bytes: [u8; 471] = [
            0x00, 0x01, 0x00, 0x00, 0x00, 0x18, 0x00, 0x04, 0x00, 0x00, 0x00, 0x58, 0x00, 0x00,
            0x00, 0x6f, 0x00, 0x00, 0x00, 0x92, 0x00, 0x00, 0x01, 0xbc, 0x00, 0x02, 0x00, 0x05,
            0xc0, 0x00, 0xc0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x40, 0x00, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x40, 0x00, 0x40, 0x00, 0xc0, 0x00, 0xc0, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x40, 0x00, 0x40, 0x00, 0x00, 0x00, 0x40, 0x00, 0x40, 0x00, 0x00, 0x00,
            0x40, 0x00, 0x40, 0x00, 0x00, 0x0f, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0xd8, 0xdd,
            0xe2, 0xec, 0xf1, 0xf6, 0xfb, 0x05, 0x0a, 0x0f, 0x14, 0x1e, 0x28, 0x32, 0x3c, 0x00,
            0x1b, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x8d, 0xa1, 0xb5, 0xba, 0xbf, 0xc4, 0xce,
            0xd8, 0xdd, 0xe2, 0xe7, 0xec, 0xf1, 0xf6, 0xfb, 0x05, 0x0a, 0x0f, 0x14, 0x19, 0x1e,
            0x28, 0x2d, 0x32, 0x3c, 0x46, 0x64, 0x00, 0x90, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00,
            0x00, 0x01, 0xba, 0xa1, 0xba, 0x14, 0xba, 0x32, 0xc4, 0xc4, 0xc4, 0xd8, 0xc4, 0xe2,
            0xc4, 0x0a, 0xc4, 0x28, 0xce, 0xba, 0xce, 0xc4, 0xce, 0xd8, 0xce, 0xe2, 0xce, 0xec,
            0xce, 0x28, 0xce, 0x32, 0xd8, 0x92, 0xd8, 0x9c, 0xd8, 0xbf, 0xd8, 0xce, 0xd8, 0xd8,
            0xd8, 0xe2, 0xd8, 0xe7, 0xd8, 0xec, 0xd8, 0xf6, 0xd8, 0x1e, 0xe2, 0xc4, 0xe2, 0xce,
            0xe2, 0xd8, 0xe2, 0xe2, 0xe2, 0xe7, 0xe2, 0xec, 0xe2, 0xf1, 0xe2, 0xf6, 0xe2, 0x0a,
            0xe2, 0x14, 0xe2, 0x28, 0xe2, 0x32, 0xe2, 0x46, 0xec, 0xba, 0xec, 0xc4, 0xec, 0xce,
            0xec, 0xd8, 0xec, 0xdd, 0xec, 0xe2, 0xec, 0xec, 0xec, 0xf1, 0xec, 0xf6, 0xec, 0xfb,
            0xec, 0x05, 0xec, 0x0a, 0xec, 0x14, 0xec, 0x1e, 0xec, 0x28, 0xec, 0x32, 0xec, 0x50,
            0xf1, 0xd3, 0xf1, 0xf6, 0xf1, 0xfb, 0xf6, 0xc4, 0xf6, 0xce, 0xf6, 0xd8, 0xf6, 0xe2,
            0xf6, 0xe7, 0xf6, 0xec, 0xf6, 0xf1, 0xf6, 0xf6, 0xf6, 0xfb, 0xf6, 0x05, 0xf6, 0x0a,
            0xf6, 0x14, 0xf6, 0x19, 0xf6, 0x1e, 0xf6, 0x28, 0xf6, 0x32, 0xf6, 0x3c, 0xf6, 0x50,
            0xfb, 0xec, 0xfb, 0xf6, 0xfb, 0x05, 0xfb, 0x0a, 0xfb, 0x14, 0xfb, 0x19, 0xfb, 0x2d,
            0xfb, 0x37, 0x05, 0xe7, 0x05, 0xec, 0x05, 0xf1, 0x05, 0xf6, 0x05, 0xfb, 0x05, 0x05,
            0x05, 0x0a, 0x0a, 0xc9, 0x0a, 0xce, 0x0a, 0xd3, 0x0a, 0xd8, 0x0a, 0xe2, 0x0a, 0xec,
            0x0a, 0xf1, 0x0a, 0xf6, 0x0a, 0xfb, 0x0a, 0x05, 0x0a, 0x0a, 0x0a, 0x0f, 0x0a, 0x14,
            0x0a, 0x1e, 0x0a, 0x28, 0x0a, 0x32, 0x0a, 0x3c, 0x0a, 0x46, 0x0a, 0x50, 0x0f, 0xfb,
            0x0f, 0x05, 0x0f, 0x0a, 0x0f, 0x0f, 0x14, 0xc4, 0x14, 0xce, 0x14, 0xd8, 0x14, 0xe2,
            0x14, 0xec, 0x14, 0xf6, 0x14, 0x0a, 0x14, 0x0f, 0x14, 0x14, 0x14, 0x1e, 0x14, 0x28,
            0x14, 0x32, 0x14, 0x3c, 0x14, 0x46, 0x1e, 0xec, 0x1e, 0xf6, 0x1e, 0xfb, 0x1e, 0x0a,
            0x1e, 0x14, 0x1e, 0x1e, 0x1e, 0x28, 0x1e, 0x32, 0x1e, 0x3c, 0x28, 0xe2, 0x28, 0x0a,
            0x28, 0x14, 0x28, 0x1e, 0x28, 0x28, 0x28, 0x32, 0x32, 0x14, 0x00, 0x05, 0x00, 0x00,
            0x00, 0x03, 0x00, 0x00, 0x00, 0x01, 0x00, 0x03, 0xe2, 0xf6, 0x1e, 0xe2, 0x00, 0x1e,
            0xec, 0x00, 0x14, 0x00, 0x1e, 0x1e, 0x14, 0x1e, 0x0a,
        ];

        let item_varstore = ItemVariationStore::read(FontData::new(&raw_bytes)).unwrap();

        let mut plan = Plan::default();
        // create inner maps
        let mut bimap = IncBiMap::with_capacity(1);
        bimap.add(10);
        plan.base_varstore_inner_maps.push(bimap);

        let mut bimap = IncBiMap::with_capacity(4);
        bimap.add(13);
        bimap.add(16);
        bimap.add(17);
        bimap.add(18);
        plan.base_varstore_inner_maps.push(bimap);

        let mut bimap = IncBiMap::with_capacity(3);
        bimap.add(100);
        bimap.add(101);
        bimap.add(122);
        plan.base_varstore_inner_maps.push(bimap);

        let bimap = IncBiMap::default();
        plan.base_varstore_inner_maps.push(bimap);

        //layout_scripts
        plan.layout_scripts.invert();

        //layout_features
        plan.layout_features
            .extend(DEFAULT_LAYOUT_FEATURES.iter().copied());

        let mut s = Serializer::new(1024);
        assert_eq!(s.start_serialize(), Ok(()));
        let ret = item_varstore.subset(&plan, &mut s, (&plan.base_varstore_inner_maps, false));
        assert_eq!(ret, Ok(()));
        assert!(!s.in_error());
        s.end_serialize();

        let subsetted_data = s.copy_bytes();
        let expected_bytes: [u8; 85] = [
            0x00, 0x01, 0x00, 0x00, 0x00, 0x39, 0x00, 0x03, 0x00, 0x00, 0x00, 0x30, 0x00, 0x00,
            0x00, 0x24, 0x00, 0x00, 0x00, 0x14, 0x00, 0x03, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00,
            0x00, 0x01, 0x0a, 0x05, 0x0a, 0x0a, 0x14, 0x14, 0x00, 0x04, 0x00, 0x00, 0x00, 0x01,
            0x00, 0x01, 0xf6, 0x0a, 0x0f, 0x14, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00,
            0x14, 0x00, 0x02, 0x00, 0x02, 0xc0, 0x00, 0xc0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x40, 0x00, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00,
        ];

        assert_eq!(subsetted_data, expected_bytes);
    }

    #[test]
    fn test_harfbuzz_item_variations() {
        const HVAR_DATA: [u8; 205] = [
            0x0, 0x1, 0x0, 0x0, 0x0, 0x0, 0x0, 0x14, 0x0, 0x0, 0x0, 0xc4, 0x0, 0x0, 0x0, 0x0, 0x0,
            0x0, 0x0, 0x0, 0x0, 0x1, 0x0, 0x0, 0x0, 0x10, 0x0, 0x2, 0x0, 0x0, 0x0, 0x74, 0x0, 0x0,
            0x0, 0x7a, 0x0, 0x2, 0x0, 0x8, 0xc0, 0x0, 0xc0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0,
            0x0, 0x0, 0x0, 0x40, 0x0, 0x40, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0,
            0x0, 0x0, 0xc0, 0x0, 0xc0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x40,
            0x0, 0x40, 0x0, 0xc0, 0x0, 0xc0, 0x0, 0x0, 0x0, 0xc0, 0x0, 0xc0, 0x0, 0x0, 0x0, 0xc0,
            0x0, 0xc0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x40, 0x0, 0x40, 0x0, 0x0, 0x0, 0x40, 0x0, 0x40,
            0x0, 0xc0, 0x0, 0xc0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x40, 0x0, 0x40, 0x0, 0x0, 0x0, 0x40,
            0x0, 0x40, 0x0, 0x0, 0x1, 0x0, 0x0, 0x0, 0x0, 0x0, 0x4, 0x0, 0x0, 0x0, 0x8, 0x0, 0x0,
            0x0, 0x1, 0x0, 0x2, 0x0, 0x3, 0x0, 0x4, 0x0, 0x5, 0x0, 0x6, 0x0, 0x7, 0xf9, 0xf, 0x2f,
            0xbf, 0xfb, 0xfb, 0x35, 0xf9, 0x4, 0x4, 0xf3, 0xb4, 0xf2, 0xfb, 0x2e, 0xf3, 0x4, 0x4,
            0xe, 0xad, 0xfa, 0x1, 0x1a, 0x1, 0x15, 0x22, 0x59, 0xd6, 0xe3, 0xf6, 0x6, 0xf5, 0x0,
            0x1, 0x0, 0x5, 0x0, 0x4, 0x7, 0x5, 0x6,
        ];
        let hvar_table = Hvar::read(FontData::new(&HVAR_DATA)).unwrap();
        let axis_idx_tag_map: FnvHashMap<usize, Tag> =
            FnvHashMap::from_iter([(0, Tag::new(b"wght")), (1, Tag::new(b"opsz"))]);
        let src_var_store = hvar_table
            .item_variation_store()
            .expect("HVAR table should contain item variation store");
        let mut item_vars =
            ItemVariations::create_from_item_varstore(&src_var_store, &axis_idx_tag_map, &[])
                .unwrap();
        // Comment in Harfbuzz test says "partial instancing wght=300:800", but axis_tag is actually
        // opsz at that point in the code, oops.
        let normalized_axes_location =
            FnvHashMap::from_iter([(Tag::new(b"opsz"), Triple::new(-0.512817, 0.0, 0.7000120))]);
        let axes_triple_distances =
            FnvHashMap::from_iter([(Tag::new(b"opsz"), TripleDistances::new(200.0, 500.0))]);
        item_vars
            .instantiate_tuple_vars(&normalized_axes_location, &axes_triple_distances)
            .expect("Instantiation should succeed");
        item_vars
            .as_item_varstore(false, true)
            .expect("Should be able to convert back to varstore");
        assert_eq!(item_vars.get_region_list().len(), 8);
    }

    #[test]
    fn test_harfbuzz_tuple_variations() {
        let axis_tag = Tag::new(b"wght");

        const CVAR_DATA: [u8; 185] = [
            0x0, 0x1, 0x0, 0x0, 0x0, 0x2, 0x0, 0x14, 0x0, 0x51, 0xa0, 0x0, 0xc0, 0x0, 0x0, 0x54,
            0xa0, 0x0, 0x40, 0x0, 0x2a, 0x29, 0x17, 0x1, 0x1, 0x1, 0x1, 0x1, 0x1, 0x1, 0x1, 0x1,
            0x1, 0x1, 0x1, 0x1, 0x1, 0x1, 0x1, 0x1, 0x1, 0x1, 0x1, 0x1, 0x1, 0x1, 0x1, 0x1, 0x1,
            0x1, 0x1, 0x1, 0x1, 0x1, 0x1, 0x1, 0x1, 0x1, 0x1, 0x1, 0x1, 0x1, 0x1, 0x1, 0xd, 0xff,
            0x0, 0xfd, 0x1, 0x0, 0xff, 0x0, 0xfd, 0x1, 0x0, 0xdb, 0xdb, 0xe6, 0xe6, 0x82, 0x0,
            0xfd, 0x84, 0x6, 0xfd, 0x0, 0x2, 0xe3, 0xe3, 0xec, 0xec, 0x82, 0x4, 0x1, 0xe3, 0xe3,
            0xec, 0xec, 0x82, 0x0, 0x1, 0x2a, 0x29, 0x17, 0x1, 0x1, 0x1, 0x1, 0x1, 0x1, 0x1, 0x1,
            0x1, 0x1, 0x1, 0x1, 0x1, 0x1, 0x1, 0x1, 0x1, 0x1, 0x1, 0x1, 0x1, 0x1, 0x1, 0x1, 0x1,
            0x1, 0x1, 0x1, 0x1, 0x1, 0x1, 0x1, 0x1, 0x1, 0x1, 0x1, 0x1, 0x1, 0x1, 0x1, 0x1, 0xd,
            0x1, 0x0, 0x5, 0xfd, 0x0, 0x1, 0x0, 0x5, 0xfd, 0x0, 0x61, 0x61, 0x44, 0x44, 0x82, 0x0,
            0x5, 0x81, 0x9, 0x1, 0xff, 0x1, 0x7, 0xff, 0xfb, 0x49, 0x49, 0x35, 0x35, 0x82, 0x4,
            0xff, 0x49, 0x49, 0x35, 0x35, 0x82, 0x0, 0xff,
        ];
        let cvar = Cvar::read(FontData::new(&CVAR_DATA)).unwrap();
        let vardata = cvar.variation_data(1).unwrap();
        let axes_map = FnvHashMap::from_iter([(0, axis_tag)]);
        let mut tuple_variations = TupleVariations::from_cvar(vardata, 65, &axes_map).unwrap();
        assert_eq!(tuple_variations.tuple_vars.len(), 2);
        for var in tuple_variations.tuple_vars.iter() {
            assert_eq!(var.axis_tuples.len(), 1);
            assert_eq!(var.indices.len(), 65);
        }
        assert_eq!(
            tuple_variations.tuple_vars[0]
                .axis_tuples
                .get(&axis_tag)
                .copied()
                .unwrap(),
            Triple::new(-1.0, -1.0, 0.0)
        );
        assert_eq!(
            tuple_variations.tuple_vars[1]
                .axis_tuples
                .get(&axis_tag)
                .copied()
                .unwrap(),
            Triple::new(0.0, 1.0, 1.0)
        );

        let deltas_1: [f32; 65] = [
            0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
            0.0, 0.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, -3.0, 1.0, 0.0, -1.0, 0.0, -3.0, 1.0, 0.0,
            -37.0, -37.0, -26.0, -26.0, 0.0, 0.0, 0.0, -3.0, 0.0, 0.0, 0.0, 0.0, 0.0, -3.0, 0.0,
            2.0, -29.0, -29.0, -20.0, -20.0, 0.0, 0.0, 0.0, 1.0, -29.0, -29.0, -20.0, -20.0, 0.0,
            0.0, 0.0, 1.0,
        ];
        assert_eq!(tuple_variations.tuple_vars[0].deltas_x, deltas_1);
        let deltas_2: [f32; 65] = [
            0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
            0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 5.0, -3.0, 0.0, 1.0, 0.0, 5.0, -3.0, 0.0, 97.0,
            97.0, 68.0, 68.0, 0.0, 0.0, 0.0, 5.0, 0.0, 0.0, 1.0, -1.0, 1.0, 7.0, -1.0, -5.0, 73.0,
            73.0, 53.0, 53.0, 0.0, 0.0, 0.0, -1.0, 73.0, 73.0, 53.0, 53.0, 0.0, 0.0, 0.0, -1.0,
        ];
        assert_eq!(tuple_variations.tuple_vars[1].deltas_x, deltas_2);

        /* partial instancing wght=300:800 */
        let normalized_axes_location =
            FnvHashMap::from_iter([(axis_tag, Triple::new(-0.512817, 0.0, 0.700012))]);
        let axes_triple_distances =
            FnvHashMap::from_iter([(axis_tag, TripleDistances::new(1.0, 1.0))]);
        tuple_variations
            .instantiate(
                &normalized_axes_location,
                &axes_triple_distances,
                None,
                false,
            )
            .unwrap();

        assert_eq!(tuple_variations.tuple_vars[0].indices.len(), 65);
        assert_eq!(tuple_variations.tuple_vars[1].indices.len(), 65);
        assert_eq!(
            tuple_variations.tuple_vars[0]
                .axis_tuples
                .get(&axis_tag)
                .copied()
                .unwrap(),
            Triple::new(-1.0, -1.0, 0.0)
        );
        assert_eq!(
            tuple_variations.tuple_vars[1]
                .axis_tuples
                .get(&axis_tag)
                .copied()
                .unwrap(),
            Triple::new(0.0, 1.0, 1.0)
        );

        let rounded_deltas_1: [f32; 65] = [
            0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
            0.0, 0.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, -2.0, 1.0, 0.0, -1.0, 0.0, -2.0, 1.0, 0.0,
            -19.0, -19.0, -13.0, -13.0, 0.0, 0.0, 0.0, -2.0, 0.0, 0.0, 0.0, 0.0, 0.0, -2.0, 0.0,
            1.0, -15.0, -15.0, -10.0, -10.0, 0.0, 0.0, 0.0, 1.0, -15.0, -15.0, -10.0, -10.0, 0.0,
            0.0, 0.0, 1.0,
        ];

        let rounded_deltas_2: [f32; 65] = [
            0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
            0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 4.0, -2.0, 0.0, 1.0, 0.0, 4.0, -2.0, 0.0, 68.0,
            68.0, 48.0, 48.0, 0.0, 0.0, 0.0, 4.0, 0.0, 0.0, 1.0, -1.0, 1.0, 5.0, -1.0, -4.0, 51.0,
            51.0, 37.0, 37.0, 0.0, 0.0, 0.0, -1.0, 51.0, 51.0, 37.0, 37.0, 0.0, 0.0, 0.0, -1.0,
        ];

        for i in 0..65 {
            if i < 23 {
                assert_eq!(tuple_variations.tuple_vars[0].indices[i], false);
                assert_eq!(tuple_variations.tuple_vars[1].indices[i], false);
            } else {
                assert_eq!(tuple_variations.tuple_vars[0].indices[i], true);
                assert_eq!(tuple_variations.tuple_vars[1].indices[i], true);
                assert_eq!(
                    (tuple_variations.tuple_vars[0].deltas_x[i]).round(),
                    rounded_deltas_1[i]
                );
                assert_eq!(
                    (tuple_variations.tuple_vars[1].deltas_x[i]).round(),
                    rounded_deltas_2[i]
                );
            }
        }
    }
}
