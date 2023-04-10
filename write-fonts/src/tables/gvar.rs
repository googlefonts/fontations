//! The gvar table

include!("../../generated/generated_gvar.rs");

use std::collections::HashMap;

use crate::{collections::HasLen, OffsetMarker};

use super::variations::{
    PackedDeltas, PackedPointNumbers, Tuple, TupleVariationCount, TupleVariationHeader,
};

/// Variation data for a single glyph, before it is compiled
#[derive(Clone, Debug)]
pub struct GlyphVariations {
    gid: GlyphId,
    variations: Vec<GlyphDeltas>,
}

/// Glyph deltas for one point in the design space.
#[derive(Clone, Debug)]
pub struct GlyphDeltas {
    peak_tuple: Tuple,
    // start and end tuples of optional intermediate region
    intermediate_region: Option<(Tuple, Tuple)>,
    // array of (x, y) deltas, one foar each point in the glyph
    deltas: Vec<(i16, i16)>,
}

/// An error representing invalid input when building a gvar table
#[derive(Clone, Debug)]
pub enum GvarInputError {
    /// Glyphs do not have a consistent axis count
    InconsistentAxisCount,
    /// A single glyph contains variations with inconsistent axis counts
    InconsistentGlyphAxisCount(GlyphId),
    /// A single glyph contains variations with different delta counts
    InconsistentDeltaLength(GlyphId),
    /// A variation in this glyph contains an intermediate region with a
    /// different length than the peak.
    InconsistentTupleLengths(GlyphId),
}

impl Gvar {
    /// Construct a gvar table from a vector of per-glyph variations.
    ///
    /// Variations must be present for each glyph, but may be empty.
    pub fn new(mut variations: Vec<GlyphVariations>) -> Result<Self, GvarInputError> {
        // a helper that handles input validation, and returns axis count
        fn validate_variations(variations: &[GlyphVariations]) -> Result<u16, GvarInputError> {
            for var in variations {
                var.validate()?;
            }

            let axis_count = variations
                .iter()
                .find_map(GlyphVariations::axis_count)
                .unwrap_or_default();
            if variations
                .iter()
                .filter_map(GlyphVariations::axis_count)
                .any(|x| x != axis_count)
            {
                return Err(GvarInputError::InconsistentAxisCount);
            }
            Ok(axis_count)
        }

        fn compute_shared_peak_tuples(glyphs: &[GlyphVariations]) -> Vec<Tuple> {
            const MAX_SHARED_TUPLES: usize = 4095;
            let mut peak_tuple_counts = HashMap::new();
            for glyph in glyphs {
                glyph.count_peak_tuples(&mut peak_tuple_counts);
            }
            let mut to_share = peak_tuple_counts
                .into_iter()
                .filter(|(_, n)| *n > 1)
                .collect::<Vec<_>>();
            to_share.sort_unstable_by_key(|(_, n)| std::cmp::Reverse(*n));
            to_share.truncate(MAX_SHARED_TUPLES);
            to_share.into_iter().map(|(t, _)| t.to_owned()).collect()
        }

        // inline functions, hard to read and hard to test.

        let axis_count = validate_variations(&variations)?;

        let shared = compute_shared_peak_tuples(&variations);
        eprintln!("share {}", shared.len());
        let shared_idx_map = shared
            .iter()
            .enumerate()
            .map(|(i, x)| (x, i as u16))
            .collect();
        variations.sort_unstable_by_key(|g| g.gid);
        let glyphs = variations
            .into_iter()
            .map(|raw_g| raw_g.build(&shared_idx_map))
            .collect();

        Ok(Gvar {
            axis_count,
            shared_tuples: SharedTuples::new(shared).into(),
            glyph_variation_data_offsets: glyphs,
        })
    }

    fn compute_flags(&self) -> GvarFlags {
        //TODO: use short offsets sometimes
        GvarFlags::LONG_OFFSETS
    }

    fn compute_glyph_count(&self) -> u16 {
        self.glyph_variation_data_offsets.len().try_into().unwrap()
    }

    fn compute_data_array_offset(&self) -> u32 {
        const BASE_OFFSET: usize = MajorMinor::RAW_BYTE_LEN
            + u16::RAW_BYTE_LEN // axis count
            + u16::RAW_BYTE_LEN // shared tuples count
            + Offset32::RAW_BYTE_LEN
            + u16::RAW_BYTE_LEN + u16::RAW_BYTE_LEN // glyph count, flags
            + u32::RAW_BYTE_LEN; // glyph_variation_data_array_offset

        let bytes_per_offset = if self.compute_flags() == GvarFlags::LONG_OFFSETS {
            u32::RAW_BYTE_LEN
        } else {
            u16::RAW_BYTE_LEN
        };

        let offsets_len = (self.glyph_variation_data_offsets.len() + 1) * bytes_per_offset;

        (BASE_OFFSET + offsets_len).try_into().unwrap()
    }

    fn compile_variation_data(&self) -> GlyphDataWriter {
        GlyphDataWriter {
            long_offsets: self.compute_flags() == GvarFlags::LONG_OFFSETS,
            data: &self.glyph_variation_data_offsets,
        }
    }
}

impl GlyphVariations {
    /// Construct a new set of variation deltas for a glyph.
    pub fn new(gid: GlyphId, variations: Vec<GlyphDeltas>) -> Self {
        Self { gid, variations }
    }

    /// called when we build gvar, so we only return errors in one place
    fn validate(&self) -> Result<(), GvarInputError> {
        let (axis_count, delta_len) = self
            .variations
            .first()
            .map(|var| (var.peak_tuple.len(), var.deltas.len()))
            .unwrap_or_default();
        for var in &self.variations {
            if var.peak_tuple.len() != axis_count {
                return Err(GvarInputError::InconsistentGlyphAxisCount(self.gid));
            }
            if let Some((start, end)) = var.intermediate_region.as_ref() {
                if start.len() != axis_count || end.len() != axis_count {
                    return Err(GvarInputError::InconsistentTupleLengths(self.gid));
                }
            }
            if var.deltas.len() != delta_len {
                return Err(GvarInputError::InconsistentDeltaLength(self.gid));
            }
        }
        Ok(())
    }

    /// Will be `None` if there are no variations for this glyph
    pub fn axis_count(&self) -> Option<u16> {
        self.variations.first().map(|var| var.peak_tuple.len())
    }

    fn count_peak_tuples<'a>(&'a self, counter: &mut HashMap<&'a Tuple, usize>) {
        for tuple in &self.variations {
            *counter.entry(&tuple.peak_tuple).or_default() += 1;
        }
    }

    fn build(self, shared_tuple_map: &HashMap<&Tuple, u16>) -> GlyphVariationData {
        //FIXME: for now we are not doing fancy efficient point encodings,
        //and all tuples contain all points (and so all are stored)
        let shared_points = PackedPointNumbers::All;
        let (tuple_headers, tuple_data): (Vec<_>, Vec<_>) = self
            .variations
            .into_iter()
            .map(|tup| tup.build(shared_tuple_map, &shared_points))
            .unzip();

        GlyphVariationData {
            tuple_variation_headers: tuple_headers,
            shared_point_numbers: Some(shared_points),
            per_tuple_data: tuple_data,
        }
    }
}

impl GlyphDeltas {
    /// Create a new set of deltas.
    pub fn new(
        peak_tuple: Tuple,
        deltas: Vec<(i16, i16)>,
        intermediate_region: Option<(Tuple, Tuple)>,
    ) -> Self {
        if let Some((start, end)) = intermediate_region.as_ref() {
            assert!(
                start.len() == end.len() && start.len() == peak_tuple.len(),
                "all tuples must have equal length"
            );
        }
        GlyphDeltas {
            peak_tuple,
            intermediate_region,
            deltas,
        }
    }

    fn build(
        self,
        shared_tuple_map: &HashMap<&Tuple, u16>,
        _shared_points: &PackedPointNumbers,
    ) -> (TupleVariationHeader, GlyphTupleVariationData) {
        let GlyphDeltas {
            peak_tuple,
            intermediate_region,
            deltas,
        } = self;
        let (x_deltas, y_deltas) = deltas.into_iter().unzip();
        let data = GlyphTupleVariationData {
            private_point_numbers: None,
            x_deltas: PackedDeltas::new(x_deltas),
            y_deltas: PackedDeltas::new(y_deltas),
        };

        let data_size = data.compute_size();
        let (idx, peak_tuple) = match shared_tuple_map.get(&peak_tuple) {
            Some(idx) => (Some(*idx), None),
            None => (None, Some(peak_tuple)),
        };

        let header =
            TupleVariationHeader::new(data_size, idx, peak_tuple, intermediate_region, false);

        (header, data)
    }
}

/// The serializable representation of a glyph's variation data
#[derive(Clone, Debug, Default)]
pub struct GlyphVariationData {
    tuple_variation_headers: Vec<TupleVariationHeader>,
    // optional; present if multiple variations have the same point numbers
    shared_point_numbers: Option<PackedPointNumbers>,
    per_tuple_data: Vec<GlyphTupleVariationData>,
}

/// The serializable representation of a single glyph tuple variation data
#[derive(Clone, Debug)]
struct GlyphTupleVariationData {
    // this is possibly shared, if multiple are identical for a given glyph
    private_point_numbers: Option<PackedPointNumbers>,
    x_deltas: PackedDeltas,
    y_deltas: PackedDeltas,
}

impl GlyphTupleVariationData {
    fn compute_size(&self) -> u16 {
        self.private_point_numbers
            .as_ref()
            .map(PackedPointNumbers::compute_size)
            .unwrap_or_default()
            .checked_add(self.x_deltas.compute_size())
            .unwrap()
            .checked_add(self.y_deltas.compute_size())
            .unwrap()
    }
}

impl FontWrite for GlyphTupleVariationData {
    fn write_into(&self, writer: &mut TableWriter) {
        self.private_point_numbers.write_into(writer);
        self.x_deltas.write_into(writer);
        self.y_deltas.write_into(writer);
    }
}

struct GlyphDataWriter<'a> {
    long_offsets: bool,
    data: &'a [GlyphVariationData],
}

impl FontWrite for GlyphDataWriter<'_> {
    fn write_into(&self, writer: &mut TableWriter) {
        assert!(self.long_offsets, "short offset logic not implemented");
        let mut last = 0u32;
        last.write_into(writer);

        // write all the offsets
        for glyph in self.data {
            last += glyph.compute_size();
            last.write_into(writer);
        }
        // then write the actual data
        for glyph in self.data {
            if !glyph.is_empty() {
                glyph.write_into(writer);
            }
        }
    }
}

impl GlyphVariationData {
    fn compute_tuple_variation_count(&self) -> TupleVariationCount {
        assert!(self.tuple_variation_headers.len() <= 4095);
        let mut bits = self.tuple_variation_headers.len() as u16;
        if self.shared_point_numbers.is_some() {
            bits |= TupleVariationCount::SHARED_POINT_NUMBERS;
        }
        TupleVariationCount::from_bits(bits)
    }

    fn is_empty(&self) -> bool {
        self.tuple_variation_headers.is_empty()
    }

    fn compute_data_offset(&self) -> u16 {
        let header_len = self
            .tuple_variation_headers
            .iter()
            .fold(0usize, |acc, header| {
                acc.checked_add(header.compute_size() as usize).unwrap()
            });
        (header_len + TupleVariationCount::RAW_BYTE_LEN + u16::RAW_BYTE_LEN)
            .try_into()
            .unwrap()
    }

    fn compute_size(&self) -> u32 {
        if self.is_empty() {
            return 0;
        }

        let data_start = self.compute_data_offset() as u32;
        let shared_point_len = self
            .shared_point_numbers
            .as_ref()
            .map(|pts| pts.compute_size())
            .unwrap_or_default() as u32;
        let tuple_data_len = self
            .per_tuple_data
            .iter()
            .fold(0u32, |acc, tup| acc + tup.compute_size() as u32);
        data_start + shared_point_len + tuple_data_len
    }
}

impl Validate for GlyphVariationData {
    fn validate_impl(&self, ctx: &mut ValidationCtx) {
        const MAX_TUPLE_VARIATIONS: usize = 4095;
        if !(0..=MAX_TUPLE_VARIATIONS).contains(&self.tuple_variation_headers.len()) {
            ctx.in_field("tuple_variation_headers", |ctx| {
                ctx.report("expected 0-4095 tuple variation tables")
            })
        }
    }
}

impl FontWrite for GlyphVariationData {
    fn write_into(&self, writer: &mut TableWriter) {
        self.compute_tuple_variation_count().write_into(writer);
        self.compute_data_offset().write_into(writer);
        self.tuple_variation_headers.write_into(writer);
        self.shared_point_numbers.write_into(writer);
        self.per_tuple_data.write_into(writer);
    }
}

impl HasLen for SharedTuples {
    fn len(&self) -> usize {
        self.tuples.len()
    }
}

#[derive(Clone, Debug, Default)]
struct GvarPerTupleData {
    private_point_numbers: Option<PackedPointNumbers>,
    x_deltas: PackedDeltas,
    y_deltas: PackedDeltas,
}

impl FontWrite for GvarPerTupleData {
    fn write_into(&self, writer: &mut TableWriter) {
        if let Some(points) = &self.private_point_numbers {
            points.write_into(writer);
        }
        self.x_deltas.write_into(writer);
        self.y_deltas.write_into(writer);
    }
}

impl FontWrite for TupleVariationCount {
    fn write_into(&self, writer: &mut TableWriter) {
        self.bits().write_into(writer)
    }
}

impl std::fmt::Display for GvarInputError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GvarInputError::InconsistentAxisCount => {
                write!(f, "Glyphs do not have a consistent axis count")
            }
            GvarInputError::InconsistentGlyphAxisCount(gid) => write!(
                f,
                "Glyph {gid} contains variations with inconsistent axis counts"
            ),
            GvarInputError::InconsistentDeltaLength(gid) => write!(
                f,
                "Glyph {gid} contains variations with inconsistent delta counts"
            ),
            GvarInputError::InconsistentTupleLengths(gid) => write!(
                f,
                "Glyph {gid} contains variations with inconsistent intermediate region sizes"
            ),
        }
    }
}

impl std::error::Error for GvarInputError {}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_test() {
        let table = Gvar::new(vec![
            GlyphVariations::new(GlyphId::new(0), vec![]),
            GlyphVariations::new(
                GlyphId::new(1),
                vec![GlyphDeltas::new(
                    Tuple::new(vec![F2Dot14::from_f32(1.0), F2Dot14::from_f32(1.0)]),
                    vec![(30, 31), (40, 41), (-50, -49), (101, 102), (10, 11)],
                    None,
                )],
            ),
            GlyphVariations::new(
                GlyphId::new(2),
                vec![
                    GlyphDeltas::new(
                        Tuple::new(vec![F2Dot14::from_f32(1.0), F2Dot14::from_f32(1.0)]),
                        vec![(11, -20), (69, -41), (-69, 49), (168, 101), (1, 2)],
                        None,
                    ),
                    GlyphDeltas::new(
                        Tuple::new(vec![F2Dot14::from_f32(0.8), F2Dot14::from_f32(1.0)]),
                        vec![(3, -200), (4, -500), (5, -800), (6, -1200), (7, -1500)],
                        None,
                    ),
                ],
            ),
        ])
        .unwrap();
        let g2 = &table.glyph_variation_data_offsets[1];
        let computed = g2.compute_size();
        let actual = crate::dump_table(g2).unwrap().len();
        assert_eq!(computed as usize, actual);

        let bytes = crate::dump_table(&table).unwrap();
        let gvar = read_fonts::tables::gvar::Gvar::read(FontData::new(&bytes)).unwrap();
        assert_eq!(gvar.version(), MajorMinor::VERSION_1_0);
        assert_eq!(gvar.shared_tuple_count(), 1);
        assert_eq!(gvar.glyph_count(), 3);

        let g1 = gvar.glyph_variation_data(GlyphId::new(1)).unwrap();
        let g1tup = g1.tuples().collect::<Vec<_>>();
        assert_eq!(g1tup.len(), 1);

        let (x, y): (Vec<_>, Vec<_>) = g1tup[0].deltas().map(|d| (d.x_delta, d.y_delta)).unzip();
        assert_eq!(x, vec![30, 40, -50, 101, 10]);
        assert_eq!(y, vec![31, 41, -49, 102, 11]);

        let g2 = gvar.glyph_variation_data(GlyphId::new(2)).unwrap();
        let g2tup = g2.tuples().collect::<Vec<_>>();
        assert_eq!(g2tup.len(), 2);

        let (x, y): (Vec<_>, Vec<_>) = g2tup[0].deltas().map(|d| (d.x_delta, d.y_delta)).unzip();
        assert_eq!(x, vec![11, 69, -69, 168, 1]);
        assert_eq!(y, vec![-20, -41, 49, 101, 2]);

        let (x, y): (Vec<_>, Vec<_>) = g2tup[1].deltas().map(|d| (d.x_delta, d.y_delta)).unzip();

        assert_eq!(x, vec![3, 4, 5, 6, 7]);
        assert_eq!(y, vec![-200, -500, -800, -1200, -1500]);
    }

    // Extracted from a fontmake-rs bug w/Texturina's A
    #[test]
    fn round_trip_glyph_variations() {
        let deltas = vec![
            (56, 0),
            (61, 0),
            (60, 3),
            (60, 6),
            (55, 9),
            (55, 9),
            (55, 11),
            (54, 13),
            (54, 14),
            (2, 19),
            (0, 15),
            (6, 11),
            (11, 8),
            (15, 20),
            (10, -2),
            (-8, -2),
            (-19, 23),
            (-21, 23),
            (-21, 23),
            (-21, 23),
            (-21, 23),
            (-39, 23),
            (-40, 22),
            (-40, 19),
            (-42, 16),
            (-42, 16),
            (-42, 15),
            (-33, 13),
            (-23, 10),
            (-11, 7),
            (1, 4),
            (10, 3),
            (20, 0),
            (20, 0),
            (26, 0),
            (28, 3),
            (29, 6),
            (31, 10),
            (31, 15),
            (31, 15),
            (30, 15),
            (30, 16),
            (21, 16),
            (24, 15),
            (22, 16),
            (20, 16),
            (20, 16),
            (43, -23),
            (38, -19),
            (36, -15),
            (33, -10),
            (29, -8),
            (25, -5),
            (20, -5),
            (16, -5),
            (10, -9),
            (3, -12),
            (-3, -17),
            (-10, -22),
            (-14, -26),
            (-19, -29),
            (-18, -29),
            (10, 85),
            (0, 49),
            (-6, 34),
            (-11, 20),
            (-14, 17),
            (-16, 15),
            (-19, 15),
            (-11, 15),
            (-11, 14),
            (-11, 14),
            (-11, 12),
            (-11, 10),
            (-11, 9),
            (-1, 8),
            (9, 6),
            (23, 4),
            (36, 3),
            (46, 1),
            (56, 0),
            (15, 16),
            (8, -23),
            (6, -24),
            (4, -23),
            (3, -21),
            (6, -25),
            (-11, 18),
            (4, 17),
            (9, 16),
            (15, 16),
            (16, 15),
            (16, 15),
            (0, 0),
            (8, 0),
            (0, 0),
            (0, 0),
        ];

        for i in 1..deltas.len() {
            let deltas = deltas[..i].to_vec();
            assert_eq!(i, deltas.len());
            let table = Gvar::new(vec![GlyphVariations::new(
                GlyphId::new(0),
                vec![GlyphDeltas::new(
                    Tuple::new(vec![F2Dot14::from_f32(1.0)]),
                    deltas.clone(),
                    None,
                )],
            )])
            .unwrap();

            let bytes = crate::dump_table(&table).unwrap();
            let gvar = read_fonts::tables::gvar::Gvar::read(FontData::new(&bytes)).unwrap();
            assert_eq!(gvar.version(), MajorMinor::VERSION_1_0);
            assert_eq!(gvar.shared_tuple_count(), 0);
            assert_eq!(gvar.glyph_count(), 1);

            let var_data = gvar.glyph_variation_data(GlyphId::new(0)).unwrap();
            let tuples = var_data.tuples().collect::<Vec<_>>();
            assert_eq!(tuples.len(), 1);

            let recovered_deltas = tuples[0]
                .deltas()
                .map(|d| (d.x_delta, d.y_delta))
                .collect::<Vec<_>>();
            eprintln!(
                "i={i}, deltas.len {} recovered.len {}",
                deltas.len(),
                recovered_deltas.len()
            );
            assert_eq!(deltas, recovered_deltas, "Failed at i={i}");
        }
    }
}
