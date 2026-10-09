//! Apply variation deltas while walking owned layout tables.
use super::{rebase::renormalize, AxisPlan, StorePlan};
use crate::SubsetError;
use std::collections::BTreeMap;
use write_fonts::{
    from_obj::ToOwnedTable,
    read::{
        tables::variations::{DeltaSetIndex, ItemVariationStore},
        FontRef, TableProvider,
    },
    tables::{base::*, gdef::*, gpos::*, layout::*},
    types::{F2Dot14, Tag},
    NullableOffsetMarker, OffsetMarker,
};

struct Context<'a> {
    store: Option<ItemVariationStore<'a>>,
    coords: &'a [F2Dot14],
    axes: &'a AxisPlan,
}
impl Context<'_> {
    fn delta(&self, outer: u16, inner: u16) -> Result<i32, SubsetError> {
        self.store
            .as_ref()
            .and_then(|s| s.compute_delta(DeltaSetIndex { outer, inner }, self.coords))
            .map(|v| v.to_f64().round() as i32)
            .ok_or(SubsetError::SubsetTableError(Tag::new(b"GDEF")))
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
    (value as i32 + delta).clamp(i16::MIN as i32, i16::MAX as i32) as i16
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
            v.default_value as i32 + c.delta((v.var_index >> 16) as u16, v.var_index as u16)? > 0
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
            v.default_value = add(
                v.default_value,
                c.delta((v.var_index >> 16) as u16, v.var_index as u16)?,
            );
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
    };
    if let Ok(gpos) = font.gpos() {
        let mut table: Gpos = gpos.to_owned_table();
        table.lookup_list.apply(&context)?;
        features(
            &mut table.feature_list,
            &mut table.feature_variations,
            &context,
        )?;
        save(tables, b"GPOS", &table)?;
    }
    if let Ok(gsub) = font.gsub() {
        let mut table: write_fonts::tables::gsub::Gsub = gsub.to_owned_table();
        features(
            &mut table.feature_list,
            &mut table.feature_variations,
            &context,
        )?;
        save(tables, b"GSUB", &table)?;
    }
    if let Some(gdef) = gdef {
        let mut table: Gdef = gdef.to_owned_table();
        table.lig_caret_list.apply(&context)?;
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
    #[test]
    fn partial_feature_conditions_keep_priority_and_remap_axes() {
        let data = std::fs::read("test-data/fonts/AdobeVFPrototype.otf").unwrap();
        let font = FontRef::new(&data).unwrap();
        let axes = AxisPlan::new(&font, &crate::parse_axis_limits("wght=900").unwrap()).unwrap();
        let c = Context {
            store: None,
            coords: &axes.coords,
            axes: &axes,
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
