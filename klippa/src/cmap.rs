//! impl subset() for cmap table
use std::cmp::Ordering;

use crate::{
    serialize::{ObjIdx, OffsetWhence, SerializeErrorFlags, Serializer},
    CollectUnicodes, Plan, Subset,
    SubsetError::{self, SubsetTableError},
};

use fnv::FnvHashMap;
use skrifa::raw::tables::cmap::UnicodeRange;
use write_fonts::{
    read::{
        collections::IntSet,
        tables::cmap::{
            Cmap, Cmap0, Cmap10, Cmap12, Cmap13, Cmap14, Cmap4, Cmap6, CmapSubtable, DefaultUvs,
            EncodingRecord, NonDefaultUvs, PlatformId, SequentialMapGroup, UvsMapping,
            VariationSelector,
        },
        types::{FixedSize, GlyphId},
        FontRef, TopLevelTable,
    },
    types::{Offset32, Scalar, Uint24},
    FontBuilder,
};

const INVALID_UNICODE_CHAR: u32 = u32::MAX;
const UNICODE_MAX: u32 = 0x10FFFF_u32;
// reference: subset() for cmap table in harfbuzz
// <https://github.com/harfbuzz/harfbuzz/blob/b14def8bb32f32c32f2e2e9e1ce3efef2a242ca0/src/hb-ot-cmap-table.hh#L1920>
impl<'a> Subset for Cmap<'a> {
    fn subset(
        &self,
        plan: &Plan,
        font: &FontRef,
        s: &mut Serializer,
        _builder: &mut FontBuilder,
    ) -> Result<(), SubsetError> {
        let it = self
            .encoding_records()
            .iter()
            .filter(|r| retain_encoding_record_for_subset(r, &self));

        let mut has_unicode_bmp = false;
        let mut has_unicode_usc4 = false;
        let mut has_ms_bmp = false;
        let mut has_ms_usc4 = false;
        let mut has_format12 = false;
        for record in it {
            if record
                .subtable(self.offset_data())
                .is_ok_and(|t| t.format() == 12)
            {
                has_format12 = true;
            }

            if record.platform_id() == PlatformId::Unicode && record.encoding_id() == 3 {
                has_unicode_bmp = true;
            } else if record.platform_id() == PlatformId::Unicode && record.encoding_id() == 4 {
                has_unicode_usc4 = true;
            } else if record.platform_id() == PlatformId::Windows && record.encoding_id() == 1 {
                has_ms_bmp = true;
            } else if record.platform_id() == PlatformId::Windows && record.encoding_id() == 10 {
                has_ms_usc4 = true;
            }
        }

        if !has_format12 && !has_unicode_bmp && !has_ms_bmp {
            return Err(SubsetTableError(Cmap::TAG));
        }

        if has_format12 && (!has_unicode_usc4 && !has_ms_usc4) {
            return Err(SubsetTableError(Cmap::TAG));
        }

        serialize(self, s, plan)
    }
}

impl<'a> CollectUnicodes for CmapSubtable<'a> {
    fn collect_unicodes(&self, num_glyphs: usize, out: &mut IntSet<u32>) {
        match self {
            Self::Format0(item) => item.collect_unicodes(num_glyphs, out),
            Self::Format4(item) => item.collect_unicodes(num_glyphs, out),
            Self::Format6(item) => item.collect_unicodes(num_glyphs, out),
            Self::Format10(item) => item.collect_unicodes(num_glyphs, out),
            Self::Format12(item) => item.collect_unicodes(num_glyphs, out),
            Self::Format13(item) => item.collect_unicodes(num_glyphs, out),
            _ => return,
        }
    }
}

impl<'a> CollectUnicodes for Cmap0<'a> {
    fn collect_unicodes(&self, _num_glyphs: usize, out: &mut IntSet<u32>) {
        for i in 0..256_u32 {
            if *self.glyph_id_array().get(i as usize).unwrap() != 0 {
                out.insert(i);
            }
        }
    }
}

impl<'a> CollectUnicodes for Cmap4<'a> {
    fn collect_unicodes(&self, _num_glyphs: usize, out: &mut IntSet<u32>) {
        let id_deltas = self.id_delta();
        let seg_count = self.seg_count_x2() / 2;
        let glyph_id_array = self.glyph_id_array();
        for (i, ((start, end), range_offset)) in self
            .start_code()
            .iter()
            .zip(self.end_code())
            .zip(self.id_range_offsets())
            .enumerate()
        {
            let start = start.get() as u32;
            if start == 0xFFFF {
                break;
            }

            let end = end.get() as u32;
            let range_offset = range_offset.get() as u32;
            out.insert_range(start..=end);
            if range_offset == 0 {
                for cp in start..=end {
                    let gid = (cp as u16).wrapping_add_signed(id_deltas[i].get());
                    if gid == 0 {
                        out.remove(cp);
                    }
                }
            } else {
                for cp in start..=end {
                    let index = range_offset / 2 + (cp - start) + i as u32 - seg_count as u32;
                    if index as usize >= glyph_id_array.len() {
                        out.remove_range(cp..=end);
                        break;
                    }
                    let Some(gid) = glyph_id_array.get(index as usize) else {
                        out.remove(cp);
                        continue;
                    };

                    if gid.get() == 0 {
                        out.remove(cp);
                    }
                }
            }
        }
    }
}

impl<'a> CollectUnicodes for Cmap6<'a> {
    fn collect_unicodes(&self, _num_glyphs: usize, out: &mut IntSet<u32>) {
        let start = self.first_code();
        let count = self.entry_count();
        for i in 0..count {
            if *self.glyph_id_array().get(i as usize).unwrap() != 0 {
                out.insert((start + i) as u32);
            }
        }
    }
}

impl<'a> CollectUnicodes for Cmap10<'a> {
    fn collect_unicodes(&self, _num_glyphs: usize, out: &mut IntSet<u32>) {
        let start = self.start_char_code();
        let count = self.num_chars();
        for i in 0..count {
            let Some(gid) = self.glyph_id_array().get(i as usize) else {
                break;
            };
            if gid.get() != 0 {
                out.insert(start + i);
            }
        }
    }
}

impl<'a> CollectUnicodes for Cmap12<'a> {
    fn collect_unicodes(&self, num_glyphs: usize, out: &mut IntSet<u32>) {
        for group in self.groups() {
            let mut start = group.start_char_code();
            let mut end = group.end_char_code().min(UNICODE_MAX);
            let mut gid = group.start_glyph_id();
            if gid == 0 {
                start += 1;
                gid += 1;
            }

            if gid as usize >= num_glyphs {
                continue;
            }

            if (gid + end - start) as usize >= num_glyphs {
                end = UNICODE_MAX.min(start + num_glyphs as u32 - gid);
            }
            out.insert_range(start..=end);
        }
    }
}

impl<'a> CollectUnicodes for Cmap13<'a> {
    fn collect_unicodes(&self, num_glyphs: usize, out: &mut IntSet<u32>) {
        for group in self.groups() {
            let start = group.start_char_code();
            let mut end = group.end_char_code().min(UNICODE_MAX);
            let gid = group.glyph_id();
            if gid == 0 {
                continue;
            }

            if gid as usize >= num_glyphs {
                continue;
            }

            if (gid + end - start) as usize >= num_glyphs {
                end = UNICODE_MAX.min(start + num_glyphs as u32 - gid);
            }
            out.insert_range(start..=end);
        }
    }
}

impl<'a> CollectUnicodes for Cmap14<'a> {
    fn collect_unicodes(&self, _num_glyphs: usize, out: &mut IntSet<u32>) {
        for selector in self.var_selector() {
            if let Some(default_uvs) = selector
                .default_uvs(self.offset_data())
                .transpose()
                .ok()
                .flatten()
            {
                default_uvs.collect_unicodes(_num_glyphs, out);
            }

            if let Some(non_default_uvs) = selector
                .non_default_uvs(self.offset_data())
                .transpose()
                .ok()
                .flatten()
            {
                non_default_uvs.collect_unicodes(_num_glyphs, out);
            }
        }
    }
}

impl<'a> CollectUnicodes for DefaultUvs<'a> {
    fn collect_unicodes(&self, _num_glyphs: usize, out: &mut IntSet<u32>) {
        for range in self.ranges() {
            let first = range.start_unicode_value().to_u32();
            let end = UNICODE_MAX.min(first + range.additional_count() as u32);
            out.insert_range(first..=end);
        }
    }
}

impl<'a> CollectUnicodes for NonDefaultUvs<'a> {
    fn collect_unicodes(&self, _num_glyphs: usize, out: &mut IntSet<u32>) {
        for uvs_mapping in self.uvs_mapping() {
            out.insert(uvs_mapping.unicode_value().to_u32());
        }
    }
}

fn retain_encoding_record_for_subset(record: &EncodingRecord, cmap: &Cmap) -> bool {
    (record.platform_id() == PlatformId::Unicode && record.encoding_id() == 3)
        || (record.platform_id() == PlatformId::Unicode && record.encoding_id() == 4)
        || (record.platform_id() == PlatformId::Windows && record.encoding_id() == 1)
        || (record.platform_id() == PlatformId::Windows && record.encoding_id() == 10)
        || record
            .subtable(cmap.offset_data())
            .is_ok_and(|t| t.format() == 14)
}

fn can_drop_format12(
    cmap12_record: &EncodingRecord,
    cmap12_unicodes: &IntSet<u32>,
    cmap: &Cmap,
    unicodes_cache: &mut SubtableUnicodeCache,
    subset_unicodes: &IntSet<u32>,
    num_glyphs: usize,
) -> bool {
    let cmap12_subset_unicodes = IntSet::from_iter(
        subset_unicodes
            .iter()
            .filter(|v| cmap12_unicodes.contains(*v)),
    );
    for cp in cmap12_subset_unicodes.iter() {
        if cp >= 0x10000 {
            return false;
        }
    }

    let (target_platform, target_encoding) =
        if cmap12_record.platform_id() == PlatformId::Unicode && cmap12_record.encoding_id() == 4 {
            (PlatformId::Unicode, 3_u16)
        } else if cmap12_record.platform_id() == PlatformId::Windows
            && cmap12_record.encoding_id() == 10
        {
            (PlatformId::Windows, 1)
        } else {
            return false;
        };

    let target_language = cmap12_record
        .subtable(cmap.offset_data())
        .unwrap()
        .language();

    for (rec_idx, rec) in cmap
        .encoding_records()
        .iter()
        .enumerate()
        .filter(|(_, r)| retain_encoding_record_for_subset(r, cmap))
    {
        let Ok(subtable) = rec.subtable(cmap.offset_data()) else {
            continue;
        };
        if rec.platform_id() != target_platform
            || rec.encoding_id() != target_encoding
            || subtable.language() != target_language
        {
            continue;
        }

        let Some(sibling_unicodes) = unicodes_cache.set_for(rec_idx, &subtable, num_glyphs) else {
            continue;
        };

        return cmap12_subset_unicodes.iter().cmp(
            subset_unicodes
                .iter()
                .filter(|v| sibling_unicodes.contains(*v)),
        ) == Ordering::Equal;
    }
    false
}

fn serialize(
    cmap: &Cmap,
    s: &mut Serializer,
    plan: &Plan,
    drop_format_4: bool,
) -> Result<(), SubsetError> {
    // allocate header: version + numTables
    s.allocate_size(4)
        .map_err(|_| SubsetError::SubsetTableError(Cmap::TAG))?;

    let snap = s.snapshot();
    //TODO: add support for cmap_cache in plan accelerator
    let unicodes_cache = SubtableUnicodeCache::new(cmap.offset_data().as_bytes().as_ptr() as usize);
    for (rec_idx, record) in cmap
        .encoding_records()
        .iter()
        .enumerate()
        .filter(|(_, r)| retain_encoding_record_for_subset(r, cmap))
    {
        if s.in_error() {
            return Err(SubsetError::SubsetTableError(Cmap::TAG));
        }

        let Ok(subtable) = record.subtable(cmap.offset_data()) else {
            continue;
        };

        let format = subtable.format();
        if format != 4 && format != 12 && format != 14 {
            continue;
        }

        let Some(unicodes_set) = unicodes_cache.set_for(rec_idx, &subtable, plan.font_num_glyphs)
        else {
            continue;
        };
        if !drop_format_4 && format == 4 {
            s.copy();
            if s.in_error() && s.only_overflow() {
                // cmap4 overflowed, reset and retry serialization without format 4 subtables.
                s.revert_snapshot(snap);
                serialize(cmap, s, plan, true);
            }
        } else if format == 12 {
            if can_drop_format12(
                record,
                unicodes_set,
                cmap,
                &mut unicodes_cache,
                &plan.unicodes,
                plan.font_num_glyphs,
            ) {
                continue;
            }
            s.copy();
        } else if format == 14 {
            s.copy();
        }
    }

    let num_retained_records = (s.length() - 4) / EncodingRecord::RAW_BYTE_LEN;
    s.check_assign::<u16>(
        2,
        num_retained_records,
        SerializeErrorFlags::SERIALIZE_ERROR_INT_OVERFLOW,
    );

    // Fail if format 4 was dropped and there is no cmap12.
    let ret = !drop_format_4 || format12objidx.is_some();

    Ok(())
}

fn copy_encoding_record(
    record: &EncodingRecord,
    s: &mut Serializer,
) -> Result<ObjIdx, SerializeErrorFlags> {
    let snap = s.snapshot();
    s.embed(record.platform_id())?;
    s.embed(record.encoding_id())?;
    let offset_pos = s.length();
    s.embed(Offset32::new(0))?;

    s.push();
    let init_len = s.length();
    subtable.serialize();
    let mut obj_idx = None;
    if s.length() > init_len {
        obj_idx = s.pop_pack(true);
    } else {
        s.pop_discard();
    }

    if obj_idx.is_none() {
        s.revert_snapshot(snap);
        return Err(s.error());
    }

    let obj_idx = obj_idx.unwrap();
    s.add_link(
        offset_pos..offset_pos + 4,
        obj_idx,
        OffsetWhence::Head,
        0,
        false,
    );
    Ok(obj_idx)
}

fn is_gid_consecutive(
    end_char_code: u32,
    start_char_code: u32,
    gid: GlyphId,
    cp: u32,
    new_gid: GlyphId,
) -> bool {
    cp - 1 == end_char_code && new_gid.to_u32() == gid.to_u32() + (cp - start_char_code)
}

fn serialize_cmap12(
    s: &mut Serializer,
    cmap12: &Cmap12,
    it: impl Iterator<Item = (u32, GlyphId)>,
) -> Result<(), SerializeErrorFlags> {
    let init_pos = s.length();
    //copy header format
    s.embed(cmap12.format())?;
    // reserved
    s.embed(0_u16)?;
    // length, initialized to 0, update later
    let length_pos = s.embed(0_u32)?;
    // language
    s.embed(cmap12.language())?;
    // numGroups: set to 0 initally
    let num_groups_pos = s.embed(0_u32)?;

    let mut start_char_code = INVALID_UNICODE_CHAR;
    let mut end_char_code = INVALID_UNICODE_CHAR;
    let mut glyph_id = GlyphId::NOTDEF;

    for (cp, gid) in it {
        if start_char_code == INVALID_UNICODE_CHAR {
            start_char_code = cp;
            end_char_code = cp;
            glyph_id = gid;
        } else if !is_gid_consecutive(end_char_code, start_char_code, glyph_id, cp, gid) {
            s.embed(start_char_code)?;
            s.embed(end_char_code)?;
            s.embed(glyph_id.to_u32())?;

            start_char_code = cp;
            end_char_code = cp;
            glyph_id = gid;
        } else {
            end_char_code = cp;
        }

        s.embed(start_char_code)?;
        s.embed(end_char_code)?;
        s.embed(glyph_id.to_u32())?;

        // update length
        s.check_assign::<u32>(
            length_pos,
            s.length() - init_pos,
            SerializeErrorFlags::SERIALIZE_ERROR_INT_OVERFLOW,
        );

        // header size = 16
        s.check_assign::<u32>(
            num_groups_pos,
            (s.length() - init_pos - 16) / SequentialMapGroup::RAW_BYTE_LEN,
            SerializeErrorFlags::SERIALIZE_ERROR_INT_OVERFLOW,
        );
    }
    Ok(())
}

// reference: <https://github.com/qxliu76/harfbuzz/blob/1c249be96e27eafd15eb86d832b67fbc3751634b/src/hb-ot-cmap-table.hh#L1369>
// reason why we
fn serialize_cmap14(
    s: &mut Serializer,
    cmap14: &Cmap14,
    plan: &Plan,
) -> Result<(), SerializeErrorFlags> {
    let snap = s.snapshot();
    let init_len = s.length();
    let init_tail = s.tail();
    //copy header format
    s.embed(cmap14.format())?;
    // length, initialized to 0, update later
    let length_pos = s.embed(0_u32)?;
    // numVarSelectorRecords, initialized to 0, update later
    let num_records_pos = s.embed(0_u32)?;

    let mut obj_indices = Vec::with_capacity(cmap14.num_var_selector_records() as usize);
    // serializer UVS tables for each variation selctor record in reverse order
    // see here for reason: <https://github.com/harfbuzz/harfbuzz/blob/40ef6c05775885241dd3f4d69f08fa4e7e1e451c/src/hb-ot-cmap-table.hh#L1385>
    for record in cmap14
        .var_selector()
        .iter()
        .rev()
        .filter(|r| plan.unicodes.contains(r.var_selector().to_u32()))
    {
        obj_indices.push(copy_var_selector_record_uvs_tables(record, cmap14, s, plan));
    }

    let offset_pos = Vec::with_capacity(obj_indices.len());
    // copy variation selector headers
    for (record, (default_uvs_obj_idx, non_default_uvs_obj_idx)) in cmap14
        .var_selector()
        .iter()
        .filter(|r| plan.unicodes.contains(r.var_selector().to_u32()))
        .zip(obj_indices.iter().rev())
        .filter(|(_, (a_idx, b_idx))| a_idx.is_some() || b_idx.is_some())
    {
        let (default_pos, non_default_pos) = copy_var_selector_record_header(record, s)?;
        offset_pos.push((default_pos, non_default_pos));
    }

    // subsetted to empty, return
    // 10 is header size of Cmap14
    if s.length() - init_len == 10 {
        s.revert_snapshot(snap);
        return Ok(());
    }

    let tail_len = s.tail() - init_tail;
    s.check_assign::<u32>(
        length_pos,
        s.length() - init_len + tail_len,
        SerializeErrorFlags::SERIALIZE_ERROR_INT_OVERFLOW,
    );
    let num_records = (s.length() - init_len - 10) / VariationSelector::RAW_BYTE_LEN;
    s.check_assign::<u32>(
        num_records_pos,
        num_records,
        SerializeErrorFlags::SERIALIZE_ERROR_INT_OVERFLOW,
    );

    add_links_to_variation_records();
    Ok(())
}

fn copy_var_selector_record_header(
    record: &VariationSelector,
    s: &mut Serializer,
) -> Result<(usize, usize), SerializeErrorFlags> {
    //copy var_selector
    s.embed(record.var_selector())?;
    // offset to default UVS, initialized to 0
    let default_pos = s.embed(0_u32)?;
    // offset to non-default UVS, initialized to 0
    let non_default_pos = s.embed(0_u32)?;
    Ok((default_pos, non_default_pos))
}

fn copy_var_selector_record_uvs_tables(
    record: &VariationSelector,
    cmap14: &Cmap14,
    s: &mut Serializer,
    plan: &Plan,
) -> (Option<ObjIdx>, Option<ObjIdx>) {
    let mut non_default_uvs_obj_idx = None;
    if let Some(non_default_uvs) = record
        .non_default_uvs(cmap14.offset_data())
        .transpose()
        .ok()
        .flatten()
    {
        s.push();
        if let Ok(num) = copy_non_default_uvs(&non_default_uvs, s, plan) {
            if num == 0 {
                s.pop_discard();
            } else {
                non_default_uvs_obj_idx = s.pop_pack(true);
            }
        } else {
            s.pop_discard();
        }
    }

    let mut default_uvs_obj_idx = None;
    if let Some(default_uvs) = record
        .default_uvs(cmap14.offset_data())
        .transpose()
        .ok()
        .flatten()
    {
        s.push();
        if let Ok(num) = copy_default_uvs(&default_uvs, s, plan) {
            if num == 0 {
                s.pop_discard();
            } else {
                default_uvs_obj_idx = s.pop_pack(true);
            }
        } else {
            s.pop_discard();
        }
    }
    (default_uvs_obj_idx, non_default_uvs_obj_idx)
}

fn copy_non_default_uvs(
    non_default_uvs: &NonDefaultUvs,
    s: &mut Serializer,
    plan: &Plan,
) -> Result<u32, SerializeErrorFlags> {
    // num_uvs_mapping, initialized to 0
    s.embed(0_u32)?;
    let mut num: u32 = 0;
    for uvs_mapping in non_default_uvs.uvs_mapping().iter() {
        if !plan.unicodes.contains(uvs_mapping.unicode_value().to_u32())
            && !plan
                .glyphs_requested
                .contains(GlyphId::from(uvs_mapping.glyph_id()))
        {
            continue;
        }
        copy_uvs_mapping(uvs_mapping, s, plan)?;
        num += 1;
    }
    if num == 0 {
        return Ok(num);
    }
    s.copy_assign(0, num);
    Ok(num)
}

fn copy_uvs_mapping(
    uvs_mapping: &UvsMapping,
    s: &mut Serializer,
    plan: &Plan,
) -> Result<(), SerializeErrorFlags> {
    s.embed(uvs_mapping.unicode_value())?;
    let glyph_id = plan
        .glyph_map
        .get(&GlyphId::from(uvs_mapping.glyph_id()))
        .unwrap();
    s.embed(glyph_id.to_u32() as u16)?;
    Ok(())
}

fn copy_default_uvs(
    default_uvs: &DefaultUvs,
    s: &mut Serializer,
    plan: &Plan,
) -> Result<u32, SerializeErrorFlags> {
    let snap = s.snapshot();
    // numUnicodeValueRanges, initialized to 0
    let len_pos = s.embed(0_u32)?;

    let init_len = s.length();
    let org_num_range = default_uvs.num_unicode_value_ranges() as usize;
    let num_bits = size_of::<u32>() - org_num_range.leading_zeros() as usize;
    let org_unicode_ranges = default_uvs.ranges();
    if org_num_range > plan.unicodes.len() as usize * num_bits {
        let mut start = INVALID_UNICODE_CHAR;
        let mut end = INVALID_UNICODE_CHAR;

        for u in plan.unicodes.iter() {
            if org_unicode_ranges
                .binary_search_by(|r| r.start_unicode_value().to_u32().cmp(&u))
                .is_err()
            {
                continue;
            }

            if start == INVALID_UNICODE_CHAR {
                start = u;
                end = start - 1;
            }

            if end + 1 != u || end - start == 255 {
                s.embed(Uint24::new(start))?;
                s.embed((end - start) as u8)?;
                start = u;
            }
            end = u;
        }

        if start != INVALID_UNICODE_CHAR {
            s.embed(Uint24::new(start))?;
            s.embed((end - start) as u8)?;
        }
    } else {
        let mut last_code = INVALID_UNICODE_CHAR;
        let mut count = 0_u8;

        for unicode_range in default_uvs.ranges() {
            let cur_entry = unicode_range.start_unicode_value().to_u32() - 1;
            let end = cur_entry + unicode_range.additional_count() as u32 + 2;

            while let Some(cur_entry) = plan.unicodes.iter_after(cur_entry).next() {
                if cur_entry >= end {
                    break;
                }

                if last_code == INVALID_UNICODE_CHAR {
                    last_code = cur_entry;
                    continue;
                }

                if last_code + count as u32 != cur_entry {
                    s.embed(Uint24::new(last_code))?;
                    s.embed(count)?;

                    last_code = cur_entry;
                    count = 0;
                    continue;
                }
                count += 1;
            }
        }
        if last_code != INVALID_UNICODE_CHAR {
            s.embed(Uint24::new(last_code))?;
            s.embed(count)?;
        }
    }

    // return if subsetted to empty
    if s.length() == init_len {
        s.revert_snapshot(snap);
        return Ok(0);
    }

    let num_ranges = (s.length() - init_len) / UnicodeRange::RAW_BYTE_LEN;
    s.check_assign::<u32>(
        len_pos,
        num_ranges,
        SerializeErrorFlags::SERIALIZE_ERROR_INT_OVERFLOW,
    );
    Ok(num_ranges as u32)
}

pub(crate) struct SubtableUnicodeCache {
    base: usize,
    cached_unicodes: FnvHashMap<usize, IntSet<u32>>,
}

impl SubtableUnicodeCache {
    fn new(base: usize) -> Self {
        Self {
            base: base,
            cached_unicodes: FnvHashMap::default(),
        }
    }

    fn set_for(
        &mut self,
        record_idx: usize,
        cmap_subtable: &CmapSubtable,
        num_glyphs: usize,
    ) -> Option<&IntSet<u32>> {
        if !self.cached_unicodes.contains_key(&record_idx) {
            let mut subtable_unicodes_set = IntSet::empty();
            cmap_subtable.collect_unicodes(num_glyphs, &mut subtable_unicodes_set);
            self.cached_unicodes
                .insert(record_idx, subtable_unicodes_set);
        }
        self.cached_unicodes.get(&record_idx)
    }
}
