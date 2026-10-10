//! VARC subsetting follows HarfBuzz's varc_subset_plan_t: collect glyphs and
//! auxiliary indices, create maps, rewrite records, then select auxiliary data.
//! Keep packed values, transforms, and reserved fields byte-for-byte.
use crate::{
    serialize::{SerializeErrorFlags, Serializer},
    Plan, SubsetError, SubsetFlags,
};
use std::{collections::BTreeMap, collections::BTreeSet, ops::Range};
use write_fonts::{
    from_obj::ToOwnedTable,
    ps::cff::v2::Index,
    read::{collections::IntSet, tables::varc::Varc as ReadVarc, FontRef, TableProvider},
    tables::{layout::*, varc::*},
    types::{GlyphId, GlyphId16, Tag},
};

type Result<T> = std::result::Result<T, ()>;
type Map = BTreeMap<u32, u32>;
fn append_varint(out: &mut Vec<u8>, value: u32) {
    let (extra, prefix) = if value < 0x80 {
        (0, 0)
    } else if value < 0x4000 {
        (1, 0x80)
    } else if value < 0x200000 {
        (2, 0xc0)
    } else if value < 0x10000000 {
        (3, 0xe0)
    } else {
        (4, 0xf0)
    };
    out.push(if extra == 4 {
        prefix
    } else {
        prefix | (value >> (extra * 8)) as u8
    });
    if extra != 0 {
        out.extend(&value.to_be_bytes()[4 - extra..]);
    }
}
const TAG: Tag = Tag::new(b"VARC");
const NO_VARIATION: u32 = u32::MAX;
const HAVE_AXES: u32 = 1 << 1;
const AXIS_VARIATION: u32 = 1 << 2;
const TRANSFORM_VARIATION: u32 = 1 << 3;
const HAVE_CONDITION: u32 = 1 << 7;
const GID_24BIT: u32 = 1 << 12;
const RESERVED: u32 = !((1 << 15) - 1);
const TRANSFORM_FIELDS: u32 = (1 << 4)
    | (1 << 5)
    | (1 << 6)
    | (1 << 8)
    | (1 << 9)
    | (1 << 10)
    | (1 << 11)
    | (1 << 13)
    | (1 << 14);

fn table<'a>(font: &FontRef<'a>) -> Result<ReadVarc<'a>> {
    let table = font.varc().map_err(|_| ())?;
    if table.version().major != 1 {
        return Err(());
    }
    Ok(table)
}

struct Field {
    value: u32,
    bytes: Range<usize>,
}

struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
}
impl<'a> Cursor<'a> {
    fn take(&mut self, len: usize) -> Result<&'a [u8]> {
        let end = self.pos.checked_add(len).ok_or(())?;
        let bytes = self.bytes.get(self.pos..end).ok_or(())?;
        self.pos = end;
        Ok(bytes)
    }
    fn varint(&mut self) -> Result<Field> {
        let start = self.pos;
        let first = self.take(1)?[0];
        let (extra, mask) = match first {
            0..=0x7f => (0, 0x7f),
            0x80..=0xbf => (1, 0x3f),
            0xc0..=0xdf => (2, 0x1f),
            0xe0..=0xef => (3, 0x0f),
            _ => (4, 0),
        };
        let mut value = (first & mask) as u32;
        for &byte in self.take(extra)? {
            value = (value << 8) | byte as u32;
        }
        Ok(Field {
            value,
            bytes: start..self.pos,
        })
    }
    fn packed_values(&mut self, count: Option<usize>) -> Result<usize> {
        let mut total = 0usize;
        while count.map_or(self.pos < self.bytes.len(), |n| total < n) {
            let control = self.take(1)?[0];
            let len = (control as usize & 0x3f) + 1;
            total = total.checked_add(len).ok_or(())?;
            if count.is_some_and(|n| total > n) {
                return Err(());
            }
            let width = match control & 0xc0 {
                0 => 1,
                0x40 => 2,
                0x80 => 0,
                _ => 4,
            };
            self.take(len * width)?;
        }
        Ok(total)
    }
}

struct Record {
    flags: u32,
    gid: Field,
    condition: Option<Field>,
    axes: Option<Field>,
    axis_var: Option<Field>,
    transform_var: Option<Field>,
    size: usize,
}
impl Record {
    // VarComponent::decompile_record stores byte spans for index fields so
    // compile_component can remap indices while copying everything else.
    fn read(table: &ReadVarc, bytes: &[u8]) -> Result<Self> {
        let mut c = Cursor { bytes, pos: 0 };
        let flags = c.varint()?.value;
        let start = c.pos;
        let mut value = 0u32;
        for &byte in c.take(if flags & GID_24BIT != 0 { 3 } else { 2 })? {
            value = (value << 8) | byte as u32;
        }
        let gid = Field {
            value,
            bytes: start..c.pos,
        };
        let condition = (flags & HAVE_CONDITION != 0)
            .then(|| c.varint())
            .transpose()?;
        let axes = if flags & HAVE_AXES != 0 {
            let field = c.varint()?;
            let list = table.axis_indices_list().ok_or(())?.map_err(|_| ())?;
            let values = list.get(field.value as usize).ok_or(())?;
            let count = Cursor {
                bytes: values,
                pos: 0,
            }
            .packed_values(None)?;
            c.packed_values(Some(count))?;
            Some(field)
        } else {
            None
        };
        let axis_var = (flags & AXIS_VARIATION != 0)
            .then(|| c.varint())
            .transpose()?;
        let transform_var = (flags & TRANSFORM_VARIATION != 0)
            .then(|| c.varint())
            .transpose()?;
        c.take((flags & TRANSFORM_FIELDS).count_ones() as usize * 2)?;
        for _ in 0..(flags & RESERVED).count_ones() {
            c.varint()?;
        }
        Ok(Self {
            flags,
            gid,
            condition,
            axes,
            axis_var,
            transform_var,
            size: c.pos,
        })
    }

    fn compile(&self, bytes: &[u8], plan: &Plan, maps: &SubsetPlan) -> Result<Vec<u8>> {
        let retain = plan
            .subset_flags
            .contains(SubsetFlags::SUBSET_FLAGS_RETAIN_GIDS);
        let gid = if retain {
            self.gid.value
        } else {
            plan.glyph_map
                .get(&GlyphId::new(self.gid.value))
                .ok_or(())?
                .to_u32()
        };
        if gid > 0xffffff {
            return Err(());
        }
        if retain && maps.auxiliary_indices_unchanged() {
            return Ok(bytes[..self.size].to_vec());
        }
        let flags = self.flags | if gid > 0xffff { GID_24BIT } else { 0 };
        let mut out = Vec::new();
        append_varint(&mut out, flags);
        let width = if flags & GID_24BIT != 0 { 3 } else { 2 };
        out.extend(&gid.to_be_bytes()[4 - width..]);
        let mut pos = self.gid.bytes.end;
        for (field, map, variation) in [
            (&self.condition, &maps.condition_map, false),
            (&self.axes, &maps.axis_map, false),
            (&self.axis_var, &maps.var_map, true),
            (&self.transform_var, &maps.var_map, true),
        ] {
            if let Some(field) = field {
                out.extend(&bytes[pos..field.bytes.start]);
                let value = if variation && field.value == NO_VARIATION {
                    NO_VARIATION
                } else {
                    *map.get(&field.value).ok_or(())?
                };
                append_varint(&mut out, value);
                pos = field.bytes.end;
            }
        }
        out.extend(&bytes[pos..self.size]);
        Ok(out)
    }
}

pub(crate) fn closure(font: &FontRef, glyphs: &mut IntSet<GlyphId>) -> Result<()> {
    if font.data_for_tag(TAG).is_none() {
        return Ok(());
    }
    let table = table(font)?;
    let coverage = table.coverage().map_err(|_| ())?;
    let records = table.var_composite_glyphs().map_err(|_| ())?;
    let mut pending = glyphs.clone();
    // The pending set visits each glyph once, including cycles and shared
    // components. Conditions never restrict closure, as in HarfBuzz.
    loop {
        let next = pending.iter().next();
        let Some(gid) = next else { break };
        pending.remove(gid);
        let Some(index) = coverage.get(gid) else {
            continue;
        };
        let mut bytes = records.get(index as usize).ok_or(())?;
        while !bytes.is_empty() {
            let component = Record::read(&table, bytes)?;
            let gid = GlyphId::new(component.gid.value);
            if !glyphs.contains(gid) {
                glyphs.insert(gid);
                pending.insert(gid);
            }
            bytes = &bytes[component.size..];
        }
    }
    Ok(())
}

struct SubsetPlan {
    old_indices: Vec<usize>,
    new_gids: Vec<GlyphId16>,
    conditions: Vec<Condition>,
    condition_map: Map,
    axis_map: Map,
    var_map: Map,
    inner_maps: BTreeMap<u32, BTreeSet<u32>>,
}
impl SubsetPlan {
    fn auxiliary_indices_unchanged(&self) -> bool {
        [&self.condition_map, &self.axis_map, &self.var_map]
            .into_iter()
            .all(|map| map.iter().all(|(old, new)| old == new))
    }

    fn collect_glyphs(table: &ReadVarc, plan: &Plan) -> Result<Self> {
        let records = table.var_composite_glyphs().map_err(|_| ())?;
        let mut out = Self {
            old_indices: Vec::new(),
            new_gids: Vec::new(),
            conditions: Vec::new(),
            condition_map: Map::new(),
            axis_map: Map::new(),
            var_map: Map::new(),
            inner_maps: BTreeMap::new(),
        };
        for (index, gid) in table
            .coverage()
            .map_err(|_| ())?
            .iter()
            .enumerate()
            .take(records.count() as usize)
        {
            let gid = GlyphId::from(gid);
            if plan.glyphset_varced.contains(gid) {
                out.old_indices.push(index);
                let new_gid = plan.glyph_map.get(&gid).ok_or(())?.to_u32();
                out.new_gids
                    .push(GlyphId16::new(u16::try_from(new_gid).map_err(|_| ())?));
            }
        }
        Ok(out)
    }

    fn collect_indices(&mut self, table: &ReadVarc, axis_remap: Option<&Map>) -> Result<()> {
        let records = table.var_composite_glyphs().map_err(|_| ())?;
        let mut conditions = BTreeSet::new();
        let mut axes = BTreeSet::new();
        let mut vars = BTreeSet::new();
        for &index in &self.old_indices {
            let mut bytes = records.get(index).ok_or(())?;
            while !bytes.is_empty() {
                let component = Record::read(table, bytes)?;
                if let Some(f) = &component.condition {
                    conditions.insert(f.value);
                }
                if let Some(f) = &component.axes {
                    axes.insert(f.value);
                }
                for field in [&component.axis_var, &component.transform_var]
                    .into_iter()
                    .flatten()
                {
                    if field.value != NO_VARIATION {
                        vars.insert(field.value);
                    }
                }
                bytes = &bytes[component.size..];
            }
        }
        let mut budget = 1_000_000;
        if !conditions.is_empty() {
            let list = table.condition_list().ok_or(())?.map_err(|_| ())?;
            for &index in &conditions {
                let offset = list
                    .condition_offsets()
                    .get(index as usize)
                    .ok_or(())?
                    .get()
                    .to_u32();
                let condition =
                    crate::conditions::at_offset(list.offset_data(), offset, 64, &mut budget)
                        .map_err(|_| ())?;
                collect_condition_vars(&condition, &mut vars);
                self.conditions.push(condition);
            }
        }
        self.condition_map = dense_map(&conditions);
        self.axis_map = dense_map(&axes);
        for index in vars {
            self.inner_maps
                .entry(index >> 16)
                .or_default()
                .insert(index & 0xffff);
        }
        for (new_outer, (&outer, inners)) in self.inner_maps.iter().enumerate() {
            for (new_inner, &inner) in inners.iter().enumerate() {
                self.var_map.insert(
                    (outer << 16) | inner,
                    (new_outer as u32) << 16 | new_inner as u32,
                );
            }
        }
        for condition in &mut self.conditions {
            remap_condition(condition, &self.var_map, axis_remap)?;
        }
        Ok(())
    }

    fn compile_records(&self, table: &ReadVarc, plan: &Plan) -> Result<Index> {
        let records = table.var_composite_glyphs().map_err(|_| ())?;
        let mut out = Vec::new();
        for &index in &self.old_indices {
            let mut bytes = records.get(index).ok_or(())?;
            let mut record = Vec::new();
            while !bytes.is_empty() {
                let component = Record::read(table, bytes)?;
                record.extend(component.compile(bytes, plan, self)?);
                bytes = &bytes[component.size..];
            }
            out.push(record);
        }
        Ok(Index::from_items(out))
    }

    fn select_axis_indices(&self, table: &ReadVarc, axes: Option<&Map>) -> Result<Option<Index>> {
        if self.axis_map.is_empty() {
            return Ok(None);
        }
        let list = table.axis_indices_list().ok_or(())?.map_err(|_| ())?;
        Ok(Some(Index::from_items(
            self.axis_map
                .keys()
                .map(|&i| {
                    let bytes = list.get(i as usize).ok_or(())?;
                    let Some(axes) = axes else {
                        return Ok(bytes.to_vec());
                    };
                    Cursor { bytes, pos: 0 }.packed_values(None)?;
                    let values = write_fonts::read::tables::variations::PackedDeltas::consume_all(
                        bytes.into(),
                    )
                    .iter()
                    .map(|index| {
                        axes.get(&(index as u32))
                            .copied()
                            .map(|i| i as i32)
                            .ok_or(())
                    })
                    .collect::<Result<Vec<_>>>()?;
                    write_fonts::dump_table(&write_fonts::tables::variations::PackedDeltas::new(
                        values,
                    ))
                    .map_err(|_| ())
                })
                .collect::<Result<Vec<_>>>()?,
        )))
    }

    fn subset_store(
        &self,
        table: &ReadVarc,
        axes: Option<&Map>,
    ) -> Result<Option<MultiItemVariationStore>> {
        if self.var_map.is_empty() {
            return Ok(None);
        }
        let store = table.multi_var_store().ok_or(())?.map_err(|_| ())?;
        if store.format() != 1 {
            return Err(());
        }
        let regions = store.region_list().map_err(|_| ())?;
        let mut region_indices = BTreeSet::new();
        let mut data = Vec::new();
        for (&outer, inners) in &self.inner_maps {
            let source = store.variation_data().get(outer as usize).map_err(|_| ())?;
            if source.format() != 1
                || source.region_indices().len() != source.region_index_count() as usize
            {
                return Err(());
            }
            region_indices.extend(source.region_indices().iter().map(|i| i.get() as u32));
            let delta_sets = source.delta_sets().map_err(|_| ())?;
            let selected = inners
                .iter()
                .map(|&i| {
                    let bytes = delta_sets.get(i as usize).ok_or(())?;
                    Cursor { bytes, pos: 0 }.packed_values(None)?;
                    Ok(bytes.to_vec())
                })
                .collect::<Result<Vec<_>>>()?;
            data.push(MultiItemVariationData::new(
                source.region_index_count(),
                source.region_indices().iter().map(|i| i.get()).collect(),
                Index::from_items(selected),
            ));
        }
        let region_map = dense_map(&region_indices);
        let retained = region_indices
            .iter()
            .map(|&i| {
                let region = regions.regions().get(i as usize).map_err(|_| ())?;
                let coordinates = region
                    .axis_coordinates()
                    .iter()
                    .map(|a| {
                        let mut a: SparseRegionAxisCoordinates =
                            a.map_err(|_| ())?.to_owned_table();
                        if let Some(axes) = axes {
                            a.axis_index =
                                u16::try_from(*axes.get(&(a.axis_index as u32)).ok_or(())?)
                                    .map_err(|_| ())?;
                        }
                        Ok(a)
                    })
                    .collect::<Result<Vec<_>>>()?;
                if coordinates.len() != region.region_axis_count() as usize {
                    return Err(());
                }
                Ok(SparseVariationRegion::new(
                    region.region_axis_count(),
                    coordinates,
                ))
            })
            .collect::<Result<Vec<SparseVariationRegion>>>()?;
        for data in &mut data {
            for region in &mut data.region_indices {
                *region = *region_map.get(&(*region as u32)).ok_or(())? as u16;
            }
        }
        Ok(Some(MultiItemVariationStore::new(
            SparseVariationRegionList::new(retained.len() as u32, retained),
            u16::try_from(data.len()).map_err(|_| ())?,
            data,
        )))
    }
}

fn dense_map(indices: &BTreeSet<u32>) -> Map {
    indices
        .iter()
        .enumerate()
        .map(|(i, &v)| (v, i as u32))
        .collect()
}
fn collect_condition_vars(condition: &Condition, vars: &mut BTreeSet<u32>) {
    match condition {
        Condition::Format2VariableValue(c) if c.var_index != NO_VARIATION => {
            vars.insert(c.var_index);
        }
        Condition::Format3And(c) => {
            for c in &c.conditions {
                collect_condition_vars(c.as_ref(), vars);
            }
        }
        Condition::Format4Or(c) => {
            for c in &c.conditions {
                collect_condition_vars(c.as_ref(), vars);
            }
        }
        Condition::Format5Negate(c) => collect_condition_vars(c.condition.as_ref(), vars),
        _ => (),
    }
}
fn remap_condition(condition: &mut Condition, map: &Map, axes: Option<&Map>) -> Result<()> {
    match condition {
        Condition::Format1AxisRange(c) => {
            if let Some(axes) = axes {
                c.axis_index =
                    u16::try_from(*axes.get(&(c.axis_index as u32)).ok_or(())?).map_err(|_| ())?;
            }
        }
        Condition::Format2VariableValue(c) if c.var_index != NO_VARIATION => {
            c.var_index = *map.get(&c.var_index).ok_or(())?;
        }
        Condition::Format3And(c) => {
            for c in &mut c.conditions {
                remap_condition(c.as_mut(), map, axes)?;
            }
        }
        Condition::Format4Or(c) => {
            for c in &mut c.conditions {
                remap_condition(c.as_mut(), map, axes)?;
            }
        }
        Condition::Format5Negate(c) => remap_condition(c.condition.as_mut(), map, axes)?,
        _ => (),
    }
    Ok(())
}

pub(crate) fn subset(
    font: &FontRef,
    plan: &Plan,
    s: &mut Serializer,
) -> std::result::Result<(), SubsetError> {
    match subset_bytes(font, plan, None) {
        Ok(bytes) => s
            .embed_bytes(&bytes)
            .map(|_| ())
            .map_err(|_| SubsetError::SubsetTableError(TAG)),
        Err(()) => {
            s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR);
            Err(SubsetError::SubsetTableError(TAG))
        }
    }
}

// HarfBuzz's varc_subset_plan_t excludes requested axes from its axis map,
// rejecting any change to an axis referenced by retained VARC data. The other
// axes only need their indices remapped; component values and deltas stay in
// the original final-coordinate space.
pub(crate) fn instance(
    font: &FontRef,
    axes: &crate::instance::AxisPlan,
) -> std::result::Result<Vec<u8>, SubsetError> {
    let error = || SubsetError::SubsetTableError(TAG);
    let fvar = font.fvar().map_err(|_| error())?;
    let mut map = Map::new();
    let mut new_index = 0;
    for (i, axis) in fvar.axes().map_err(|_| error())?.iter().enumerate() {
        if axes.pinned[i] {
            continue;
        }
        if !axes.values.iter().any(|(tag, _)| *tag == axis.axis_tag()) {
            map.insert(i as u32, new_index);
        }
        new_index += 1;
    }
    let mut plan = Plan::keep_everything(font);
    plan.subset_flags |= SubsetFlags::SUBSET_FLAGS_RETAIN_GIDS;
    subset_bytes(font, &plan, Some(&map)).map_err(|_| error())
}

fn subset_bytes(font: &FontRef, plan: &Plan, axes: Option<&Map>) -> Result<Vec<u8>> {
    let table = table(font)?;
    let mut subset = SubsetPlan::collect_glyphs(&table, plan)?;
    if subset.old_indices.is_empty() {
        return Ok(Vec::new());
    }
    subset.collect_indices(&table, axes)?;
    let records = subset.compile_records(&table, plan)?;
    let axis_indices = subset.select_axis_indices(&table, axes)?;
    let store = subset.subset_store(&table, axes)?;
    let conditions = (!subset.conditions.is_empty())
        .then(|| ConditionList::new(subset.conditions.len() as u32, subset.conditions));
    let out = Varc::new(
        CoverageTable::from_iter(subset.new_gids),
        store,
        conditions,
        axis_indices,
        records,
    );
    let mut bytes = write_fonts::dump_table(&out).map_err(|_| ())?;
    bytes[..4].copy_from_slice(&table.offset_data().as_bytes()[..4]);
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conditions::own as own_condition;
    use write_fonts::read::FontData;

    #[test]
    fn reserved_fields_and_24_bit_gids_survive_record_rewriting() {
        let flags = GID_24BIT | (1 << 4) | (1 << 14) | (1 << 15) | (1 << 31);
        let mut bytes = Vec::new();
        append_varint(&mut bytes, flags);
        bytes.extend([0, 0, 1]);
        bytes.extend([0xff, 0xf0, 0x12, 0x34]); // translation and skew
        append_varint(&mut bytes, 65535);
        append_varint(&mut bytes, u32::MAX);
        let table = ReadVarc::default();
        let record = Record::read(&table, &bytes).unwrap();
        assert_eq!(record.size, bytes.len());
        let mut plan = Plan::default();
        plan.glyph_map.insert(GlyphId::new(1), GlyphId::new(3));
        let maps = SubsetPlan {
            old_indices: Vec::new(),
            new_gids: Vec::new(),
            conditions: Vec::new(),
            condition_map: Map::new(),
            axis_map: Map::new(),
            var_map: Map::new(),
            inner_maps: BTreeMap::new(),
        };
        let out = record.compile(&bytes, &plan, &maps).unwrap();
        let rewritten = Record::read(&table, &out).unwrap();
        assert_eq!(rewritten.flags, flags);
        assert_eq!(rewritten.gid.value, 3);
        assert_eq!(
            &bytes[record.gid.bytes.end..],
            &out[rewritten.gid.bytes.end..]
        );
        for len in 0..bytes.len() {
            assert!(Record::read(&table, &bytes[..len]).is_err());
        }
        plan.subset_flags = SubsetFlags::SUBSET_FLAGS_RETAIN_GIDS;
        assert_eq!(record.compile(&bytes, &plan, &maps).unwrap(), bytes);
    }

    #[test]
    fn packed_axis_values_reject_short_and_oversized_runs() {
        for (bytes, count) in [
            (&[0x41, 0, 1][..], 2),
            (&[0x81][..], 1),
            (&[0xc0, 0, 0, 0][..], 1),
        ] {
            assert!(Cursor { bytes, pos: 0 }.packed_values(Some(count)).is_err());
        }
        assert_eq!(
            Cursor {
                bytes: &[0xc0, 0, 0, 0, 1, 0x81],
                pos: 0
            }
            .packed_values(None)
            .unwrap(),
            3
        );
    }

    #[test]
    fn null_conditions_and_nested_variation_indices() {
        assert!(own_condition(FontData::new(&[0, 0]), 64, &mut 100).is_err());
        let c = crate::conditions::at_offset(FontData::new(&[]), 0, 64, &mut 100).unwrap();
        assert_eq!(c, Condition::format_2_variable_value(1, NO_VARIATION));
        let condition = Condition::format_3_and(
            2,
            vec![
                Condition::format_2_variable_value(100, 0x20003),
                Condition::format_5_negate(Condition::format_4_or(
                    1,
                    vec![Condition::format_2_variable_value(-100, 0x30004)],
                )),
            ],
        );
        let bytes = write_fonts::dump_table(&condition).unwrap();
        assert!(own_condition(FontData::new(&bytes), 2, &mut 100).is_err());
        let mut owned = own_condition(FontData::new(&bytes), 64, &mut 100).unwrap();
        let mut used = BTreeSet::new();
        collect_condition_vars(&owned, &mut used);
        assert_eq!(used, [0x20003, 0x30004].into_iter().collect());
        let map = [(0x20003, 0), (0x30004, 0x10000)].into_iter().collect();
        remap_condition(&mut owned, &map, None).unwrap();
        used.clear();
        collect_condition_vars(&owned, &mut used);
        assert_eq!(used, [0, 0x10000].into_iter().collect());
    }
}
