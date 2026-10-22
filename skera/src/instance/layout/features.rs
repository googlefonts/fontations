//! Evaluate and rebase feature conditions, then remap their shared-store rows.
use super::{ConditionRow, LayoutContext};
use crate::{
    instance::{optimize, rebase::renormalize},
    SubsetError,
};
use read_fonts::{
    types::{F2Dot14, Tag},
    ResolveOffset,
};
use write_fonts::{from_obj::ToOwnedTable, tables::layout::*, NullableOffsetMarker};

pub(super) fn condition(
    cond: &Condition,
    c: &LayoutContext,
    depth: usize,
) -> Result<bool, SubsetError> {
    if depth > 32 {
        return Err(SubsetError::SubsetTableError(Tag::new(b"GSUB")));
    }
    Ok(match cond {
        Condition::Format1AxisRange(v) => {
            let coord = c
                .coords
                .get(v.axis_index as usize)
                .copied()
                .unwrap_or(F2Dot14::ZERO);
            coord >= v.filter_range_min_value && coord <= v.filter_range_max_value
        }
        Condition::Format2VariableValue(v) => {
            v.default_value as f64 + c.condition_delta(v.var_index) > 0.
        }
        Condition::Format3And(v) => {
            let mut result = true;
            for v in &v.conditions {
                result &= condition(v, c, depth + 1)?;
            }
            result
        }
        Condition::Format4Or(v) => {
            let mut result = false;
            for v in &v.conditions {
                result |= condition(v, c, depth + 1)?;
            }
            result
        }
        Condition::Format5Negate(v) => !condition(&v.condition, c, depth + 1)?,
    })
}
pub(super) fn instance_features(
    list: &mut FeatureList,
    vars: &mut NullableOffsetMarker<FeatureVariations, 4>,
    c: &LayoutContext,
) -> Result<(), SubsetError> {
    if !c.axes.all_pinned() {
        if let Some(vars) = vars.as_mut() {
            let mut records = Vec::new();
            for record in &vars.feature_variation_records {
                let mut conditions = Vec::new();
                let mut possible = true;
                if let Some(set) = record.condition_set.as_ref() {
                    for cond in &set.conditions {
                        match partial_condition(cond, c, 0)? {
                            PartialCondition::Constant(false) => {
                                possible = false;
                                break;
                            }
                            PartialCondition::Constant(true) => {}
                            PartialCondition::Variable(v) => conditions.push(v.into()),
                        }
                    }
                }
                if !possible {
                    continue;
                }
                let mut record = record.clone();
                let unconditional = conditions.is_empty();
                record.condition_set = ConditionSet { conditions }.into();
                records.push(record);
                if unconditional {
                    break;
                }
            }
            vars.feature_variation_records = records;
            for variation in vars.lookup_variation_records.iter_mut().flatten() {
                let mut records = Vec::new();
                for record in &variation.feature_lookups.lookup_condition_records {
                    let mut record = record.clone();
                    if let Some(cond) = record.condition.as_ref() {
                        match partial_condition(cond, c, 0)? {
                            PartialCondition::Constant(false) => continue,
                            PartialCondition::Constant(true) => {
                                record.condition = Default::default()
                            }
                            PartialCondition::Variable(cond) => {
                                record.condition = Some(cond).into()
                            }
                        }
                    }
                    records.push(record);
                }
                variation.feature_lookups.lookup_condition_records = records;
            }
        }
        return Ok(());
    }
    if let Some(vars) = vars.as_ref() {
        for record in &vars.feature_variation_records {
            let mut matches = true;
            if let Some(set) = record.condition_set.as_ref() {
                for cond in &set.conditions {
                    matches &= condition(cond, c, 0)?;
                }
            }
            if !matches {
                continue;
            }
            if let Some(subs) = record.feature_table_substitution.as_ref() {
                for sub in &subs.substitutions {
                    let feature = list
                        .feature_records
                        .get_mut(sub.feature_index as usize)
                        .ok_or(SubsetError::SubsetTableError(Tag::new(b"GSUB")))?;
                    feature.feature = (*sub.alternate_feature).clone().into();
                }
            }
            break;
        }
    }
    if let Some(vars) = vars.as_ref() {
        for variation in vars.lookup_variation_records.iter().flatten() {
            let feature = list
                .feature_records
                .get_mut(usize::from(variation.feature_index))
                .ok_or(SubsetError::SubsetTableError(Tag::new(b"GSUB")))?;
            let table = variation.feature_lookups.as_ref();
            let lookups = &mut feature.feature.lookup_list_indices;
            if !table
                .flags
                .contains(FeatureLookupsFlags::ADD_DEFAULT_LOOKUPS)
            {
                lookups.clear();
            }
            for record in &table.lookup_condition_records {
                if record
                    .condition
                    .as_ref()
                    .map(|cond| condition(cond, c, 0))
                    .transpose()?
                    .unwrap_or(true)
                {
                    lookups.extend(&record.lookup_index_list.lookup_indices);
                }
            }
            lookups.sort_unstable();
            lookups.dedup();
        }
    }
    *vars = Default::default();
    Ok(())
}
pub(super) enum PartialCondition {
    Constant(bool),
    Variable(Condition),
}
pub(super) fn partial_condition(
    cond: &Condition,
    c: &LayoutContext,
    depth: usize,
) -> Result<PartialCondition, SubsetError> {
    if depth > 32 {
        return Err(SubsetError::SubsetTableError(Tag::new(b"GSUB")));
    }
    Ok(match cond {
        Condition::Format1AxisRange(v) => {
            let i = v.axis_index as usize;
            let limit = *c
                .axes
                .normalized
                .get(i)
                .ok_or(SubsetError::SubsetTableError(Tag::new(b"GSUB")))?;
            if let Some(index) = c.axes.new_index(i) {
                let min = v.filter_range_min_value.to_f64().max(limit.0);
                let max = v.filter_range_max_value.to_f64().min(limit.2);
                if min > max {
                    PartialCondition::Constant(false)
                } else if min == limit.0 && max == limit.2 {
                    PartialCondition::Constant(true)
                } else {
                    let mut v = v.clone();
                    v.axis_index = index as u16;
                    v.filter_range_min_value =
                        F2Dot14::from_f64(renormalize(min, limit, c.axes.distances[i]));
                    v.filter_range_max_value =
                        F2Dot14::from_f64(renormalize(max, limit, c.axes.distances[i]));
                    PartialCondition::Variable(Condition::Format1AxisRange(v))
                }
            } else {
                PartialCondition::Constant(condition(cond, c, depth)?)
            }
        }
        Condition::Format2VariableValue(v) => {
            let err = || SubsetError::SubsetTableError(Tag::new(b"GDEF"));
            let key = (v.var_index, v.default_value);
            if let Some(&(var_index, default_value)) = c.condition_rows.borrow().mapping.get(&key) {
                return Ok(PartialCondition::Variable(
                    Condition::format_2_variable_value(default_value, var_index),
                ));
            }
            let value = v.default_value as f64 + c.condition_delta(v.var_index);
            let data = c
                .store
                .as_ref()
                .and_then(|store| {
                    store
                        .item_variation_data()
                        .get((v.var_index >> 16) as usize)
                })
                .and_then(Result::ok);
            let Some(data) = data.filter(|data| (v.var_index as u16) < data.item_count()) else {
                return Ok(PartialCondition::Constant(value > 0.));
            };
            let plan = c.store_plan()?.unwrap();
            let transform = &plan.transforms[(v.var_index >> 16) as usize];
            let deltas: Vec<_> = data
                .delta_set(v.var_index as u16)
                .map(|d| d as f64)
                .collect();
            let residual = transform.residual(&deltas)?;
            // Condition signs depend on unrounded deltas, even when a shared
            // positioning row rounds to zero and loses its devices.
            if residual.iter().all(|&delta| delta == 0.) {
                return Ok(PartialCondition::Constant(value > 0.));
            }
            // A subnormal pinned scalar can round to zero at some surviving
            // coordinates. Rescaling the residual loses that source boundary.
            if plan.has_subnormal_pins(&data, v.var_index as u16) {
                return Err(err());
            }
            // Positive rescaling preserves the Boolean boundary. Prefer exact
            // integers, otherwise use the available signed 32-bit precision.
            let maximum = residual.iter().fold(value.abs(), |m, d| m.max(d.abs()));
            if !maximum.is_finite() {
                return Err(err());
            }
            let mut scale = 1.;
            while maximum * scale > i32::MAX as f64 {
                scale *= 0.5;
            }
            while maximum * scale * 2. <= i32::MAX as f64 {
                if value * scale == (value * scale).round()
                    && residual.iter().all(|d| d * scale == (d * scale).round())
                {
                    break;
                }
                scale *= 2.;
            }
            let default = (value * scale).round() as i32;
            let mut v = v.clone();
            let mut rows = c.condition_rows.borrow_mut();
            if scale != 1. || i16::try_from(default).is_err() {
                // Give conditions a private row so scaling cannot change the
                // positioning values that share the original VariationIndex.
                let outer = c
                    .store
                    .as_ref()
                    .ok_or_else(err)?
                    .item_variation_data_count() as usize
                    + rows.rows.len();
                if outer >= u16::MAX as usize {
                    return Err(err());
                }
                let bias = i16::try_from(default).is_err().then_some(default);
                rows.rows.push(ConditionRow {
                    indices: transform.indices.clone(),
                    deltas: residual.iter().map(|d| (d * scale).round()).collect(),
                    bias,
                });
                v.var_index = (outer as u32) << 16;
                v.default_value = if bias.is_some() { 0 } else { default as i16 };
            } else {
                v.default_value = default as i16;
            }
            rows.mapping.insert(key, (v.var_index, v.default_value));
            PartialCondition::Variable(Condition::Format2VariableValue(v))
        }
        Condition::Format3And(v) => {
            let mut conditions = Vec::new();
            for cond in &v.conditions {
                match partial_condition(cond, c, depth + 1)? {
                    PartialCondition::Constant(false) => {
                        return Ok(PartialCondition::Constant(false))
                    }
                    PartialCondition::Constant(true) => {}
                    PartialCondition::Variable(v) => conditions.push(v.into()),
                }
            }
            if conditions.is_empty() {
                PartialCondition::Constant(true)
            } else {
                let mut v = v.clone();
                v.condition_count = conditions.len() as u8;
                v.conditions = conditions;
                PartialCondition::Variable(Condition::Format3And(v))
            }
        }
        Condition::Format4Or(v) => {
            let mut conditions = Vec::new();
            for cond in &v.conditions {
                match partial_condition(cond, c, depth + 1)? {
                    PartialCondition::Constant(true) => {
                        return Ok(PartialCondition::Constant(true))
                    }
                    PartialCondition::Constant(false) => {}
                    PartialCondition::Variable(v) => conditions.push(v.into()),
                }
            }
            if conditions.is_empty() {
                PartialCondition::Constant(false)
            } else {
                let mut v = v.clone();
                v.condition_count = conditions.len() as u8;
                v.conditions = conditions;
                PartialCondition::Variable(Condition::Format4Or(v))
            }
        }
        Condition::Format5Negate(v) => match partial_condition(&v.condition, c, depth + 1)? {
            PartialCondition::Constant(b) => PartialCondition::Constant(!b),
            PartialCondition::Variable(cond) => {
                let mut v = v.clone();
                v.condition = cond.into();
                PartialCondition::Variable(Condition::Format5Negate(v))
            }
        },
    })
}

pub(super) fn own_feature_variations(
    vars: Option<read_fonts::tables::layout::FeatureVariations>,
    features: &read_fonts::tables::layout::FeatureList,
    tag: Tag,
) -> Result<Option<FeatureVariations>, SubsetError> {
    let err = || SubsetError::SubsetTableError(tag);
    let mut remaining = 200_000;
    if let Some(vars) = vars {
        // Check before calling the generated range getter on 32-bit hosts.
        if vars.version().major != 1
            || u64::from(vars.feature_variation_record_count()) * 8
                > vars.offset_data().len().saturating_sub(8) as u64
        {
            return Err(err());
        }
        if vars.feature_variation_records().len() != vars.feature_variation_record_count() as usize
        {
            return Err(err());
        }
        let mut records = Vec::new();
        for record in vars.feature_variation_records() {
            let condition_set = if let Some(set) = record
                .condition_set(vars.offset_data())
                .transpose()
                .map_err(|_| err())?
            {
                if set.condition_offsets().len() != set.condition_count() as usize {
                    return Err(err());
                }
                let conditions = set
                    .condition_offsets()
                    .iter()
                    .map(|offset| {
                        crate::conditions::at_offset(
                            set.offset_data(),
                            offset.get().to_u32(),
                            33,
                            &mut remaining,
                        )
                        .map(Into::into)
                        .map_err(|_| err())
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Some(ConditionSet { conditions })
            } else {
                None
            };
            let feature_table_substitution = if let Some(source) = record
                .feature_table_substitution(vars.offset_data())
                .transpose()
                .map_err(|_| err())?
            {
                if source.substitutions().len() != source.substitution_count() as usize {
                    return Err(err());
                }
                let mut owned = FeatureTableSubstitution::default();
                for substitution in source.substitutions() {
                    let feature_index = substitution.feature_index();
                    let tag = features
                        .feature_records()
                        .get(feature_index as usize)
                        .ok_or_else(err)?
                        .feature_tag();
                    let alternate = substitution
                        .alternate_feature_offset()
                        .resolve_with_args::<read_fonts::tables::layout::Feature>(
                            source.offset_data(),
                            tag,
                        )
                        .map_err(|_| err())?;
                    alternate.feature_params().transpose().map_err(|_| err())?;
                    let alternate: Feature = alternate.to_owned_table();
                    owned.substitutions.push(FeatureTableSubstitutionRecord {
                        feature_index,
                        alternate_feature: alternate.into(),
                    });
                }
                Some(owned)
            } else {
                None
            };
            records.push(FeatureVariationRecord {
                condition_set: condition_set.into(),
                feature_table_substitution: feature_table_substitution.into(),
            });
        }
        return Ok(Some(FeatureVariations {
            feature_variation_records: records,
            lookup_variation_records: own_lookup_variations(&vars, features, tag, &mut remaining)?,
        }));
    }
    Ok(None)
}

pub(super) fn remap_feature_conditions(
    variations: &mut NullableOffsetMarker<FeatureVariations, 4>,
    map: &optimize::IndexMap,
) {
    fn remap(condition: &mut Condition, map: &optimize::IndexMap) {
        match condition {
            Condition::Format2VariableValue(v) => {
                v.var_index = map.get(&v.var_index).copied().unwrap_or(u32::MAX);
            }
            Condition::Format3And(v) => {
                for condition in &mut v.conditions {
                    remap(condition, map);
                }
            }
            Condition::Format4Or(v) => {
                for condition in &mut v.conditions {
                    remap(condition, map);
                }
            }
            Condition::Format5Negate(v) => remap(&mut v.condition, map),
            _ => {}
        }
    }
    if let Some(variations) = variations.as_mut() {
        for variation in variations.lookup_variation_records.iter_mut().flatten() {
            for record in &mut variation.feature_lookups.lookup_condition_records {
                if let Some(condition) = record.condition.as_mut() {
                    remap(condition, map);
                }
            }
        }
        for record in &mut variations.feature_variation_records {
            if let Some(set) = record.condition_set.as_mut() {
                for condition in &mut set.conditions {
                    remap(condition, map);
                }
            }
        }
    }
}

/// Own lookup variations with the same tree and expansion bounds as feature
/// substitutions, including null conditions (which mean true).
fn own_lookup_variations(
    vars: &read_fonts::tables::layout::FeatureVariations,
    features: &read_fonts::tables::layout::FeatureList,
    tag: Tag,
    remaining: &mut usize,
) -> Result<Option<Vec<LookupVariationRecord>>, SubsetError> {
    let err = || SubsetError::SubsetTableError(tag);
    if vars.version().major != 1 || vars.version().minor < 1 {
        return Ok(None);
    }
    let records = vars.lookup_variation_records().ok_or_else(err)?;
    if !records
        .windows(2)
        .all(|pair| pair[0].feature_index() < pair[1].feature_index())
    {
        return Err(err());
    }
    *remaining = remaining.checked_sub(records.len()).ok_or_else(err)?;
    let mut owned = Vec::new();
    for record in records {
        if usize::from(record.feature_index()) >= features.feature_records().len() {
            return Err(err());
        }
        let source = record
            .feature_lookups(vars.offset_data())
            .map_err(|_| err())?;
        let count = usize::try_from(source.lookup_condition_count()).map_err(|_| err())?;
        // Check before calling the generated array getter on 32-bit targets.
        let end = count
            .checked_mul(8)
            .and_then(|n| n.checked_add(10))
            .ok_or_else(err)?;
        if source.version().major != 1 || end > source.offset_data().len() {
            return Err(err());
        }
        *remaining = remaining.checked_sub(count).ok_or_else(err)?;
        let mut conditions = Vec::new();
        for record in source.lookup_condition_records() {
            let offset = record.condition_offset().offset().to_u32();
            let condition = if offset == 0 {
                None
            } else {
                Some(
                    crate::conditions::at_offset(source.offset_data(), offset, 33, remaining)
                        .map_err(|_| err())?,
                )
            };
            let list = record
                .lookup_index_list(source.offset_data())
                .map_err(|_| err())?;
            let indices = list.lookup_indices();
            if indices.len() != usize::from(list.lookup_index_count()) {
                return Err(err());
            }
            *remaining = remaining.checked_sub(indices.len()).ok_or_else(err)?;
            conditions.push(LookupConditionRecord {
                condition: condition.into(),
                lookup_index_list: LookupIndexList::new(indices.iter().map(|v| v.get()).collect())
                    .into(),
            });
        }
        owned.push(LookupVariationRecord {
            feature_index: record.feature_index(),
            feature_lookups: FeatureLookups::new(source.flags(), conditions).into(),
        });
    }
    Ok(Some(owned))
}
