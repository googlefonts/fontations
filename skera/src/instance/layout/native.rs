//! Layout metadata and stores planned once, with borrowed lookup serialization.
use super::{
    features::{self, PartialCondition},
    ConditionRows, LayoutContext,
};
use crate::{
    instance::{optimize, AxisPlan},
    offset::{SerializeCopy, SerializeSubset},
    serialize::{SerializeErrorFlags, Serializer},
    FastHashMap, Plan, SubsetError, SubsetTable,
};
use read_fonts::{
    tables::layout::{Feature, FeatureList, FeatureVariations},
    types::{Offset16, Offset24, Offset32, Tag},
    FontRef, MinByteRange, TableProvider,
};
use std::collections::BTreeMap;
use write_fonts::tables::layout::Condition;

#[derive(Default)]
pub(crate) struct NativeLayoutPlan {
    pub features: BTreeMap<Tag, FeaturePlan>,
    pub stores: BTreeMap<Tag, Vec<u8>>,
    rows: ConditionRows,
}
pub(crate) struct FeaturePlan {
    pub list: Vec<u8>,
    pub variations: Option<Vec<u8>>,
}
fn error(tag: Tag) -> SubsetError {
    SubsetError::SubsetTableError(tag)
}
fn read_error(_: read_fonts::ReadError) -> SerializeErrorFlags {
    SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR
}

impl NativeLayoutPlan {
    pub fn new(font: &FontRef, axes: &AxisPlan) -> Result<Self, SubsetError> {
        let gdef = font.gdef().ok();
        let context = LayoutContext::new(
            gdef.and_then(|v| v.item_var_store())
                .transpose()
                .map_err(|_| error(Tag::new(b"GDEF")))?,
            axes,
        );
        let mut result = Self::default();
        if axes.coupled && axes.values.is_empty() {
            return Ok(result);
        }
        macro_rules! features {
            ($getter:ident, $tag:literal) => {
                if let Ok(table) = font.$getter() {
                    let tag = Tag::new($tag);
                    if let Some(variations) = table
                        .feature_variations()
                        .transpose()
                        .map_err(|_| error(tag))?
                    {
                        let list = table.feature_list().map_err(|_| error(tag))?;
                        result.features.insert(
                            tag,
                            FeaturePlan::new(&list, Some(&variations), &context)
                                .map_err(|_| error(tag))?,
                        );
                    }
                }
            };
        }
        features!(gsub, b"GSUB");
        features!(gpos, b"GPOS");
        result.rows = context.condition_rows.into_inner();
        Ok(result)
    }
    pub fn finish(
        &mut self,
        font: &FontRef,
        axes: &AxisPlan,
        plan: &mut Plan,
    ) -> Result<(), SubsetError> {
        let gdef = font.gdef().ok();
        let mut context = LayoutContext::new(
            gdef.and_then(|v| v.item_var_store())
                .transpose()
                .map_err(|_| error(Tag::new(b"GDEF")))?,
            axes,
        );
        context.condition_rows = std::cell::RefCell::new(std::mem::take(&mut self.rows));
        if let Some(bytes) = finish_store(
            &context,
            &mut plan.layout_varidx_delta_map,
            plan.instance_only,
        )? {
            self.stores.insert(Tag::new(b"GDEF"), bytes);
        }
        if let Ok(base) = font.base() {
            let context = LayoutContext::new(
                base.item_var_store()
                    .transpose()
                    .map_err(|_| error(Tag::new(b"BASE")))?,
                axes,
            );
            if let Some(bytes) = finish_store(
                &context,
                &mut plan.base_varidx_delta_map,
                plan.instance_only,
            )? {
                self.stores.insert(Tag::new(b"BASE"), bytes);
            }
        }
        Ok(())
    }
}

fn finish_store(
    context: &LayoutContext,
    values: &mut FastHashMap<u32, (u32, i32)>,
    instance_only: bool,
) -> Result<Option<Vec<u8>>, SubsetError> {
    if instance_only && context.axes.coupled && context.axes.values.is_empty() && !values.is_empty()
    {
        for (&index, value) in values.iter_mut() {
            *value = (index, 0);
        }
        let Some(store) = context.store.as_ref() else {
            return Ok(None);
        };
        let table = context.store_plan()?.unwrap().rebuild(store)?;
        return super::super::serialize::item_store(&table).map(Some);
    }
    let mut indices = Vec::new();
    for (&index, value) in values.iter_mut() {
        value.1 = context.delta((index >> 16) as u16, index as u16)?;
        if context.axes.all_pinned() || context.index_is_constant(index)? {
            value.0 = u32::MAX;
        } else {
            indices.push(index);
        }
    }
    if context.axes.all_pinned() {
        return Ok(None);
    }
    let Some(store) = context.store.as_ref() else {
        return Ok(None);
    };
    indices.sort_unstable();
    let transform = context.store_plan()?.unwrap();
    let (mut rebuilt, mut mapping) = transform.rebuild_selected(store, &indices)?;
    // Conditions may need private rows to preserve signs independently of
    // positioning's integer rounding. Their temporary indices follow the source.
    let rows = context.condition_rows.borrow();
    super::append_condition_rows(&mut rebuilt, &rows)?;
    let source_count = store.item_variation_data_count() as u32;
    let destination_count = rebuilt.item_variation_data.len() - rows.rows.len();
    for i in 0..rows.rows.len() {
        mapping.insert(
            (source_count + i as u32) << 16,
            ((destination_count + i) as u32) << 16,
        );
    }
    let (rebuilt, optimized) = optimize::optimize(rebuilt)?;
    for (&old, &new) in &mapping {
        let new = optimized.get(&new).copied().unwrap_or(u32::MAX);
        values.entry(old).or_default().0 = new;
    }
    if rebuilt.item_variation_data.is_empty() {
        return Ok(None);
    }
    super::super::serialize::item_store(&rebuilt).map(Some)
}

impl FeaturePlan {
    fn new(
        list: &FeatureList,
        vars: Option<&FeatureVariations>,
        context: &LayoutContext,
    ) -> Result<Self, SerializeErrorFlags> {
        let mut size = list.min_table_bytes().len().saturating_add(64);
        loop {
            match Self::build(list, vars, context, size) {
                Err(error) if error == SerializeErrorFlags::SERIALIZE_ERROR_OUT_OF_ROOM => {
                    size = size
                        .checked_mul(2)
                        .ok_or(SerializeErrorFlags::SERIALIZE_ERROR_INT_OVERFLOW)?;
                }
                result => return result,
            }
        }
    }
    fn build(
        list: &FeatureList,
        vars: Option<&FeatureVariations>,
        context: &LayoutContext,
        size: usize,
    ) -> Result<Self, SerializeErrorFlags> {
        let mut selected: BTreeMap<u16, Feature<'_>> = BTreeMap::new();
        let mut lookups: BTreeMap<u16, Vec<u16>> = BTreeMap::new();
        if context.axes.all_pinned() {
            if let Some(vars) = vars {
                validate_variations(vars)?;
                for record in vars.feature_variation_records() {
                    let set = record
                        .condition_set(vars.offset_data())
                        .transpose()
                        .map_err(read_error)?;
                    let mut matches = true;
                    if let Some(set) = set {
                        if set.conditions().len() != usize::from(set.condition_count()) {
                            return Err(read_error(read_fonts::ReadError::OutOfBounds));
                        }
                        for offset in set.condition_offsets() {
                            let condition = crate::conditions::at_offset(
                                set.offset_data(),
                                offset.get().to_u32(),
                                33,
                                &mut 200_000,
                            )
                            .map_err(read_error)?;
                            matches &= features::condition(&condition, context, 0)
                                .map_err(|_| SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)?;
                        }
                    }
                    if !matches {
                        continue;
                    }
                    if let Some(subs) = record
                        .feature_table_substitution(vars.offset_data())
                        .transpose()
                        .map_err(read_error)?
                    {
                        for sub in subs.substitutions() {
                            let tag = list
                                .feature_records()
                                .get(usize::from(sub.feature_index()))
                                .ok_or(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)?
                                .feature_tag();
                            selected.insert(
                                sub.feature_index(),
                                sub.alternate_feature_with_tag(subs.offset_data(), tag)
                                    .map_err(read_error)?,
                            );
                        }
                    }
                    break;
                }
                for variation in vars.lookup_variation_records().unwrap_or_default() {
                    let i = variation.feature_index();
                    let record = list
                        .feature_records()
                        .get(usize::from(i))
                        .ok_or(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)?;
                    let feature = selected
                        .get(&i)
                        .cloned()
                        .map(Ok)
                        .unwrap_or_else(|| record.feature(list.offset_data()))
                        .map_err(read_error)?;
                    let table = variation
                        .feature_lookups(vars.offset_data())
                        .map_err(read_error)?;
                    let mut indices = if table.flags().contains(
                        read_fonts::tables::layout::FeatureLookupsFlags::ADD_DEFAULT_LOOKUPS,
                    ) {
                        feature
                            .lookup_list_indices()
                            .iter()
                            .map(|v| v.get())
                            .collect()
                    } else {
                        Vec::new()
                    };
                    for record in table.lookup_condition_records() {
                        let condition = crate::conditions::at_offset(
                            table.offset_data(),
                            record.condition_offset().offset().to_u32(),
                            33,
                            &mut 200_000,
                        )
                        .map_err(read_error)?;
                        if features::condition(&condition, context, 0)
                            .map_err(|_| SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)?
                        {
                            indices.extend(
                                record
                                    .lookup_index_list(table.offset_data())
                                    .map_err(read_error)?
                                    .lookup_indices()
                                    .iter()
                                    .map(|v| v.get()),
                            );
                        }
                    }
                    indices.sort_unstable();
                    indices.dedup();
                    lookups.insert(i, indices);
                }
            }
        }
        let mut s = Serializer::new(size)?;
        s.start_serialize()?;
        s.embed(list.feature_count())?;
        for (i, record) in list.feature_records().iter().enumerate() {
            let feature = selected
                .get(&(i as u16))
                .cloned()
                .map(Ok)
                .unwrap_or_else(|| record.feature(list.offset_data()))
                .map_err(read_error)?;
            s.embed(record.feature_tag())?;
            let pos = s.embed(0u16)?;
            s.push()?;
            let params_pos = s.embed(0u16)?;
            let indices = lookups.get(&(i as u16));
            s.embed(indices.map_or(feature.lookup_index_count(), |v| v.len() as u16))?;
            if let Some(indices) = indices {
                for &index in indices {
                    s.embed(index)?;
                }
            } else {
                for index in feature.lookup_list_indices() {
                    s.embed(index.get())?;
                }
            }
            if let Some(params) = feature.feature_params().transpose().map_err(read_error)? {
                Offset16::serialize_subset(&params, &mut s, &Plan::default(), (), params_pos)?;
            }
            let obj = s.pop_pack(true).ok_or(s.error())?;
            s.add_link(
                pos..pos + 2,
                obj,
                crate::serialize::OffsetWhence::Head,
                0,
                false,
            )?;
        }
        s.end_serialize();
        let bytes = finish_metadata(s)?;
        let variations = if context.axes.all_pinned() {
            None
        } else {
            vars.map(|v| partial_variations(v, list, context))
                .transpose()?
                .flatten()
        };
        Ok(Self {
            list: bytes,
            variations,
        })
    }
}

fn validate_variations(vars: &FeatureVariations) -> Result<(), SerializeErrorFlags> {
    if vars.version().major != 1
        || vars.version().minor > 1
        || u64::from(vars.feature_variation_record_count()) * 8
            > vars.offset_data().len().saturating_sub(8) as u64
    {
        return Err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR);
    }
    Ok(())
}

fn partial_variations(
    vars: &FeatureVariations,
    features: &FeatureList,
    context: &LayoutContext,
) -> Result<Option<Vec<u8>>, SerializeErrorFlags> {
    let mut size = vars.min_table_bytes().len().saturating_add(64);
    loop {
        match build_partial_variations(vars, features, context, size) {
            Err(error) if error == SerializeErrorFlags::SERIALIZE_ERROR_OUT_OF_ROOM => {
                size = size
                    .checked_mul(2)
                    .ok_or(SerializeErrorFlags::SERIALIZE_ERROR_INT_OVERFLOW)?;
            }
            result => return result,
        }
    }
}
fn build_partial_variations(
    vars: &FeatureVariations,
    features: &FeatureList,
    context: &LayoutContext,
    size: usize,
) -> Result<Option<Vec<u8>>, SerializeErrorFlags> {
    validate_variations(vars)?;
    let mut records = Vec::new();
    for (i, record) in vars.feature_variation_records().iter().enumerate() {
        let mut conditions = Vec::new();
        let mut possible = true;
        if let Some(set) = record
            .condition_set(vars.offset_data())
            .transpose()
            .map_err(read_error)?
        {
            if set.condition_offsets().len() != usize::from(set.condition_count()) {
                return Err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR);
            }
            for offset in set.condition_offsets() {
                let condition = crate::conditions::at_offset(
                    set.offset_data(),
                    offset.get().to_u32(),
                    33,
                    &mut 200_000,
                )
                .map_err(read_error)?;
                match features::partial_condition(&condition, context, 0)
                    .map_err(|_| SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)?
                {
                    PartialCondition::Constant(false) => {
                        possible = false;
                        break;
                    }
                    PartialCondition::Constant(true) => {}
                    PartialCondition::Variable(c) => conditions.push(c),
                }
            }
        }
        if possible {
            let unconditional = conditions.is_empty();
            records.push((i, conditions));
            if unconditional {
                break;
            }
        }
    }
    // This metadata graph contains no glyph lookup or paint graph. Serialize
    // original substitutions and parameters through the regular subset code.
    let mut s = Serializer::new(size)?;
    s.start_serialize()?;
    s.embed(vars.version())?;
    s.embed(records.len() as u32)?;
    for (i, conditions) in &records {
        let record = &vars.feature_variation_records()[*i];
        let pos = s.embed(0u32)?;
        s.push()?;
        s.embed(conditions.len() as u16)?;
        for condition in conditions {
            let pos = s.embed(0u32)?;
            Offset32::serialize_subset(condition, &mut s, &Plan::default(), (), pos)?;
        }
        let obj = s.pop_pack(true).ok_or(s.error())?;
        s.add_link(
            pos..pos + 4,
            obj,
            crate::serialize::OffsetWhence::Head,
            0,
            false,
        )?;
        let pos = s.embed(0u32)?;
        if let Some(subs) = record
            .feature_table_substitution(vars.offset_data())
            .transpose()
            .map_err(read_error)?
        {
            // The caller will resolve params with the source feature tag. Raw
            // Feature records have only a parameter offset and lookup indices.
            s.push()?;
            s.embed(subs.version())?;
            s.embed(subs.substitution_count())?;
            for sub in subs.substitutions() {
                s.embed(sub.feature_index())?;
                let pos = s.embed(0u32)?;
                let tag = features
                    .feature_records()
                    .get(usize::from(sub.feature_index()))
                    .ok_or(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)?
                    .feature_tag();
                let feature = sub
                    .alternate_feature_with_tag(subs.offset_data(), tag)
                    .map_err(read_error)?;
                s.push()?;
                let params_pos = s.embed(0u16)?;
                s.embed(feature.lookup_index_count())?;
                for index in feature.lookup_list_indices() {
                    s.embed(index.get())?;
                }
                if let Some(params) = feature.feature_params().transpose().map_err(read_error)? {
                    Offset16::serialize_subset(&params, &mut s, &Plan::default(), (), params_pos)?;
                }
                let obj = s.pop_pack(true).ok_or(s.error())?;
                s.add_link(
                    pos..pos + 4,
                    obj,
                    crate::serialize::OffsetWhence::Head,
                    0,
                    false,
                )?;
            }
            let obj = s.pop_pack(true).ok_or(s.error())?;
            s.add_link(
                pos..pos + 4,
                obj,
                crate::serialize::OffsetWhence::Head,
                0,
                false,
            )?;
        }
    }
    if vars.version().minor == 1 {
        let variations = vars
            .lookup_variation_records()
            .ok_or(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)?;
        s.embed(variations.len() as u32)?;
        for variation in variations {
            s.embed(variation.feature_index())?;
            let pos = s.embed(0u32)?;
            let table = variation
                .feature_lookups(vars.offset_data())
                .map_err(read_error)?;
            let mut records = Vec::new();
            for record in table.lookup_condition_records() {
                let condition = crate::conditions::at_offset(
                    table.offset_data(),
                    record.condition_offset().offset().to_u32(),
                    33,
                    &mut 200_000,
                )
                .map_err(read_error)?;
                let condition = features::partial_condition(&condition, context, 0)
                    .map_err(|_| SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)?;
                if !matches!(condition, PartialCondition::Constant(false)) {
                    records.push((record, condition));
                }
            }
            s.push()?;
            s.embed(table.version())?;
            s.embed(table.flags())?;
            s.embed(records.len() as u32)?;
            for (record, condition) in records {
                let pos = s.embed(0u32)?;
                if let PartialCondition::Variable(condition) = condition {
                    Offset32::serialize_subset(&condition, &mut s, &Plan::default(), (), pos)?;
                }
                let pos = s.embed(0u32)?;
                let list = record
                    .lookup_index_list(table.offset_data())
                    .map_err(read_error)?;
                Offset32::serialize_copy_from_bytes(list.min_table_bytes(), &mut s, pos)?;
            }
            let obj = s.pop_pack(true).ok_or(s.error())?;
            s.add_link(
                pos..pos + 4,
                obj,
                crate::serialize::OffsetWhence::Head,
                0,
                false,
            )?;
        }
    }
    s.end_serialize();
    finish_metadata(s).map(Some)
}

impl SubsetTable<'_> for Condition {
    type ArgsForSubset = ();
    type Output = ();
    fn subset(&self, plan: &Plan, s: &mut Serializer, _: ()) -> Result<(), SerializeErrorFlags> {
        match self {
            Condition::Format1AxisRange(v) => {
                s.embed(1u16)?;
                s.embed(v.axis_index)?;
                s.embed(v.filter_range_min_value)?;
                s.embed(v.filter_range_max_value)?;
            }
            Condition::Format2VariableValue(v) => {
                s.embed(2u16)?;
                s.embed(v.default_value)?;
                s.embed(v.var_index)?;
            }
            Condition::Format3And(v) => {
                s.embed(3u16)?;
                s.embed(v.condition_count)?;
                for c in &v.conditions {
                    let pos = s.allocate_size(3, true)?;
                    Offset24::serialize_subset(c.as_ref(), s, plan, (), pos)?;
                }
            }
            Condition::Format4Or(v) => {
                s.embed(4u16)?;
                s.embed(v.condition_count)?;
                for c in &v.conditions {
                    let pos = s.allocate_size(3, true)?;
                    Offset24::serialize_subset(c.as_ref(), s, plan, (), pos)?;
                }
            }
            Condition::Format5Negate(v) => {
                s.embed(5u16)?;
                let pos = s.allocate_size(3, true)?;
                Offset24::serialize_subset(v.condition.as_ref(), s, plan, (), pos)?;
            }
        }
        Ok(())
    }
}

fn finish_metadata(s: Serializer) -> Result<Vec<u8>, SerializeErrorFlags> {
    if s.only_offset_overflow() {
        crate::repack::resolve_overflows(s, Tag::new(b"meta"), 32)
            .map_err(|_| SerializeErrorFlags::SERIALIZE_ERROR_OFFSET_OVERFLOW)
    } else if s.in_error() {
        Err(s.error())
    } else {
        Ok(s.copy_bytes())
    }
}
