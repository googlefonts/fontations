//! OpenType variations common table formats

include!("../../generated/generated_variations.rs");

pub use read_fonts::tables::variations::{DeltaRunType, TupleIndex, TupleVariationCount};

pub mod ivs_builder;

impl TupleVariationHeader {
    pub fn new(
        variation_data_size: u16,
        shared_tuple_idx: Option<u16>,
        peak_tuple: Option<Tuple>,
        intermediate_region: Option<(Tuple, Tuple)>,
        has_private_points: bool,
    ) -> Self {
        assert!(
            shared_tuple_idx.is_some() != peak_tuple.is_some(),
            "one and only one of peak_tuple or shared_tuple_idx must be present"
        );
        let mut idx = shared_tuple_idx.unwrap_or_default();
        if peak_tuple.is_some() {
            idx |= TupleIndex::EMBEDDED_PEAK_TUPLE;
        }
        if intermediate_region.is_some() {
            idx |= TupleIndex::INTERMEDIATE_REGION;
        }
        if has_private_points {
            idx |= TupleIndex::PRIVATE_POINT_NUMBERS;
        }
        let (intermediate_start_tuple, intermediate_end_tuple) = intermediate_region
            .map(|(start, end)| (start.values, end.values))
            .unwrap_or_default();

        TupleVariationHeader {
            variation_data_size,
            tuple_index: TupleIndex::from_bits(idx),
            peak_tuple: peak_tuple.map(|tup| tup.values).unwrap_or_default(),
            intermediate_start_tuple,
            intermediate_end_tuple,
        }
    }

    /// Return the number of bytes required to encode this header
    pub fn compute_size(&self) -> u16 {
        let len: usize = 2 + 2 // variationDataSize, tupleIndex
        + self.peak_tuple.len() * F2Dot14::RAW_BYTE_LEN
        + self.intermediate_start_tuple.len()  * F2Dot14::RAW_BYTE_LEN
        + self.intermediate_end_tuple.len()  * F2Dot14::RAW_BYTE_LEN;
        len.try_into().unwrap()
    }
}

/// <https://learn.microsoft.com/en-us/typography/opentype/spec/otvarcommonformats#packed-point-numbers>
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum PackedPointNumbers {
    /// Contains deltas for all point numbers
    #[default]
    All,
    /// Contains deltas only for these specific point numbers
    Some(Vec<u16>),
}

/// <https://learn.microsoft.com/en-us/typography/opentype/spec/otvarcommonformats#packed-deltas>
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PackedDeltas {
    deltas: Vec<i32>,
}

impl Validate for PackedDeltas {
    fn validate_impl(&self, _ctx: &mut ValidationCtx) {}
}

impl FontWrite for PackedDeltas {
    fn write_into(&self, writer: &mut TableWriter) {
        for run in self.iter_runs() {
            run.write_into(writer)
        }
    }
}

impl PackedDeltas {
    /// Construct a `PackedDeltas` from a vector of raw delta values.
    pub fn new(deltas: Vec<i32>) -> Self {
        Self { deltas }
    }

    /// Compute the number of bytes required to encode these deltas
    pub(crate) fn compute_size(&self) -> u16 {
        self.iter_runs().fold(0u16, |acc, run| {
            acc.checked_add(run.compute_size()).unwrap()
        })
    }

    fn iter_runs(&self) -> impl Iterator<Item = PackedDeltaRun> {
        // 6 bits for length per https://learn.microsoft.com/en-us/typography/opentype/spec/otvarcommonformats#packed-deltas
        const MAX_POINTS_PER_RUN: usize = 64;

        // preferred run type for this value
        fn preferred_run_type(v: i32) -> DeltaRunType {
            match v {
                0 => DeltaRunType::Zero,
                _ if v > i16::MAX as i32 || v < i16::MIN as i32 => DeltaRunType::I32,
                _ if v > i8::MAX as i32 || v < i8::MIN as i32 => DeltaRunType::I16,
                _ => DeltaRunType::I8,
            }
        }

        fn count_leading_zeros(slice: &[i32]) -> u8 {
            slice
                .iter()
                .take(MAX_POINTS_PER_RUN)
                .take_while(|v| **v == 0)
                .count() as u8
        }

        /// compute the number of deltas in the next run, and the value type
        fn next_run_len(slice: &[i32]) -> (usize, DeltaRunType) {
            let first = *slice.first().expect("bounds checked before here");
            debug_assert!(first != 0, "Zeroes are supposed to be handled separately");
            let run_type = preferred_run_type(first);

            let mut idx = 1;
            while idx < MAX_POINTS_PER_RUN && idx < slice.len() {
                let cur = slice[idx];
                let cur_type = preferred_run_type(cur);
                let next_type = slice.get(idx + 1).copied().map(preferred_run_type);

                // Any reason to stop?
                if run_type == DeltaRunType::I8 {
                    // a single zero is best stored literally inline, but two or more
                    // should get a new run:
                    // https://github.com/fonttools/fonttools/blob/eeaa499981c587/Lib/fontTools/ttLib/tables/TupleVariation.py#L423
                    match cur_type {
                        DeltaRunType::Zero if next_type == Some(DeltaRunType::Zero) => break,
                        DeltaRunType::I16 | DeltaRunType::I32 => break,
                        _ => (),
                    }
                } else if run_type == DeltaRunType::I16 {
                    // with word deltas, a single zero justifies a new run:
                    //https://github.com/fonttools/fonttools/blob/eeaa499981c587/Lib/fontTools/ttLib/tables/TupleVariation.py#L457
                    match (cur_type, next_type) {
                        (DeltaRunType::Zero | DeltaRunType::I32, _) => break,
                        // and a single byte-size value should be inlined, if it lets
                        // us combine two adjoining word-size runs:
                        // https://github.com/fonttools/fonttools/blob/eeaa499981c587/Lib/fontTools/ttLib/tables/TupleVariation.py#L467
                        (DeltaRunType::I8, Some(DeltaRunType::Zero | DeltaRunType::I8)) => break,
                        _ => (),
                    }
                } else if run_type == DeltaRunType::I32 && cur_type != DeltaRunType::I32 {
                    break;
                }

                idx += 1;
            }
            (idx, run_type)
        }

        let mut deltas = self.deltas.as_slice();

        std::iter::from_fn(move || {
            let run_start = *deltas.first()?;
            if run_start == 0 {
                let n_zeros = count_leading_zeros(deltas);
                deltas = &deltas[n_zeros as usize..];
                Some(PackedDeltaRun::Zeros(n_zeros))
            } else {
                let (len, value_type) = next_run_len(deltas);
                let (head, tail) = deltas.split_at(len);
                deltas = tail;
                Some(match value_type {
                    DeltaRunType::I32 => PackedDeltaRun::FourBytes(head),
                    DeltaRunType::I16 => PackedDeltaRun::TwoBytes(head),
                    DeltaRunType::I8 => PackedDeltaRun::OneByte(head),
                    _ => {
                        unreachable!("We should have taken the other branch for first={run_start}")
                    }
                })
            }
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum PackedDeltaRun<'a> {
    Zeros(u8),
    OneByte(&'a [i32]),
    TwoBytes(&'a [i32]),
    FourBytes(&'a [i32]),
}

impl PackedDeltaRun<'_> {
    fn compute_flag(&self) -> u8 {
        /// Flag indicating that this run contains no data,
        /// and that the deltas for this run are all zero.
        const DELTAS_ARE_ZERO: u8 = 0x80;
        /// Flag indicating the data type for delta values in the run.
        const DELTAS_ARE_WORDS: u8 = 0x40;

        match self {
            PackedDeltaRun::Zeros(count) => (count - 1) | DELTAS_ARE_ZERO,
            PackedDeltaRun::OneByte(deltas) => deltas.len() as u8 - 1,
            PackedDeltaRun::TwoBytes(deltas) => (deltas.len() as u8 - 1) | DELTAS_ARE_WORDS,
            PackedDeltaRun::FourBytes(deltas) => {
                (deltas.len() as u8 - 1) | DELTAS_ARE_WORDS | DELTAS_ARE_ZERO
            }
        }
    }

    fn compute_size(&self) -> u16 {
        match self {
            PackedDeltaRun::Zeros(_) => 1,
            PackedDeltaRun::OneByte(vals) => vals.len() as u16 + 1,
            PackedDeltaRun::TwoBytes(vals) => vals.len() as u16 * 2 + 1,
            PackedDeltaRun::FourBytes(vals) => vals.len() as u16 * 4 + 1,
        }
    }
}

impl FontWrite for PackedDeltaRun<'_> {
    fn write_into(&self, writer: &mut TableWriter) {
        self.compute_flag().write_into(writer);
        match self {
            PackedDeltaRun::Zeros(_) => (),
            PackedDeltaRun::OneByte(deltas) => {
                deltas.iter().for_each(|v| (*v as i8).write_into(writer))
            }
            PackedDeltaRun::TwoBytes(deltas) => {
                deltas.iter().for_each(|v| (*v as i16).write_into(writer))
            }
            PackedDeltaRun::FourBytes(deltas) => deltas.iter().for_each(|v| v.write_into(writer)),
        }
    }
}

impl crate::validate::Validate for PackedPointNumbers {
    fn validate_impl(&self, ctx: &mut ValidationCtx) {
        if let PackedPointNumbers::Some(pts) = self {
            if pts.len() > 0x7FFF {
                ctx.report("length cannot be stored in 15 bites");
            }
        }
    }
}

impl FontWrite for PackedPointNumbers {
    fn write_into(&self, writer: &mut TableWriter) {
        // compute the actual count:
        match self.as_slice().len() {
            len @ 0..=127 => (len as u8).write_into(writer),
            len => (len as u16 | 0x8000u16).write_into(writer),
        }
        for run in self.iter_runs() {
            run.write_into(writer);
        }
    }
}

impl PackedPointNumbers {
    /// Compute the number of bytes required to encode these points
    pub(crate) fn compute_size(&self) -> u16 {
        let mut count = match self {
            PackedPointNumbers::All => return 1,
            PackedPointNumbers::Some(pts) if pts.len() < 128 => 1u16,
            PackedPointNumbers::Some(_) => 2,
        };
        for run in self.iter_runs() {
            count = count.checked_add(run.compute_size()).unwrap();
        }
        count
    }

    fn as_slice(&self) -> &[u16] {
        match self {
            PackedPointNumbers::All => &[],
            PackedPointNumbers::Some(pts) => pts.as_slice(),
        }
    }

    fn iter_runs(&self) -> impl Iterator<Item = PackedPointRun> {
        const U8_MAX: u16 = u8::MAX as u16;
        const MAX_POINTS_PER_RUN: usize = 128;

        let mut points = match self {
            PackedPointNumbers::Some(pts) => pts.as_slice(),
            PackedPointNumbers::All => &[],
        };

        let mut prev_point = 0u16;

        // split a run off the front of points:
        // - if point is more than 255 away from prev, we're using words
        std::iter::from_fn(move || {
            let next = points.first()?;
            let are_words = (next - prev_point) > U8_MAX;
            let run_len = points
                .iter()
                .take(MAX_POINTS_PER_RUN)
                .scan(prev_point, |prev, point| {
                    let take_this = if are_words {
                        (point - *prev) > U8_MAX
                    } else {
                        (point - *prev) <= U8_MAX
                    };
                    *prev = *point;
                    take_this.then_some(point)
                })
                .count();

            let (head, tail) = points.split_at(run_len);
            points = tail;
            let last_point = prev_point;
            prev_point = head.last().copied().unwrap();

            Some(PackedPointRun {
                last_point,
                are_words,
                points: head,
            })
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
struct PackedPointRun<'a> {
    last_point: u16,
    are_words: bool,
    points: &'a [u16],
}

impl PackedPointRun<'_> {
    fn compute_size(&self) -> u16 {
        const LEN_BYTE: u16 = 1;
        let per_point_len = if self.are_words { 2 } else { 1 };
        self.points.len() as u16 * per_point_len + LEN_BYTE
    }
}

impl FontWrite for PackedPointRun<'_> {
    fn write_into(&self, writer: &mut TableWriter) {
        assert!(!self.points.is_empty() && self.points.len() <= 128);
        let mut len = self.points.len() as u8 - 1;
        if self.are_words {
            len |= 0x80;
        }
        len.write_into(writer);
        let mut last_point = self.last_point;
        for point in self.points {
            let delta = point - last_point;
            last_point = *point;
            if self.are_words {
                delta.write_into(writer);
            } else {
                debug_assert!(delta <= u8::MAX as u16);
                (delta as u8).write_into(writer);
            }
        }
    }
}

impl FontWrite for TupleIndex {
    fn write_into(&self, writer: &mut TableWriter) {
        self.bits().write_into(writer)
    }
}

//hack: unclear if we're even going to do any codegen for writing, but
//for the time being this lets us compile
impl<'a> FromObjRef<Option<read_fonts::tables::variations::Tuple<'a>>> for Vec<F2Dot14> {
    fn from_obj_ref(
        from: &Option<read_fonts::tables::variations::Tuple<'a>>,
        _data: FontData,
    ) -> Self {
        from.as_ref()
            .map(|tup| tup.values.iter().map(BigEndian::get).collect())
            .unwrap_or_default()
    }
}

impl Tuple {
    pub fn len(&self) -> u16 {
        self.values.len().try_into().unwrap()
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

impl DeltaSetIndexMap {
    /// Return the most compact entry format that can represent this mapping.
    ///
    /// EntryFormat is a packed u8 field that describes the compressed representation
    /// of delta-set indices. For more info, see:
    /// <https://learn.microsoft.com/en-us/typography/opentype/spec/otvarcommonformats#associating-target-items-to-variation-data>
    // This is a direct port from fonttools' DeltaSetMap.getEntryFormat:
    // https://github.com/fonttools/fonttools/blob/6d531f/Lib/fontTools/ttLib/tables/otTables.py#L644-L666
    fn get_entry_format(mapping: &[u32]) -> EntryFormat {
        let ored = mapping.iter().fold(0, |acc, idx| acc | *idx);

        let inner = (ored & 0xFFFF) as u16;
        let inner_bits = (16 - inner.leading_zeros() as u8).max(1);
        assert!(inner_bits <= 16);

        let ored = (ored >> (16 - inner_bits)) | (ored & ((1 << inner_bits) - 1));
        let entry_size = match ored {
            0..=0xFF => 1,
            0x100..=0xFFFF => 2,
            0x10000..=0xFFFFFF => 3,
            _ => 4,
        };

        EntryFormat::from_bits(((entry_size - 1) << 4) | (inner_bits - 1)).unwrap()
    }

    /// Compress u32's into packed data using the most compact entry format.
    ///
    /// Returns the computed entry format and the packed data.
    ///
    /// For more info, see:
    /// <https://learn.microsoft.com/en-us/typography/opentype/spec/otvarcommonformats#associating-target-items-to-variation-data>
    // Ported from fonttools' VarIdxMapValue.write method:
    // https://github.com/fonttools/fonttools/blob/6d531fe/Lib/fontTools/ttLib/tables/otConverters.py#L1764-L1781
    fn pack_map_data(mapping: &[u32]) -> (EntryFormat, Vec<u8>) {
        let fmt = DeltaSetIndexMap::get_entry_format(mapping);
        let inner_bits = fmt.bit_count();
        let inner_mask = (1 << inner_bits as u32) - 1;
        let outer_shift = 16 - inner_bits;
        let entry_size = fmt.entry_size();
        assert!((1..=4).contains(&entry_size));

        // omit trailing entries that are the same as the previous one;
        // the last entry is assumed when index is >= map_count
        let mut map_count = mapping.len();
        while map_count > 1 && mapping[map_count - 1] == mapping[map_count - 2] {
            map_count -= 1;
        }

        let mut map_data = Vec::with_capacity(map_count * entry_size as usize);
        for idx in mapping.iter().take(map_count) {
            let idx = ((idx & 0xFFFF0000) >> outer_shift) | (idx & inner_mask);
            // append entry_size bytes to map_data in BigEndian order
            map_data.extend_from_slice(&idx.to_be_bytes()[4 - entry_size as usize..]);
        }
        assert_eq!(map_data.len(), map_count * entry_size as usize);
        (fmt, map_data)
    }
}

impl<I> FromIterator<I> for DeltaSetIndexMap
where
    I: Into<u32>,
{
    /// Create a DeltaSetIndexMap from an iterator of delta-set indices.
    fn from_iter<T: IntoIterator<Item = I>>(iter: T) -> Self {
        let mapping: Vec<u32> = iter.into_iter().map(|v| v.into()).collect();
        let (fmt, map_data) = DeltaSetIndexMap::pack_map_data(&mapping);
        let map_count = map_data.len() / fmt.entry_size() as usize;
        let delta_set_index_map: DeltaSetIndexMap = if map_count <= u16::MAX as usize {
            DeltaSetIndexMap::format_0(fmt, map_count as u16, map_data)
        } else {
            DeltaSetIndexMap::format_1(fmt, map_count as u32, map_data)
        };
        delta_set_index_map
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[test]
    fn point_pack_words() {
        let thing = PackedPointNumbers::Some(vec![1002, 2002, 8408, 12228]);

        let runs = thing.iter_runs().collect::<Vec<_>>();
        assert_eq!(runs.len(), 1);
        assert!(runs[0].are_words);
        assert_eq!(runs[0].last_point, 0);
        assert_eq!(runs[0].points, &[1002, 2002, 8408, 12228]);
    }

    #[test]
    fn serialize_packed_points() {
        let thing = PackedPointNumbers::Some(vec![1002, 2002, 8408, 12228]);

        let bytes = crate::dump_table(&thing).unwrap();
        assert_eq!(thing.compute_size() as usize, bytes.len());
        let (read, _) = read_fonts::tables::variations::PackedPointNumbers::split_off_front(
            FontData::new(&bytes),
        );
        assert_eq!(thing.as_slice(), read.iter().collect::<Vec<_>>());
    }

    #[test]
    fn point_pack_runs() {
        let thing = PackedPointNumbers::Some(vec![5, 25, 225, 1002, 2002, 2008, 2228]);

        let runs = thing.iter_runs().collect::<Vec<_>>();
        assert!(!runs[0].are_words);
        assert_eq!(runs[0].last_point, 0);
        assert_eq!(runs[0].points, &[5, 25, 225]);

        assert!(runs[1].are_words);
        assert_eq!(runs[1].last_point, 225);
        assert_eq!(runs[1].points, &[1002, 2002]);

        assert!(!runs[2].are_words);
        assert_eq!(runs[2].last_point, 2002);
        assert_eq!(runs[2].points, &[2008, 2228]);

        assert_eq!(runs.len(), 3);
    }

    #[test]
    fn point_pack_long_runs() {
        let mut numbers = vec![0u16; 130];
        numbers.extend(1u16..=130u16);
        let thing = PackedPointNumbers::Some(numbers);

        let runs = thing.iter_runs().collect::<Vec<_>>();
        assert!(!runs[0].are_words);
        assert_eq!(runs[0].points.len(), 128);
        assert_eq!(runs[1].last_point, 0);
        assert_eq!(runs[1].points.len(), 128);
        assert_eq!(runs[2].last_point, 126);
        assert_eq!(runs[2].points, &[127, 128, 129, 130]);
        assert!(runs.get(3).is_none());
    }

    #[test]
    fn point_pack_write_one_byte() {
        let thing = PackedPointNumbers::Some(vec![5, 25, 225, 1002, 2002, 2008, 2228, 10000]);

        let bytes = crate::dump_table(&thing).unwrap();
        assert_eq!(thing.compute_size() as usize, bytes.len());
        let (read, _) = read_fonts::tables::variations::PackedPointNumbers::split_off_front(
            FontData::new(&bytes),
        );
        assert_eq!(thing.as_slice(), read.iter().collect::<Vec<_>>());
    }

    #[test]
    fn point_pack_write_two_byte() {
        let thing = PackedPointNumbers::Some(vec![0; 200]);

        let bytes = crate::dump_table(&thing).unwrap();
        assert_eq!(thing.compute_size() as usize, bytes.len());
        let (read, _) = read_fonts::tables::variations::PackedPointNumbers::split_off_front(
            FontData::new(&bytes),
        );
        assert_eq!(thing.as_slice(), read.iter().collect::<Vec<_>>());
    }

    static PACKED_DELTA_BYTES: &[u8] = &[
        0x03, 0x0A, 0x97, 0x00, 0xC6, 0x87, 0x41, 0x10, 0x22, 0xFB, 0x34,
    ];

    // <https://learn.microsoft.com/en-us/typography/opentype/spec/otvarcommonformats#packed-deltas>
    #[test]
    fn packed_deltas_spec_runs() {
        let deltas = PackedDeltas::new(vec![10, -105, 0, -58, 0, 0, 0, 0, 0, 0, 0, 0, 4130, -1228]);
        let runs = deltas.iter_runs().collect::<Vec<_>>();
        assert_eq!(
            runs,
            vec![
                PackedDeltaRun::OneByte(&[10, -105, 0, -58]),
                PackedDeltaRun::Zeros(8),
                PackedDeltaRun::TwoBytes(&[4130, -1228]),
            ]
        );
    }

    #[test]
    fn packed_deltas_spec_write() {
        let deltas = PackedDeltas::new(vec![10, -105, 0, -58, 0, 0, 0, 0, 0, 0, 0, 0, 4130, -1228]);
        let bytes = crate::dump_table(&deltas).unwrap();
        assert_eq!(bytes, PACKED_DELTA_BYTES);
        let read = read_fonts::tables::variations::PackedDeltas::consume_all(FontData::new(&bytes));
        let decoded = read.iter().collect::<Vec<_>>();
        assert_eq!(deltas.deltas.len(), decoded.len());
        assert_eq!(deltas.deltas, decoded);
        assert_eq!(bytes, PACKED_DELTA_BYTES);
    }

    #[test]
    fn empty_deltas() {
        let deltas = PackedDeltas::new(vec![]);
        let bytes = crate::dump_table(&deltas).unwrap();
        assert!(bytes.is_empty());
    }

    #[test]
    fn lots_of_zero() {
        let num_zeroes = 65;
        let deltas = PackedDeltas::new(vec![0; num_zeroes]);
        assert_eq!(
            vec![PackedDeltaRun::Zeros(64), PackedDeltaRun::Zeros(1)],
            deltas.iter_runs().collect::<Vec<_>>()
        );
    }

    #[test]
    fn respect_my_run_length_authority() {
        let mut values = (1..196).collect::<Vec<_>>();
        values.extend([0, 0, 0]);
        values.push(i16::MAX as i32 + 1);
        values.push(i16::MIN as i32 - 1);
        values.push(i16::MAX as i32 * 2);
        let deltas = PackedDeltas::new(values);
        assert_eq!(
            vec![
                // 64 entries per run please and thank you
                PackedDeltaRun::OneByte(&(1..65).collect::<Vec<i32>>()),
                // 63 entries this time because at 128 we switch to 2 bytes
                PackedDeltaRun::OneByte(&(65..128).collect::<Vec<i32>>()),
                // 64 per run again
                PackedDeltaRun::TwoBytes(&(128..192).collect::<Vec<i32>>()),
                // tail
                PackedDeltaRun::TwoBytes(&(192..=195).collect::<Vec<i32>>()),
                PackedDeltaRun::Zeros(3),
                PackedDeltaRun::FourBytes(&[
                    i16::MAX as i32 + 1,
                    i16::MIN as i32 - 1,
                    i16::MAX as i32 * 2
                ]),
            ],
            deltas.iter_runs().collect::<Vec<_>>()
        )
    }

    #[test]
    fn inline_single_zeros_with_bytes() {
        let packed = PackedDeltas::new(vec![1, 2, 0, 3]);
        assert_eq!(packed.iter_runs().count(), 1)
    }

    #[test]
    fn split_two_zeros_in_bytes() {
        let packed = PackedDeltas::new(vec![1, 2, 0, 0, 3]);
        assert_eq!(packed.iter_runs().count(), 3)
    }

    #[test]
    fn split_single_zero_in_words() {
        let packed = PackedDeltas::new(vec![150, 200, 0, -300]);
        assert_eq!(packed.iter_runs().count(), 3)
    }

    #[test]
    fn inline_single_byte_in_words() {
        let packed = PackedDeltas::new(vec![150, 200, 1, -300]);
        assert_eq!(packed.iter_runs().count(), 1)
    }

    #[test]
    fn split_double_byte_in_words() {
        let packed = PackedDeltas::new(vec![150, 200, 1, 3, -300]);
        assert_eq!(packed.iter_runs().count(), 3)
    }

    #[test]
    fn split_byte_then_zero_after_words() {
        // without split: 10 = 1 + 2 + 2 + 2 + 1 + 2
        //    with split:  9 = 1 + 2 + 2 + 1 + 3
        let packed = PackedDeltas::new(vec![150, 200, 1, 0, 1]);
        assert_eq!(packed.iter_runs().count(), 2);
        assert_eq!(packed.compute_size(), 9);
    }

    #[rstest]
    // Note how the packed data below is b"\x00\x01" and not b"\x00\x01\x01", for the
    // repeated trailing values can be omitted
    #[case::one_byte_one_inner_bit(
        vec![0, 1, 1], 0b00_0000, 1, 1, b"\x00\x01",
    )]
    #[case::one_byte_two_inner_bits(
        vec![0, 1, 2], 0b00_0001, 1, 2, b"\x00\x01\x02",
    )]
    #[case::one_byte_three_inner_bits(
        vec![0, 1, 4], 0b00_0010, 1, 3, b"\x00\x01\x04",
    )]
    #[case::one_byte_four_inner_bits(
        vec![0, 1, 8], 0b00_0011, 1, 4, b"\x00\x01\x08",
    )]
    // 256 needs 2 bytes, of which 9 bits for the inner value
    #[case::two_bytes_nine_inner_bits(
        vec![0, 1, 256], 0b01_1000, 2, 9, b"\x00\x00\x00\x01\x01\x00",
    )]
    #[case::two_bytes_sixteen_inner_bits(
        vec![0, 1, 0x8000], 0b01_1111, 2, 16, b"\x00\x00\x00\x01\x80\x00",
    )]
    // note this gets packed the same as case 'one_byte_two_inner_bits': [0, 1, 2]
    // above, but it uses only 1 bit for the inner value, while the other bits are
    // used for the outer value:
    // 0x0001_0000 => b"\x02" => 0b00000010 => {outer: 1, inner: 0)
    #[case::one_byte_one_inner_bit_two_vardatas(
        vec![0, 1, 0x01_0000], 0b00_0000, 1, 1, b"\x00\x01\x02",
    )]
    #[case::three_bytes_sixteen_inner_bits(
        vec![0, 0xFFFF, 0x01_0000],
        0b10_1111,
        3,
        16,
        b"\x00\x00\x00\x00\xFF\xFF\x01\x00\x00",
    )]
    #[case::four_bytes_sixteen_inner_bits(
        vec![0, 0xFFFF, 0xFFFF_FFFF],
        0b11_1111,
        4,
        16,
        b"\x00\x00\x00\x00\x00\x00\xFF\xFF\xFF\xFF\xFF\xFF",
    )]
    #[test]
    fn delta_set_index_map_entry_format_and_packed_data(
        #[case] mapping: Vec<u32>,
        #[case] expected_format_bits: u8,
        #[case] expected_entry_size: u8,
        #[case] expected_inner_bit_count: u8,
        #[case] expected_map_data: &[u8],
    ) {
        let (format, data) = DeltaSetIndexMap::pack_map_data(&mapping);
        assert_eq!(format.bits(), expected_format_bits);
        assert_eq!(format.entry_size(), expected_entry_size);
        assert_eq!(format.bit_count(), expected_inner_bit_count);
        assert_eq!(data, expected_map_data);

        let dsim: DeltaSetIndexMap = mapping.iter().copied().collect();
        // all test mappings have fewer than 65536 entries (for practical reasons)
        // so we should generate a Format0
        assert!(matches!(dsim, DeltaSetIndexMap::Format0 { .. }));

        // make sure we get the same mapping back after round-tripping to/from bytes
        let raw_dsim = crate::dump_table(&dsim).unwrap();
        let dsim2 =
            read_fonts::tables::variations::DeltaSetIndexMap::read(FontData::new(&raw_dsim))
                .unwrap();
        assert_eq!(
            (0..mapping.len())
                .map(|i| {
                    let index = dsim2.get(i as u32).unwrap();
                    ((index.outer as u32) << 16) | index.inner as u32
                })
                .collect::<Vec<_>>(),
            mapping
        );
    }

    #[test]
    fn delta_set_index_map_from_variation_index_iterator() {
        // as returned from VariationStoreBuilder::build() in the VariationIndexRemapping
        use crate::tables::layout::VariationIndex;

        let mapping = vec![
            VariationIndex::new(0, 0),
            VariationIndex::new(0, 1),
            VariationIndex::new(0, 2),
            VariationIndex::new(1, 0),
            VariationIndex::new(1, 1),
            VariationIndex::new(1, 2),
        ];

        let dsim: DeltaSetIndexMap = mapping.into_iter().collect();
        let DeltaSetIndexMap::Format0(dsim) = dsim else {
            panic!("expected DeltaSetIndexMap::Format0, got {:?}", dsim);
        };
        assert_eq!(dsim.map_count, 6);
        assert_eq!(dsim.entry_format.bits(), 0b000001);
        assert_eq!(dsim.entry_format.entry_size(), 1); // one byte per entry
        assert_eq!(dsim.entry_format.bit_count(), 2);
        // for each entry/byte, the right-most 2 bits are the inner value,
        // the remaining bits are the outer value
        assert_eq!(
            dsim.map_data,
            vec![
                0b00_00, // (0, 0)
                0b00_01, // (0, 1)
                0b00_10, // (0, 2)
                0b01_00, // (1, 0)
                0b01_01, // (1, 1)
                0b01_10, // (1, 2)
            ]
        );
    }

    #[test]
    fn huge_mapping_generates_format_1_delta_set_index_map() {
        // 65536 entries, so we need a Format1 with u32 map_count
        let mapping = (0..=0xFFFF).collect::<Vec<u32>>();
        let map_count = mapping.len() as u32;
        let dsim: DeltaSetIndexMap = mapping.into_iter().collect();
        let DeltaSetIndexMap::Format1(dsim) = dsim else {
            panic!("expected DeltaSetIndexMap::Format1, got {:?}", dsim);
        };
        assert_eq!(dsim.map_count, map_count);
    }
}
