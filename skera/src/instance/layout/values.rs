//! Walk owned layout values in exactly one of the two store passes.
use super::LayoutContext;
use crate::SubsetError;
use read_fonts::types::Tag;
use write_fonts::{
    tables::{base::*, gdef::*, gpos::*, jstf::*, layout::*, math::*},
    NullableOffsetMarker, OffsetMarker,
};

/// A remapping pass has no source store or coordinates, so it cannot apply
/// default deltas a second time. Both passes visit the same value fields.
pub(super) enum ValuePass<'a, 'b> {
    Instance(&'a LayoutContext<'b>),
    Remap(&'a crate::instance::optimize::IndexMap),
}
impl ValuePass<'_, '_> {
    fn device(&self, device: &DeviceOrVariationIndex) -> Result<Option<i32>, SubsetError> {
        match device {
            DeviceOrVariationIndex::VariationIndex(v) => match self {
                Self::Instance(context) => context
                    .delta(v.delta_set_outer_index, v.delta_set_inner_index)
                    .map(Some),
                Self::Remap(_) => Ok(Some(0)),
            },
            DeviceOrVariationIndex::Device(_) => Ok(None),
            _ => Err(SubsetError::SubsetTableError(Tag::new(b"GDEF"))),
        }
    }
    fn device_is_constant(&self, device: &DeviceOrVariationIndex) -> Result<bool, SubsetError> {
        Ok(match device {
            DeviceOrVariationIndex::VariationIndex(v) => {
                let index =
                    ((v.delta_set_outer_index as u32) << 16) | v.delta_set_inner_index as u32;
                match self {
                    Self::Instance(context) => {
                        context.axes.all_pinned() || context.index_is_constant(index)?
                    }
                    Self::Remap(map) => map.get(&index).copied().unwrap_or(u32::MAX) == u32::MAX,
                }
            }
            _ => false,
        })
    }
    fn remap_device(&self, device: &mut DeviceOrVariationIndex) {
        if let (Self::Remap(map), DeviceOrVariationIndex::VariationIndex(v)) = (self, device) {
            let index = ((v.delta_set_outer_index as u32) << 16) | v.delta_set_inner_index as u32;
            let mapped = map.get(&index).copied().unwrap_or(u32::MAX);
            v.delta_set_outer_index = (mapped >> 16) as u16;
            v.delta_set_inner_index = mapped as u16;
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
            if self.device_is_constant(device.as_ref().unwrap())? {
                *device = Default::default();
            } else {
                self.remap_device(device.as_mut().unwrap());
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

pub(super) trait ApplyValues {
    fn apply(&mut self, c: &ValuePass) -> Result<(), SubsetError>;
}
impl<T: ApplyValues> ApplyValues for Vec<T> {
    fn apply(&mut self, c: &ValuePass) -> Result<(), SubsetError> {
        for v in self {
            v.apply(c)?;
        }
        Ok(())
    }
}
impl<T: ApplyValues, const N: usize> ApplyValues for OffsetMarker<T, N> {
    fn apply(&mut self, c: &ValuePass) -> Result<(), SubsetError> {
        self.as_mut().apply(c)
    }
}
impl<T: ApplyValues, const N: usize> ApplyValues for NullableOffsetMarker<T, N> {
    fn apply(&mut self, c: &ValuePass) -> Result<(), SubsetError> {
        if let Some(v) = self.as_mut() {
            v.apply(c)?;
        }
        Ok(())
    }
}
macro_rules! fields {
    ($table:ty; $($field:ident),+) => {
        impl ApplyValues for $table {
            fn apply(&mut self, pass: &ValuePass) -> Result<(), SubsetError> {
                $(self.$field.apply(pass)?;)+
                Ok(())
            }
        }
    };
}
impl<T: ApplyValues> ApplyValues for Lookup<T> {
    fn apply(&mut self, c: &ValuePass) -> Result<(), SubsetError> {
        self.subtables.apply(c)
    }
}
impl<T: ApplyValues> ApplyValues for LookupList<T> {
    fn apply(&mut self, c: &ValuePass) -> Result<(), SubsetError> {
        self.lookups.apply(c)
    }
}
impl<T: ApplyValues> ApplyValues for LookupList2<T> {
    fn apply(&mut self, c: &ValuePass) -> Result<(), SubsetError> {
        self.lookups.apply(c)
    }
}
impl<T: ApplyValues> ApplyValues for ExtensionPosFormat1<T> {
    fn apply(&mut self, c: &ValuePass) -> Result<(), SubsetError> {
        self.extension.apply(c)
    }
}
macro_rules! variants {
    ($table:ty; $($variant:ident),+; $($ignore:ident),*) => {
        impl ApplyValues for $table {
            fn apply(&mut self, pass: &ValuePass) -> Result<(), SubsetError> {
                match self {
                    $(Self::$variant(value) => value.apply(pass),)+
                    $(Self::$ignore(_) => Ok(()),)*
                }
            }
        }
    };
}
variants!(PositionLookup;Single,Pair,Cursive,MarkToBase,MarkToLig,MarkToMark,Extension;Contextual,ChainContextual);
variants!(ExtensionSubtable;Single,Pair,Cursive,MarkToBase,MarkToLig,MarkToMark;Contextual,ChainContextual);
variants!(SinglePos;Format1,Format2,Format3,Format4;);
variants!(CursivePos;Format1,Format2;);
variants!(MarkBasePos;Format1,Format2;);
variants!(MarkLigPos;Format1,Format2;);
variants!(MarkMarkPos;Format1,Format2;);
variants!(PairPos;Format1,Format2,Format3,Format4;);
fields!(SinglePosFormat1;value_record);
fields!(SinglePosFormat3;value_record);
fn uniform_value_format<'a>(records: impl Iterator<Item = &'a mut ValueRecord>) {
    let records = records.collect::<Vec<_>>();
    let format = records.iter().fold(ValueFormat::empty(), |format, record| {
        format | record.format()
    });
    for record in records {
        record.set_explicit_value_format(format);
    }
}
impl ApplyValues for SinglePosFormat2 {
    fn apply(&mut self, c: &ValuePass) -> Result<(), SubsetError> {
        self.value_records.apply(c)?;
        uniform_value_format(self.value_records.iter_mut());
        Ok(())
    }
}
impl ApplyValues for SinglePosFormat4 {
    fn apply(&mut self, c: &ValuePass) -> Result<(), SubsetError> {
        self.value_records.apply(c)?;
        uniform_value_format(self.value_records.iter_mut());
        Ok(())
    }
}
impl ApplyValues for PairPosFormat1 {
    fn apply(&mut self, c: &ValuePass) -> Result<(), SubsetError> {
        self.pair_sets.apply(c)?;
        uniform_value_format(
            self.pair_sets
                .iter_mut()
                .flat_map(|set| set.pair_value_records.iter_mut())
                .map(|record| &mut record.value_record1),
        );
        uniform_value_format(
            self.pair_sets
                .iter_mut()
                .flat_map(|set| set.pair_value_records.iter_mut())
                .map(|record| &mut record.value_record2),
        );
        Ok(())
    }
}
impl ApplyValues for PairPosFormat3 {
    fn apply(&mut self, c: &ValuePass) -> Result<(), SubsetError> {
        self.pair_sets.apply(c)?;
        uniform_value_format(
            self.pair_sets
                .iter_mut()
                .flat_map(|set| set.pair_value_records.iter_mut())
                .map(|record| &mut record.value_record1),
        );
        uniform_value_format(
            self.pair_sets
                .iter_mut()
                .flat_map(|set| set.pair_value_records.iter_mut())
                .map(|record| &mut record.value_record2),
        );
        Ok(())
    }
}
fields!(PairSet;pair_value_records);
fields!(PairSet2;pair_value_records);
fields!(PairValueRecord;value_record1,value_record2);
fields!(PairValueRecord2;value_record1,value_record2);
impl ApplyValues for PairPosFormat2 {
    fn apply(&mut self, c: &ValuePass) -> Result<(), SubsetError> {
        self.class1_records.apply(c)?;
        uniform_value_format(
            self.class1_records
                .iter_mut()
                .flat_map(|class| class.class2_records.iter_mut())
                .map(|record| &mut record.value_record1),
        );
        uniform_value_format(
            self.class1_records
                .iter_mut()
                .flat_map(|class| class.class2_records.iter_mut())
                .map(|record| &mut record.value_record2),
        );
        Ok(())
    }
}
impl ApplyValues for PairPosFormat4 {
    fn apply(&mut self, c: &ValuePass) -> Result<(), SubsetError> {
        self.class1_records.apply(c)?;
        uniform_value_format(
            self.class1_records
                .iter_mut()
                .flat_map(|class| class.class2_records.iter_mut())
                .map(|record| &mut record.value_record1),
        );
        uniform_value_format(
            self.class1_records
                .iter_mut()
                .flat_map(|class| class.class2_records.iter_mut())
                .map(|record| &mut record.value_record2),
        );
        Ok(())
    }
}
fields!(Class1Record;class2_records);
fields!(Class2Record;value_record1,value_record2);
fields!(CursivePosFormat1;entry_exit_record);
fields!(CursivePosFormat2;entry_exit_record);
fields!(EntryExitRecord;entry_anchor,exit_anchor);
fields!(EntryExitRecord2;entry_anchor,exit_anchor);
fields!(MarkBasePosFormat1;mark_array,base_array);
fields!(MarkBasePosFormat2;mark_array,base_array);
fields!(MarkArray;mark_records);
fields!(MarkArray2;mark_records);
fields!(MarkRecord;mark_anchor);
fields!(MarkRecord2;mark_anchor);
fields!(BaseArray;base_records);
fields!(BaseArray2;base_records);
fields!(BaseRecord;base_anchors);
fields!(BaseRecord2;base_anchors);
fields!(MarkLigPosFormat1;mark_array,ligature_array);
fields!(MarkLigPosFormat2;mark_array,ligature_array);
fields!(LigatureArray;ligature_attaches);
fields!(LigatureArray2;ligature_attaches);
fields!(LigatureAttach;component_records);
fields!(LigatureAttach2;component_records);
fields!(ComponentRecord;ligature_anchors);
fields!(ComponentRecord2;ligature_anchors);
fields!(MarkMarkPosFormat1;mark1_array,mark2_array);
fields!(MarkMarkPosFormat2;mark1_array,mark2_array);
fields!(Mark2Array;mark2_records);
fields!(Mark2Array2;mark2_records);
fields!(Mark2Record;mark2_anchors);
fields!(Mark2Record2;mark2_anchors);
fields!(LigCaretList;lig_glyphs);
fields!(LigCaretList2;lig_glyphs);
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
fields!(Jstf;jstf_script_records);
fields!(JstfScriptRecord;jstf_script);
fields!(JstfScript;def_jstf_lang_sys,jstf_lang_sys_records);
fields!(JstfLangSysRecord;jstf_lang_sys);
fields!(JstfLangSys;jstf_priorities);
fields!(JstfPriority;shrinkage_jstf_max,extension_jstf_max);

impl ApplyValues for MathValueRecord {
    fn apply(&mut self, c: &ValuePass) -> Result<(), SubsetError> {
        let mut value = self.value.to_i16();
        c.apply(&mut value, &mut self.device)?;
        self.value = value.into();
        Ok(())
    }
}

impl ApplyValues for ValueRecord {
    fn apply(&mut self, c: &ValuePass) -> Result<(), SubsetError> {
        let mut fmt = self.format();
        for (value, device, value_flag, device_flag) in [
            (
                &mut self.x_placement,
                &mut self.x_placement_device,
                ValueFormat::X_PLACEMENT,
                ValueFormat::X_PLACEMENT_DEVICE,
            ),
            (
                &mut self.y_placement,
                &mut self.y_placement_device,
                ValueFormat::Y_PLACEMENT,
                ValueFormat::Y_PLACEMENT_DEVICE,
            ),
            (
                &mut self.x_advance,
                &mut self.x_advance_device,
                ValueFormat::X_ADVANCE,
                ValueFormat::X_ADVANCE_DEVICE,
            ),
            (
                &mut self.y_advance,
                &mut self.y_advance_device,
                ValueFormat::Y_ADVANCE,
                ValueFormat::Y_ADVANCE_DEVICE,
            ),
        ] {
            if let Some(delta) = device.as_ref().map(|d| c.device(d)).transpose()?.flatten() {
                if delta != 0 {
                    fmt |= value_flag;
                }
                *value = Some(add(value.unwrap_or(0), delta));
                if c.device_is_constant(device.as_ref().unwrap())? {
                    *device = Default::default();
                } else {
                    c.remap_device(device.as_mut().unwrap());
                }
            }
            if device.is_none() {
                fmt.remove(device_flag);
            }
        }
        // Subtables merge these effective formats across all their records,
        // including rows that had null offsets or no original base value.
        self.set_explicit_value_format(fmt);
        Ok(())
    }
}
impl ApplyValues for AnchorTable {
    fn apply(&mut self, c: &ValuePass) -> Result<(), SubsetError> {
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
impl ApplyValues for CaretValue {
    fn apply(&mut self, c: &ValuePass) -> Result<(), SubsetError> {
        if let Self::Format3(a) = self {
            if let Some(delta) = c.device(&a.device)? {
                a.coordinate = add(a.coordinate, delta);
                if c.device_is_constant(&a.device)? {
                    *self = Self::Format1(CaretValueFormat1 {
                        coordinate: a.coordinate,
                    });
                } else {
                    c.remap_device(&mut a.device);
                }
            }
        }
        Ok(())
    }
}
impl ApplyValues for BaseCoord {
    fn apply(&mut self, c: &ValuePass) -> Result<(), SubsetError> {
        if let Self::Format3(a) = self {
            c.apply(&mut a.coordinate, &mut a.device)?;
        }
        Ok(())
    }
}
