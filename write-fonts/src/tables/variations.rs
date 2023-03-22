//! OpenType variations common table formats

include!("../../generated/generated_variations.rs");

pub use read_fonts::tables::variations::TupleIndex;

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
pub struct PackedPointNumbers {
    is_all: bool,
    numbers: Vec<u16>,
}

/// <https://learn.microsoft.com/en-us/typography/opentype/spec/otvarcommonformats#packed-deltas>
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

    fn iter_runs(&self) -> impl Iterator<Item = PackedDeltaRun> {
        const MAX_POINTS_PER_RUN: usize = 64;

        fn in_i8_range(val: i16) -> bool {
            const MIN: i16 = i8::MIN as i16;
            const MAX: i16 = i8::MAX as i16;
            (MIN..=MAX).contains(&val)
        }

        fn count_leading_zeros(slice: &[i16]) -> usize {
            slice.iter().position(|x| *x != 0).unwrap_or(slice.len())
        }

        /// compute the number of deltas in the next run, and whether they are i8s or not
        fn next_run_len(slice: &[i16]) -> (usize, bool) {
            let (first, rest) = slice.split_first().expect("bounds checked before here");
            debug_assert!(*first != 0);
            let is_1_byte = in_i8_range(*first);

            for (i, raw) in rest.iter().copied().enumerate().take(MAX_POINTS_PER_RUN) {
                let two_zeros = raw == 0 && rest.get(i + 1) == Some(&0);
                let different_enc_len = in_i8_range(raw) != is_1_byte;
                if two_zeros || different_enc_len {
                    return (i + 1, is_1_byte);
                }
            }

            (slice.len(), is_1_byte)
        }

        let mut deltas = self.deltas.as_slice();

        std::iter::from_fn(move || {
            if deltas.is_empty() {
                return None;
            }
            // first handle the zero case
            let n_zeros = count_leading_zeros(deltas);
            if n_zeros > 0 {
                deltas = &deltas[n_zeros..];
                return Some(PackedDeltaRun::Zeros(n_zeros as u8));
            }

            let (len, is_i8) = next_run_len(deltas);
            let (head, tail) = deltas.split_at(len);
            deltas = tail;
            let result = if is_i8 {
                PackedDeltaRun::OneByte(head)
            } else {
                PackedDeltaRun::TwoBytes(head)
            };
            Some(result)
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
        if self.numbers.len() > 0x7FFF {
            ctx.report("length cannot be stored in 15 bites");
        }
    }
}

impl FontWrite for PackedPointNumbers {
    fn write_into(&self, writer: &mut TableWriter) {
        // compute the actual count:
        if self.is_all {
            0u8.write_into(writer);
        } else if self.numbers.len() <= 127 {
            (self.numbers.len() as u8).write_into(writer);
        } else {
            (self.numbers.len() as u16).write_into(writer);
        }

        for run in self.iter_runs() {
            run.write_into(writer);
        }
    }
}

impl PackedPointNumbers {
    /// Create new packed numbers from raw numbers.
    ///
    /// The `is_all` flag should be true if there is a number value for each
    /// point in the corresponding glyph (or CVT value in the cvar table).
    pub fn new(numbers: Vec<u16>, is_all: bool) -> Self {
        Self { is_all, numbers }
    }
    fn iter_runs(&self) -> impl Iterator<Item = PackedPointRun> {
        const U8_MAX: u16 = u8::MAX as u16;
        const MAX_POINTS_PER_RUN: usize = 128;

        let mut points = self.numbers.as_slice();
        let mut prev_point = 0u16;

        // split a run off the front of points:
        // - if point is more than 255 away from prev, we're using words
        std::iter::from_fn(move || {
            let next = points.first()?;
            let (run_len, are_words) = if (next - prev_point) > U8_MAX {
                let count = points
                    .iter()
                    .take(MAX_POINTS_PER_RUN)
                    .scan(prev_point, |prev, point| {
                        let result = (point - *prev > U8_MAX).then_some(point);
                        *prev = *point;
                        result
                    })
                    .count();
                (count, true)
            } else {
                let count = points
                    .iter()
                    .take(MAX_POINTS_PER_RUN)
                    .scan(prev_point, |prev, point| {
                        let result = (point - *prev <= U8_MAX).then_some(point);
                        *prev = *point;
                        result
                    })
                    .count();
                (count, false)
            };

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

struct PackedPointRun<'a> {
    last_point: u16,
    are_words: bool,
    points: &'a [u16],
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_pack_words() {
        let thing = PackedPointNumbers {
            is_all: false,
            numbers: vec![1002, 2002, 8408, 12228],
        };

        let runs = thing.iter_runs().collect::<Vec<_>>();
        assert!(runs[0].are_words);
        assert_eq!(runs[0].last_point, 0);
        assert_eq!(runs[0].points, &[1002, 2002, 8408, 12228]);

        let bytes = crate::dump_table(&thing).unwrap();
        let (read, _) = read_fonts::tables::variations::PackedPointNumbers::split_off_front(
            FontData::new(&bytes),
        );
        assert_eq!(thing.numbers, read.iter().collect::<Vec<_>>());
    }

    #[test]
    fn smoke_test_point_packing() {
        let thing = PackedPointNumbers {
            is_all: false,
            numbers: vec![5, 25, 225, 1002, 2002, 2008, 2228],
        };

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
        let bytes = crate::dump_table(&thing).unwrap();
        let (read, _) = read_fonts::tables::variations::PackedPointNumbers::split_off_front(
            FontData::new(&bytes),
        );
        assert_eq!(thing.numbers, read.iter().collect::<Vec<_>>());
    }

    // <https://learn.microsoft.com/en-us/typography/opentype/spec/otvarcommonformats#packed-deltas>
    #[test]
    fn packed_deltas_spec() {
        static EXPECTED: &[u8] = &[
            0x03, 0x0A, 0x97, 0x00, 0xC6, 0x87, 0x41, 0x10, 0x22, 0xFB, 0x34,
        ];

        let deltas = PackedDeltas::new(vec![10, -105, 0, -58, 0, 0, 0, 0, 0, 0, 0, 0, 4130, -1228]);
        let runs = deltas.iter_runs().collect::<Vec<_>>();
        assert_eq!(runs[0], PackedDeltaRun::OneByte(&[10, -105, 0, -58]));
        assert_eq!(runs[1], PackedDeltaRun::Zeros(8));
        assert_eq!(runs[2], PackedDeltaRun::TwoBytes(&[4130, -1228]));

        let bytes = crate::dump_table(&deltas).unwrap();
        assert_eq!(bytes, EXPECTED);
        let read = read_fonts::tables::variations::PackedDeltas::new(FontData::new(&bytes));
        let decoded = read.iter().collect::<Vec<_>>();
        assert_eq!(deltas.deltas.len(), decoded.len());
        assert_eq!(deltas.deltas, decoded);
        assert_eq!(bytes, EXPECTED);
    }
}
