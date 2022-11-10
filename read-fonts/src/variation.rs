//! OpenType font variations common tables.

include!("../generated/generated_variation.rs");

/// Outer and inner indices for reading from an [ItemVariationStore].
#[derive(Copy, Clone, Debug)]
pub struct DeltaSetIndex {
    /// Outer delta set index.
    pub outer: u16,
    /// Inner delta set index.
    pub inner: u16,
}

impl<'a> DeltaSetIndexMap<'a> {
    /// Returns the delta set index for the specified value.
    pub fn get(&self, index: u32) -> Result<DeltaSetIndex, ReadError> {
        let (entry_format, data) = match self {
            Self::Format0(fmt) => (fmt.entry_format() as u32, fmt.map_data()),
            Self::Format1(fmt) => (fmt.entry_format() as u32, fmt.map_data()),
        };
        let entry_size = ((entry_format & EntryFormat::MAP_ENTRY_SIZE_MASK.bits() as u32) >> 4) + 1;
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
        let bit_count = (entry_format & 0xF) + 1;
        Ok(DeltaSetIndex {
            outer: (entry >> bit_count) as u16,
            inner: (entry & ((1 << bit_count) - 1)) as u16,
        })
    }
}

impl<'a> ItemVariationStore<'a> {
    /// Returns the delta value for the specified index and set of normalized
    /// variation coordinates.
    pub fn delta(&self, index: DeltaSetIndex, coords: &[F2Dot14]) -> Result<Fixed, ReadError> {
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
        for (i, region_delta) in data.deltas(index.inner).enumerate() {
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
        let mut scalar = Fixed::from_f64(1.0);
        for (i, axis_coords) in self.region_axes().iter().enumerate() {
            let coord = coords
                .get(i)
                .copied()
                .map(|coord| coord.to_fixed())
                .unwrap_or(ZERO);
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
    pub fn deltas(&self, inner_index: u16) -> impl Iterator<Item = Fixed> + 'a + Clone {
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
