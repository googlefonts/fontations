use crate::*;

#[derive(Clone, Debug, FontThing)]
pub struct Cmap<'a> {
    pub version: uint16,
    pub num_tables: uint16,
    #[font_thing(count = "num_tables")]
    pub encoding_records: Array<'a, EncodingRecord>,
    #[font_thing(data)]
    data: Blob<'a>,
}

#[derive(Clone, Debug, FontThing)]
pub struct EncodingRecord {
    pub platform_id: uint16,
    pub encoding_id: uint16,
    pub subtable_offset: Offset32,
}

#[derive(Clone, Debug, FontThing)]
pub struct Cmap4<'a> {
    pub format: uint16,
    pub length: uint16,
    pub language: uint16,
    pub seg_count_x2: uint16,
    pub search_range: uint16,
    pub entry_selector: uint16,
    pub range_shift: uint16,
    #[font_thing(count(fn = "div_by_two", args("seg_count_x2")))]
    pub end_code: Array<'a, uint16>,
    pub reserved_pad: uint16,
    #[font_thing(count(fn = "div_by_two", args("seg_count_x2")))]
    pub start_code: Array<'a, uint16>,
    #[font_thing(count(fn = "div_by_two", args("seg_count_x2")))]
    pub id_delta: Array<'a, int16>,
    #[font_thing(count(fn = "div_by_two", args("seg_count_x2")))]
    pub id_range_offsets: Array<'a, uint16>,
    #[font_thing(all)]
    glyph_id_array: Array<'a, uint16>,
}

fn div_by_two(seg_count_x2: uint16) -> uint16 {
    seg_count_x2 / 2
}

impl<'a> Cmap<'a> {
    pub fn get_subtable_version(&self, offset: Offset32) -> Option<u16> {
        self.data.read(offset as usize)
    }

    pub fn get_subtable<T: FontRead<'a>>(&self, offset: Offset32) -> Option<T> {
        self.data
            .get(offset as usize..self.data.len())
            .and_then(T::read)
    }
}

impl<'a> Cmap4<'a> {
    /// Find a glyphid, maybe
    ///
    /// Each segment is described by a startCode and endCode, along with an idDelta
    /// and an idRangeOffset, which are used for mapping the character codes in
    /// the segment. The segments are sorted in order of increasing endCode values,
    /// and the segment values are specified in four parallel arrays. You search
    /// for the first endCode that is greater than or equal to the character code
    /// you want to map. If the corresponding startCode is less than or equal to the
    /// character code, then you use the corresponding idDelta and idRangeOffset
    /// to map the character code to a glyph index (otherwise, the missingGlyph
    /// is returned). For the search to terminate, the final start code and endCode
    /// values must be 0xFFFF. This segment need not contain any valid mappings.
    /// (It can just map the single character code 0xFFFF to missingGlyph).
    /// However, the segment must be present.
    pub fn glyph_id_for_char(&self, chr: char) -> Option<uint16> {
        //NOTE: this impl is bad
        let raw_char = (chr as u32).try_into().unwrap_or(0_u16);
        let end_code_idx = match self.end_code.binary_search(&raw_char) {
            Ok(idx) => idx,
            Err(idx) => idx,
        };
        let start_code = self.start_code.get(end_code_idx).unwrap();
        if start_code > raw_char {
            return None;
        }

        let id_delta = self.id_delta.get(end_code_idx).unwrap();
        // stored in bytes, convert to index (u16 == two bytes)
        let range_offset = self.id_range_offsets.get(end_code_idx).unwrap() / 2;
        let range_array_len = self.id_range_offsets.len() as u16;
        let glyf_idx: u16 = if range_offset != 0 {
            // this is the offset relative to our position in
            // id_range_offsets, e.g. we're supposed to do ptr::add from
            // the position corresponding to id_range_offsets[idx].
            // in practice this means we need to add the # of items > idx in
            // order to figure out the position relative to the glyph array.
            let offset_rel_id_range = range_offset + (raw_char - start_code);
            let glyf_idx = offset_rel_id_range - (range_array_len - end_code_idx as u16);
            if glyf_idx != 0 {
                wrapping_add_delta(glyf_idx, id_delta)
            } else {
                0
            }
        } else {
            wrapping_add_delta(raw_char, id_delta)
        };
        self.glyph_id_array.get(glyf_idx as usize)
    }
}

#[inline(always)]
pub fn wrapping_add_delta(base: u16, delta: i16) -> u16 {
    let r: u32 = (base as i32 + delta as i32).max(0) as u32;
    (r % 0xFFFF) as u16
}
