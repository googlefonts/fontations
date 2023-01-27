//! OpenType font variations common tables.

include!("../../generated/generated_variations.rs");

/// Outer and inner indices for reading from an [ItemVariationStore].
#[derive(Copy, Clone, Debug)]
pub struct DeltaSetIndex {
    /// Outer delta set index.
    pub outer: u16,
    /// Inner delta set index.
    pub inner: u16,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TupleIndex(u16);

impl TupleIndex {
    /// Flag indicating that this tuple variation header includes an embedded
    /// peak tuple record, immediately after the tupleIndex field.
    ///
    /// If set, the low 12 bits of the tupleIndex value are ignored.
    ///
    /// Note that this must always be set within the 'cvar' table.
    const EMBEDDED_PEAK_TUPLE: u16 = 0x8000;

    /// Flag indicating that this tuple variation table applies to an
    /// intermediate region within the variation space.
    ///
    /// If set, the header includes the two intermediate-region, start and end
    /// tuple records, immediately after the peak tuple record (if present).
    const INTERMEDIATE_REGION: u16 = 0x4000;
    /// Flag indicating that the serialized data for this tuple variation table
    /// includes packed “point” number data.
    ///
    /// If set, this tuple variation table uses that number data; if clear,
    /// this tuple variation table uses shared number data found at the start
    /// of the serialized data for this glyph variation data or 'cvar' table.
    const PRIVATE_POINT_NUMBERS: u16 = 0x2000;
    //0x1000	Reserved	Reserved for future use — set to 0.
    //
    /// Mask for the low 12 bits to give the shared tuple records index.
    const TUPLE_INDEX_MASK: u16 = 0x0FFF;

    fn tuple_len(self, axis_count: u16, flag: usize) -> usize {
        match flag {
            0 => self.embedded_peak_tuple(),
            1 => self.intermediate_region(),
            _ => panic!("only 0 or 1 allowed here"),
        }
        .then_some(axis_count as usize)
        .unwrap_or_default()
    }

    pub fn bits(self) -> u16 {
        self.0
    }

    /// `true` if the header includes an embedded peak tuple.
    pub fn embedded_peak_tuple(self) -> bool {
        (self.0 & Self::EMBEDDED_PEAK_TUPLE) != 0
    }

    /// `true` if the header includes the two intermediate region tuple records.
    pub fn intermediate_region(self) -> bool {
        (self.0 & Self::INTERMEDIATE_REGION) != 0
    }

    /// `true` if the data for this table includes packed point number data.
    pub fn private_point_numbers(self) -> bool {
        (self.0 & Self::PRIVATE_POINT_NUMBERS) != 0
    }

    pub fn tuple_records_index(self) -> Option<u16> {
        (!self.embedded_peak_tuple()).then_some(self.0 & Self::TUPLE_INDEX_MASK)
    }
}

impl types::Scalar for TupleIndex {
    type Raw = <u16 as types::Scalar>::Raw;
    fn to_raw(self) -> Self::Raw {
        self.0.to_raw()
    }
    fn from_raw(raw: Self::Raw) -> Self {
        let t = <u16>::from_raw(raw);
        Self(t)
    }
}

/// The 'tupleVariationCount' field of the [Tuple Variation Store Header][header]
///
/// The high 4 bits are flags, and the low 12 bits are the number of tuple
/// variation tables for this glyph. The count can be any number between 1 and 4095.
///
/// [header]: https://learn.microsoft.com/en-us/typography/opentype/spec/otvarcommonformats#tuple-variation-store-header
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TupleVariationCount(u16);

impl TupleVariationCount {
    /// Flag indicating that some or all tuple variation tables reference a
    /// shared set of “point” numbers.
    ///
    /// These shared numbers are represented as packed point number data at the
    /// start of the serialized data.
    const SHARED_POINT_NUMBERS: u16 = 0x8000;

    /// Mask for the low 12 bits to give the shared tuple records index.
    const COUNT_MASK: u16 = 0x0FFF;

    pub fn bits(self) -> u16 {
        self.0
    }

    /// `true` if any tables reference a shared set of point numbers
    pub fn shared_point_numbers(self) -> bool {
        (self.0 & Self::SHARED_POINT_NUMBERS) != 0
    }

    pub fn count(self) -> u16 {
        self.0 & Self::COUNT_MASK
    }
}

impl types::Scalar for TupleVariationCount {
    type Raw = <u16 as types::Scalar>::Raw;
    fn to_raw(self) -> Self::Raw {
        self.0.to_raw()
    }
    fn from_raw(raw: Self::Raw) -> Self {
        let t = <u16>::from_raw(raw);
        Self(t)
    }
}

impl<'a> TupleVariationHeader<'a> {
    #[cfg(feature = "traversal")]
    fn traverse_tuple_index(&self) -> traversal::FieldType<'a> {
        self.tuple_index().0.into()
    }

    /// Peak tuple record for this tuple variation table — optional,
    /// determined by flags in the tupleIndex value.  Note that this
    /// must always be included in the 'cvar' table.
    pub fn peak_tuple(&self) -> Option<Tuple<'a>> {
        self.tuple_index().embedded_peak_tuple().then(|| {
            let range = self.shape.peak_tuple_byte_range();
            Tuple {
                values: self.data.read_array(range).unwrap(),
            }
        })
    }

    /// Intermediate start tuple record for this tuple variation table
    /// — optional, determined by flags in the tupleIndex value.
    pub fn intermediate_start_tuple(&self) -> Option<Tuple<'a>> {
        self.tuple_index().intermediate_region().then(|| {
            let range = self.shape.intermediate_start_tuple_byte_range();
            Tuple {
                values: self.data.read_array(range).unwrap(),
            }
        })
    }

    /// Intermediate end tuple record for this tuple variation table
    /// — optional, determined by flags in the tupleIndex value.
    pub fn intermediate_end_tuple(&self) -> Option<Tuple<'a>> {
        self.tuple_index().intermediate_region().then(|| {
            let range = self.shape.intermediate_end_tuple_byte_range();
            Tuple {
                values: self.data.read_array(range).unwrap(),
            }
        })
    }

    /// Compute the actual length of this table in bytes
    fn byte_len(&self, axis_count: u16) -> usize {
        const FIXED_LEN: usize = u16::RAW_BYTE_LEN + TupleIndex::RAW_BYTE_LEN;
        let tuple_byte_len = F2Dot14::RAW_BYTE_LEN * axis_count as usize;
        let index = self.tuple_index();
        FIXED_LEN
            + index
                .embedded_peak_tuple()
                .then_some(tuple_byte_len)
                .unwrap_or(0)
            + index
                .intermediate_region()
                .then_some(tuple_byte_len)
                .unwrap_or(0)
    }
}

impl<'a> Tuple<'a> {
    pub fn len(&self) -> usize {
        self.values().len()
    }

    pub fn get(&self, idx: usize) -> Option<F2Dot14> {
        self.values.get(idx).copied().map(BigEndian::get)
    }
}

//FIXME: add an #[extra_traits(..)] attribute!
impl Default for Tuple<'_> {
    fn default() -> Self {
        Self {
            values: Default::default(),
        }
    }
}

/// [Packed "Point" Numbers](https://learn.microsoft.com/en-us/typography/opentype/spec/otvarcommonformats#packed-point-numbers)
#[derive(Clone, Debug)]
pub struct PackedPointNumbers<'a> {
    count: Count,
    data: FontData<'a>,
}

/// A helper for distinguishing between an explicit count of numbers and an implicit 'all' value.
///
/// If there is an explicit count, we need to decode the values from the data;
/// if the count is implicit, we do not.
#[derive(Clone, Copy, Debug)]
enum Count {
    AllInFont(u16),
    Explicit(u16),
}

impl Count {
    fn raw(self) -> u16 {
        match self {
            Count::AllInFont(val) => val,
            Count::Explicit(val) => val,
        }
    }
}

impl ReadArgs for PackedPointNumbers<'_> {
    /// the total number of glyph points or CVTs for this tuple.
    type Args = u16;
}

impl<'a> FontReadWithArgs<'a> for PackedPointNumbers<'a> {
    fn read_with_args(data: FontData<'a>, args: &Self::Args) -> Result<Self, ReadError> {
        let (count, count_bytes) = match data.read_at::<u8>(0)? {
            0 => (Count::AllInFont(*args), 1),
            count @ 1..=127 => (Count::Explicit(count as u16), 1),
            _ => {
                // "If the high bit of the first byte is set, then a second byte is used.
                // The count is read from interpreting the two bytes as a big-endian
                // uint16 value with the high-order bit masked out."

                let count = data.read_at::<u16>(0)? & 0x7FFF;
                // a weird case where I'm following fonttools: if the 'use words' bit
                // is set, but the total count is still 0, treat it like 0 first byte
                if count == 0 {
                    (Count::AllInFont(*args), 2)
                } else {
                    (Count::Explicit(count & 0x7FFF), 2)
                }
            }
        };

        let data = data.split_off(count_bytes).ok_or(ReadError::OutOfBounds)?;
        Ok(PackedPointNumbers { count, data })
    }
}

impl<'a> PackedPointNumbers<'a> {
    /// The number of points in this set
    pub fn count(&self) -> u16 {
        self.count.raw()
    }

    /// Iterate over the packed points
    pub fn iter(&self) -> PackedPointNumbersIter<'a> {
        PackedPointNumbersIter::new(self.count, self.data.cursor())
    }
}

/// An iterator over the packed point numbers data.
#[derive(Clone, Debug)]
pub struct PackedPointNumbersIter<'a> {
    count: Count,
    seen: u16,
    last_val: u16,
    current_run: PointRunIter<'a>,
}

impl<'a> PackedPointNumbersIter<'a> {
    fn new(count: Count, cursor: Cursor<'a>) -> Self {
        PackedPointNumbersIter {
            count,
            seen: 0,
            last_val: 0,
            current_run: PointRunIter {
                remaining: 0,
                two_bytes: false,
                cursor,
            },
        }
    }
}

/// Implements the logic for iterating over the individual runs
#[derive(Clone, Debug)]
struct PointRunIter<'a> {
    remaining: u8,
    two_bytes: bool,
    cursor: Cursor<'a>,
}

impl Iterator for PointRunIter<'_> {
    type Item = u16;

    fn next(&mut self) -> Option<Self::Item> {
        // if no items remain in this run, start the next one.
        if self.remaining == 0 {
            let control: u8 = self.cursor.read().ok()?;
            self.two_bytes = (control & 0x80) != 0;
            self.remaining = (control & 0x7F) + 1;
        }

        self.remaining -= 1;
        if self.two_bytes {
            self.cursor.read().ok()
        } else {
            self.cursor.read::<u8>().ok().map(|v| v as u16)
        }
    }
}

impl Iterator for PackedPointNumbersIter<'_> {
    type Item = u16;

    fn next(&mut self) -> Option<Self::Item> {
        if self.count.raw() == self.seen {
            return None;
        }
        self.seen += 1;
        match self.count {
            // we implement the iterator in both cases, for simplicity
            Count::AllInFont(_) => Some(self.seen - 1),
            Count::Explicit(_) => {
                self.last_val += self.current_run.next()?;
                Some(self.last_val)
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.count.raw() as usize, Some(self.count.raw() as usize))
    }
}

// completely unnecessary?
impl<'a> ExactSizeIterator for PackedPointNumbersIter<'a> {}

impl EntryFormat {
    pub fn entry_size(self) -> u8 {
        ((self.bits() & Self::MAP_ENTRY_SIZE_MASK.bits()) >> 4) + 1
    }

    pub fn bit_count(self) -> u8 {
        (self.bits() & Self::INNER_INDEX_BIT_COUNT_MASK.bits()) + 1
    }

    // called from codegen
    pub(crate) fn map_size(self, map_count: impl Into<u32>) -> usize {
        self.entry_size() as usize * map_count.into() as usize
    }
}

impl<'a> DeltaSetIndexMap<'a> {
    /// Returns the delta set index for the specified value.
    pub fn get(&self, index: u32) -> Result<DeltaSetIndex, ReadError> {
        let (entry_format, data) = match self {
            Self::Format0(fmt) => (fmt.entry_format(), fmt.map_data()),
            Self::Format1(fmt) => (fmt.entry_format(), fmt.map_data()),
        };
        let entry_size = entry_format.entry_size();
        let data = FontData::new(data);
        let offset = index as usize * entry_size as usize;
        let entry = match entry_size {
            1 => data.read_at::<u8>(offset)? as u32,
            2 => data.read_at::<u16>(offset)? as u32,
            3 => data.read_at::<Uint24>(offset)?.into(),
            4 => data.read_at::<u32>(offset)?,
            _ => {
                return Err(ReadError::MalformedData(
                    "invalid entry size in DeltaSetIndexMap",
                ))
            }
        };
        let bit_count = entry_format.bit_count();
        Ok(DeltaSetIndex {
            outer: (entry >> bit_count) as u16,
            inner: (entry & ((1 << bit_count) - 1)) as u16,
        })
    }
}

impl<'a> ItemVariationStore<'a> {
    /// Computes the delta value for the specified index and set of normalized
    /// variation coordinates.
    pub fn compute_delta(
        &self,
        index: DeltaSetIndex,
        coords: &[F2Dot14],
    ) -> Result<Fixed, ReadError> {
        let data = match self
            .item_variation_datas()
            .nth(index.outer as usize)
            .flatten()
        {
            Some(data) => data?,
            None => return Ok(Fixed::default()),
        };
        let regions = self.variation_region_list()?.variation_regions();
        let region_indices = data.region_indexes();
        let mut delta = Fixed::ZERO;
        for (i, region_delta) in data.delta_set(index.inner).enumerate() {
            let region_index = region_indices
                .get(i)
                .ok_or(ReadError::MalformedData(
                    "invalid delta sets in ItemVariationStore",
                ))?
                .get() as usize;
            let region = regions.get(region_index)?;
            let scalar = region.compute_scalar(coords);
            delta += region_delta * scalar;
        }
        Ok(delta)
    }
}

impl<'a> VariationRegion<'a> {
    /// Computes a scalar value for this region and the specified
    /// normalized variation coordinates.
    pub fn compute_scalar(&self, coords: &[F2Dot14]) -> Fixed {
        const ZERO: Fixed = Fixed::ZERO;
        let mut scalar = Fixed::ONE;
        for (i, axis_coords) in self.region_axes().iter().enumerate() {
            let coord = coords.get(i).map(|coord| coord.to_fixed()).unwrap_or(ZERO);
            let start = axis_coords.start_coord.get().to_fixed();
            let end = axis_coords.end_coord.get().to_fixed();
            let peak = axis_coords.peak_coord.get().to_fixed();
            if start > peak || peak > end || peak == ZERO || start < ZERO && end > ZERO {
                continue;
            } else if coord < start || coord > end {
                return ZERO;
            } else if coord == peak {
                continue;
            } else if coord < peak {
                scalar = scalar * (coord - start) / (peak - start)
            } else {
                scalar = scalar * (end - coord) / (end - peak)
            };
        }
        scalar
    }
}

impl<'a> ItemVariationData<'a> {
    /// Returns an iterator over the per-region delta values for the specified
    /// inner index.
    pub fn delta_set(&self, inner_index: u16) -> impl Iterator<Item = Fixed> + 'a + Clone {
        let word_delta_count = self.word_delta_count();
        let long_words = word_delta_count & 0x8000 != 0;
        let (word_size, small_size) = if long_words { (4, 2) } else { (2, 1) };
        let word_delta_count = word_delta_count & 0x7FFF;
        let region_count = self.region_index_count() as usize;
        let row_size = word_delta_count as usize * word_size
            + region_count.saturating_sub(word_delta_count as usize) * small_size;
        let offset = row_size * inner_index as usize;
        ItemDeltas {
            cursor: FontData::new(self.delta_sets())
                .slice(offset..)
                .unwrap_or_default()
                .cursor(),
            word_delta_count,
            long_words,
            len: region_count as u16,
            pos: 0,
        }
    }
}

#[derive(Clone)]
struct ItemDeltas<'a> {
    cursor: Cursor<'a>,
    word_delta_count: u16,
    long_words: bool,
    len: u16,
    pos: u16,
}

impl<'a> Iterator for ItemDeltas<'a> {
    type Item = Fixed;

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos >= self.len {
            return None;
        }
        let pos = self.pos;
        self.pos += 1;
        let value = match (pos >= self.word_delta_count, self.long_words) {
            (true, true) | (false, false) => self.cursor.read::<i16>().ok()? as i32,
            (true, false) => self.cursor.read::<i8>().ok()? as i32,
            (false, true) => self.cursor.read::<i32>().ok()?,
        };
        Some(Fixed::from_raw((value << 16).to_be_bytes()))
    }
}

pub(crate) fn advance_delta(
    dsim: Option<Result<DeltaSetIndexMap, ReadError>>,
    ivs: Result<ItemVariationStore, ReadError>,
    glyph_id: GlyphId,
    coords: &[F2Dot14],
) -> Result<Fixed, ReadError> {
    let gid = glyph_id.to_u16();
    let ix = match dsim {
        Some(Ok(dsim)) => dsim.get(gid as u32)?,
        _ => DeltaSetIndex {
            outer: 0,
            inner: gid,
        },
    };
    ivs?.compute_delta(ix, coords)
}

pub(crate) fn item_delta(
    dsim: Option<Result<DeltaSetIndexMap, ReadError>>,
    ivs: Result<ItemVariationStore, ReadError>,
    glyph_id: GlyphId,
    coords: &[F2Dot14],
) -> Result<Fixed, ReadError> {
    let gid = glyph_id.to_u16();
    let ix = match dsim {
        Some(Ok(dsim)) => dsim.get(gid as u32)?,
        _ => return Err(ReadError::NullOffset),
    };
    ivs?.compute_delta(ix, coords)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{test_data, FontRef, TableProvider};

    #[test]
    fn ivs_regions() {
        let font = FontRef::new(test_data::test_fonts::VAZIRMATN_VAR).unwrap();
        let hvar = font.hvar().expect("missing HVAR table");
        let ivs = hvar
            .item_variation_store()
            .expect("missing item variation store in HVAR");
        let region_list = ivs.variation_region_list().expect("missing region list!");
        let regions = region_list.variation_regions();
        let expected = &[
            // start_coord, peak_coord, end_coord
            vec![[-1.0f32, -1.0, 0.0]],
            vec![[0.0, 1.0, 1.0]],
        ][..];
        let region_coords = regions
            .iter()
            .map(|region| {
                region
                    .unwrap()
                    .region_axes()
                    .iter()
                    .map(|coords| {
                        [
                            coords.start_coord().to_f32(),
                            coords.peak_coord().to_f32(),
                            coords.end_coord().to_f32(),
                        ]
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        assert_eq!(expected, &region_coords);
    }

    // adapted from https://github.com/fonttools/fonttools/blob/f73220816264fc383b8a75f2146e8d69e455d398/Tests/ttLib/tables/TupleVariation_test.py#L492
    #[test]
    fn packed_points() {
        fn decode_points(bytes: &[u8], total_points: u16) -> Vec<u16> {
            let data = FontData::new(bytes);
            PackedPointNumbers::read_with_args(data, &total_points)
                .unwrap()
                .iter()
                .collect()
        }

        assert_eq!(decode_points(&[0], 3), vec![0, 1, 2]);
        // all points in glyph (in overly verbose encoding, not explicitly prohibited by spec)
        assert_eq!(decode_points(&[0x80, 0], 4), vec![0, 1, 2, 3]);
        // 2 points; first run: [9, 9+6]
        assert_eq!(decode_points(&[0x02, 0x01, 0x09, 0x06], 4), vec![9, 15]);
        // 2 points; first run: [0xBEEF, 0xCAFE]. (0x0C0F = 0xCAFE - 0xBEEF)
        assert_eq!(
            decode_points(&[0x02, 0x81, 0xbe, 0xef, 0x0c, 0x0f], 4),
            vec![0xbeef, 0xcafe]
        );
        // 1 point; first run: [7]
        assert_eq!(decode_points(&[0x01, 0, 0x07], 4), vec![7]);
        // 1 point; first run: [7] in overly verbose encoding
        assert_eq!(decode_points(&[0x01, 0x80, 0, 0x07], 4), vec![7]);
        // 1 point; first run: [65535]; requires words to be treated as unsigned numbers
        assert_eq!(decode_points(&[0x01, 0x80, 0xff, 0xff], 4), vec![65535]);
        // 4 points; first run: [7, 8]; second run: [255, 257]. 257 is stored in delta-encoded bytes (0xFF + 2).
        assert_eq!(
            decode_points(&[0x04, 1, 7, 1, 1, 0xff, 2], 4),
            vec![7, 8, 263, 265]
        );
    }
}
