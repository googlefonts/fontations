//! Apply variation deltas while walking owned layout tables.
use super::{rebase::renormalize, AxisPlan, StorePlan};
use crate::SubsetError;
use std::cell::RefCell;
use std::collections::BTreeMap;
use write_fonts::{
    from_obj::ToOwnedTable,
    read::{
        tables::variations::{DeltaSetIndex, ItemVariationStore},
        FontRef, TableProvider,
    },
    tables::{base::*, gdef::*, gpos::*, layout::*, math::*},
    types::{F2Dot14, Tag},
    NullableOffsetMarker, OffsetMarker,
};

struct Context<'a> {
    store: Option<ItemVariationStore<'a>>,
    coords: &'a [F2Dot14],
    axes: &'a AxisPlan,
    condition_biases: RefCell<Vec<(DeltaSetIndex, i64)>>,
}
impl Context<'_> {
    fn wide_delta(&self, outer: u16, inner: u16) -> Result<i64, SubsetError> {
        if (DeltaSetIndex { outer, inner }) == DeltaSetIndex::NO_VARIATION_INDEX {
            return Ok(0);
        }
        self.store
            .as_ref()
            .and_then(|s| s.compute_delta(DeltaSetIndex { outer, inner }, self.coords))
            .map(|v| v.to_f64().round() as i64)
            .ok_or(SubsetError::SubsetTableError(Tag::new(b"GDEF")))
    }
    fn delta(&self, outer: u16, inner: u16) -> Result<i32, SubsetError> {
        Ok(self
            .wide_delta(outer, inner)?
            .clamp(i32::MIN as i64, i32::MAX as i64) as i32)
    }
    fn device(&self, d: &DeviceOrVariationIndex) -> Result<Option<i32>, SubsetError> {
        match d {
            DeviceOrVariationIndex::VariationIndex(v) => self
                .delta(v.delta_set_outer_index, v.delta_set_inner_index)
                .map(Some),
            DeviceOrVariationIndex::Device(_) => Ok(None),
            _ => Err(SubsetError::SubsetTableError(Tag::new(b"GDEF"))),
        }
    }
    fn apply(
        &self,
        value: &mut i16,
        device: &mut NullableOffsetMarker<DeviceOrVariationIndex>,
    ) -> Result<(), SubsetError> {
        if let Some(delta) = device
            .as_ref()
            .map(|d| self.device(d))
            .transpose()?
            .flatten()
        {
            *value = add(*value, delta);
            if self.axes.all_pinned() {
                *device = Default::default();
            }
        }
        Ok(())
    }
}
fn add(value: i16, delta: i32) -> i16 {
    (value as i32)
        .saturating_add(delta)
        .clamp(i16::MIN as i32, i16::MAX as i32) as i16
}

trait Apply {
    fn apply(&mut self, c: &Context) -> Result<(), SubsetError>;
}
impl<T: Apply> Apply for Vec<T> {
    fn apply(&mut self, c: &Context) -> Result<(), SubsetError> {
        for v in self {
            v.apply(c)?;
        }
        Ok(())
    }
}
impl<T: Apply, const N: usize> Apply for OffsetMarker<T, N> {
    fn apply(&mut self, c: &Context) -> Result<(), SubsetError> {
        self.as_mut().apply(c)
    }
}
impl<T: Apply, const N: usize> Apply for NullableOffsetMarker<T, N> {
    fn apply(&mut self, c: &Context) -> Result<(), SubsetError> {
        if let Some(v) = self.as_mut() {
            v.apply(c)?;
        }
        Ok(())
    }
}
macro_rules! fields {($t:ty;$($field:ident),+)=>{impl Apply for $t {fn apply(&mut self,c:&Context)->Result<(),SubsetError>{$ (self.$field.apply(c)?;)+ Ok(())}}};}
impl<T: Apply> Apply for Lookup<T> {
    fn apply(&mut self, c: &Context) -> Result<(), SubsetError> {
        self.subtables.apply(c)
    }
}
impl<T: Apply> Apply for LookupList<T> {
    fn apply(&mut self, c: &Context) -> Result<(), SubsetError> {
        self.lookups.apply(c)
    }
}
impl<T: Apply> Apply for ExtensionPosFormat1<T> {
    fn apply(&mut self, c: &Context) -> Result<(), SubsetError> {
        self.extension.apply(c)
    }
}
macro_rules! variants {($t:ty;$($variant:ident),+; $($ignore:ident),*)=>{impl Apply for $t{fn apply(&mut self,c:&Context)->Result<(),SubsetError>{match self {$ (Self::$variant(v)=>v.apply(c),)+ $(Self::$ignore(_)=>Ok(()),)*}}}};}
variants!(PositionLookup;Single,Pair,Cursive,MarkToBase,MarkToLig,MarkToMark,Extension;Contextual,ChainContextual);
variants!(ExtensionSubtable;Single,Pair,Cursive,MarkToBase,MarkToLig,MarkToMark;Contextual,ChainContextual);
variants!(SinglePos;Format1,Format2;);
variants!(PairPos;Format1,Format2;);
fields!(SinglePosFormat1;value_record);
fields!(SinglePosFormat2;value_records);
fields!(PairPosFormat1;pair_sets);
fields!(PairSet;pair_value_records);
fields!(PairValueRecord;value_record1,value_record2);
fields!(PairPosFormat2;class1_records);
fields!(Class1Record;class2_records);
fields!(Class2Record;value_record1,value_record2);
fields!(CursivePosFormat1;entry_exit_record);
fields!(EntryExitRecord;entry_anchor,exit_anchor);
fields!(MarkBasePosFormat1;mark_array,base_array);
fields!(MarkArray;mark_records);
fields!(MarkRecord;mark_anchor);
fields!(BaseArray;base_records);
fields!(BaseRecord;base_anchors);
fields!(MarkLigPosFormat1;mark_array,ligature_array);
fields!(LigatureArray;ligature_attaches);
fields!(LigatureAttach;component_records);
fields!(ComponentRecord;ligature_anchors);
fields!(MarkMarkPosFormat1;mark1_array,mark2_array);
fields!(Mark2Array;mark2_records);
fields!(Mark2Record;mark2_anchors);
fields!(LigCaretList;lig_glyphs);
fields!(LigGlyph;caret_values);
fields!(Axis;base_script_list);
fields!(BaseScriptList;base_script_records);
fields!(BaseScriptRecord;base_script);
fields!(BaseScript;base_values,default_min_max,base_lang_sys_records);
fields!(BaseValues;base_coords);
fields!(BaseLangSysRecord;min_max);
fields!(MinMax;min_coord,max_coord,feat_min_max_records);
fields!(FeatMinMaxRecord;min_coord,max_coord);
fields!(Math;math_constants,math_glyph_info,math_variants);
fields!(MathConstants;math_leading,axis_height,accent_base_height,flattened_accent_base_height,
    subscript_shift_down,subscript_top_max,subscript_baseline_drop_min,superscript_shift_up,
    superscript_shift_up_cramped,superscript_bottom_min,superscript_baseline_drop_max,
    sub_superscript_gap_min,superscript_bottom_max_with_subscript,space_after_script,
    upper_limit_gap_min,upper_limit_baseline_rise_min,lower_limit_gap_min,lower_limit_baseline_drop_min,
    stack_top_shift_up,stack_top_display_style_shift_up,stack_bottom_shift_down,stack_bottom_display_style_shift_down,
    stack_gap_min,stack_display_style_gap_min,stretch_stack_top_shift_up,stretch_stack_bottom_shift_down,
    stretch_stack_gap_above_min,stretch_stack_gap_below_min,fraction_numerator_shift_up,
    fraction_numerator_display_style_shift_up,fraction_denominator_shift_down,fraction_denominator_display_style_shift_down,
    fraction_numerator_gap_min,fraction_num_display_style_gap_min,fraction_rule_thickness,
    fraction_denominator_gap_min,fraction_denom_display_style_gap_min,skewed_fraction_horizontal_gap,
    skewed_fraction_vertical_gap,overbar_vertical_gap,overbar_rule_thickness,overbar_extra_ascender,
    underbar_vertical_gap,underbar_rule_thickness,underbar_extra_descender,radical_vertical_gap,
    radical_display_style_vertical_gap,radical_rule_thickness,radical_extra_ascender,
    radical_kern_before_degree,radical_kern_after_degree);
fields!(MathGlyphInfo;math_italics_correction_info,math_top_accent_attachment,math_kern_info);
fields!(MathItalicsCorrectionInfo;italics_correction);
fields!(MathTopAccentAttachment;top_accent_attachment);
fields!(MathKernInfo;math_kern_info_records);
fields!(MathKernInfoRecord;top_right_math_kern,top_left_math_kern,bottom_right_math_kern,bottom_left_math_kern);
fields!(MathKern;correction_height,kern_values);
fields!(MathVariants;vert_glyph_constructions,horiz_glyph_constructions);
fields!(MathGlyphConstruction;glyph_assembly);
fields!(GlyphAssembly;italics_correction);

impl Apply for MathValueRecord {
    fn apply(&mut self, c: &Context) -> Result<(), SubsetError> {
        let mut value = self.value.to_i16();
        c.apply(&mut value, &mut self.device)?;
        self.value = value.into();
        Ok(())
    }
}

impl Apply for ValueRecord {
    fn apply(&mut self, c: &Context) -> Result<(), SubsetError> {
        let fmt = self.format();
        for (value, device) in [
            (&mut self.x_placement, &mut self.x_placement_device),
            (&mut self.y_placement, &mut self.y_placement_device),
            (&mut self.x_advance, &mut self.x_advance_device),
            (&mut self.y_advance, &mut self.y_advance_device),
        ] {
            if let Some(delta) = device.as_ref().map(|d| c.device(d)).transpose()?.flatten() {
                *value = Some(add(value.unwrap_or(0), delta));
                if c.axes.all_pinned() {
                    *device = Default::default();
                }
            }
        }
        // All records in a subtable must retain the same serialized format,
        // including records whose device offsets were already null.
        self.set_explicit_value_format(fmt | ValueFormat::from_bits_truncate(fmt.bits() >> 4));
        Ok(())
    }
}
impl Apply for AnchorTable {
    fn apply(&mut self, c: &Context) -> Result<(), SubsetError> {
        if let Self::Format3(a) = self {
            c.apply(&mut a.x_coordinate, &mut a.x_device)?;
            c.apply(&mut a.y_coordinate, &mut a.y_device)?;
            if a.x_device.is_none() && a.y_device.is_none() {
                *self = Self::Format1(AnchorFormat1 {
                    x_coordinate: a.x_coordinate,
                    y_coordinate: a.y_coordinate,
                });
            }
        }
        Ok(())
    }
}
impl Apply for CaretValue {
    fn apply(&mut self, c: &Context) -> Result<(), SubsetError> {
        if let Self::Format3(a) = self {
            if let Some(delta) = c.device(&a.device)? {
                a.coordinate = add(a.coordinate, delta);
                if c.axes.all_pinned() {
                    *self = Self::Format1(CaretValueFormat1 {
                        coordinate: a.coordinate,
                    });
                }
            }
        }
        Ok(())
    }
}
impl Apply for BaseCoord {
    fn apply(&mut self, c: &Context) -> Result<(), SubsetError> {
        if let Self::Format3(a) = self {
            c.apply(&mut a.coordinate, &mut a.device)?;
            if a.device.is_none() {
                *self = Self::Format1(BaseCoordFormat1 {
                    coordinate: a.coordinate,
                });
            }
        }
        Ok(())
    }
}

fn condition(cond: &Condition, c: &Context, depth: usize) -> Result<bool, SubsetError> {
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
            v.default_value as i64 + c.wide_delta((v.var_index >> 16) as u16, v.var_index as u16)?
                > 0
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
fn features(
    list: &mut FeatureList,
    vars: &mut NullableOffsetMarker<FeatureVariations, 4>,
    c: &Context,
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
    *vars = Default::default();
    Ok(())
}
enum PartialCondition {
    Constant(bool),
    Variable(Condition),
}
fn partial_condition(
    cond: &Condition,
    c: &Context,
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
            let mut v = v.clone();
            let value = v.default_value as i64
                + c.wide_delta((v.var_index >> 16) as u16, v.var_index as u16)?;
            if let Ok(default) = i16::try_from(value) {
                v.default_value = default;
            } else {
                // Preserve the sign threshold with an added store row when
                // its folded default cannot fit the condition's i16 field.
                let mut biases = c.condition_biases.borrow_mut();
                let index = DeltaSetIndex {
                    outer: (v.var_index >> 16) as u16,
                    inner: v.var_index as u16,
                };
                let slot = biases
                    .iter()
                    .position(|b| *b == (index, value))
                    .unwrap_or(biases.len());
                let outer = c
                    .store
                    .as_ref()
                    .ok_or(SubsetError::SubsetTableError(Tag::new(b"GDEF")))?
                    .item_variation_data_count() as usize
                    + slot;
                if outer >= u16::MAX as usize {
                    return Err(SubsetError::SubsetTableError(Tag::new(b"GDEF")));
                }
                if slot == biases.len() {
                    biases.push((index, value));
                }
                v.var_index = (outer as u32) << 16;
                v.default_value = 0;
            }
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

fn validate_condition(
    cond: write_fonts::read::tables::layout::Condition,
    depth: usize,
    remaining: &mut usize,
) -> Result<(), SubsetError> {
    crate::layout::walk_conditions(&cond, depth, remaining, &mut |_| {})
        .map_err(|_| SubsetError::SubsetTableError(Tag::new(b"GSUB")))
}

fn validate_features(
    vars: Option<write_fonts::read::tables::layout::FeatureVariations>,
) -> Result<(), SubsetError> {
    let err = || SubsetError::SubsetTableError(Tag::new(b"GSUB"));
    let mut remaining = 200_000;
    if let Some(vars) = vars {
        for record in vars.feature_variation_records() {
            if let Some(set) = record
                .condition_set(vars.offset_data())
                .transpose()
                .map_err(|_| err())?
            {
                for condition in set.conditions().iter() {
                    validate_condition(condition.map_err(|_| err())?, 0, &mut remaining)?;
                }
            }
        }
    }
    Ok(())
}

fn rebuild_store(
    c: &Context,
) -> Result<Option<write_fonts::tables::variations::ItemVariationStore>, SubsetError> {
    use write_fonts::tables::variations::{RegionAxisCoordinates, VariationRegion};
    let err = || SubsetError::SubsetTableError(Tag::new(b"GDEF"));
    let Some(store) = &c.store else {
        return Ok(None);
    };
    let plan = StorePlan::new(store, c.axes)?;
    let mut result = plan.rebuild(store)?;
    if c.condition_biases.borrow().is_empty() {
        return Ok(Some(result));
    }
    let regions = result.variation_region_list.as_mut();
    let neutral = if let Some(i) = regions
        .variation_regions
        .iter()
        .position(|r| r.region_axes.iter().all(|a| a.peak_coord == F2Dot14::ZERO))
    {
        i
    } else {
        let index = regions.variation_regions.len();
        if index >= u16::MAX as usize {
            return Err(err());
        }
        regions.variation_regions.push(VariationRegion {
            region_axes: vec![
                RegionAxisCoordinates::new(
                    F2Dot14::ZERO,
                    F2Dot14::ZERO,
                    F2Dot14::ZERO
                );
                regions.axis_count as usize
            ],
        });
        index
    } as u16;
    for &(index, bias) in c.condition_biases.borrow().iter() {
        let data = store
            .item_variation_data()
            .get(index.outer as usize)
            .ok_or_else(err)?
            .map_err(|_| err())?;
        let transform = &plan.transforms[index.outer as usize];
        let values: Vec<_> = data.delta_set(index.inner).map(|v| v as f64).collect();
        let mut region_indexes = transform.indices.clone();
        let mut deltas = transform.residual(&values)?;
        region_indexes.push(neutral);
        deltas.push(bias as f64);
        result
            .item_variation_data
            .push(super::store::encode_rows(&region_indexes, &[deltas])?.into());
    }
    Ok(Some(result))
}

// ComputedArray cannot infer the number of zero-byte PairPos records from
// their byte slice. Recover their dimensions from the borrowed table before
// instancing, preserving the subtable's coverage and lookup matching behavior.
fn own_gpos(source: &write_fonts::read::tables::gpos::Gpos) -> Result<Gpos, SubsetError> {
    use write_fonts::read::tables::gpos as read;
    let error = || SubsetError::SubsetTableError(Tag::new(b"GPOS"));
    let mut table: Gpos = source.to_owned_table();
    for (source, target) in source
        .lookup_list()
        .map_err(|_| error())?
        .lookups()
        .iter()
        .zip(&mut table.lookup_list.lookups)
    {
        match (source.map_err(|_| error())?, target.as_mut()) {
            (read::PositionLookup::Pair(source), PositionLookup::Pair(target)) => {
                for (source, target) in source.subtables().iter().zip(&mut target.subtables) {
                    own_empty_pairs(source.map_err(|_| error())?, target.as_mut())?;
                }
            }
            (read::PositionLookup::Extension(source), PositionLookup::Extension(target)) => {
                for (source, target) in source.subtables().iter().zip(&mut target.subtables) {
                    if let (
                        read::ExtensionSubtable::Pair(source),
                        ExtensionSubtable::Pair(target),
                    ) = (source.map_err(|_| error())?, target.as_mut())
                    {
                        own_empty_pairs(
                            source.extension().map_err(|_| error())?,
                            target.extension.as_mut(),
                        )?;
                    }
                }
            }
            _ => {}
        }
    }
    Ok(table)
}

fn own_empty_pairs(
    source: write_fonts::read::tables::gpos::PairPos,
    target: &mut PairPos,
) -> Result<(), SubsetError> {
    if let (write_fonts::read::tables::gpos::PairPos::Format2(source), PairPos::Format2(target)) =
        (source, target)
    {
        if source.value_format1().is_empty() && source.value_format2().is_empty() {
            let rows = source.class1_count() as usize;
            let columns = source.class2_count() as usize;
            if rows.saturating_mul(columns) > 200_000 {
                return Err(SubsetError::SubsetTableError(Tag::new(b"GPOS")));
            }
            target.class1_records = vec![
                Class1Record::new(vec![
                    Class2Record::new(
                        ValueRecord::new(),
                        ValueRecord::new()
                    );
                    columns
                ]);
                rows
            ];
        }
    }
    Ok(())
}

pub(super) fn instance(
    font: &FontRef,
    axes: &AxisPlan,
    tables: &mut BTreeMap<Tag, Vec<u8>>,
) -> Result<(), SubsetError> {
    let gdef = font.gdef().ok();
    let context = Context {
        store: gdef
            .as_ref()
            .and_then(|g| g.item_var_store())
            .transpose()
            .map_err(|_| SubsetError::SubsetTableError(Tag::new(b"GDEF")))?,
        coords: &axes.coords,
        axes,
        condition_biases: RefCell::default(),
    };
    if let Ok(gpos) = font.gpos() {
        validate_features(
            gpos.feature_variations()
                .transpose()
                .map_err(|_| SubsetError::SubsetTableError(Tag::new(b"GPOS")))?,
        )?;
        let mut table = own_gpos(&gpos)?;
        table.lookup_list.apply(&context)?;
        features(
            &mut table.feature_list,
            &mut table.feature_variations,
            &context,
        )?;
        save(tables, b"GPOS", &table)?;
    }
    if let Ok(gsub) = font.gsub() {
        validate_features(
            gsub.feature_variations()
                .transpose()
                .map_err(|_| SubsetError::SubsetTableError(Tag::new(b"GSUB")))?,
        )?;
        let mut table: write_fonts::tables::gsub::Gsub = gsub.to_owned_table();
        features(
            &mut table.feature_list,
            &mut table.feature_variations,
            &context,
        )?;
        save(tables, b"GSUB", &table)?;
    }
    if let Ok(math) = font.math() {
        let mut table: Math = math.to_owned_table();
        table.apply(&context)?;
        save(tables, b"MATH", &table)?;
    }
    if let Some(gdef) = gdef {
        let mut table: Gdef = gdef.to_owned_table();
        table.lig_caret_list.apply(&context)?;
        table.item_var_store = if axes.all_pinned() {
            Default::default()
        } else {
            rebuild_store(&context)?.map(Into::into).unwrap_or_default()
        };
        save(tables, b"GDEF", &table)?;
    }
    if let Ok(base) = font.base() {
        let context = Context {
            store: base
                .item_var_store()
                .transpose()
                .map_err(|_| SubsetError::SubsetTableError(Tag::new(b"BASE")))?,
            coords: &axes.coords,
            axes,
            condition_biases: RefCell::default(),
        };
        let mut table: Base = base.to_owned_table();
        table.horiz_axis.apply(&context)?;
        table.vert_axis.apply(&context)?;
        table.item_var_store = if axes.all_pinned() {
            Default::default()
        } else {
            context
                .store
                .as_ref()
                .map(|s| StorePlan::new(s, axes)?.rebuild(s))
                .transpose()?
                .map(Into::into)
                .unwrap_or_default()
        };
        save(tables, b"BASE", &table)?;
    }
    Ok(())
}
fn save(
    tables: &mut BTreeMap<Tag, Vec<u8>>,
    tag: &[u8; 4],
    table: &(impl write_fonts::FontWrite + write_fonts::validate::Validate),
) -> Result<(), SubsetError> {
    let tag = Tag::new(tag);
    tables.insert(
        tag,
        write_fonts::dump_table(table).map_err(|_| SubsetError::SubsetTableError(tag))?,
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use write_fonts::read::{FontData, FontRead};

    fn store_bytes(rows: &[[i32; 2]]) -> Vec<u8> {
        use write_fonts::tables::variations::*;
        let zero = RegionAxisCoordinates::new(F2Dot14::ZERO, F2Dot14::ZERO, F2Dot14::ZERO);
        let pos = RegionAxisCoordinates::new(F2Dot14::ZERO, F2Dot14::ONE, F2Dot14::ONE);
        write_fonts::dump_table(&write_fonts::tables::variations::ItemVariationStore {
            variation_region_list: VariationRegionList::new(
                2,
                vec![
                    VariationRegion::new(vec![pos.clone(), zero.clone()]),
                    VariationRegion::new(vec![zero, pos]),
                ],
            )
            .into(),
            item_variation_data: vec![ItemVariationData {
                item_count: rows.len() as u16,
                word_delta_count: 0x8002,
                region_indexes: vec![0, 1],
                delta_sets: rows
                    .iter()
                    .flatten()
                    .flat_map(|v| v.to_be_bytes())
                    .collect(),
            }
            .into()],
        })
        .unwrap()
    }

    #[test]
    fn zero_byte_pair_matrices_survive_full_instancing() {
        use write_fonts::types::GlyphId16;
        let bytes = std::fs::read("test-data/fonts/AdobeVFPrototype.otf").unwrap();
        let font = FontRef::new(&bytes).unwrap();
        let axes = AxisPlan::new(
            &font,
            &crate::parse_axis_limits("wght=900,CNTR=100").unwrap(),
        )
        .unwrap();
        let pair = PairPos::format_2(
            CoverageTable::from_iter([GlyphId16::new(1), GlyphId16::new(2)]),
            ClassDef::from_iter([(GlyphId16::new(2), 2)]),
            ClassDef::from_iter([(GlyphId16::new(1), 4), (GlyphId16::new(2), 1)]),
            vec![
                Class1Record::new(vec![
                    Class2Record::new(
                        ValueRecord::new(),
                        ValueRecord::new()
                    );
                    5
                ]);
                3
            ],
        );
        let mut oversized = write_fonts::dump_table(&pair).unwrap();
        oversized[12..16].copy_from_slice(&[0xff; 4]);
        let borrowed =
            write_fonts::read::tables::gpos::PairPos::read(FontData::new(&oversized)).unwrap();
        let mut owned = borrowed.to_owned_table();
        assert!(own_empty_pairs(borrowed, &mut owned).is_err());
        for extension in [false, true] {
            let lookup = if extension {
                PositionLookup::Extension(Lookup::new(
                    LookupFlag::empty(),
                    vec![ExtensionSubtable::Pair(ExtensionPosFormat1::new(
                        2,
                        pair.clone(),
                    ))],
                ))
            } else {
                PositionLookup::Pair(Lookup::new(LookupFlag::empty(), vec![pair.clone()]))
            };
            let gpos = Gpos::new(
                ScriptList::default(),
                FeatureList::default(),
                LookupList::new(vec![lookup]),
            );
            let mut builder = write_fonts::FontBuilder::new();
            for record in font.table_directory().table_records() {
                builder.add_raw(record.tag(), font.data_for_tag(record.tag()).unwrap());
            }
            builder.add_table(&gpos).unwrap();
            let bytes = builder.build();
            let font = FontRef::new(&bytes).unwrap();
            let mut tables = BTreeMap::new();
            instance(&font, &axes, &mut tables).unwrap();
            let output = write_fonts::read::tables::gpos::Gpos::read(FontData::new(
                &tables[&Tag::new(b"GPOS")],
            ))
            .unwrap();
            let output = own_gpos(&output).unwrap();
            assert_eq!(output, gpos);
        }
    }

    #[test]
    fn long_word_deltas_saturate_fields_and_preserve_condition_sign() {
        let bytes = std::fs::read("test-data/fonts/AdobeVFPrototype.otf").unwrap();
        let font = FontRef::new(&bytes).unwrap();
        let axes = AxisPlan::new(
            &font,
            &crate::parse_axis_limits("wght=900,CNTR=100").unwrap(),
        )
        .unwrap();
        let bytes = store_bytes(&[[i32::MAX, i32::MAX], [i32::MIN, i32::MIN]]);
        let c = Context {
            store: Some(ItemVariationStore::read(FontData::new(&bytes)).unwrap()),
            coords: &axes.coords,
            axes: &axes,
            condition_biases: RefCell::default(),
        };
        for (inner, base, expected) in [(0, 100, i16::MAX), (1, -100, i16::MIN)] {
            let mut value = ValueRecord::new()
                .with_x_advance(base)
                .with_x_advance_device(VariationIndex::new(0, inner));
            value.apply(&c).unwrap();
            assert_eq!(value.x_advance, Some(expected));
            assert!(value.x_advance_device.is_none());
            let cond = Condition::Format2VariableValue(ConditionFormat2 {
                default_value: base,
                var_index: inner as u32,
            });
            assert_eq!(condition(&cond, &c, 0).unwrap(), inner == 0);
        }
        let no_store = Context { store: None, ..c };
        let cond = Condition::Format2VariableValue(ConditionFormat2 {
            default_value: 1,
            var_index: u32::MAX,
        });
        assert!(condition(&cond, &no_store, 0).unwrap());
    }

    #[test]
    fn partial_variable_conditions_preserve_large_folded_defaults() {
        let bytes = std::fs::read("test-data/fonts/AdobeVFPrototype.otf").unwrap();
        let font = FontRef::new(&bytes).unwrap();
        let axes = AxisPlan::new(&font, &crate::parse_axis_limits("wght=900").unwrap()).unwrap();
        let bytes = store_bytes(&[[i32::MAX, -i32::MAX]]);
        let c = Context {
            store: Some(ItemVariationStore::read(FontData::new(&bytes)).unwrap()),
            coords: &axes.coords,
            axes: &axes,
            condition_biases: RefCell::default(),
        };
        let cond = Condition::Format2VariableValue(ConditionFormat2 {
            default_value: 100,
            var_index: 0,
        });
        let PartialCondition::Variable(Condition::Format2VariableValue(cond)) =
            partial_condition(&cond, &c, 0).unwrap()
        else {
            panic!()
        };
        assert_eq!(cond.default_value, 0);
        assert_eq!(cond.var_index, 1 << 16);
        let bytes = write_fonts::dump_table(&rebuild_store(&c).unwrap().unwrap()).unwrap();
        let store = ItemVariationStore::read(FontData::new(&bytes)).unwrap();
        for coord in [F2Dot14::ZERO, F2Dot14::from_f64(0.5), F2Dot14::ONE] {
            let delta = store
                .compute_delta(DeltaSetIndex { outer: 1, inner: 0 }, &[coord])
                .unwrap()
                .to_f64();
            assert_eq!(
                delta.round() as i64,
                (100. + i32::MAX as f64 * (1. - coord.to_f64())).round() as i64
            );
            assert!(delta > 0.);
        }
    }

    #[test]
    fn subsetting_partial_instances_preserves_variable_feature_conditions() {
        use write_fonts::read::{
            collections::IntSet,
            types::{GlyphId, GlyphId16, NameId},
        };
        let bytes = std::fs::read("test-data/fonts/AdobeVFPrototype.otf").unwrap();
        let font = FontRef::new(&bytes).unwrap();
        let store_bytes = store_bytes(&[[i32::MAX, -i32::MAX]]);
        let store: write_fonts::tables::variations::ItemVariationStore =
            ItemVariationStore::read(FontData::new(&store_bytes))
                .unwrap()
                .to_owned_table();
        let gdef = Gdef {
            item_var_store: store.into(),
            ..Default::default()
        };
        let tree = Condition::format_5_negate(Condition::format_3_and(
            2,
            vec![
                Condition::format_2_variable_value(-100, 0),
                Condition::format_4_or(
                    1,
                    vec![Condition::format_1_axis_range(
                        1,
                        F2Dot14::ZERO,
                        F2Dot14::ONE,
                    )],
                ),
            ],
        ));
        let mut gpos = Gpos::new(
            ScriptList::new(vec![ScriptRecord::new(
                Tag::new(b"DFLT"),
                Script::new(Some(LangSys::new(vec![0])), vec![]),
            )]),
            FeatureList::new(vec![FeatureRecord::new(
                Tag::new(b"kern"),
                Feature::new(None, vec![0]),
            )]),
            LookupList::new(
                [10, 20]
                    .into_iter()
                    .map(|advance| {
                        PositionLookup::Single(Lookup::new(
                            LookupFlag::empty(),
                            vec![SinglePos::Format1(SinglePosFormat1::new(
                                CoverageFormat1::new(vec![GlyphId16::new(1)]).into(),
                                ValueRecord::new().with_x_advance(advance),
                            ))],
                        ))
                    })
                    .collect(),
            ),
        );
        gpos.feature_variations = FeatureVariations {
            feature_variation_records: vec![FeatureVariationRecord {
                condition_set: ConditionSet {
                    conditions: vec![tree.into()],
                }
                .into(),
                feature_table_substitution: FeatureTableSubstitution {
                    substitutions: vec![FeatureTableSubstitutionRecord {
                        feature_index: 0,
                        alternate_feature: Feature::new(None, vec![1]).into(),
                    }],
                }
                .into(),
            }],
        }
        .into();
        let mut builder = write_fonts::FontBuilder::new();
        for r in font.table_directory().table_records() {
            if ![Tag::new(b"GPOS"), Tag::new(b"GSUB"), Tag::new(b"GDEF")].contains(&r.tag()) {
                builder.add_raw(r.tag(), font.data_for_tag(r.tag()).unwrap());
            }
        }
        builder.add_table(&gpos).unwrap();
        builder.add_table(&gdef).unwrap();
        let bytes = builder.build();
        let font = FontRef::new(&bytes).unwrap();
        let subset = |font: &FontRef| {
            let plan = crate::Plan::new(
                &[GlyphId::new(1)].into_iter().collect(),
                &IntSet::empty(),
                font,
                crate::SubsetFlags::default(),
                &IntSet::empty(),
                &IntSet::all(),
                &IntSet::all(),
                &IntSet::<NameId>::all(),
                &IntSet::all(),
            );
            crate::subset_font(font, &plan).unwrap()
        };
        // First preserve every recursive format during ordinary subsetting.
        let subset_bytes = subset(&font);
        let subset_font = FontRef::new(&subset_bytes).unwrap();
        let subset_gpos: Gpos = subset_font.gpos().unwrap().to_owned_table();
        assert_eq!(subset_gpos.feature_variations, gpos.feature_variations);
        // Then exercise the added GDEF bias row after partial instancing.
        let bytes =
            crate::instance_font(&font, &crate::parse_axis_limits("wght=900").unwrap()).unwrap();
        let partial = FontRef::new(&bytes).unwrap();
        let bytes = subset(&partial);
        let partial_subset = FontRef::new(&bytes).unwrap();
        assert!(partial_subset.gdef().unwrap().item_var_store().is_some());
        for (contrast, expected) in [(0, 10), (100, 20)] {
            let bytes = crate::instance_font(
                &partial_subset,
                &crate::parse_axis_limits(&format!("CNTR={contrast}")).unwrap(),
            )
            .unwrap();
            let full = FontRef::new(&bytes).unwrap();
            let table: Gpos = full.gpos().unwrap().to_owned_table();
            let index = table.feature_list.feature_records[0]
                .feature
                .lookup_list_indices[0] as usize;
            let PositionLookup::Single(lookup) = table.lookup_list.lookups[index].as_ref() else {
                panic!()
            };
            let SinglePos::Format1(pos) = lookup.subtables[0].as_ref() else {
                panic!()
            };
            assert_eq!(pos.value_record.x_advance, Some(expected));
        }
    }

    #[test]
    fn deep_conditions_are_rejected_before_owned_conversion() {
        let mut bytes = [0, 5, 0, 0, 5].repeat(4096);
        bytes.extend([0, 1, 0, 0, 0, 0, 0x40, 0]);
        let cond =
            write_fonts::read::tables::layout::Condition::read(FontData::new(&bytes)).unwrap();
        assert!(validate_condition(cond, 0, &mut 200_000).is_err());
    }

    #[test]
    fn math_values_are_instanced_with_the_gdef_store() {
        let bytes = std::fs::read("test-data/fonts/AdobeVFPrototype.otf").unwrap();
        let font = FontRef::new(&bytes).unwrap();
        let store_data = store_bytes(&[[40, 20]]);
        let store = ItemVariationStore::read(FontData::new(&store_data)).unwrap();
        let owned_store: write_fonts::tables::variations::ItemVariationStore =
            store.to_owned_table();
        let gdef = Gdef {
            item_var_store: owned_store.into(),
            ..Default::default()
        };
        let mut math = Math::default();
        math.math_constants.math_leading =
            MathValueRecord::new(100.into(), Some(VariationIndex::new(0, 0).into()));
        let mut builder = write_fonts::FontBuilder::new();
        for r in font.table_directory().table_records() {
            if ![Tag::new(b"GPOS"), Tag::new(b"GSUB"), Tag::new(b"GDEF")].contains(&r.tag()) {
                builder.add_raw(r.tag(), font.data_for_tag(r.tag()).unwrap());
            }
        }
        builder.add_table(&gdef).unwrap();
        builder.add_table(&math).unwrap();
        let bytes = builder.build();
        let font = FontRef::new(&bytes).unwrap();
        let partial =
            crate::instance_font(&font, &crate::parse_axis_limits("wght=900").unwrap()).unwrap();
        let partial = FontRef::new(&partial).unwrap();
        let bytes =
            crate::instance_font(&partial, &crate::parse_axis_limits("CNTR=100").unwrap()).unwrap();
        let full = FontRef::new(&bytes).unwrap();
        let value = full
            .math()
            .unwrap()
            .math_constants()
            .unwrap()
            .math_leading();
        assert_eq!(value.value().to_i16(), 160);
        assert!(value
            .device(full.math().unwrap().math_constants().unwrap().offset_data())
            .is_none());
        let bytes = crate::instance_font(
            &font,
            &crate::parse_axis_limits("wght=900,CNTR=100").unwrap(),
        )
        .unwrap();
        let direct = FontRef::new(&bytes).unwrap();
        let direct_math: Math = direct.math().unwrap().to_owned_table();
        let composed_math: Math = full.math().unwrap().to_owned_table();
        assert_eq!(direct_math, composed_math);
    }

    #[test]
    fn partial_feature_conditions_keep_priority_and_remap_axes() {
        let data = std::fs::read("test-data/fonts/AdobeVFPrototype.otf").unwrap();
        let font = FontRef::new(&data).unwrap();
        let axes = AxisPlan::new(&font, &crate::parse_axis_limits("wght=900").unwrap()).unwrap();
        let c = Context {
            store: None,
            coords: &axes.coords,
            axes: &axes,
            condition_biases: RefCell::default(),
        };
        let range = |axis| {
            Condition::Format1AxisRange(ConditionFormat1 {
                axis_index: axis,
                filter_range_min_value: F2Dot14::from_f64(0.5),
                filter_range_max_value: F2Dot14::ONE,
            })
        };
        let record = |conditions: Vec<Condition>, index| FeatureVariationRecord {
            condition_set: ConditionSet {
                conditions: conditions.into_iter().map(Into::into).collect(),
            }
            .into(),
            feature_table_substitution: FeatureTableSubstitution {
                substitutions: vec![FeatureTableSubstitutionRecord {
                    feature_index: 0,
                    alternate_feature: Feature {
                        lookup_list_indices: vec![index],
                        ..Default::default()
                    }
                    .into(),
                }],
            }
            .into(),
        };
        let mut vars: NullableOffsetMarker<FeatureVariations, 4> = FeatureVariations {
            feature_variation_records: vec![
                record(vec![range(0), range(1)], 1),
                record(vec![range(0)], 2),
            ],
        }
        .into();
        let mut list = FeatureList {
            feature_records: vec![FeatureRecord {
                feature_tag: Tag::new(b"rvrn"),
                feature: Feature {
                    lookup_list_indices: vec![0],
                    ..Default::default()
                }
                .into(),
            }],
        };
        features(&mut list, &mut vars, &c).unwrap();
        assert_eq!(vars.as_ref().unwrap().feature_variation_records.len(), 2);
        let set = vars.as_ref().unwrap().feature_variation_records[0]
            .condition_set
            .as_ref()
            .unwrap();
        let Condition::Format1AxisRange(cond) = set.conditions[0].as_ref() else {
            panic!()
        };
        assert_eq!(cond.axis_index, 0);
        for (value, expected) in [(0., 2), (75., 1)] {
            let axes = AxisPlan::new(
                &font,
                &crate::parse_axis_limits(&format!("wght=900,CNTR={value}")).unwrap(),
            )
            .unwrap();
            // The retained condition's index is in the output's axis order.
            let coords = vec![axes.coords[1]];
            let c = Context {
                store: None,
                coords: &coords,
                axes: &axes,
                condition_biases: RefCell::default(),
            };
            let mut list = list.clone();
            let mut vars = vars.clone();
            features(&mut list, &mut vars, &c).unwrap();
            assert_eq!(
                list.feature_records[0].feature.lookup_list_indices,
                vec![expected]
            );
            assert!(vars.is_none());
        }
    }
}
