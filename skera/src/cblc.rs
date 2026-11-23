//! impl subset() for CBLC
use crate::{
    layout::OpsWalk,
    serialize::{ObjIdx, OffsetWhence, SerializeErrorFlags, SerializeResultEmpty, Serializer},
    Plan, Subset, SubsetError, SubsetTable,
};
use read_fonts::{
    collections::IntSet,
    tables::{
        bitmap::{
            BitmapSize, IndexSubtable, IndexSubtable1, IndexSubtable3, IndexSubtableList,
            IndexSubtableRecord,
        },
        cbdt::Cbdt,
        cblc::Cblc,
    },
    types::{FixedSize, GlyphId, Offset32},
    FontRef, MinByteRange, TableProvider, TopLevelTable,
};
use write_fonts::FontBuilder;

/// Bytes the CBDT subsetter may copy out of the source CBDT table, as a multiple of that
/// table's length. Every strike may point at the same index subtable list and every record of a
/// list at the same subtable, whose images are copied again on each visit, so bounds-checking
/// each image against the table doesn't bound their sum. Matches HarfBuzz's
/// HB_SUBSET_CBDT_MAX_COPY_FACTOR. Offsets to images in the new table are 32 bits, which bounds
/// it too.
const MAX_CBDT_COPY_FACTOR: usize = 64;

/// State shared by the subsetting of all strikes of a CBLC table.
pub(crate) struct CblcSubsetContext {
    /// The CBDT table being written.
    cbdt_out: Vec<u8>,
    /// What is left of the bytes that may be copied out of the source CBDT table.
    cbdt_bytes_left: usize,
}

impl CblcSubsetContext {
    /// Appends the image data of a glyph to the CBDT table, failing once the bytes copied would
    /// exceed the budget.
    fn copy_image(&mut self, s: &mut Serializer, data: &[u8]) -> Result<(), SerializeErrorFlags> {
        let Some(bytes_left) = self.cbdt_bytes_left.checked_sub(data.len()) else {
            return Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR));
        };
        self.cbdt_bytes_left = bytes_left;
        self.cbdt_out.extend_from_slice(data);
        Ok(())
    }
}

/// Returns the (new gid, old gid) pairs of `retained_glyphs`, the retained glyphs of an index
/// subtable, sorted by new gid like plan.new_to_old_gid_list. This costs what the subtable covers,
/// rather than a scan of all the retained glyphs.
fn new_to_old_gids(plan: &Plan, retained_glyphs: &IntSet<GlyphId>) -> Vec<(GlyphId, GlyphId)> {
    // every retained glyph is in the glyph map, and the filter hides that from collect()
    let mut gids = Vec::with_capacity(retained_glyphs.len() as usize);
    gids.extend(retained_glyphs.iter().filter_map(|old_gid| {
        plan.glyph_map
            .get(&old_gid)
            .map(|new_gid| (*new_gid, old_gid))
    }));
    // only a glyph map that reorders glyphs makes this differ from old gid order
    gids.sort_unstable();
    gids
}

// reference: subset() for CBLC in fonttools, Harfbuzz implementation is suboptimal
// <https://github.com/fonttools/fonttools/blob/7854669acd63be43e1ad41d0486297d8d6da325d/Lib/fontTools/subset/__init__.py#L1799>
impl Subset for Cblc<'_> {
    fn subset(
        &self,
        plan: &Plan,
        font: &FontRef,
        s: &mut Serializer,
        builder: &mut FontBuilder,
    ) -> Result<(), SubsetError> {
        let cbdt = font
            .cbdt()
            .or(Err(SubsetError::SubsetTableError(Cbdt::TAG)))?;

        s.embed(self.major_version())
            .map_err(|_| SubsetError::SubsetTableError(Cblc::TAG))?;
        s.embed(self.minor_version())
            .map_err(|_| SubsetError::SubsetTableError(Cblc::TAG))?;

        let mut num_sizes: u32 = 0;
        let num_sizes_pos = s
            .embed(num_sizes)
            .map_err(|_| SubsetError::SubsetTableError(Cblc::TAG))?;

        let bitmapsize_records = self.bitmap_sizes();
        let bitmapsize_bytes = self
            .offset_data()
            .as_bytes()
            .get(self.bitmap_sizes_byte_range())
            .unwrap();

        // cbdt out
        let mut cbdt_out = Vec::with_capacity(cbdt.offset_data().len());
        // cbdt header
        cbdt_out.extend_from_slice(cbdt.min_table_bytes());

        let cbdt_bytes_left = cbdt
            .offset_data()
            .len()
            .saturating_mul(MAX_CBDT_COPY_FACTOR)
            .min((u32::MAX as usize).saturating_sub(cbdt_out.len()));
        let mut context = CblcSubsetContext {
            cbdt_out,
            cbdt_bytes_left,
        };
        s.set_layout_walk(OpsWalk::new(self.offset_data()));

        for (idx, bitmap_size_table) in bitmapsize_records.iter().enumerate() {
            let start = idx * BitmapSize::RAW_BYTE_LEN;
            let src_bytes = bitmapsize_bytes
                .get(start..start + BitmapSize::RAW_BYTE_LEN)
                .unwrap();
            if !bitmap_size_table
                .subset(plan, s, (self, &cbdt, src_bytes, &mut context))
                .is_empty()
                .map_err(|_| SubsetError::SubsetTableError(Cblc::TAG))?
            {
                num_sizes += 1;
            }
        }

        s.copy_assign(num_sizes_pos, num_sizes);
        builder.add_raw(Cbdt::TAG, context.cbdt_out);
        Ok(())
    }
}

/// Returns true when subsetting would copy the bitmap tables without changing them.
///
/// Keep this deliberately conservative. In particular, multiple subtables or empty bitmap
/// ranges may require repacking even when the glyph-id map itself is the identity map.
pub(crate) fn can_passthrough_bitmap_tables(cblc: &Cblc, cbdt: &Cbdt, plan: &Plan) -> bool {
    if !plan.has_identity_glyph_map() {
        return false;
    }

    let [bitmap_size] = cblc.bitmap_sizes() else {
        return false;
    };
    if bitmap_size.number_of_index_subtables() != 1
        || bitmap_size.index_subtable_list_offset() as usize != cblc.bitmap_sizes_byte_range().end
        || bitmap_size.index_subtable_list_offset() as usize
            + bitmap_size.index_subtable_list_size() as usize
            != cblc.offset_data().len()
    {
        return false;
    }

    let Ok(index_subtable_list) = bitmap_size.index_subtable_list(cblc.offset_data()) else {
        return false;
    };
    let [record] = index_subtable_list.index_subtable_records() else {
        return false;
    };
    if bitmap_size.start_glyph_index() != record.first_glyph_index()
        || bitmap_size.end_glyph_index() != record.last_glyph_index()
    {
        return false;
    }

    let Ok(subtable) = record.index_subtable(index_subtable_list.offset_data()) else {
        return false;
    };
    if IndexSubtableRecord::RAW_BYTE_LEN + subtable.min_table_bytes().len()
        != bitmap_size.index_subtable_list_size() as usize
    {
        return false;
    }

    let cbdt_header_len = cbdt.min_table_bytes().len();
    let cbdt_len = cbdt.offset_data().len();
    match subtable {
        IndexSubtable::Format1(subtable) => {
            let offsets = subtable.sbit_offsets();
            subtable.image_data_offset() as usize == cbdt_header_len
                && offsets.first().is_some_and(|offset| offset.get() == 0)
                && offsets.windows(2).all(|pair| pair[0].get() < pair[1].get())
                && offsets.last().is_some_and(|offset| {
                    subtable.image_data_offset() as usize + offset.get() as usize == cbdt_len
                })
        }
        IndexSubtable::Format3(subtable) => {
            let offsets = subtable.sbit_offsets();
            subtable.image_data_offset() as usize == cbdt_header_len
                && offsets.first().is_some_and(|offset| offset.get() == 0)
                && offsets.windows(2).all(|pair| pair[0].get() < pair[1].get())
                && offsets.last().is_some_and(|offset| {
                    subtable.image_data_offset() as usize + offset.get() as usize == cbdt_len
                })
        }
        _ => false,
    }
}

impl<'a> SubsetTable<'a> for BitmapSize {
    // (Cblc table, CBDT table, src_bitmapsize_bytes, context)
    type ArgsForSubset = (
        &'a Cblc<'a>,
        &'a Cbdt<'a>,
        &'a [u8],
        &'a mut CblcSubsetContext,
    );
    type Output = ();
    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        args: Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        let (cblc, cbdt, src_bytes, context) = args;

        if self.start_glyph_index() > plan.glyphset.last().unwrap()
            || self.end_glyph_index() < plan.glyphset.first().unwrap()
            || self.index_subtable_list_offset() == 0
            || self.index_subtable_list_size() == 0
        {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY);
        }

        let snap = s.snapshot();
        let offset_pos = s.embed_bytes(src_bytes)?;
        let Ok(index_subtable_list) = self.index_subtable_list(cblc.offset_data()) else {
            return Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR));
        };

        s.push()?;
        match index_subtable_list.subset(plan, s, (cbdt, context)) {
            Ok((min_gid, max_gid, table_list_size, num_subtables)) => {
                let Some(obj_idx) = s.pop_pack(true) else {
                    return Err(s.error());
                };
                let _ = s.add_link(
                    offset_pos..offset_pos + Offset32::RAW_BYTE_LEN,
                    obj_idx,
                    OffsetWhence::Head,
                    0,
                    false,
                );

                //update table list size, biye pos = 4
                s.copy_assign(offset_pos + 4, table_list_size as u32);
                // update number of index subtable, byte_pos = 4
                s.copy_assign(offset_pos + 8, num_subtables as u32);

                // startGlyphIndex, byte_pos = 40
                s.copy_assign(offset_pos + 40, min_gid.to_u32() as u16);

                // endGlyphIndex, byte_pos = 42
                s.copy_assign(offset_pos + 42, max_gid.to_u32() as u16);
            }
            Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY) => {
                s.pop_discard();
                s.revert_snapshot(snap);
                return Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY);
            }
            Err(e) => {
                return Err(e);
            }
        }
        Ok(())
    }
}

impl<'a> SubsetTable<'a> for IndexSubtableList<'a> {
    // (CBDT table, context)
    type ArgsForSubset = (&'a Cbdt<'a>, &'a mut CblcSubsetContext);
    // min_gid(new), max_gid(new), indexSubtableListSize, numberOfIndexSubtables
    type Output = (GlyphId, GlyphId, usize, usize);

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        args: Self::ArgsForSubset,
    ) -> Result<Self::Output, SerializeErrorFlags> {
        let (cbdt, context) = args;
        // every strike may point at this list, so it is charged on each visit
        s.visit_table(self)?;

        let records = self.index_subtable_records();
        let src_num_records = records.len();

        let mut obj_idxes = Vec::with_capacity(src_num_records);
        let mut table_list_size = 0;
        let init_len = s.length();
        // serialize subtables in reverse order
        for idx in (0..src_num_records).rev() {
            let record = records[idx];
            if record.index_subtable_offset().is_null() {
                continue;
            }
            let Ok(subtable) = record.index_subtable(self.offset_data()) else {
                return Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR));
            };
            // and every record may point at the same subtable
            s.visit_table(&subtable)?;
            s.push()?;
            match subset_index_subtable(&subtable, plan, s, &record, cbdt, context) {
                Ok((start_gid, end_gid, table_size)) => {
                    let Some(obj_idx) = s.pop_pack(true) else {
                        return Err(s.error());
                    };
                    obj_idxes.push((idx, obj_idx, start_gid, end_gid));
                    table_list_size += table_size;
                }
                Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY) => {
                    s.pop_discard();
                    continue;
                }
                Err(e) => {
                    return Err(e);
                }
            }
        }

        if obj_idxes.is_empty() {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY);
        }

        let mut min_start_gid = GlyphId::from(u16::MAX);
        let mut max_end_gid = GlyphId::NOTDEF;
        for (record_idx, objidx, start_gid, end_gid) in obj_idxes.iter().rev() {
            let record = records[*record_idx];
            record.subset(plan, s, (*objidx, *start_gid, *end_gid))?;

            min_start_gid = min_start_gid.min(*start_gid);
            max_end_gid = max_end_gid.max(*end_gid);
        }

        table_list_size += s.length() - init_len;
        Ok((min_start_gid, max_end_gid, table_list_size, obj_idxes.len()))
    }
}

impl SubsetTable<'_> for IndexSubtableRecord {
    // (obj_idx, first_glyph_id(new), last_glyph_id(new))
    type ArgsForSubset = (ObjIdx, GlyphId, GlyphId);
    type Output = ();
    fn subset(
        &self,
        _plan: &Plan,
        s: &mut Serializer,
        args: Self::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        s.embed(args.1.to_u32() as u16)?;
        s.embed(args.2.to_u32() as u16)?;

        let offset_pos = s.embed(0_u32)?;
        let _ = s.add_link(
            offset_pos..offset_pos + 4,
            args.0,
            OffsetWhence::Head,
            0,
            false,
        );
        Ok(())
    }
}

fn subset_index_subtable(
    table: &IndexSubtable,
    plan: &Plan,
    s: &mut Serializer,
    index_subtable_record: &IndexSubtableRecord,
    src_cbdt: &Cbdt,
    context: &mut CblcSubsetContext,
) -> Result<(GlyphId, GlyphId, usize), SerializeErrorFlags> {
    let args = (index_subtable_record, src_cbdt, context);
    match table {
        IndexSubtable::Format1(item) => item.subset(plan, s, args),
        IndexSubtable::Format3(item) => item.subset(plan, s, args),
        //TODO: support format 2/4/5?
        _ => Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY),
    }
}

impl<'a> SubsetTable<'a> for IndexSubtable1<'a> {
    // (src record, src CBDT, context)
    type ArgsForSubset = (
        &'a IndexSubtableRecord,
        &'a Cbdt<'a>,
        &'a mut CblcSubsetContext,
    );

    // output:(first_gid, end_gid, size of subsetted table)
    type Output = (GlyphId, GlyphId, usize);

    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        args: Self::ArgsForSubset,
    ) -> Result<(GlyphId, GlyphId, usize), SerializeErrorFlags> {
        let (src_record, src_cbdt, context) = args;
        let src_gid_min = src_record.first_glyph_index();
        let src_gid_max = src_record.last_glyph_index();

        let mut retained_glyphs = IntSet::empty();
        retained_glyphs.insert_range(GlyphId::from(src_gid_min)..=GlyphId::from(src_gid_max));
        retained_glyphs.intersect(&plan.glyphset);

        if retained_glyphs.is_empty() {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY);
        }

        let src_offsets = self.sbit_offsets();
        let src_gid_min = src_gid_min.to_u32() as usize;
        // find the last glyph that has image_data
        let mut end_glyph = None;
        for gid in retained_glyphs.iter().rev() {
            let idx = gid.to_u32() as usize - src_gid_min;
            let offset_start = src_offsets[idx].get();
            let offset_end = src_offsets[idx + 1].get();

            if offset_end > offset_start {
                end_glyph = Some(gid);
                break;
            }
        }

        if end_glyph.is_none() {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY);
        }

        let init_len = s.length();
        s.embed(self.index_format())?;
        s.embed(self.image_format())?;

        let image_offset = context.cbdt_out.len() as u32;
        s.embed(image_offset)?;

        let mut start_glyph = None;

        let mut cur_offset = 0_u32;
        let mut last = 0;

        for (new_gid, old_gid) in new_to_old_gids(plan, &retained_glyphs).iter() {
            let idx = old_gid.to_u32() as usize - src_gid_min;
            let offset_start = src_offsets[idx].get();
            let offset_end = src_offsets[idx + 1].get();

            // for retain-gids
            if start_glyph.is_some() {
                while last < new_gid.to_u32() {
                    s.embed(cur_offset)?;
                    last += 1;
                }
            }

            if offset_end <= offset_start {
                if start_glyph.is_none() {
                    continue;
                } else {
                    // add skip glyph that has no image data
                    s.embed(cur_offset)?;
                }
            } else {
                if start_glyph.is_none() {
                    start_glyph = Some(*new_gid);
                    last = new_gid.to_u32();
                }
                //copy glyph image data into cbdt_out
                let src_glyph_offset = (self.image_data_offset() + offset_start) as usize;
                let len = offset_end - offset_start;

                let Some(glyph_data) = src_cbdt
                    .offset_data()
                    .as_bytes()
                    .get(src_glyph_offset..src_glyph_offset + len as usize)
                else {
                    return Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR));
                };

                context.copy_image(s, glyph_data)?;
                // add offset for glyph
                s.embed(cur_offset)?;
                // the new CBDT table fits 32 bit offsets
                cur_offset = cur_offset
                    .checked_add(len)
                    .ok_or_else(|| s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR))?;
            }
            // Skip over gid
            last += 1;

            if *old_gid == end_glyph.unwrap() {
                end_glyph = Some(*new_gid);
                s.embed(cur_offset)?;
                break;
            }
        }

        let len = s.length() - init_len;
        Ok((start_glyph.unwrap(), end_glyph.unwrap(), len))
    }
}

impl<'a> SubsetTable<'a> for IndexSubtable3<'a> {
    // (src record, src CBDT, context)
    type ArgsForSubset = (
        &'a IndexSubtableRecord,
        &'a Cbdt<'a>,
        &'a mut CblcSubsetContext,
    );

    // output:(first_gid, end_gid, size of subsetted table)
    type Output = (GlyphId, GlyphId, usize);
    fn subset(
        &self,
        plan: &Plan,
        s: &mut Serializer,
        args: Self::ArgsForSubset,
    ) -> Result<(GlyphId, GlyphId, usize), SerializeErrorFlags> {
        let (src_record, src_cbdt, context) = args;
        let src_gid_min = src_record.first_glyph_index();
        let src_gid_max = src_record.last_glyph_index();

        let mut retained_glyphs = IntSet::empty();
        retained_glyphs.insert_range(GlyphId::from(src_gid_min)..=GlyphId::from(src_gid_max));
        retained_glyphs.intersect(&plan.glyphset);

        if retained_glyphs.is_empty() {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY);
        }

        let src_offsets = self.sbit_offsets();
        let src_gid_min = src_gid_min.to_u32() as usize;
        // find the last glyph that has image_data
        let mut end_glyph = None;
        for gid in retained_glyphs.iter().rev() {
            let idx = gid.to_u32() as usize - src_gid_min;
            let offset_start = src_offsets[idx].get();
            let offset_end = src_offsets[idx + 1].get();

            if offset_end > offset_start {
                end_glyph = Some(gid);
                break;
            }
        }

        if end_glyph.is_none() {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY);
        }

        let init_len = s.length();
        s.embed(self.index_format())?;
        s.embed(self.image_format())?;

        let image_offset = context.cbdt_out.len() as u32;
        s.embed(image_offset)?;

        let mut start_glyph = None;

        let mut cur_offset = 0_u16;
        let mut num_offsets: u32 = 0;
        let mut last = 0;

        for (new_gid, old_gid) in new_to_old_gids(plan, &retained_glyphs).iter() {
            let idx = old_gid.to_u32() as usize - src_gid_min;
            let offset_start = src_offsets[idx].get();
            let offset_end = src_offsets[idx + 1].get();

            // for retain-gids
            if start_glyph.is_some() {
                while last < new_gid.to_u32() {
                    s.embed(cur_offset)?;
                    num_offsets += 1;
                    last += 1;
                }
            }

            if offset_end <= offset_start {
                if start_glyph.is_none() {
                    continue;
                } else {
                    // add skip glyph that has no image data
                    s.embed(cur_offset)?;
                    num_offsets += 1;
                }
            } else {
                if start_glyph.is_none() {
                    start_glyph = Some(*new_gid);
                    last = new_gid.to_u32();
                }
                //copy glyph image data into cbdt_out
                let src_glyph_offset = self.image_data_offset() as usize + offset_start as usize;
                let len = offset_end - offset_start;

                let Some(glyph_data) = src_cbdt
                    .offset_data()
                    .as_bytes()
                    .get(src_glyph_offset..src_glyph_offset + len as usize)
                else {
                    return Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR));
                };

                context.copy_image(s, glyph_data)?;
                // add offset for glyph
                s.embed(cur_offset)?;
                // Ascending source offsets keep the images of a subtable within 16 bit
                // offsets, but images that overlap can add up to more.
                cur_offset = cur_offset
                    .checked_add(len)
                    .ok_or_else(|| s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR))?;
                num_offsets += 1;
            }

            // Skip over gid
            last += 1;

            if *old_gid == end_glyph.unwrap() {
                end_glyph = Some(*new_gid);
                s.embed(cur_offset)?;
                num_offsets += 1;
                break;
            }
        }
        //pad for 32-bit alignment if needed
        if num_offsets % 2 == 1 {
            s.embed(0_u16)?;
        }

        let len = s.length() - init_len;
        Ok((start_glyph.unwrap(), end_glyph.unwrap(), len))
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn single_subtable_bitmap_font() -> Vec<u8> {
        let mut cblc = Vec::from([
            0x00, 0x03, 0x00, 0x00, // version 3.0
            0x00, 0x00, 0x00, 0x01, // one bitmap-size record
            0x00, 0x00, 0x00, 0x38, // index-subtable list offset
            0x00, 0x00, 0x00, 0x1C, // index-subtable list size
            0x00, 0x00, 0x00, 0x01, // one index subtable
            0x00, 0x00, 0x00, 0x00, // color ref
        ]);
        cblc.extend_from_slice(&[0; 24]); // horizontal and vertical line metrics
        cblc.extend_from_slice(&[
            0x00, 0x01, 0x00, 0x02, // glyph range 1..=2
            0x10, 0x10, 0x20, 0x01, // ppem, bit depth, and flags
            0x00, 0x01, 0x00, 0x02, // index-subtable record glyph range
            0x00, 0x00, 0x00, 0x08, // subtable follows the record
            0x00, 0x01, 0x00, 0x11, // index format 1, image format 17
            0x00, 0x00, 0x00, 0x04, // image data follows the CBDT header
            0x00, 0x00, 0x00, 0x00, // first bitmap offset
            0x00, 0x00, 0x00, 0x02, // second bitmap offset
            0x00, 0x00, 0x00, 0x05, // end of bitmap data
        ]);

        let mut builder = FontBuilder::new();
        builder.add_raw(Cblc::TAG, cblc);
        builder.add_raw(
            Cbdt::TAG,
            [0x00, 0x03, 0x00, 0x00, 10, 11, 12, 13, 14].as_slice(),
        );
        builder.build()
    }

    #[test]
    fn detects_unchanged_bitmap_tables() {
        let font_data = single_subtable_bitmap_font();
        let font = FontRef::new(&font_data).unwrap();
        let cblc = font.cblc().unwrap();
        let cbdt = font.cbdt().unwrap();

        let mut plan = Plan {
            font_num_glyphs: 3,
            num_output_glyphs: 3,
            ..Default::default()
        };
        plan.glyphset
            .insert_range(GlyphId::NOTDEF..=GlyphId::from(2_u32));
        plan.new_to_old_gid_list.extend((0_u32..3).map(|gid| {
            let gid = GlyphId::from(gid);
            (gid, gid)
        }));
        plan.glyph_map.extend(
            plan.new_to_old_gid_list
                .iter()
                .map(|&(new_gid, old_gid)| (old_gid, new_gid)),
        );

        assert!(can_passthrough_bitmap_tables(&cblc, &cbdt, &plan));

        // Verify that the existing subsetter really does reproduce both tables byte-for-byte
        // under the conditions accepted by the fast path.
        let mut output_builder = FontBuilder::new();
        let mut s = Serializer::new(1024).unwrap();
        s.start_serialize().unwrap();
        cblc.subset(&plan, &font, &mut s, &mut output_builder)
            .unwrap();
        s.end_serialize();
        output_builder.add_raw(Cblc::TAG, s.copy_bytes());
        let output_data = output_builder.build();
        let output = FontRef::new(&output_data).unwrap();
        for tag in [Cblc::TAG, Cbdt::TAG] {
            assert_eq!(
                font.data_for_tag(tag).unwrap().as_bytes(),
                output.data_for_tag(tag).unwrap().as_bytes()
            );
        }
    }

    #[test]
    fn test_subset_cbdt_noop() {
        let font = FontRef::new(include_bytes!(
            "../test-data/fonts/NotoColorEmoji.subset.ttf"
        ))
        .unwrap();

        let cblc = font.cblc().unwrap();
        let mut builder = FontBuilder::new();

        let mut plan = Plan::default();
        plan.glyphset
            .insert_range(GlyphId::NOTDEF..=GlyphId::from(5_u32));

        plan.glyph_map.insert(GlyphId::NOTDEF, GlyphId::NOTDEF);
        plan.glyph_map
            .insert(GlyphId::from(1_u32), GlyphId::from(1_u32));
        plan.glyph_map
            .insert(GlyphId::from(2_u32), GlyphId::from(2_u32));
        plan.glyph_map
            .insert(GlyphId::from(3_u32), GlyphId::from(3_u32));
        plan.glyph_map
            .insert(GlyphId::from(4_u32), GlyphId::from(4_u32));
        plan.glyph_map
            .insert(GlyphId::from(5_u32), GlyphId::from(5_u32));

        plan.new_to_old_gid_list
            .push((GlyphId::NOTDEF, GlyphId::NOTDEF));
        plan.new_to_old_gid_list
            .push((GlyphId::from(1_u32), GlyphId::from(1_u32)));
        plan.new_to_old_gid_list
            .push((GlyphId::from(2_u32), GlyphId::from(2_u32)));
        plan.new_to_old_gid_list
            .push((GlyphId::from(3_u32), GlyphId::from(3_u32)));
        plan.new_to_old_gid_list
            .push((GlyphId::from(4_u32), GlyphId::from(4_u32)));
        plan.new_to_old_gid_list
            .push((GlyphId::from(5_u32), GlyphId::from(5_u32)));

        let mut s = Serializer::new(1024).unwrap();
        assert_eq!(s.start_serialize(), Ok(()));
        let ret = cblc.subset(&plan, &font, &mut s, &mut builder);
        assert!(ret.is_ok());
        assert!(!s.in_error());
        s.end_serialize();

        let expected_bytes: [u8; 116] = [
            0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x38, 0x00, 0x00,
            0x00, 0x3C, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x65, 0xE5, 0x88, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x65, 0xE5, 0x88, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x05, 0x6D, 0x6D, 0x20, 0x01,
            0x00, 0x01, 0x00, 0x03, 0x00, 0x00, 0x00, 0x10, 0x00, 0x04, 0x00, 0x05, 0x00, 0x00,
            0x00, 0x28, 0x00, 0x01, 0x00, 0x11, 0x00, 0x00, 0x0E, 0xA5, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x03, 0xA6, 0x00, 0x00, 0x07, 0x50, 0x00, 0x00, 0x14, 0xB7, 0x00, 0x01,
            0x00, 0x11, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x52,
            0x00, 0x00, 0x0E, 0xA1,
        ];
        let subsetted_data = s.copy_bytes();
        assert_eq!(subsetted_data, expected_bytes);
    }

    #[test]
    fn test_subset_cbdt_keep_one() {
        let font = FontRef::new(include_bytes!(
            "../test-data/fonts/NotoColorEmoji.subset.ttf"
        ))
        .unwrap();

        let cblc = font.cblc().unwrap();
        let mut builder = FontBuilder::new();

        let mut plan = Plan::default();
        plan.glyphset.insert(GlyphId::NOTDEF);
        plan.glyphset.insert(GlyphId::from(2_u32));

        plan.glyph_map.insert(GlyphId::NOTDEF, GlyphId::NOTDEF);
        plan.glyph_map
            .insert(GlyphId::from(2_u32), GlyphId::from(1_u32));

        plan.new_to_old_gid_list
            .push((GlyphId::NOTDEF, GlyphId::NOTDEF));
        plan.new_to_old_gid_list
            .push((GlyphId::from(1_u32), GlyphId::from(2_u32)));

        let mut s = Serializer::new(1024).unwrap();
        assert_eq!(s.start_serialize(), Ok(()));
        let ret = cblc.subset(&plan, &font, &mut s, &mut builder);
        assert!(ret.is_ok());
        assert!(!s.in_error());
        s.end_serialize();

        let subsetted_data = s.copy_bytes();
        let expected_bytes: [u8; 80] = [
            0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x38, 0x00, 0x00,
            0x00, 0x18, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x65, 0xe5, 0x88, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x65, 0xe5, 0x88, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x6d, 0x6d, 0x20, 0x01,
            0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x08, 0x00, 0x01, 0x00, 0x11, 0x00, 0x00,
            0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0xaa,
        ];
        assert_eq!(subsetted_data, expected_bytes);
    }

    #[test]
    fn test_subset_cbdt_keep_one_last_subtable() {
        let font = FontRef::new(include_bytes!(
            "../test-data/fonts/NotoColorEmoji.subset.ttf"
        ))
        .unwrap();

        let cblc = font.cblc().unwrap();
        let mut builder = FontBuilder::new();

        let mut plan = Plan::default();
        plan.glyphset.insert(GlyphId::NOTDEF);
        plan.glyphset.insert(GlyphId::from(4_u32));

        plan.glyph_map.insert(GlyphId::NOTDEF, GlyphId::NOTDEF);
        plan.glyph_map
            .insert(GlyphId::from(4_u32), GlyphId::from(1_u32));

        plan.new_to_old_gid_list
            .push((GlyphId::NOTDEF, GlyphId::NOTDEF));
        plan.new_to_old_gid_list
            .push((GlyphId::from(1_u32), GlyphId::from(4_u32)));

        let mut s = Serializer::new(1024).unwrap();
        assert_eq!(s.start_serialize(), Ok(()));
        let ret = cblc.subset(&plan, &font, &mut s, &mut builder);
        assert!(ret.is_ok());
        assert!(!s.in_error());
        s.end_serialize();

        let subsetted_data = s.copy_bytes();
        let expected_bytes: [u8; 80] = [
            00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x38, 0x00, 0x00, 0x00,
            0x18, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x65, 0xe5, 0x88, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x65, 0xe5, 0x88, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x6d, 0x6d, 0x20, 0x01, 0x00,
            0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x08, 0x00, 0x01, 0x00, 0x11, 0x00, 0x00, 0x00,
            0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x52,
        ];
        assert_eq!(subsetted_data, expected_bytes);
    }

    #[test]
    fn test_subset_cbdt_keep_multiple_subtables() {
        let font = FontRef::new(include_bytes!(
            "../test-data/fonts/NotoColorEmoji.subset.multiple_size_tables.ttf"
        ))
        .unwrap();

        let cblc = font.cblc().unwrap();
        let mut builder = FontBuilder::new();

        let mut plan = Plan::default();
        plan.glyphset.insert(GlyphId::NOTDEF);
        plan.glyphset.insert(GlyphId::from(1_u32));
        plan.glyphset.insert(GlyphId::from(3_u32));
        plan.glyphset.insert(GlyphId::from(4_u32));

        plan.glyph_map.insert(GlyphId::NOTDEF, GlyphId::NOTDEF);
        plan.glyph_map
            .insert(GlyphId::from(1_u32), GlyphId::from(1_u32));
        plan.glyph_map
            .insert(GlyphId::from(3_u32), GlyphId::from(2_u32));
        plan.glyph_map
            .insert(GlyphId::from(4_u32), GlyphId::from(3_u32));

        plan.new_to_old_gid_list
            .push((GlyphId::NOTDEF, GlyphId::NOTDEF));
        plan.new_to_old_gid_list
            .push((GlyphId::from(1_u32), GlyphId::from(1_u32)));
        plan.new_to_old_gid_list
            .push((GlyphId::from(2_u32), GlyphId::from(3_u32)));
        plan.new_to_old_gid_list
            .push((GlyphId::from(3_u32), GlyphId::from(4_u32)));

        let mut s = Serializer::new(1024).unwrap();
        assert_eq!(s.start_serialize(), Ok(()));
        let ret = cblc.subset(&plan, &font, &mut s, &mut builder);
        assert!(ret.is_ok());
        assert!(!s.in_error());
        s.end_serialize();

        let subsetted_data = s.copy_bytes();
        let expected_bytes: [u8; 208] = [
            0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x9C, 0x00, 0x00,
            0x00, 0x34, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x65, 0xE5, 0x88, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x65, 0xE5, 0x88, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x03, 0xD2, 0xD2, 0x20, 0x01,
            0x00, 0x00, 0x00, 0x68, 0x00, 0x00, 0x00, 0x34, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00,
            0x00, 0x00, 0x65, 0xE5, 0x88, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x65, 0xE5, 0x88, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01,
            0x00, 0x03, 0x6D, 0x6D, 0x20, 0x01, 0x00, 0x01, 0x00, 0x02, 0x00, 0x00, 0x00, 0x10,
            0x00, 0x03, 0x00, 0x03, 0x00, 0x00, 0x00, 0x24, 0x00, 0x01, 0x00, 0x11, 0x00, 0x00,
            0x21, 0xB5, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0xA6, 0x00, 0x00, 0x11, 0x0D,
            0x00, 0x01, 0x00, 0x11, 0x00, 0x00, 0x19, 0x63, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x08, 0x52, 0x00, 0x01, 0x00, 0x02, 0x00, 0x00, 0x00, 0x10, 0x00, 0x03, 0x00, 0x03,
            0x00, 0x00, 0x00, 0x24, 0x00, 0x01, 0x00, 0x11, 0x00, 0x00, 0x08, 0x56, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x03, 0xA6, 0x00, 0x00, 0x11, 0x0D, 0x00, 0x01, 0x00, 0x11,
            0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x52,
        ];
        assert_eq!(subsetted_data, expected_bytes);
    }

    #[test]
    fn test_subset_cbdt_index_format3() {
        let font = FontRef::new(include_bytes!(
            "../test-data/fonts/NotoColorEmoji.subset.index_format3.ttf"
        ))
        .unwrap();

        let cblc = font.cblc().unwrap();
        let mut builder = FontBuilder::new();

        let mut plan = Plan::default();
        plan.glyphset.insert(GlyphId::NOTDEF);
        plan.glyphset.insert(GlyphId::from(1_u32));
        plan.glyphset.insert(GlyphId::from(3_u32));
        plan.glyphset.insert(GlyphId::from(4_u32));

        plan.glyph_map.insert(GlyphId::NOTDEF, GlyphId::NOTDEF);
        plan.glyph_map
            .insert(GlyphId::from(1_u32), GlyphId::from(1_u32));
        plan.glyph_map
            .insert(GlyphId::from(3_u32), GlyphId::from(2_u32));
        plan.glyph_map
            .insert(GlyphId::from(4_u32), GlyphId::from(3_u32));

        plan.new_to_old_gid_list
            .push((GlyphId::NOTDEF, GlyphId::NOTDEF));
        plan.new_to_old_gid_list
            .push((GlyphId::from(1_u32), GlyphId::from(1_u32)));
        plan.new_to_old_gid_list
            .push((GlyphId::from(2_u32), GlyphId::from(3_u32)));
        plan.new_to_old_gid_list
            .push((GlyphId::from(3_u32), GlyphId::from(4_u32)));

        let mut s = Serializer::new(1024).unwrap();
        assert_eq!(s.start_serialize(), Ok(()));
        let ret = cblc.subset(&plan, &font, &mut s, &mut builder);
        assert!(ret.is_ok());
        assert!(!s.in_error());
        s.end_serialize();

        let subsetted_data = s.copy_bytes();
        let expected_bytes: [u8; 100] = [
            0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x38, 0x00, 0x00,
            0x00, 0x2C, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x65, 0xE5, 0x88, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x65, 0xE5, 0x88, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x03, 0x6D, 0x6D, 0x20, 0x01,
            0x00, 0x01, 0x00, 0x02, 0x00, 0x00, 0x00, 0x10, 0x00, 0x03, 0x00, 0x03, 0x00, 0x00,
            0x00, 0x20, 0x00, 0x03, 0x00, 0x11, 0x00, 0x00, 0x08, 0x56, 0x00, 0x00, 0x03, 0xA6,
            0x11, 0x0D, 0x00, 0x00, 0x00, 0x03, 0x00, 0x11, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00,
            0x08, 0x52,
        ];
        assert_eq!(subsetted_data, expected_bytes);
    }

    // Every strike points at one IndexSubtableList, and every record of it at one IndexSubtable
    // in which no glyph has an image. Visiting the same bytes over and over must hit the budget.
    #[test]
    fn limits_visits_of_shared_index_subtables() {
        const NUM_GLYPHS: u16 = 256;

        let mut plan = Plan::default();
        plan.glyphset
            .insert_range(GlyphId::NOTDEF..=GlyphId::from(NUM_GLYPHS - 1));
        plan.new_to_old_gid_list.extend((0..NUM_GLYPHS).map(|gid| {
            let gid = GlyphId::from(gid);
            (gid, gid)
        }));
        plan.glyph_map.extend(
            plan.new_to_old_gid_list
                .iter()
                .map(|&(new_gid, old_gid)| (old_gid, new_gid)),
        );

        // (number of strikes and of records per strike, whether subsetting succeeds)
        for (count, expected) in [(2_u32, true), (64, false)] {
            let bitmap_sizes_len = count * BitmapSize::RAW_BYTE_LEN as u32;
            let records_len = count * IndexSubtableRecord::RAW_BYTE_LEN as u32;
            let subtable_len = 8 + (NUM_GLYPHS as u32 + 1) * 2;

            let mut cblc = Vec::from([0x00, 0x03, 0x00, 0x00]); // version 3.0
            cblc.extend_from_slice(&count.to_be_bytes());
            // every strike points at the same list
            for _ in 0..count {
                cblc.extend_from_slice(&(8 + bitmap_sizes_len).to_be_bytes());
                cblc.extend_from_slice(&(records_len + subtable_len).to_be_bytes());
                cblc.extend_from_slice(&count.to_be_bytes());
                cblc.extend_from_slice(&[0; 28]); // color ref, horizontal and vertical line metrics
                cblc.extend_from_slice(&0_u16.to_be_bytes());
                cblc.extend_from_slice(&(NUM_GLYPHS - 1).to_be_bytes());
                cblc.extend_from_slice(&[0x10, 0x10, 0x20, 0x01]); // ppem, bit depth, and flags
            }
            // every record covers all glyphs and points at the same subtable
            for _ in 0..count {
                cblc.extend_from_slice(&0_u16.to_be_bytes());
                cblc.extend_from_slice(&(NUM_GLYPHS - 1).to_be_bytes());
                cblc.extend_from_slice(&records_len.to_be_bytes());
            }
            cblc.extend_from_slice(&[
                0x00, 0x03, 0x00, 0x11, // index format 3, image format 17
                0x00, 0x00, 0x00, 0x04, // image data follows the CBDT header
            ]);
            // no glyph has image data, so nothing is written for the subtable
            cblc.resize(cblc.len() + (NUM_GLYPHS as usize + 1) * 2, 0);

            let mut builder = FontBuilder::new();
            builder.add_raw(Cblc::TAG, cblc);
            builder.add_raw(Cbdt::TAG, [0x00, 0x03, 0x00, 0x00].as_slice());
            let font_data = builder.build();
            let font = FontRef::new(&font_data).unwrap();

            let mut s = Serializer::new(1024).unwrap();
            s.start_serialize().unwrap();
            let cblc = font.cblc().unwrap();
            let ret = cblc.subset(&plan, &font, &mut s, &mut FontBuilder::new());
            assert_eq!(ret.is_ok(), expected, "count {count}");
            assert_eq!(s.in_error(), !expected, "count {count}");
            assert!(!s.ran_out_of_room());
        }
    }

    // Every strike points at one IndexSubtableList, and every record of it at one IndexSubtable
    // for glyph 1, whose image is copied into the CBDT table on each visit. The visits fit the
    // budget of the CBLC table, so the bytes copied must hit the budget of the CBDT table.
    #[test]
    fn limits_image_bytes_copied_from_shared_index_subtables() {
        const IMAGE_LEN: u32 = 1024;

        let mut plan = Plan::default();
        plan.glyphset
            .insert_range(GlyphId::NOTDEF..=GlyphId::new(1));
        plan.new_to_old_gid_list
            .extend([GlyphId::NOTDEF, GlyphId::new(1)].map(|gid| (gid, gid)));
        plan.glyph_map.extend(
            plan.new_to_old_gid_list
                .iter()
                .map(|&(new_gid, old_gid)| (old_gid, new_gid)),
        );

        // (number of strikes and of records per strike, whether subsetting succeeds)
        for (count, expected) in [(2_u32, true), (16, false)] {
            let bitmap_sizes_len = count * BitmapSize::RAW_BYTE_LEN as u32;
            let records_len = count * IndexSubtableRecord::RAW_BYTE_LEN as u32;
            let subtable_len = 16;

            let mut cblc = Vec::from([0x00, 0x03, 0x00, 0x00]); // version 3.0
            cblc.extend_from_slice(&count.to_be_bytes());
            // every strike points at the same list
            for _ in 0..count {
                cblc.extend_from_slice(&(8 + bitmap_sizes_len).to_be_bytes());
                cblc.extend_from_slice(&(records_len + subtable_len).to_be_bytes());
                cblc.extend_from_slice(&count.to_be_bytes());
                cblc.extend_from_slice(&[0; 28]); // color ref, horizontal and vertical line metrics
                cblc.extend_from_slice(&[0x00, 0x01, 0x00, 0x01]); // glyph 1 only
                cblc.extend_from_slice(&[0x10, 0x10, 0x20, 0x01]); // ppem, bit depth, and flags
            }
            // every record covers glyph 1 and points at the same subtable
            for _ in 0..count {
                cblc.extend_from_slice(&[0x00, 0x01, 0x00, 0x01]);
                cblc.extend_from_slice(&records_len.to_be_bytes());
            }
            cblc.extend_from_slice(&[
                0x00, 0x01, 0x00, 0x11, // index format 1, image format 17
                0x00, 0x00, 0x00, 0x04, // image data follows the CBDT header
                0x00, 0x00, 0x00, 0x00, // glyph 1 starts the image data
            ]);
            cblc.extend_from_slice(&IMAGE_LEN.to_be_bytes());

            let mut cbdt = Vec::from([0x00, 0x03, 0x00, 0x00]); // version 3.0
            cbdt.resize(cbdt.len() + IMAGE_LEN as usize, 0);

            let mut builder = FontBuilder::new();
            builder.add_raw(Cblc::TAG, cblc);
            builder.add_raw(Cbdt::TAG, cbdt);
            let font_data = builder.build();
            let font = FontRef::new(&font_data).unwrap();

            // 4 copies of the image fit 64 times the CBDT table, 256 copies don't
            let mut s = Serializer::new(1024 * 64).unwrap();
            s.start_serialize().unwrap();
            let cblc = font.cblc().unwrap();
            let ret = cblc.subset(&plan, &font, &mut s, &mut FontBuilder::new());
            assert_eq!(ret.is_ok(), expected, "count {count}");
            assert_eq!(s.in_error(), !expected, "count {count}");
            assert!(!s.ran_out_of_room());
        }
    }
    /// A font with one strike of one index subtable of `index_format` (1 or 3) whose glyph
    /// offsets are `offsets`, so that it covers glyphs `0..offsets.len() - 1`, and a CBDT table
    /// with `image_len` bytes of image data.
    fn single_subtable_font(index_format: u16, offsets: &[u32], image_len: usize) -> Vec<u8> {
        let num_glyphs = offsets.len() as u16 - 1;
        let offset_size = if index_format == 1 { 4 } else { 2 };
        let subtable_len = 8 + offsets.len() as u32 * offset_size;
        let mut cblc = Vec::from([
            0x00, 0x03, 0x00, 0x00, // version 3.0
            0x00, 0x00, 0x00, 0x01, // one bitmap-size record
            0x00, 0x00, 0x00, 0x38, // index-subtable list offset
        ]);
        cblc.extend_from_slice(&(8 + subtable_len).to_be_bytes());
        cblc.extend_from_slice(&1_u32.to_be_bytes()); // one index subtable
        cblc.extend_from_slice(&[0; 28]); // color ref, horizontal and vertical line metrics
        cblc.extend_from_slice(&0_u16.to_be_bytes());
        cblc.extend_from_slice(&(num_glyphs - 1).to_be_bytes());
        cblc.extend_from_slice(&[0x10, 0x10, 0x20, 0x01]); // ppem, bit depth, and flags
        cblc.extend_from_slice(&0_u16.to_be_bytes());
        cblc.extend_from_slice(&(num_glyphs - 1).to_be_bytes());
        cblc.extend_from_slice(&8_u32.to_be_bytes()); // subtable follows the record
        cblc.extend_from_slice(&index_format.to_be_bytes());
        cblc.extend_from_slice(&[
            0x00, 0x11, // image format 17
            0x00, 0x00, 0x00, 0x04, // image data follows the CBDT header
        ]);
        for offset in offsets {
            match index_format {
                1 => cblc.extend_from_slice(&offset.to_be_bytes()),
                _ => cblc.extend_from_slice(&(*offset as u16).to_be_bytes()),
            }
        }

        let mut cbdt = Vec::from([0x00, 0x03, 0x00, 0x00]); // version 3.0
        cbdt.resize(4 + image_len, 0xAB);

        let mut builder = FontBuilder::new();
        builder.add_raw(Cblc::TAG, cblc);
        builder.add_raw(Cbdt::TAG, cbdt);
        builder.build()
    }

    /// Subsets the CBLC table of `font_data` to all its `num_glyphs` glyphs, and returns whether
    /// that succeeded and whether it wrote a CBDT table, after checking that a failure is an
    /// error of the serializer.
    fn subset_all_glyphs(font_data: &[u8], num_glyphs: u16) -> (bool, bool) {
        let font = FontRef::new(font_data).unwrap();
        let mut plan = Plan::default();
        plan.glyphset
            .insert_range(GlyphId::NOTDEF..=GlyphId::from(num_glyphs - 1));
        plan.new_to_old_gid_list.extend((0..num_glyphs).map(|gid| {
            let gid = GlyphId::from(gid);
            (gid, gid)
        }));
        plan.glyph_map.extend(
            plan.new_to_old_gid_list
                .iter()
                .map(|&(new_gid, old_gid)| (old_gid, new_gid)),
        );

        let mut output_builder = FontBuilder::new();
        let mut s = Serializer::new(1 << 16).unwrap();
        s.start_serialize().unwrap();
        let cblc = font.cblc().unwrap();
        let ret = cblc.subset(&plan, &font, &mut s, &mut output_builder);
        assert_eq!(s.in_error(), ret.is_err());
        assert!(!s.ran_out_of_room());
        (ret.is_ok(), output_builder.contains(Cbdt::TAG))
    }

    // Glyph offsets need not ascend: with 0, 16, 0, 16, ... every other glyph's image is all of
    // the 16 bytes of image data, so one subtable copies the CBDT table about half its glyph
    // count times. That must stop at 64 times the 20 bytes of the table.
    #[test]
    fn limits_image_bytes_copied_from_overlapping_images() {
        const IMAGE_LEN: u32 = 16;
        // (number of glyphs, whether subsetting succeeds)
        for (num_glyphs, expected) in [(100_u16, true), (200, false)] {
            let offsets: Vec<_> = (0..=num_glyphs as u32)
                .map(|i| (i % 2) * IMAGE_LEN)
                .collect();
            let font_data = single_subtable_font(1, &offsets, IMAGE_LEN as usize);
            let (ok, has_cbdt) = subset_all_glyphs(&font_data, num_glyphs);
            assert_eq!(ok, expected, "{num_glyphs} glyphs");
            assert_eq!(has_cbdt, expected, "{num_glyphs} glyphs");
        }
    }

    // The glyph offsets of an index subtable of format 3 are 16 bits. Overlapping images can add
    // up to more than that, which fails the table rather than wrap the offsets.
    #[test]
    fn rejects_format3_images_past_16_bit_offsets() {
        // (image length, whether subsetting succeeds): two glyphs of the whole image data
        for (image_len, expected) in [(30000_u32, true), (40000, false)] {
            let offsets = [0, image_len, 0, image_len];
            let font_data = single_subtable_font(3, &offsets, image_len as usize);
            let (ok, has_cbdt) = subset_all_glyphs(&font_data, 3);
            assert_eq!(ok, expected, "images of {image_len} bytes");
            assert_eq!(has_cbdt, expected, "images of {image_len} bytes");
        }
    }
}
