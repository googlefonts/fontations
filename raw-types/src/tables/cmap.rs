use crate::*;

toy_table_macro::tables! {
    Cmap<'a> {
        version: Uint16,
        num_tables: Uint16,
        #[count(num_tables)]
        encoding_records: [EncodingRecord],
    }

    EncodingRecord {
        platform_id: Uint16,
        encoding_id: Uint16,
        subtable_offset: Offset32,
    }
}

toy_table_macro::tables! {

    Cmap0<'a> {
        format: Uint16,
        length: Uint16,
        language: Uint16,
        #[count(256)]
        glyph_id_array: [Uint8],
    }

    Cmap4<'a> {
        format: Uint16,
        length: Uint16,
        language: Uint16,
        seg_count_x2: Uint16,
        search_range: Uint16,
        entry_selector: Uint16,
        range_shift: Uint16,
        #[count_with(div_by_two, seg_count_x2)]
        end_code: [Uint16],
        #[hidden]
        reserved_pad: Uint16,
        #[count_with(div_by_two, seg_count_x2)]
        start_code: [Uint16],
        #[count_with(div_by_two, seg_count_x2)]
        id_delta: [Int16],
        #[count_with(div_by_two, seg_count_x2)]
        id_range_offsets: [Uint16],
        #[count_all]
        glyph_id_array: [Uint16],
    }

    Cmap12<'a> {
        format: Uint16,
        #[hidden]
        reserved: Uint16,
        length: Uint32,
        language: Uint32,
        num_groups: Uint32,
        #[count(num_groups)]
        groups: [SequentialMapGroup],
    }

    SequentialMapGroup {
        start_char_code: Uint32,
        end_char_code: Uint32,
        start_gylph_id: Uint32,
    }

    Cmap13<'a> {
        format: Uint16,
        #[hidden]
        reserved: Uint16,
        length: Uint32,
        language: Uint32,
        num_groups: Uint32,
        #[count(num_groups)]
        groups: [ConstantMapGroup],
    }

    ConstantMapGroup {
        start_char_code: Uint32,
        end_char_code: Uint32,
        start_gylph_id: Uint32,
    }

    #[format(Uint16)]
    enum CmapSubtable<'a> {
        #[version(Cmap::FORMAT_0)]
        Format0(Cmap0<'a>),
        #[version(Cmap::FORMAT_4)]
        Format4(Cmap4<'a>),
        #[version(Cmap::FORMAT_12)]
        Format12(Cmap12<'a>),
        #[version(Cmap::FORMAT_13)]
        Format13(Cmap13<'a>),
    }
}

impl Cmap<'_> {
    const FORMAT_0: Uint16 = Uint16::from_bytes(0u16.to_be_bytes());
    const FORMAT_4: Uint16 = Uint16::from_bytes(1u16.to_be_bytes());
    const FORMAT_12: Uint16 = Uint16::from_bytes(12u16.to_be_bytes());
    const FORMAT_13: Uint16 = Uint16::from_bytes(13u16.to_be_bytes());

    pub fn subtable(&self, offset: Offset32) -> Option<CmapSubtable> {
        self.0
            .get(offset.get() as usize..self.0.len())
            .and_then(FontRead::read)
    }
}
impl<'a> Cmap<'a> {
    /// Get the subtable at the given offset and attempt to interpret it as `T`
    pub fn parse_subtable<T: FontRead<'a>>(&self, offset: Offset32) -> Option<T> {
        self.0
            .get(offset.get() as usize..self.0.len())
            .and_then(T::read)
    }
}

fn div_by_two(seg_count_x2: Uint16) -> usize {
    (seg_count_x2.get() / 2) as usize
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
    pub fn glyph_id_for_char(&self, chr: char) -> Uint16 {
        let n_segs = self.seg_count_x2().unwrap_or_default().get() / 2;
        let raw_char = (chr as u32).try_into().unwrap_or_default();
        let seg_idx = match self
            .end_code()
            .unwrap()
            .binary_search_by(|probe| probe.get().cmp(&raw_char))
        {
            Ok(idx) => idx,
            Err(idx) => idx,
        };

        // safety: `seg_idx` must be < end_code.len(), and all arrays have equal length;
        // therefore the index must be valid in all of them.
        let start_code = unsafe { self.start_code().unwrap().get_unchecked(seg_idx).get() };
        // TODO: get rid of this branch?
        if start_code > raw_char {
            return 0.into();
        }
        let id_delta = unsafe { self.id_delta().unwrap().get_unchecked(seg_idx).get() };
        let range_offset = unsafe {
            self.id_range_offsets()
                .unwrap()
                .get_unchecked(seg_idx)
                .get()
                / 2
        };
        let glyf_idx = if range_offset == 0 {
            wrapping_add_delta(raw_char, id_delta)
        } else {
            let offset_rel_id_range = range_offset + (raw_char - start_code);
            let glyf_idx = offset_rel_id_range - (n_segs - seg_idx as u16);
            if glyf_idx != 0 {
                wrapping_add_delta(glyf_idx, id_delta)
            } else {
                0
            }
        };

        self.glyph_id_array()
            .unwrap_or_default()
            .get(glyf_idx as usize)
            .copied()
            //.map(|val| val)
            .unwrap_or_default()
    }
}

#[inline(always)]
pub fn wrapping_add_delta(base: u16, delta: i16) -> u16 {
    let r: u32 = (base as i32 + delta as i32).max(0) as u32;
    (r % 0xFFFF) as u16
}
