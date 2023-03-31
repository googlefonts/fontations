//! The gvar table

include!("../../generated/generated_gvar.rs");

use std::collections::{HashMap, HashSet};

use crate::{collections::HasLen, OffsetMarker};

use super::variations::{
    PackedDeltas, PackedPointNumbers, Tuple, TupleVariationCount, TupleVariationHeader,
};

pub struct GvarBuilder {
    axis_count: u16,
    seen_glyphs: HashSet<GlyphId>,
    glyphs: Vec<RawGlyphVariationData>,
}

/// Variation data for a single glyph, before it is compiled
struct RawGlyphVariationData {
    gid: GlyphId,
    variations: Vec<RawGlyphTupleVariation>,
}

/// Variation data for a single point in varspace, before it is compiled
pub struct RawGlyphTupleVariation {
    peak_tuple: Tuple,
    intermediate_region: Option<(Tuple, Tuple)>,
    x_deltas: Vec<i16>,
    y_deltas: Vec<i16>,
}

impl GvarBuilder {
    pub fn new(axis_count: u16) -> Self {
        GvarBuilder {
            axis_count,
            glyphs: Default::default(),
            seen_glyphs: Default::default(),
        }
    }

    pub fn add(
        &mut self,
        gid: GlyphId,
        variations: Vec<RawGlyphTupleVariation>,
    ) -> Result<(), GvarBuilderError> {
        let n_deltas = variations.first().map(|x| x.x_deltas.len());
        if variations
            .iter()
            .any(|var| Some(var.x_deltas.len()) != n_deltas)
        {
            return Err(GvarBuilderError::DeltaLengthMismatch(gid));
        }
        if let Some(tuple_len) = variations.first().map(|var| var.peak_tuple.len()) {
            if tuple_len != self.axis_count {
                return Err(GvarBuilderError::TupleLengthMismatch {
                    gid,
                    expected: self.axis_count,
                    found: tuple_len,
                });
            }
        }
        if !self.seen_glyphs.insert(gid) {
            return Err(GvarBuilderError::DuplicateGlyphId(gid));
        }

        self.glyphs.push(RawGlyphVariationData { gid, variations });
        Ok(())
    }

    pub fn build(mut self) -> Gvar {
        let shared = self.compute_shared_peak_tuples();
        let shared_idx_map = shared
            .iter()
            .enumerate()
            .map(|(i, x)| (x, i as u16))
            .collect();
        self.glyphs.sort_unstable_by_key(|g| g.gid);
        let glyphs = self
            .glyphs
            .into_iter()
            .map(|raw_g| raw_g.build(&shared_idx_map))
            .collect();

        Gvar {
            axis_count: self.axis_count,
            shared_tuples: SharedTuples::new(shared).into(),
            glyph_variation_data_offsets: glyphs,
        }
    }

    fn compute_shared_peak_tuples(&mut self) -> Vec<Tuple> {
        const MAX_SHARED_TUPLES: usize = 4095;
        let mut peak_tuple_counts = HashMap::new();
        for glyph in &self.glyphs {
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
}

impl RawGlyphVariationData {
    fn count_peak_tuples<'a>(&'a self, counter: &mut HashMap<&'a Tuple, usize>) {
        for tuple in &self.variations {
            *counter.entry(&tuple.peak_tuple).or_default() += 1;
        }
    }

    fn build(self, shared_tuple_map: &HashMap<&Tuple, u16>) -> GlyphVariationData {
        //FIXME: for now we are not doing fancy efficient point encodings,
        //and all tuples contain all points (and so all are stored)
        let shared_points = PackedPointNumbers::new(Vec::new(), true);
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

// GlyphVariationDataTable:
// **header block**
// u16 tupleVariationCount
// u16 dataOffset: offset to the datablock THAT FOLLOWS IMMEDIATELY
// [TupleVariationHeader]
// **data block**
// - shared point numbers
// - per-tuple-var-data:
//  - private point numbers
//  - x deltas
//  - y deltas

impl RawGlyphTupleVariation {
    pub fn new(
        peak_tuple: Tuple,
        x_deltas: Vec<i16>,
        y_deltas: Vec<i16>,
        intermediate_region: Option<(Tuple, Tuple)>,
    ) -> Result<Self, BadTupleVariation> {
        if x_deltas.len() != y_deltas.len() {
            return Err(BadTupleVariation::DeltaLengthMismatch {
                x: x_deltas.len(),
                y: y_deltas.len(),
            });
        }
        match intermediate_region
            .as_ref()
            .map(|(start, end)| (start.len(), end.len()))
        {
            None => (),
            Some((start, end)) if start == peak_tuple.len() && start == end => (),
            Some(_) => return Err(BadTupleVariation::TupleLengthMismatch),
        };

        Ok(RawGlyphTupleVariation {
            peak_tuple,
            intermediate_region,
            x_deltas,
            y_deltas,
        })
    }

    fn build(
        self,
        shared_tuple_map: &HashMap<&Tuple, u16>,
        _shared_points: &PackedPointNumbers,
    ) -> (TupleVariationHeader, GlyphTupleVariationData) {
        let RawGlyphTupleVariation {
            peak_tuple,
            intermediate_region,
            x_deltas,
            y_deltas,
        } = self;
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

/// An error representing malformed glyph tuple variation data.
#[derive(Clone, Debug)]
pub enum BadTupleVariation {
    /// x and y deltas have different lengths
    DeltaLengthMismatch { x: usize, y: usize },
    /// Tuples for this variation have different lengths
    TupleLengthMismatch,
}

#[derive(Clone, Debug)]
pub enum GvarBuilderError {
    /// The same GlyphId has been added twice
    DuplicateGlyphId(GlyphId),
    /// The tuples for this glyph do not match the expected axis count
    TupleLengthMismatch {
        gid: GlyphId,
        expected: u16,
        found: u16,
    },
    /// Delta length varies between different tuples for the same glyph
    DeltaLengthMismatch(GlyphId),
}

#[derive(Clone, Debug, Default)]
pub struct GlyphVariationData {
    // - tuple_variation_count goes here
    // - offset to serialized data
    tuple_variation_headers: Vec<TupleVariationHeader>, // includes the data, at this point
    // optional; present if multiple variations have the same point numbers
    shared_point_numbers: Option<PackedPointNumbers>,
    per_tuple_data: Vec<GlyphTupleVariationData>,
}

#[derive(Clone, Debug)]
pub struct GlyphTupleVariationData {
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

impl Gvar {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_test() {
        let mut builder = GvarBuilder::new(2);
        builder.add(GlyphId::new(0), vec![]).unwrap();
        builder
            .add(
                GlyphId::new(1),
                vec![RawGlyphTupleVariation::new(
                    Tuple::new(vec![F2Dot14::from_f32(1.0), F2Dot14::from_f32(1.0)]),
                    vec![30, 40, -50, 101, 10],
                    vec![31, 41, -49, 102, 11],
                    None,
                )
                .unwrap()],
            )
            .unwrap();
        builder
            .add(
                GlyphId::new(2),
                vec![
                    RawGlyphTupleVariation::new(
                        Tuple::new(vec![F2Dot14::from_f32(1.0), F2Dot14::from_f32(1.0)]),
                        vec![11, 69, -69, 168, 1],
                        vec![-20, -41, 49, 101, 2],
                        None,
                    )
                    .unwrap(),
                    RawGlyphTupleVariation::new(
                        Tuple::new(vec![F2Dot14::from_f32(0.8), F2Dot14::from_f32(1.0)]),
                        vec![3, 4, 5, 6, 7],
                        vec![-200, -500, -800, -1200, -1500],
                        None,
                    )
                    .unwrap(),
                ],
            )
            .unwrap();
        let table = builder.build();
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
}
