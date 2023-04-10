//! OpenType variations common table formats

include!("../../generated/generated_variations.rs");

pub use read_fonts::tables::variations::{TupleIndex, TupleVariationCount};

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

impl VariationRegionList {
    fn compute_axis_count(&self) -> usize {
        let count = self
            .variation_regions
            .first()
            .map(|reg| reg.region_axes.len())
            .unwrap_or(0);
        //TODO: check this at validation time
        debug_assert!(self
            .variation_regions
            .iter()
            .map(|reg| reg.region_axes.len())
            .all(|n| n == count));
        count
    }
}

/// <https://learn.microsoft.com/en-us/typography/opentype/spec/otvarcommonformats#packed-point-numbers>
#[derive(Clone, Debug, Default, Hash, PartialEq, Eq)]
pub enum PackedPointNumbers {
    /// Contains deltas for all point numbers
    #[default]
    All,
    /// Contains deltas only for these specific point numbers
    Some(Vec<u16>),
}

/// <https://learn.microsoft.com/en-us/typography/opentype/spec/otvarcommonformats#packed-deltas>
#[derive(Clone, Debug, Default)]
pub struct PackedDeltas {
    deltas: Vec<i16>,
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
    pub fn new(deltas: Vec<i16>) -> Self {
        Self { deltas }
    }

    /// Compute the number of bytes required to encode these deltas
    pub(crate) fn compute_size(&self) -> u16 {
        self.iter_runs().fold(0u16, |acc, run| {
            acc.checked_add(run.compute_size()).unwrap()
        })
    }

    fn iter_runs(&self) -> impl Iterator<Item = PackedDeltaRun> {
        const MAX_POINTS_PER_RUN: usize = 64;

        fn in_i8_range(val: i16) -> bool {
            const MIN: i16 = i8::MIN as i16;
            const MAX: i16 = i8::MAX as i16;
            (MIN..=MAX).contains(&val)
        }

        fn count_leading_zeros(slice: &[i16]) -> u8 {
            slice
                .iter()
                .take(u8::MAX.into())
                .take_while(|v| **v == 0)
                .count() as u8
        }

        /// compute the number of deltas in the next run, and whether they are i8s or not
        fn next_run_len(slice: &[i16]) -> (usize, bool) {
            let first = *slice.first().expect("bounds checked before here");
            debug_assert!(first != 0);
            let is_1_byte = in_i8_range(first);

            let mut idx = 1;
            while idx < MAX_POINTS_PER_RUN && idx < slice.len() {
                let cur = slice[idx];

                // Any reason to stop?
                let two_zeros = cur == 0 && slice.get(idx + 1) == Some(&0);
                let different_enc_len = in_i8_range(cur) != is_1_byte;
                if two_zeros || different_enc_len {
                    break;
                }

                idx += 1;
            }
            (idx, is_1_byte)
        }

        let mut deltas = self.deltas.as_slice();

        std::iter::from_fn(move || {
            if *deltas.first()? == 0 {
                let n_zeros = count_leading_zeros(deltas);
                deltas = &deltas[n_zeros as usize..];
                Some(PackedDeltaRun::Zeros(n_zeros))
            } else {
                let (len, is_i8) = next_run_len(deltas);
                let (head, tail) = deltas.split_at(len);
                deltas = tail;
                if is_i8 {
                    Some(PackedDeltaRun::OneByte(head))
                } else {
                    Some(PackedDeltaRun::TwoBytes(head))
                }
            }
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum PackedDeltaRun<'a> {
    Zeros(u8),
    OneByte(&'a [i16]),
    TwoBytes(&'a [i16]),
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
        }
    }

    fn compute_size(&self) -> u16 {
        match self {
            PackedDeltaRun::Zeros(_) => 1,
            PackedDeltaRun::OneByte(vals) => vals.len() as u16 + 1,
            PackedDeltaRun::TwoBytes(vals) => vals.len() as u16 * 2 + 1,
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
            PackedDeltaRun::TwoBytes(deltas) => deltas.iter().for_each(|v| v.write_into(writer)),
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
            len => (len as u16).write_into(writer),
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

//FIXME: get that "derive extra traits" stuff merged.
impl std::hash::Hash for Tuple {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.values.hash(state);
    }
}

impl std::cmp::PartialEq for Tuple {
    fn eq(&self, other: &Self) -> bool {
        self.values == other.values
    }
}

impl std::cmp::Eq for Tuple {}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn point_pack_write() {
        let thing = PackedPointNumbers::Some(vec![5, 25, 225, 1002, 2002, 2008, 2228]);

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
        assert_eq!(runs[0], PackedDeltaRun::OneByte(&[10, -105, 0, -58]));
        assert_eq!(runs[1], PackedDeltaRun::Zeros(8));
        assert_eq!(runs[2], PackedDeltaRun::TwoBytes(&[4130, -1228]));
        assert!(runs.get(3).is_none());
    }

    #[test]
    fn packed_deltas_spec_write() {
        let deltas = PackedDeltas::new(vec![10, -105, 0, -58, 0, 0, 0, 0, 0, 0, 0, 0, 4130, -1228]);
        let bytes = crate::dump_table(&deltas).unwrap();
        assert_eq!(bytes, PACKED_DELTA_BYTES);
        let read = read_fonts::tables::variations::PackedDeltas::new(FontData::new(&bytes));
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
        let num_zeroes = u8::MAX as usize + 1;
        let deltas = PackedDeltas::new(vec![0; num_zeroes]);
        assert_eq!(
            vec![PackedDeltaRun::Zeros(u8::MAX), PackedDeltaRun::Zeros(1)],
            deltas.iter_runs().collect::<Vec<_>>()
        );
    }

    #[test]
    fn respect_my_run_length_authority() {
        let values = (1..201).collect::<Vec<_>>();
        let deltas = PackedDeltas::new(values);
        assert_eq!(
            vec![
                // 64 entries per run please and thank you
                PackedDeltaRun::OneByte(&(1..65).collect::<Vec<i16>>()),
                // 63 entries this time because at 128 we switch to 2 bytes
                PackedDeltaRun::OneByte(&(65..128).collect::<Vec<i16>>()),
                // 64 per run again
                PackedDeltaRun::TwoBytes(&(128..192).collect::<Vec<i16>>()),
                // tail
                PackedDeltaRun::TwoBytes(&(192..=200).collect::<Vec<i16>>()),
            ],
            deltas.iter_runs().collect::<Vec<_>>()
        )
    }
}
