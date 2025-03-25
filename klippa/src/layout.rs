//! impl subset() for layout common tables

use std::cmp::Ordering;

use crate::{
    serialize::{SerializeErrorFlags, Serializer},
    CollectVariationIndices, NameIdClosure, Plan, SubsetTable,
};
use fnv::FnvHashMap;
use write_fonts::{
    read::{
        collections::IntSet,
        tables::layout::{
            CharacterVariantParams, ClassDef, ClassDefFormat1, ClassDefFormat2, ClassRangeRecord,
            CoverageFormat1, CoverageFormat2, CoverageTable, DeltaFormat, Device,
            DeviceOrVariationIndex, Feature, FeatureParams, RangeRecord, SizeParams,
            StylisticSetParams, VariationIndex,
        },
        types::{GlyphId, GlyphId16, NameId},
    },
    types::FixedSize,
};

impl NameIdClosure for StylisticSetParams<'_> {
    fn collect_name_ids(&self, plan: &mut Plan) {
        plan.name_ids.insert(self.ui_name_id());
    }
}

impl NameIdClosure for SizeParams<'_> {
    fn collect_name_ids(&self, plan: &mut Plan) {
        plan.name_ids.insert(NameId::new(self.name_entry()));
    }
}

impl NameIdClosure for CharacterVariantParams<'_> {
    fn collect_name_ids(&self, plan: &mut Plan) {
        plan.name_ids.insert(self.feat_ui_label_name_id());
        plan.name_ids.insert(self.feat_ui_tooltip_text_name_id());
        plan.name_ids.insert(self.sample_text_name_id());

        let first_name_id = self.first_param_ui_label_name_id();
        let num_named_params = self.num_named_parameters();
        if first_name_id == NameId::COPYRIGHT_NOTICE
            || num_named_params == 0
            || num_named_params >= 0x7FFF
        {
            return;
        }

        let last_name_id = first_name_id.to_u16() as u32 + num_named_params as u32 - 1;
        plan.name_ids
            .insert_range(first_name_id..=NameId::new(last_name_id as u16));
    }
}

impl NameIdClosure for Feature<'_> {
    fn collect_name_ids(&self, plan: &mut Plan) {
        let Some(Ok(feature_params)) = self.feature_params() else {
            return;
        };
        match feature_params {
            FeatureParams::StylisticSet(table) => table.collect_name_ids(plan),
            FeatureParams::Size(table) => table.collect_name_ids(plan),
            FeatureParams::CharacterVariant(table) => table.collect_name_ids(plan),
        }
    }
}

impl<'a> SubsetTable<'a> for DeviceOrVariationIndex<'a> {
    type ArgsForSubset = &'a FnvHashMap<u32, (u32, i32)>;
    type Output = ();

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        args: &FnvHashMap<u32, (u32, i32)>,
    ) -> Result<(), SerializeErrorFlags> {
        match self {
            Self::Device(item) => item.subset(plan, s, ()),
            Self::VariationIndex(item) => item.subset(plan, s, args),
        }
    }
}

impl SubsetTable<'_> for Device<'_> {
    type ArgsForSubset = ();
    type Output = ();
    fn subset(
        &self,
        _plan: &Plan,
        s: &mut Serializer,
        _args: (),
    ) -> Result<(), SerializeErrorFlags> {
        s.embed_bytes(self.min_table_bytes()).map(|_| ())
    }
}

impl<'a> SubsetTable<'a> for VariationIndex<'a> {
    type ArgsForSubset = &'a FnvHashMap<u32, (u32, i32)>;
    type Output = ();

    fn subset(
        &self,
        _plan: &Plan,
        s: &mut Serializer,
        args: &FnvHashMap<u32, (u32, i32)>,
    ) -> Result<(), SerializeErrorFlags> {
        let var_idx =
            ((self.delta_set_outer_index() as u32) << 16) + self.delta_set_inner_index() as u32;
        let Some((new_idx, _)) = args.get(&var_idx) else {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER);
        };

        s.embed(*new_idx)?;
        s.embed(self.delta_format()).map(|_| ())
    }
}

impl CollectVariationIndices for DeviceOrVariationIndex<'_> {
    fn collect_variation_indices(&self, plan: &Plan, varidx_set: &mut IntSet<u32>) {
        match self {
            Self::Device(_item) => (),
            Self::VariationIndex(item) => item.collect_variation_indices(plan, varidx_set),
        }
    }
}

impl CollectVariationIndices for VariationIndex<'_> {
    fn collect_variation_indices(&self, _plan: &Plan, varidx_set: &mut IntSet<u32>) {
        if self.delta_format() == DeltaFormat::VariationIndex {
            let var_idx =
                ((self.delta_set_outer_index() as u32) << 16) + self.delta_set_inner_index() as u32;
            varidx_set.insert(var_idx);
        }
    }
}

pub(crate) struct ClassDefSubsetStruct<'a> {
    pub(crate) remap_class: bool,
    pub(crate) keep_empty_table: bool,
    pub(crate) use_class_zero: bool,
    pub(crate) glyph_filter: Option<&'a CoverageTable<'a>>,
}

impl<'a> SubsetTable<'a> for ClassDef<'a> {
    type ArgsForSubset = &'a ClassDefSubsetStruct<'a>;
    // class_map: Option<FnvHashMap<u16, u16>>
    type Output = Option<FnvHashMap<u16, u16>>;
    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        args: Self::ArgsForSubset,
    ) -> Result<Self::Output, SerializeErrorFlags> {
        match self {
            Self::Format1(item) => item.subset(plan, s, args),
            Self::Format2(item) => item.subset(plan, s, args),
        }
    }
}

impl<'a> SubsetTable<'a> for ClassDefFormat1<'a> {
    type ArgsForSubset = &'a ClassDefSubsetStruct<'a>;
    // class_map: Option<FnvHashMap<u16, u16>>
    type Output = Option<FnvHashMap<u16, u16>>;
    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        args: Self::ArgsForSubset,
    ) -> Result<Self::Output, SerializeErrorFlags> {
        let glyph_map = &plan.glyph_map_gsub;

        let start = self.start_glyph_id().to_u32();
        let end = start + self.glyph_count() as u32 - 1;
        let end = plan.glyphset_gsub.last().unwrap().to_u32().min(end);

        let class_values = self.class_value_array();
        let mut retained_classes = IntSet::empty();

        let cap = glyph_map.len().min(self.glyph_count() as usize);
        let mut new_gid_classes = Vec::with_capacity(cap);

        for g in start..=end {
            let gid = GlyphId::from(g);
            let Some(new_gid) = glyph_map.get(&gid) else {
                continue;
            };

            if let Some(glyph_filter) = args.glyph_filter {
                if glyph_filter.get(gid).is_none() {
                    continue;
                }
            }

            let Some(class) = class_values.get((g - start) as usize) else {
                return Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR));
            };

            let class = class.get();
            if class == 0 {
                continue;
            }

            retained_classes.insert(class);
            new_gid_classes.push((new_gid.to_u32() as u16, class));
        }

        let use_class_zero = if args.use_class_zero {
            let glyph_count = if let Some(glyph_filter) = args.glyph_filter {
                glyph_map
                    .keys()
                    .filter(|&g| glyph_filter.get(*g).is_some())
                    .count()
            } else {
                glyph_map.len()
            };
            glyph_count <= new_gid_classes.len()
        } else {
            false
        };

        if !args.keep_empty_table && new_gid_classes.is_empty() {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY);
        }

        classdef_remap_and_serialize(
            args.remap_class,
            &retained_classes,
            use_class_zero,
            &mut new_gid_classes,
            s,
        )
    }
}

impl<'a> SubsetTable<'a> for ClassDefFormat2<'a> {
    type ArgsForSubset = &'a ClassDefSubsetStruct<'a>;
    // class_map: Option<FnvHashMap<u16, u16>>
    type Output = Option<FnvHashMap<u16, u16>>;
    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        args: Self::ArgsForSubset,
    ) -> Result<Self::Output, SerializeErrorFlags> {
        let glyph_map = &plan.glyph_map_gsub;
        let glyph_set = &plan.glyphset_gsub;

        let mut retained_classes = IntSet::empty();

        let population = self.population();
        let cap = glyph_map.len().min(population);
        let mut new_gid_classes = Vec::with_capacity(cap);

        let num_bits = 16 - self.class_range_count().leading_zeros() as u64;
        if population as u64 > glyph_set.len() * num_bits {
            for g in glyph_set.iter() {
                if g.to_u32() > 0xFFFF_u32 {
                    break;
                }
                let Some(new_gid) = glyph_map.get(&g) else {
                    continue;
                };
                if let Some(glyph_filter) = args.glyph_filter {
                    if glyph_filter.get(g).is_none() {
                        continue;
                    }
                }

                let class = self.get(GlyphId16::from(g.to_u32() as u16));
                if class == 0 {
                    continue;
                }

                retained_classes.insert(class);
                new_gid_classes.push((new_gid.to_u32() as u16, class));
            }
        } else {
            for record in self.class_range_records() {
                let class = record.class();
                if class == 0 {
                    continue;
                }

                let start = record.start_glyph_id().to_u32();
                let end = record
                    .end_glyph_id()
                    .to_u32()
                    .min(glyph_set.last().unwrap().to_u32());
                for g in start..=end {
                    let gid = GlyphId::from(g);
                    let Some(new_gid) = glyph_map.get(&gid) else {
                        continue;
                    };
                    if let Some(glyph_filter) = args.glyph_filter {
                        if glyph_filter.get(gid).is_none() {
                            continue;
                        }
                    }

                    retained_classes.insert(class);
                    new_gid_classes.push((new_gid.to_u32() as u16, class));
                }
            }
        }

        new_gid_classes.sort_by(|a, b| a.0.cmp(&b.0));
        let use_class_zero = if args.use_class_zero {
            let glyph_count = if let Some(glyph_filter) = args.glyph_filter {
                glyph_map
                    .keys()
                    .filter(|&g| glyph_filter.get(*g).is_some())
                    .count()
            } else {
                glyph_map.len()
            };
            glyph_count <= new_gid_classes.len()
        } else {
            false
        };

        if !args.keep_empty_table && new_gid_classes.is_empty() {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY);
        }

        classdef_remap_and_serialize(
            args.remap_class,
            &retained_classes,
            use_class_zero,
            &mut new_gid_classes,
            s,
        )
    }
}

fn classdef_remap_and_serialize(
    remap_class: bool,
    retained_classes: &IntSet<u16>,
    use_class_zero: bool,
    new_gid_classes: &mut [(u16, u16)],
    s: &mut Serializer,
) -> Result<Option<FnvHashMap<u16, u16>>, SerializeErrorFlags> {
    if !remap_class {
        return ClassDef::serialize(s, new_gid_classes).map(|()| None);
    }

    let mut class_map = FnvHashMap::default();
    if !use_class_zero {
        class_map.insert(0_u16, 0_u16);
    }

    let mut new_idx = if use_class_zero { 0_u16 } else { 1 };
    for class in retained_classes.iter() {
        class_map.insert(class, new_idx);
        new_idx += 1;
    }

    for (_, class) in new_gid_classes.iter_mut() {
        let Some(new_class) = class_map.get(class) else {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER);
        };
        *class = *new_class;
    }
    ClassDef::serialize(s, new_gid_classes).map(|()| Some(class_map))
}

trait Serialize<'a> {
    type Args: 'a;
    /// Serialize this table
    fn serialize(s: &mut Serializer, args: Self::Args) -> Result<(), SerializeErrorFlags>;
}

impl<'a> Serialize<'a> for ClassDef<'a> {
    type Args = &'a [(u16, u16)];
    fn serialize(
        s: &mut Serializer,
        new_gid_classes: &[(u16, u16)],
    ) -> Result<(), SerializeErrorFlags> {
        let mut glyph_min = 0;
        let mut glyph_max = 0;
        let mut prev_g = 0;
        let mut prev_class = 0;

        let mut num_glyphs = 0_u16;
        let mut num_ranges = 1_u16;
        for (g, class) in new_gid_classes.iter().filter(|(_, class)| *class != 0) {
            num_glyphs += 1;
            if num_glyphs == 1 {
                glyph_min = *g;
                glyph_max = *g;
                prev_g = *g;
                prev_class = *class;
                continue;
            }

            glyph_max = glyph_max.max(*g);
            if *g != prev_g + 1 || *class != prev_class + 1 {
                num_ranges += 1;
            }

            prev_g = *g;
            prev_class = *class;
        }

        if num_glyphs > 0 && (glyph_max - glyph_min + 1) < num_ranges * 3 {
            ClassDefFormat1::serialize(s, new_gid_classes)
        } else {
            ClassDefFormat2::serialize(s, new_gid_classes)
        }
    }
}

impl<'a> Serialize<'a> for ClassDefFormat1<'a> {
    type Args = &'a [(u16, u16)];
    fn serialize(
        s: &mut Serializer,
        new_gid_classes: &[(u16, u16)],
    ) -> Result<(), SerializeErrorFlags> {
        // format 1
        s.embed(1_u16)?;
        // start_glyph
        let start_glyph_pos = s.embed(0_u16)?;
        // glyph count
        let glyph_count_pos = s.embed(0_u16)?;

        let mut num = 0;
        let mut glyph_min = 0;
        let mut glyph_max = 0;
        for (g, _) in new_gid_classes.iter().filter(|(_, class)| *class != 0) {
            if num == 0 {
                glyph_min = *g;
                glyph_max = *g;
            } else {
                glyph_max = *g.max(&glyph_max);
            }
            num += 1;
        }

        if num == 0 {
            return Ok(());
        }

        s.copy_assign(start_glyph_pos, glyph_min);

        let glyph_count = glyph_max - glyph_min + 1;
        s.copy_assign(glyph_count_pos, glyph_count);

        let pos = s.allocate_size((glyph_count as usize) * 2, true)?;
        for (g, class) in new_gid_classes.iter().filter(|(_, class)| *class != 0) {
            let idx = (*g - glyph_min) as usize;
            s.copy_assign(pos + idx * 2, *class);
        }
        Ok(())
    }
}

impl<'a> Serialize<'a> for ClassDefFormat2<'a> {
    type Args = &'a [(u16, u16)];
    fn serialize(
        s: &mut Serializer,
        new_gid_classes: &[(u16, u16)],
    ) -> Result<(), SerializeErrorFlags> {
        // format 2
        s.embed(2_u16)?;
        //classRange count
        let range_count_pos = s.embed(0_u16)?;

        let mut num = 0_u16;
        let mut prev_g = 0;
        let mut prev_class = 0;

        let mut num_ranges = 0_u16;
        let mut pos = 0;
        for (g, class) in new_gid_classes.iter().filter(|(_, class)| *class != 0) {
            num += 1;
            if num == 1 {
                prev_g = *g;
                prev_class = *class;

                pos = s.allocate_size(ClassRangeRecord::RAW_BYTE_LEN, true)?;
                s.copy_assign(pos, prev_g);
                s.copy_assign(pos + 2, prev_g);
                s.copy_assign(pos + 4, prev_class);

                num_ranges += 1;
                continue;
            }

            if *g != prev_g + 1 || *class != prev_class {
                num_ranges += 1;
                // update last_gid of previous record
                s.copy_assign(pos + 2, prev_g);

                pos = s.allocate_size(ClassRangeRecord::RAW_BYTE_LEN, true)?;
                s.copy_assign(pos, *g);
                s.copy_assign(pos + 2, *g);
                s.copy_assign(pos + 4, *class);
            }

            prev_class = *class;
            prev_g = *g;
        }

        if num == 0 {
            return Ok(());
        }

        // update end glyph of the last record
        s.copy_assign(pos + 2, prev_g);
        // update range count
        s.copy_assign(range_count_pos, num_ranges);
        Ok(())
    }
}

impl<'a> SubsetTable<'a> for CoverageTable<'a> {
    type ArgsForSubset = ();
    type Output = ();
    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        args: Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        match self {
            CoverageTable::Format1(sub) => sub.subset(plan, s, args),
            CoverageTable::Format2(sub) => sub.subset(plan, s, args),
        }
    }
}

impl<'a> SubsetTable<'a> for CoverageFormat1<'a> {
    type ArgsForSubset = ();
    type Output = ();
    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        _args: Self::ArgsForSubset,
    ) -> Result<Self::Output, SerializeErrorFlags> {
        let glyph_count = (self.glyph_count() as usize).min(plan.font_num_glyphs);
        let Some(glyph_array) = self.glyph_array().get(0..glyph_count) else {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR);
        };

        let num_bits = 16 - (glyph_count as u16).leading_zeros() as usize;
        // if/else branches return the same result, it's just an optimization that
        // we pick the faster approach depending on the number of glyphs
        let retained_glyphs: Vec<u32> =
            if glyph_count > (plan.glyphset_gsub.len() as usize) * num_bits {
                plan.glyphset_gsub
                    .iter()
                    .filter_map(|old_gid| {
                        glyph_array
                            .binary_search_by(|g| g.get().to_u32().cmp(&old_gid.to_u32()))
                            .ok()
                            .and_then(|_| plan.glyph_map_gsub.get(&old_gid))
                            .map(|g| g.to_u32())
                    })
                    .collect()
            } else {
                glyph_array
                    .iter()
                    .filter_map(|g| {
                        plan.glyph_map_gsub
                            .get(&GlyphId::from(g.get()))
                            .map(|g| g.to_u32())
                    })
                    .collect()
            };

        CoverageTable::serialize(s, &retained_glyphs)
    }
}

impl<'a> SubsetTable<'a> for CoverageFormat2<'a> {
    type ArgsForSubset = ();
    type Output = ();
    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        _args: Self::ArgsForSubset,
    ) -> Result<Self::Output, SerializeErrorFlags> {
        let range_count = self.range_count();
        if range_count as usize > plan.font_num_glyphs {
            return Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR));
        }
        let num_bits = 16 - range_count.leading_zeros() as usize;
        // if/else branches return the same result, it's just an optimization that
        // we pick the faster approach depending on the number of glyphs
        let retained_glyphs: Vec<u32> = if self.population() > plan.glyph_map_gsub.len() * num_bits
        {
            let range_records = self.range_records();
            plan.glyphset_gsub
                .iter()
                .filter_map(|g| {
                    range_records
                        .binary_search_by(|rec| {
                            if rec.end_glyph_id().to_u32() < g.to_u32() {
                                Ordering::Less
                            } else if rec.start_glyph_id().to_u32() > g.to_u32() {
                                Ordering::Greater
                            } else {
                                Ordering::Equal
                            }
                        })
                        .ok()
                        .and_then(|_| plan.glyph_map_gsub.get(&g))
                        .map(|gid| gid.to_u32())
                })
                .collect()
        } else {
            self.range_records()
                .iter()
                .flat_map(|r| {
                    r.iter().filter_map(|g| {
                        plan.glyph_map_gsub
                            .get(&GlyphId::from(g))
                            .map(|gid| gid.to_u32())
                    })
                })
                .collect()
        };
        CoverageTable::serialize(s, &retained_glyphs)
    }
}

impl<'a> Serialize<'a> for CoverageTable<'a> {
    type Args = &'a [u32];
    fn serialize(s: &mut Serializer, glyphs: &[u32]) -> Result<(), SerializeErrorFlags> {
        if glyphs.is_empty() {
            return CoverageFormat1::serialize(s, glyphs);
        }

        let glyph_count = glyphs.len();
        let mut num_ranges = 1_u16;
        let mut last = glyphs[0];

        for g in glyphs.iter().skip(1) {
            if last + 1 != *g {
                num_ranges += 1;
            }

            last = *g;
        }

        // TODO: add support for unsorted glyph list??
        // ref: <https://github.com/harfbuzz/harfbuzz/blob/59001aa9527c056ad08626cfec9a079b65d8aec8/src/OT/Layout/Common/Coverage.hh#L143>
        if glyph_count <= num_ranges as usize * 3 {
            CoverageFormat1::serialize(s, glyphs)
        } else {
            CoverageFormat2::serialize(s, (glyphs, num_ranges))
        }
    }
}

impl<'a> Serialize<'a> for CoverageFormat1<'a> {
    type Args = &'a [u32];
    fn serialize(s: &mut Serializer, glyphs: &[u32]) -> Result<(), SerializeErrorFlags> {
        //format
        s.embed(1_u16)?;

        // count
        let count = glyphs.len();
        s.embed(count as u16)?;

        let pos = s.allocate_size(count * 2, true)?;
        for (idx, g) in glyphs.iter().enumerate() {
            s.copy_assign(pos + idx * 2, *g as u16);
        }
        Ok(())
    }
}

impl<'a> Serialize<'a> for CoverageFormat2<'a> {
    type Args = (&'a [u32], u16);
    fn serialize(s: &mut Serializer, args: Self::Args) -> Result<(), SerializeErrorFlags> {
        let (glyphs, range_count) = args;
        //format
        s.embed(2_u16)?;

        //range_count
        s.embed(range_count)?;

        // range records
        let pos = s.allocate_size((range_count as usize) * RangeRecord::RAW_BYTE_LEN, true)?;

        let mut last = glyphs[0] as u16;
        let mut range = 0;
        for (idx, g) in glyphs.iter().enumerate() {
            let g = *g as u16;
            let range_pos = pos + range * RangeRecord::RAW_BYTE_LEN;
            if last + 1 != g {
                if range == 0 {
                    //start glyph
                    s.copy_assign(range_pos, g);

                    //coverage index
                    s.copy_assign(range_pos + 4, idx as u16);
                } else {
                    //end glyph
                    s.copy_assign(range_pos + 2, last);
                    range += 1;

                    let new_range_pos = range_pos + RangeRecord::RAW_BYTE_LEN;
                    //start glyph
                    s.copy_assign(new_range_pos, g);
                    //coverage index
                    s.copy_assign(new_range_pos + 4, idx as u16);
                }
            }
            last = g;
        }

        let last_range_pos = pos + range * RangeRecord::RAW_BYTE_LEN;
        s.copy_assign(last_range_pos, last);
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use write_fonts::{
        read::{
            tables::gpos::{PairPos, PositionSubtables},
            FontRef, TableProvider,
        },
        types::GlyphId,
    };

    #[test]
    fn test_subset_gpos_classdefs() {
        let font = FontRef::new(include_bytes!("../test-data/fonts/AdobeVFPrototype.otf")).unwrap();
        let gpos = font.gpos().unwrap();
        let gpos_lookup = gpos.lookup_list().unwrap().lookups().get(0).unwrap();

        let Ok(PositionSubtables::Pair(subtables)) = gpos_lookup.subtables() else {
            panic!("invalid lookup!")
        };
        let Ok(PairPos::Format2(pair_pos2)) = subtables.get(1) else {
            panic!("invalid lookup!")
        };

        let class_def1 = pair_pos2.class_def1().unwrap();
        let class_def2 = pair_pos2.class_def2().unwrap();
        let coverage = pair_pos2.coverage().unwrap();

        let mut plan = Plan::default();
        plan.glyphset_gsub.insert(GlyphId::NOTDEF);
        plan.glyphset_gsub.insert(GlyphId::from(34_u32));
        plan.glyphset_gsub.insert(GlyphId::from(35_u32));

        plan.glyph_map_gsub.insert(GlyphId::NOTDEF, GlyphId::NOTDEF);
        plan.glyph_map_gsub
            .insert(GlyphId::from(34_u32), GlyphId::from(1_u32));
        plan.glyph_map_gsub
            .insert(GlyphId::from(35_u32), GlyphId::from(2_u32));

        let mut s = Serializer::new(1024);
        assert_eq!(s.start_serialize(), Ok(()));
        //test ClassDef1: remap_class: true, keep_empty_table: true, use_class_zero: true, use glyph_filter:Some(&Coverage)
        let ret = class_def1.subset(
            &plan,
            &mut s,
            &ClassDefSubsetStruct {
                remap_class: true,
                keep_empty_table: true,
                use_class_zero: true,
                glyph_filter: Some(&coverage),
            },
        );
        assert!(ret.is_ok());
        assert!(!s.in_error());
        s.end_serialize();

        let subsetted_data = s.copy_bytes();
        let expected_bytes: [u8; 8] = [0x00, 0x01, 0x00, 0x02, 0x00, 0x01, 0x00, 0x01];
        assert_eq!(subsetted_data, expected_bytes);

        let ret_hashmap = ret.unwrap().unwrap();
        assert_eq!(ret_hashmap.len(), 2);
        assert_eq!(ret_hashmap.get(&2), Some(&0));
        assert_eq!(ret_hashmap.get(&44), Some(&1));

        // test subset ClassDef2:
        // remap_class: true, keep_empty_table: true, use_class_zero: false, no glyph_filter: None
        let mut s = Serializer::new(1024);
        assert_eq!(s.start_serialize(), Ok(()));
        let ret = class_def2.subset(
            &plan,
            &mut s,
            &ClassDefSubsetStruct {
                remap_class: true,
                keep_empty_table: true,
                use_class_zero: false,
                glyph_filter: None,
            },
        );
        assert!(ret.is_ok());
        assert!(!s.in_error());
        s.end_serialize();

        let subsetted_data = s.copy_bytes();
        let expected_bytes: [u8; 10] = [0x00, 0x01, 0x00, 0x01, 0x00, 0x02, 0x00, 0x02, 0x00, 0x01];
        assert_eq!(subsetted_data, expected_bytes);

        let ret_hashmap = ret.unwrap().unwrap();
        assert_eq!(ret_hashmap.len(), 3);
        assert_eq!(ret_hashmap.get(&0), Some(&0));
        assert_eq!(ret_hashmap.get(&1), Some(&1));
        assert_eq!(ret_hashmap.get(&5), Some(&2));
    }

    #[test]
    fn test_subset_gdef_glyph_classdef() {
        let font = FontRef::new(include_bytes!(
            "../test-data/fonts/IndicTestHowrah-Regular.ttf"
        ))
        .unwrap();
        let gdef = font.gdef().unwrap();
        let glyph_class_def = gdef.glyph_class_def().unwrap().unwrap();

        let mut plan = Plan::default();
        plan.glyphset_gsub.insert(GlyphId::NOTDEF);
        plan.glyphset_gsub.insert(GlyphId::from(68_u32));

        plan.glyph_map_gsub.insert(GlyphId::NOTDEF, GlyphId::NOTDEF);
        plan.glyph_map_gsub
            .insert(GlyphId::from(68_u32), GlyphId::from(1_u32));

        let mut s = Serializer::new(1024);
        assert_eq!(s.start_serialize(), Ok(()));
        //remap_class: false, keep_empty_table: false, use_class_zero: true, no glyph_filter:None
        let ret = glyph_class_def.subset(
            &plan,
            &mut s,
            &ClassDefSubsetStruct {
                remap_class: false,
                keep_empty_table: false,
                use_class_zero: true,
                glyph_filter: None,
            },
        );
        assert!(ret.is_ok());
        assert!(ret.unwrap().is_none());

        assert!(!s.in_error());
        s.end_serialize();

        let subsetted_data = s.copy_bytes();
        let expected_bytes: [u8; 8] = [0x00, 0x01, 0x00, 0x01, 0x00, 0x01, 0x00, 0x01];
        assert_eq!(subsetted_data, expected_bytes);
    }

    #[test]
    fn test_subset_coverage_format2_to_format1() {
        let font = FontRef::new(include_bytes!(
            "../test-data/fonts/IndicTestHowrah-Regular.ttf"
        ))
        .unwrap();
        let gdef = font.gdef().unwrap();
        let attach_list = gdef.attach_list().unwrap().unwrap();
        let coverage = attach_list.coverage().unwrap();

        let mut plan = Plan {
            font_num_glyphs: 611,
            ..Default::default()
        };
        plan.glyphset_gsub.insert(GlyphId::NOTDEF);
        plan.glyphset_gsub.insert(GlyphId::from(68_u32));

        plan.glyph_map_gsub.insert(GlyphId::NOTDEF, GlyphId::NOTDEF);
        plan.glyph_map_gsub
            .insert(GlyphId::from(68_u32), GlyphId::from(3_u32));

        let mut s = Serializer::new(1024);
        assert_eq!(s.start_serialize(), Ok(()));
        let ret = coverage.subset(&plan, &mut s, ());
        assert!(ret.is_ok());

        assert!(!s.in_error());
        s.end_serialize();

        let subsetted_data = s.copy_bytes();
        let expected_bytes: [u8; 6] = [0x00, 0x01, 0x00, 0x01, 0x00, 0x03];
        assert_eq!(subsetted_data, expected_bytes);
    }

    #[test]
    fn test_subset_coverage_format1() {
        let font = FontRef::new(include_bytes!("../test-data/fonts/AdobeVFPrototype.otf")).unwrap();
        let gpos = font.gpos().unwrap();
        let gpos_lookup = gpos.lookup_list().unwrap().lookups().get(0).unwrap();

        let Ok(PositionSubtables::Pair(subtables)) = gpos_lookup.subtables() else {
            panic!("invalid lookup!")
        };
        let Ok(PairPos::Format1(pair_pos1)) = subtables.get(0) else {
            panic!("invalid lookup!")
        };

        let coverage = pair_pos1.coverage().unwrap();

        let mut plan = Plan {
            font_num_glyphs: 313,
            ..Default::default()
        };
        plan.glyphset_gsub.insert(GlyphId::NOTDEF);
        plan.glyphset_gsub.insert(GlyphId::from(34_u32));
        plan.glyphset_gsub.insert(GlyphId::from(35_u32));
        plan.glyphset_gsub.insert(GlyphId::from(36_u32));
        plan.glyphset_gsub.insert(GlyphId::from(56_u32));

        plan.glyph_map_gsub.insert(GlyphId::NOTDEF, GlyphId::NOTDEF);
        plan.glyph_map_gsub
            .insert(GlyphId::from(34_u32), GlyphId::from(1_u32));
        plan.glyph_map_gsub
            .insert(GlyphId::from(35_u32), GlyphId::from(2_u32));
        plan.glyph_map_gsub
            .insert(GlyphId::from(36_u32), GlyphId::from(3_u32));
        plan.glyph_map_gsub
            .insert(GlyphId::from(56_u32), GlyphId::from(4_u32));

        let mut s = Serializer::new(1024);
        assert_eq!(s.start_serialize(), Ok(()));
        let ret = coverage.subset(&plan, &mut s, ());
        assert!(ret.is_ok());
        assert!(!s.in_error());
        s.end_serialize();

        let subsetted_data = s.copy_bytes();
        let expected_bytes: [u8; 8] = [0x00, 0x01, 0x00, 0x02, 0x00, 0x02, 0x00, 0x04];
        assert_eq!(subsetted_data, expected_bytes);
    }
}
