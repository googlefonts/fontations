use std::hash::Hash;

use fnv::FnvHashMap;
use font_types::{F2Dot14, Offset32};
use skrifa::{
    raw::{
        collections::IntSet,
        tables::{gsub::FeatureList, layout::ConditionFormat1},
        FontData, ReadError, TopLevelTable,
    },
    Tag,
};
use write_fonts::read::tables::{
    gsub::Gsub,
    layout::{
        Condition, ConditionSet, FeatureTableSubstitution, FeatureTableSubstitutionRecord,
        FeatureVariationRecord, FeatureVariations,
    },
};

use crate::{
    layout::SubsetLayoutContext,
    offset::SerializeSubset,
    offset_array::SubsetOffsetArray,
    serialize::{SerializeErrorFlags, Serializer},
    variations::solver::{renormalize_value, Triple},
    Plan, SubsetError, SubsetTable,
};
enum CondWithVar {
    KeepCondition,
    KeepRecord,
    DropCondition,
    DropRecord,
}

#[derive(Default, Debug, PartialEq, Eq)]
pub(crate) struct ConditionMap(FnvHashMap<usize, u32>);
impl Hash for ConditionMap {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // Custom hash implementation to ensure the order of conditions does not affect the hash
        let mut sorted_conditions: Vec<(&usize, &u32)> = self.0.iter().collect();
        sorted_conditions.sort_by_key(|&(k, _)| k);
        for (k, v) in sorted_conditions {
            k.hash(state);
            v.hash(state);
        }
    }
}

trait KeepWithVariations {
    fn keep_with_variations(
        &self,
        ctx: &mut CollectFeatureSubstitutesContext,
        map: &mut ConditionMap,
    ) -> CondWithVar;
}

pub(crate) struct CollectFeatureSubstitutesContext<'a> {
    pub(crate) axes_index_tag_map: &'a FnvHashMap<usize, Tag>,
    pub(crate) axes_location: &'a FnvHashMap<Tag, Triple<f64>>,
    pub(crate) record_cond_idx_map: FnvHashMap<u16, IntSet<u16>>,
    pub(crate) catch_all_record_feature_indices: IntSet<u16>,

    pub(crate) feature_indices: IntSet<u16>,
    pub(crate) apply: bool,
    pub(crate) variation_applied: bool,
    pub(crate) universal: bool,
    pub(crate) cur_record_idx: u16,
    pub(crate) conditionset_map: FnvHashMap<ConditionMap, u16>,
    /// For each feature index that has a substitute, store the lookup indices from that substitute
    pub(crate) feature_substitutes_lookup_map: FnvHashMap<u16, IntSet<u16>>,
}

impl<'a> SubsetTable<'a> for FeatureVariations<'_> {
    type ArgsForSubset = (&'a mut SubsetLayoutContext, bool);
    type Output = ();
    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        args: Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        let (c, insert_catch_all) = args;
        let feature_index_map = if c.table_tag == Gsub::TAG {
            &plan.gsub_features_w_duplicates
        } else {
            &plan.gpos_features_w_duplicates
        };
        let num_retained_records = num_variation_record_to_retain(self, feature_index_map, s)?;
        if num_retained_records == 0 {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY);
        }

        s.embed(self.version())?;
        s.embed(num_retained_records)?;

        let font_data = self.offset_data();

        let variation_records = self.feature_variation_records();
        for i in 0..num_retained_records {
            if !c.feature_record_cond_idx_map.is_empty()
                && !c.feature_record_cond_idx_map.contains_key(&(i as u16))
            {
                continue;
            }
            c.cur_feature_var_record_idx = i as u16;
            variation_records[i as usize].subset(
                plan,
                s,
                (font_data, feature_index_map, c, insert_catch_all),
            )?;
        }
        Ok(())
    }
}

// Prune empty records at the end only
// ref: <https://github.com/fonttools/fonttools/blob/3c1822544d608f87c41fc8fb9ba41ea129257aa8/Lib/fontTools/subset/__init__.py#L1782>
fn num_variation_record_to_retain(
    feature_variations: &FeatureVariations,
    feature_index_map: &FnvHashMap<u16, u16>,
    s: &mut Serializer,
) -> Result<u32, SerializeErrorFlags> {
    let num_records = feature_variations.feature_variation_record_count();
    let variation_records = feature_variations.feature_variation_records();
    let font_data = feature_variations.offset_data();

    for i in (0..num_records).rev() {
        let Some(feature_substitution) = variation_records[i as usize]
            .feature_table_substitution(font_data)
            .transpose()
            .map_err(|_| s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR))?
        else {
            continue;
        };

        if feature_substitution
            .substitutions()
            .iter()
            .any(|subs| feature_index_map.contains_key(&subs.feature_index()))
        {
            return Ok(i + 1);
        }
    }
    Ok(0)
}

impl<'a> SubsetTable<'a> for FeatureVariationRecord {
    type ArgsForSubset = (
        FontData<'a>,
        &'a FnvHashMap<u16, u16>,
        &'a mut SubsetLayoutContext,
        bool,
    );
    type Output = ();
    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        args: Self::ArgsForSubset,
    ) -> Result<Self::Output, SerializeErrorFlags> {
        let (font_data, feature_index_map, c, insert_catch_all) = args;
        let condition_set_offset_pos = s.embed(0_u32)?;
        if let Some(condition_set) = self
            .condition_set(font_data)
            .transpose()
            .map_err(|_| s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR))?
        {
            Offset32::serialize_subset(
                &condition_set,
                s,
                plan,
                (c, insert_catch_all),
                condition_set_offset_pos,
            )?;
        }

        let feature_substitutions_offset_pos = s.embed(0_u32)?;
        if let Some(feature_subs) = self
            .feature_table_substitution(font_data)
            .transpose()
            .map_err(|_| s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR))?
        {
            Offset32::serialize_subset(
                &feature_subs,
                s,
                plan,
                (feature_index_map, c),
                feature_substitutions_offset_pos,
            )?;
        }

        Ok(())
    }
}

impl<'a> SubsetTable<'a> for ConditionSet<'a> {
    type ArgsForSubset = (&'a mut SubsetLayoutContext, bool);
    type Output = ();
    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        args: Self::ArgsForSubset,
    ) -> Result<Self::Output, SerializeErrorFlags> {
        let (context, insert_catch_all) = args;
        let count_pos = s.embed(0_u16)?;
        let mut count = 0_u16;

        if insert_catch_all {
            // If we are inserting a catch-all record, keep things as they are
            return Ok(());
        }
        let retained_cond_set: Option<&IntSet<u16>> = context
            .feature_record_cond_idx_map
            .get(&context.cur_feature_var_record_idx);

        let conditions = self.conditions();
        for i in 0..self.condition_count() {
            if retained_cond_set.is_some_and(|set| !set.contains(i)) {
                continue;
            }
            match conditions.subset_offset(i as usize, s, plan, ()) {
                Ok(()) => count += 1,
                Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY) => continue,
                Err(e) => return Err(e),
            }
        }

        if count == 0 {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY);
        }
        s.copy_assign(count_pos, count);
        Ok(())
    }
}

// Not using the trait, we are top-level. We don't return anything, just mutate context
fn keep_with_variations(
    c: ConditionSet<'_>,
    ctx: &mut CollectFeatureSubstitutesContext,
) -> Result<(), ReadError> {
    // Expect a new empty map
    let mut map = ConditionMap::default();
    let mut cond_set = IntSet::default();

    ctx.apply = true;
    let mut should_keep = false;
    let mut num_kept_cond = 0;
    for (ix, condition) in c.conditions().iter().enumerate() {
        match condition?.keep_with_variations(ctx, &mut map) {
            CondWithVar::DropRecord => {
                return Ok(());
            }
            CondWithVar::KeepCondition => {
                should_keep = true;
                cond_set.insert(ix as u16);
                num_kept_cond += 1;
            }
            CondWithVar::KeepRecord => {
                should_keep = true;
            }
            CondWithVar::DropCondition => {}
        }
    }

    if !should_keep {
        return Ok(());
    }
    //check if condition_set is unique with variations
    if ctx.conditionset_map.contains_key(&map) {
        //duplicate found, drop the entire record
        return Ok(());
    }
    ctx.conditionset_map.insert(map, 1);
    ctx.record_cond_idx_map.insert(ctx.cur_record_idx, cond_set);
    if should_keep && num_kept_cond == 0 {
        ctx.universal = true;
    }
    Ok(())
}

impl SubsetTable<'_> for Condition<'_> {
    type ArgsForSubset = ();
    type Output = ();
    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        _args: Self::ArgsForSubset,
    ) -> Result<Self::Output, SerializeErrorFlags> {
        match self {
            Self::Format1AxisRange(item) => item.subset(plan, s, ()),
            // TODO: support other formats
            _ => Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY),
        }
    }
}

impl KeepWithVariations for Condition<'_> {
    fn keep_with_variations(
        &self,
        ctx: &mut CollectFeatureSubstitutesContext,
        map: &mut ConditionMap,
    ) -> CondWithVar {
        match self {
            Condition::Format1AxisRange(c) => c.keep_with_variations(ctx, map),
            _ => {
                ctx.apply = false;
                CondWithVar::KeepCondition
            }
        }
    }
}

impl SubsetTable<'_> for ConditionFormat1<'_> {
    type ArgsForSubset = ();
    type Output = ();
    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        _args: Self::ArgsForSubset,
    ) -> Result<Self::Output, SerializeErrorFlags> {
        let axis_index = self.axis_index() as usize;
        s.embed_bytes(self.min_table_bytes()).map(|_| ())?;
        if plan.axes_index_map.is_empty() || !plan.axes_index_map.contains_key(&axis_index) {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY);
        }
        let Some(axis_tag) = plan.axes_old_index_tag_map.get(&axis_index) else {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY);
        };
        let axis_limit = plan
            .axes_location
            .get(axis_tag)
            .copied()
            .unwrap_or_default();
        let axis_triple_distances = plan
            .axes_triple_distances
            .get(axis_tag)
            .copied()
            .unwrap_or_default();
        let normalized_min = renormalize_value(
            self.filter_range_min_value().to_f32() as f64,
            axis_limit,
            axis_triple_distances,
            false,
        );
        let normalized_max = renormalize_value(
            self.filter_range_max_value().to_f32() as f64,
            axis_limit,
            axis_triple_distances,
            false,
        );

        s.copy_assign(
            self.shape().filter_range_min_value_byte_range().start,
            F2Dot14::from_f32(normalized_min as f32),
        );
        s.copy_assign(
            self.shape().filter_range_max_value_byte_range().start,
            F2Dot14::from_f32(normalized_max as f32),
        );
        Ok(())
    }
}

impl KeepWithVariations for ConditionFormat1<'_> {
    fn keep_with_variations(
        &self,
        ctx: &mut CollectFeatureSubstitutesContext,
        map: &mut ConditionMap,
    ) -> CondWithVar {
        //invalid axis index, drop the entire record
        let axis_index = self.axis_index() as usize;
        let Some(axis_tag) = ctx.axes_index_tag_map.get(&axis_index) else {
            return CondWithVar::DropRecord;
        };

        let (axis_range, set_by_user): (Triple<f64>, bool) =
            if let Some(axis_limit) = ctx.axes_location.get(axis_tag) {
                (*axis_limit, true)
            } else {
                (Triple::default(), false)
            };

        let axis_min_val = axis_range.minimum;
        let axis_default_val = axis_range.middle;
        let axis_max_val = axis_range.maximum;

        let filter_min_val = self.filter_range_min_value().to_f32() as f64;
        let filter_max_val = self.filter_range_max_value().to_f32() as f64;

        // log::debug!("Filter min/max: {filter_min_val}, {filter_max_val}");
        // log::debug!("Axis min/default/max: {axis_min_val}, {axis_default_val}, {axis_max_val}");

        if axis_default_val < filter_min_val || axis_default_val > filter_max_val {
            // log::debug!("  Don't apply");
            ctx.apply = false;
        }

        //condition not met, drop the entire record
        if axis_min_val > filter_max_val
            || axis_max_val < filter_min_val
            || filter_min_val > filter_max_val
        {
            // log::debug!("  Condition not met, drop the record");
            return CondWithVar::DropRecord;
        }

        //condition met and axis pinned, drop the condition
        if set_by_user && axis_range.is_point() {
            // log::debug!("  Condition met and axis pinned, drop the condition");
            return CondWithVar::DropCondition;
        }

        if filter_max_val != axis_max_val || filter_min_val != axis_min_val {
            // add axisIndex->value into the hashmap so we can check if the record is
            // unique with variations
            let int_filter_max_val = self.filter_range_max_value().to_bits() as u16; // Unsure about this cast
            let int_filter_min_val = self.filter_range_min_value().to_bits() as u16;
            let val: u32 = ((int_filter_max_val as u32) << 16) + (int_filter_min_val as u32);

            map.0.insert(axis_index, val);
            // log::debug!("  Condition met, keep the condition");
            return CondWithVar::KeepCondition;
        }

        CondWithVar::KeepRecord
    }
}

impl<'a> SubsetTable<'a> for FeatureTableSubstitution<'_> {
    type ArgsForSubset = (&'a FnvHashMap<u16, u16>, &'a mut SubsetLayoutContext);
    type Output = ();
    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        args: Self::ArgsForSubset,
    ) -> Result<Self::Output, SerializeErrorFlags> {
        s.embed(self.version())?;

        // substitution count
        let subs_count_pos = s.embed(0_u16)?;
        let mut subs_count = 0_u16;

        let (feature_index_map, c) = args;
        let font_data = self.offset_data();
        for sub in self.substitutions() {
            match sub.subset(plan, s, (feature_index_map, c, font_data)) {
                Ok(()) => subs_count += 1,
                Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY) => continue,
                Err(e) => return Err(e),
            }
        }

        if subs_count == 0 {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY);
        }
        s.copy_assign(subs_count_pos, subs_count);
        Ok(())
    }
}

impl<'a> SubsetTable<'a> for FeatureTableSubstitutionRecord {
    type ArgsForSubset = (
        &'a FnvHashMap<u16, u16>,
        &'a mut SubsetLayoutContext,
        FontData<'a>,
    );
    type Output = ();
    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        args: Self::ArgsForSubset,
    ) -> Result<Self::Output, SerializeErrorFlags> {
        let (feature_index_map, c, font_data) = args;
        let Some(new_feature_indx) = feature_index_map.get(&self.feature_index()) else {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY);
        };

        let alternate_feature = self
            .alternate_feature(font_data)
            .map_err(|_| s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR))?;
        s.embed(*new_feature_indx)?;

        let feature_offset_pos = s.embed(0_u32)?;
        Offset32::serialize_subset(&alternate_feature, s, plan, (c, None), feature_offset_pos)
    }
}

pub(crate) fn collect_feature_substitutes_with_variations(
    feature_variations: &FeatureVariations,
    ctx: &mut CollectFeatureSubstitutesContext,
) -> Result<(), SubsetError> {
    // Not returning anything, just modify context
    let data = feature_variations.offset_data();

    for (record_idx, record) in feature_variations
        .feature_variation_records()
        .iter()
        .enumerate()
    {
        ctx.cur_record_idx = record_idx as u16;

        // varRecords[i].collect_feature_substitutes_with_variations (c, this)
        if let Some(Ok(cond_set)) = record.condition_set(data) {
            keep_with_variations(cond_set, ctx).map_err(|e| SubsetError::ReadError(e))?;
            // log::debug!(
            //     "Record {}: kept conditions {:?} after applying variations",
            //     record_idx,
            //     &ctx.record_cond_idx_map.get(&ctx.cur_record_idx)
            // );

            if ctx.apply && !ctx.variation_applied {
                // Extract feature substitutes from FeatureTableSubstitution
                // (base+substitutions).collect_feature_substitutes_with_variations (c);
                if let Some(Ok(subs)) = record.feature_table_substitution(data) {
                    for sub_rec in subs.substitutions() {
                        // record.collect_feature_substitutes_with_variations
                        let feature_index = sub_rec.feature_index();
                        if ctx.feature_indices.contains(feature_index) {
                            // Extract lookup indices from the substitute feature
                            if let Ok(alternate_feature) =
                                sub_rec.alternate_feature(subs.offset_data())
                            {
                                let mut lookups = IntSet::default();
                                for lookup_idx in alternate_feature.lookup_list_indices() {
                                    lookups.insert(lookup_idx.get());
                                }
                                ctx.feature_substitutes_lookup_map
                                    .insert(feature_index, lookups);
                                ctx.catch_all_record_feature_indices.insert(feature_index);
                            }
                        }
                    }
                }
                ctx.variation_applied = true; // Set variations only once
            }
        }
        if ctx.universal {
            break;
        }
    }

    if ctx.universal || ctx.record_cond_idx_map.is_empty() {
        ctx.catch_all_record_feature_indices.clear();
    }

    Ok(())
}

pub(crate) struct FeatureSubstituteCollectionResult {
    pub(crate) catch_all_record_feature_indices: IntSet<u16>,
    /// For each feature index that has a substitute, store the lookup indices from that substitute
    pub(crate) feature_substitutes_lookup_map: FnvHashMap<u16, IntSet<u16>>,
    pub(crate) variation_applied: bool,
    pub(crate) record_cond_idx_map: FnvHashMap<u16, IntSet<u16>>,
}
impl FeatureSubstituteCollectionResult {
    pub(crate) fn empty() -> Self {
        Self {
            catch_all_record_feature_indices: IntSet::default(),
            feature_substitutes_lookup_map: FnvHashMap::default(),
            variation_applied: false,
            record_cond_idx_map: FnvHashMap::default(),
        }
    }
}

impl Into<FeatureSubstituteCollectionResult> for CollectFeatureSubstitutesContext<'_> {
    fn into(self) -> FeatureSubstituteCollectionResult {
        FeatureSubstituteCollectionResult {
            catch_all_record_feature_indices: self.catch_all_record_feature_indices,
            feature_substitutes_lookup_map: self.feature_substitutes_lookup_map,
            variation_applied: self.variation_applied,
            record_cond_idx_map: self.record_cond_idx_map,
        }
    }
}

/// Collect lookups from features, using substitutes where they exist
pub(crate) fn collect_lookups_with_substitutes(
    // gsub: &Gsub,
    feature_list: FeatureList<'_>,
    feature_indices: &IntSet<u16>,
    feature_substitutes: &FeatureSubstituteCollectionResult,
) -> Result<IntSet<u16>, SubsetError> {
    let mut lookup_indices = IntSet::empty();

    for feature_index in feature_indices.iter() {
        // Check if this feature has a substitute from feature variations
        if let Some(substitute_lookups) = feature_substitutes
            .feature_substitutes_lookup_map
            .get(&feature_index)
        {
            // Use the substitute feature's lookups
            lookup_indices.extend(substitute_lookups.iter());
        } else {
            // Use the default feature's lookups
            if let Some(record) = feature_list.feature_records().get(feature_index as usize) {
                if let Ok(feature) = record.feature(feature_list.offset_data()) {
                    for lookup_idx in feature.lookup_list_indices() {
                        lookup_indices.insert(lookup_idx.get());
                    }
                }
            }
        }
    }

    Ok(lookup_indices)
}

/// Collect lookups from feature variations whose conditions matched
pub(crate) fn feature_variation_collect_lookups(
    feature_variations: &FeatureVariations,
    font_data: FontData,
    feature_indices: &IntSet<u16>,
    feature_record_cond_idx_map: &FnvHashMap<u16, IntSet<u16>>,
    lookup_indices: &mut IntSet<u16>,
) -> Result<(), SubsetError> {
    let var_records = feature_variations.feature_variation_records();

    for (record_idx, var_record) in var_records.iter().enumerate() {
        // Only process records whose conditions matched (are in the map)
        if !feature_record_cond_idx_map.contains_key(&(record_idx as u16)) {
            continue;
        }

        if let Some(feature_subs) = var_record
            .feature_table_substitution(font_data)
            .transpose()
            .ok()
            .flatten()
        {
            for sub_rec in feature_subs.substitutions() {
                let feature_index = sub_rec.feature_index();

                // Only collect if this is a feature we're keeping
                if !feature_indices.contains(feature_index) {
                    continue;
                }

                if let Ok(substitute_feature) =
                    sub_rec.alternate_feature(feature_subs.offset_data())
                {
                    for lookup_idx in substitute_feature.lookup_list_indices() {
                        lookup_indices.insert(lookup_idx.get());
                    }
                }
            }
        }
    }

    Ok(())
}
