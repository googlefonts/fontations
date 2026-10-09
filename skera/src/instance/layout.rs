//! Apply variation deltas while walking owned layout tables.
use super::AxisPlan;
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
            *device = Default::default();
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
                *device = Default::default();
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
                *self = Self::Format1(CaretValueFormat1 {
                    coordinate: add(a.coordinate, delta),
                });
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
        table.item_var_store = Default::default();
        save(tables, b"GDEF", &table)?;
    }
    if let Ok(base) = font.base() {
        let context = Context {
            store: base
                .item_var_store()
                .transpose()
                .map_err(|_| SubsetError::SubsetTableError(Tag::new(b"BASE")))?,
            coords: &axes.coords,
        };
        let mut table: Base = base.to_owned_table();
        table.horiz_axis.apply(&context)?;
        table.vert_axis.apply(&context)?;
        table.item_var_store = Default::default();
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
