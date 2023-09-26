//! The gvar table

include!("../../generated/generated_gvar.rs");

use std::collections::HashMap;

use crate::{collections::HasLen, OffsetMarker};

use super::variations::{
    PackedDeltas, PackedPointNumbers, Tuple, TupleVariationCount, TupleVariationHeader,
};

pub mod iup;

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
    // (x, y) deltas or None for do not encode. One entry per point in the glyph.
    deltas: Vec<GlyphDelta>,
    best_point_packing: PackedPointNumbers,
}

/// A delta for a single value in a glyph.
///
/// This includes a flag indicating whether or not this delta is required (i.e
/// it cannot be interpolated from neighbouring deltas and coordinates).
/// This is only relevant for simple glyphs; interpolatable points may be omitted
/// in the final binary when doing so saves space.
/// See <https://learn.microsoft.com/en-us/typography/opentype/spec/gvar#inferred-deltas-for-un-referenced-point-numbers>
/// for more information.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct GlyphDelta {
    pub x: i16,
    pub y: i16,
    /// This delta must be included, i.e. cannot be interpolated
    pub required: bool,
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

        let axis_count = validate_variations(&variations)?;

        let shared = compute_shared_peak_tuples(&variations);
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

    /// Determine if we should use 'shared point numbers'
    ///
    /// If multiple tuple variations for a given glyph use the same point numbers,
    /// it is possible to store this in the glyph table, avoiding duplicating
    /// data.
    ///
    /// This implementation is currently based on the one in fonttools, where it
    /// is part of the compileTupleVariationStore method:
    /// <https://github.com/fonttools/fonttools/blob/0a3360e52727cdefce2e9b28286b074faf99033c/Lib/fontTools/ttLib/tables/TupleVariation.py#L641>
    ///
    /// # Note
    ///
    /// There is likely room for some optimization here, depending on the
    /// structure of the point numbers. If it is common for point numbers to only
    /// vary by an item or two, it may be worth picking a set of shared points
    /// that is a subset of multiple different tuples; this would mean you could
    /// make some tuples include deltas that they might otherwise omit, but let
    /// them omit their explicit point numbers.
    ///
    /// For fonts with a large number of variations, this could produce reasonable
    /// savings, at the cost of a significantly more complicated algorithm.
    fn compute_shared_points(&self) -> Option<PackedPointNumbers> {
        let mut point_number_counts = HashMap::new();
        // count how often each set of numbers occurs
        for deltas in &self.variations {
            // for each set points, get compiled size + number of occurances
            let (_, count) = point_number_counts
                .entry(&deltas.best_point_packing)
                .or_insert_with(|| {
                    let size = deltas.best_point_packing.compute_size();
                    (size as usize, 0usize)
                });
            *count += 1;
        }

        // find the one that saves the most bytes
        let (pts, (_, count)) = point_number_counts
            .into_iter()
            .filter(|(_, (_, count))| *count > 1)
            .max_by_key(|(_, (size, count))| *count * *size)?;

        // no use sharing points if they only occur once
        (count > 1).then(|| pts.to_owned())
    }

    fn build(self, shared_tuple_map: &HashMap<&Tuple, u16>) -> GlyphVariationData {
        let shared_points = self.compute_shared_points();

        let (tuple_headers, tuple_data): (Vec<_>, Vec<_>) = self
            .variations
            .into_iter()
            .map(|tup| tup.build(shared_tuple_map, shared_points.as_ref()))
            .unzip();

        GlyphVariationData {
            tuple_variation_headers: tuple_headers,
            shared_point_numbers: shared_points,
            per_tuple_data: tuple_data,
        }
    }
}

impl GlyphDelta {
    /// Create a new delta value.
    pub fn new(x: i16, y: i16, required: bool) -> Self {
        Self { x, y, required }
    }

    /// Create a new delta value that must be encoded (cannot be interpolated)
    pub fn required(x: i16, y: i16) -> Self {
        Self::new(x, y, true)
    }

    /// Create a new delta value that may be omitted (can be interpolated)
    pub fn optional(x: i16, y: i16) -> Self {
        Self::new(x, y, false)
    }

    fn to_tuple(self) -> (i16, i16) {
        (self.x, self.y)
    }
}

impl GlyphDeltas {
    /// Create a new set of deltas.
    ///
    /// A None delta means do not explicitly encode, typically because IUP suggests
    /// it isn't required.
    pub fn new(
        peak_tuple: Tuple,
        deltas: Vec<GlyphDelta>,
        intermediate_region: Option<(Tuple, Tuple)>,
    ) -> Self {
        if let Some((start, end)) = intermediate_region.as_ref() {
            assert!(
                start.len() == end.len() && start.len() == peak_tuple.len(),
                "all tuples must have equal length"
            );
        }
        // at construction time we build both iup optimized & not versions
        // of ourselves, to determine what representation is most efficient;
        // the caller will look at the generated packed points to decide which
        // set should be shared.
        let best_point_packing = Self::pick_best_point_number_repr(&deltas);
        GlyphDeltas {
            peak_tuple,
            intermediate_region,
            deltas,
            best_point_packing,
        }
    }

    // this is a type method just to expose it for testing, we call it before
    // we finish instantiating self.
    //
    // NOTE: we do a lot of duplicate work here with creating & throwing away
    // buffers, and that can be improved at the cost of a bit more complexity
    fn pick_best_point_number_repr(deltas: &[GlyphDelta]) -> PackedPointNumbers {
        if deltas.iter().all(|d| d.required) {
            return PackedPointNumbers::All;
        }

        let dense = Self::build_non_sparse_data(deltas);
        let sparse = Self::build_sparse_data(deltas);
        let dense_size = dense.compute_size();
        let sparse_size = sparse.compute_size();
        log::trace!("dense {dense_size}, sparse {sparse_size}");
        if sparse_size < dense_size {
            sparse.private_point_numbers.unwrap()
        } else {
            PackedPointNumbers::All
        }
    }

    fn build_non_sparse_data(deltas: &[GlyphDelta]) -> GlyphTupleVariationData {
        let (x_deltas, y_deltas) = deltas.iter().map(|delta| (delta.x, delta.y)).unzip();
        GlyphTupleVariationData {
            private_point_numbers: Some(PackedPointNumbers::All),
            x_deltas: PackedDeltas::new(x_deltas),
            y_deltas: PackedDeltas::new(y_deltas),
        }
    }

    fn build_sparse_data(deltas: &[GlyphDelta]) -> GlyphTupleVariationData {
        let (x_deltas, y_deltas) = deltas
            .iter()
            .filter_map(|delta| delta.required.then_some((delta.x, delta.y)))
            .unzip();
        let point_numbers = deltas
            .iter()
            .enumerate()
            .filter_map(|(i, delta)| delta.required.then_some(i as u16))
            .collect();
        GlyphTupleVariationData {
            private_point_numbers: Some(PackedPointNumbers::Some(point_numbers)),
            x_deltas: PackedDeltas::new(x_deltas),
            y_deltas: PackedDeltas::new(y_deltas),
        }
    }

    // shared points is just "whatever points, if any, are shared." We are
    // responsible for seeing if these are actually our points, in which case
    // we are using shared points.
    fn build(
        self,
        shared_tuple_map: &HashMap<&Tuple, u16>,
        shared_points: Option<&PackedPointNumbers>,
    ) -> (TupleVariationHeader, GlyphTupleVariationData) {
        let GlyphDeltas {
            peak_tuple,
            intermediate_region,
            deltas,
            best_point_packing: point_numbers,
        } = self;

        let (idx, peak_tuple) = match shared_tuple_map.get(&peak_tuple) {
            Some(idx) => (Some(*idx), None),
            None => (None, Some(peak_tuple)),
        };

        let has_private_points = Some(&point_numbers) != shared_points;
        let (x_deltas, y_deltas) = match &point_numbers {
            PackedPointNumbers::All => deltas.iter().map(|d| (d.x, d.y)).unzip(),
            PackedPointNumbers::Some(pts) => pts
                .iter()
                .map(|idx| deltas[*idx as usize].to_tuple())
                .unzip(),
        };

        let data = GlyphTupleVariationData {
            private_point_numbers: has_private_points.then_some(point_numbers),
            x_deltas: PackedDeltas::new(x_deltas),
            y_deltas: PackedDeltas::new(y_deltas),
        };
        let data_size = data.compute_size();

        let header = TupleVariationHeader::new(
            data_size,
            idx,
            peak_tuple,
            intermediate_region,
            has_private_points,
        );

        (header, data)
    }
}

/// The serializable representation of a glyph's variation data
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct GlyphVariationData {
    tuple_variation_headers: Vec<TupleVariationHeader>,
    // optional; present if multiple variations have the same point numbers
    shared_point_numbers: Option<PackedPointNumbers>,
    per_tuple_data: Vec<GlyphTupleVariationData>,
}

/// The serializable representation of a single glyph tuple variation data
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
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
    fn gvar_smoke_test() {
        let _ = env_logger::builder().is_test(true).try_init();
        let table = Gvar::new(vec![
            GlyphVariations::new(GlyphId::new(0), vec![]),
            GlyphVariations::new(
                GlyphId::new(1),
                vec![GlyphDeltas::new(
                    Tuple::new(vec![F2Dot14::from_f32(1.0), F2Dot14::from_f32(1.0)]),
                    vec![
                        GlyphDelta::required(30, 31),
                        GlyphDelta::required(40, 41),
                        GlyphDelta::required(-50, -49),
                        GlyphDelta::required(101, 102),
                        GlyphDelta::required(10, 11),
                    ],
                    None,
                )],
            ),
            GlyphVariations::new(
                GlyphId::new(2),
                vec![
                    GlyphDeltas::new(
                        Tuple::new(vec![F2Dot14::from_f32(1.0), F2Dot14::from_f32(1.0)]),
                        vec![
                            GlyphDelta::required(11, -20),
                            GlyphDelta::required(69, -41),
                            GlyphDelta::required(-69, 49),
                            GlyphDelta::required(168, 101),
                            GlyphDelta::required(1, 2),
                        ],
                        None,
                    ),
                    GlyphDeltas::new(
                        Tuple::new(vec![F2Dot14::from_f32(0.8), F2Dot14::from_f32(1.0)]),
                        vec![
                            GlyphDelta::required(3, -200),
                            GlyphDelta::required(4, -500),
                            GlyphDelta::required(5, -800),
                            GlyphDelta::required(6, -1200),
                            GlyphDelta::required(7, -1500),
                        ],
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

    #[test]
    fn use_iup_when_appropriate() {
        // IFF iup provides space savings, we should prefer it.
        let _ = env_logger::builder().is_test(true).try_init();
        let gid = GlyphId::new(0);
        let table = Gvar::new(vec![GlyphVariations::new(
            gid,
            vec![GlyphDeltas::new(
                Tuple::new(vec![F2Dot14::from_f32(1.0), F2Dot14::from_f32(1.0)]),
                vec![
                    GlyphDelta::required(30, 31),
                    GlyphDelta::optional(30, 31),
                    GlyphDelta::optional(30, 31),
                    GlyphDelta::required(101, 102),
                    GlyphDelta::required(10, 11),
                    GlyphDelta::optional(10, 11),
                ],
                None,
            )],
        )])
        .unwrap();

        let bytes = crate::dump_table(&table).unwrap();
        let gvar = read_fonts::tables::gvar::Gvar::read(FontData::new(&bytes)).unwrap();
        assert_eq!(gvar.version(), MajorMinor::VERSION_1_0);
        assert_eq!(gvar.shared_tuple_count(), 0);
        assert_eq!(gvar.glyph_count(), 1);

        let g1 = gvar.glyph_variation_data(gid).unwrap();
        let g1tup = g1.tuples().collect::<Vec<_>>();
        assert_eq!(g1tup.len(), 1);
        let tuple_variation = &g1tup[0];

        assert!(!tuple_variation.has_deltas_for_all_points());
        assert_eq!(
            vec![0, 3, 4],
            tuple_variation.point_numbers().collect::<Vec<_>>()
        );

        let points: Vec<_> = tuple_variation
            .deltas()
            .map(|d| (d.x_delta, d.y_delta))
            .collect();
        assert_eq!(points, vec![(30, 31), (101, 102), (10, 11)]);
    }

    #[test]
    fn disregard_iup_when_appropriate() {
        // if the cost of encoding the list of points is greater than the savings
        // from omitting some deltas, we should just encode explicit zeros
        let points = vec![
            GlyphDelta::required(1, 2),
            GlyphDelta::required(3, 4),
            GlyphDelta::required(5, 6),
            GlyphDelta::optional(5, 6),
            GlyphDelta::required(7, 8),
        ];
        let gid = GlyphId::new(0);
        let table = Gvar::new(vec![GlyphVariations::new(
            gid,
            vec![GlyphDeltas::new(
                Tuple::new(vec![F2Dot14::from_f32(1.0), F2Dot14::from_f32(1.0)]),
                points,
                None,
            )],
        )])
        .unwrap();
        let bytes = crate::dump_table(&table).unwrap();
        let gvar = read_fonts::tables::gvar::Gvar::read(FontData::new(&bytes)).unwrap();
        assert_eq!(gvar.version(), MajorMinor::VERSION_1_0);
        assert_eq!(gvar.shared_tuple_count(), 0);
        assert_eq!(gvar.glyph_count(), 1);

        let g1 = gvar.glyph_variation_data(gid).unwrap();
        let g1tup = g1.tuples().collect::<Vec<_>>();
        assert_eq!(g1tup.len(), 1);
        let tuple_variation = &g1tup[0];

        assert!(tuple_variation.has_deltas_for_all_points());
        let points: Vec<_> = tuple_variation
            .deltas()
            .map(|d| (d.x_delta, d.y_delta))
            .collect();
        assert_eq!(points, vec![(1, 2), (3, 4), (5, 6), (5, 6), (7, 8)]);
    }

    #[test]
    fn share_points() {
        let _ = env_logger::builder().is_test(true).try_init();
        let variations = GlyphVariations::new(
            GlyphId::new(0),
            vec![
                GlyphDeltas::new(
                    Tuple::new(vec![F2Dot14::from_f32(1.0), F2Dot14::from_f32(1.0)]),
                    vec![
                        GlyphDelta::required(1, 2),
                        GlyphDelta::optional(3, 4),
                        GlyphDelta::required(5, 6),
                        GlyphDelta::optional(5, 6),
                        GlyphDelta::required(7, 8),
                        GlyphDelta::optional(7, 8),
                    ],
                    None,
                ),
                GlyphDeltas::new(
                    Tuple::new(vec![F2Dot14::from_f32(-1.0), F2Dot14::from_f32(-1.0)]),
                    vec![
                        GlyphDelta::required(10, 20),
                        GlyphDelta::optional(30, 40),
                        GlyphDelta::required(50, 60),
                        GlyphDelta::optional(50, 60),
                        GlyphDelta::required(70, 80),
                        GlyphDelta::optional(70, 80),
                    ],
                    None,
                ),
            ],
        );

        assert_eq!(
            variations.compute_shared_points(),
            Some(PackedPointNumbers::Some(vec![0, 2, 4]))
        )
    }

    #[test]
    fn share_all_points() {
        let variations = GlyphVariations::new(
            GlyphId::new(0),
            vec![
                GlyphDeltas::new(
                    Tuple::new(vec![F2Dot14::from_f32(1.0), F2Dot14::from_f32(1.0)]),
                    vec![
                        GlyphDelta::required(1, 2),
                        GlyphDelta::required(3, 4),
                        GlyphDelta::required(5, 6),
                    ],
                    None,
                ),
                GlyphDeltas::new(
                    Tuple::new(vec![F2Dot14::from_f32(-1.0), F2Dot14::from_f32(-1.0)]),
                    vec![
                        GlyphDelta::required(2, 4),
                        GlyphDelta::required(6, 8),
                        GlyphDelta::required(7, 9),
                    ],
                    None,
                ),
            ],
        );

        let shared_tups = HashMap::new();
        let built = variations.build(&shared_tups);
        assert_eq!(built.shared_point_numbers, Some(PackedPointNumbers::All))
    }

    // comparing our behaviour against what we know fonttools does.
    #[test]
    #[allow(non_snake_case)]
    fn oswald_Lcaron() {
        let _ = env_logger::builder().is_test(true).try_init();
        let d1 = GlyphDeltas::new(
            Tuple::new(vec![F2Dot14::from_f32(-1.0), F2Dot14::from_f32(-1.0)]),
            vec![
                GlyphDelta::optional(0, 0),
                GlyphDelta::required(35, 0),
                GlyphDelta::optional(0, 0),
                GlyphDelta::required(-24, 0),
                GlyphDelta::optional(0, 0),
                GlyphDelta::optional(0, 0),
            ],
            None,
        );

        let d1_sparse = GlyphDeltas::build_sparse_data(&d1.deltas);

        assert_eq!(
            d1_sparse
                .private_point_numbers
                .clone()
                .unwrap()
                .compute_size(),
            4
        );
        assert_eq!(d1_sparse.x_deltas.compute_size(), 3);
        assert_eq!(d1_sparse.y_deltas.compute_size(), 1);

        let d1_dense = GlyphDeltas::build_non_sparse_data(&d1.deltas);

        assert_eq!(d1_dense.x_deltas.compute_size(), 6);
        assert_eq!(d1_dense.y_deltas.compute_size(), 1);

        assert_eq!(d1_sparse.compute_size(), d1_dense.compute_size());

        let d2 = GlyphDeltas::new(
            Tuple::new(vec![F2Dot14::from_f32(1.0), F2Dot14::from_f32(1.0)]),
            vec![
                GlyphDelta::optional(0, 0),
                GlyphDelta::required(26, 15),
                GlyphDelta::optional(0, 0),
                GlyphDelta::required(46, 0),
                GlyphDelta::optional(0, 0),
                GlyphDelta::optional(0, 0),
            ],
            None,
        );
        let d2_sparse = GlyphDeltas::build_sparse_data(&d2.deltas);

        assert_eq!(
            d2_sparse
                .private_point_numbers
                .as_ref()
                .unwrap()
                .compute_size(),
            4
        );
        assert_eq!(d2_sparse.x_deltas.compute_size(), 3);
        assert_eq!(d2_sparse.y_deltas.compute_size(), 3,);

        let d2_dense = GlyphDeltas::build_non_sparse_data(&d2.deltas);

        assert_eq!(d2_dense.x_deltas.compute_size(), 6);
        assert_eq!(d2_dense.y_deltas.compute_size(), 4);

        assert!(d2_sparse.compute_size() < d2_dense.compute_size());

        let tups = HashMap::new();
        let variations = GlyphVariations::new(GlyphId::new(0), vec![d1, d2]);
        assert!(variations.compute_shared_points().is_none());
        let built = variations.build(&tups);
        assert_eq!(
            built.per_tuple_data[0].private_point_numbers,
            Some(PackedPointNumbers::All)
        );
        assert_eq!(
            built.per_tuple_data[1].private_point_numbers,
            Some(PackedPointNumbers::Some(vec![1, 3]))
        );
    }
}
