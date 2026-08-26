//! Shapes the generated tables do not reach.
//!
//! Everything the emitter produces is checked in [`tables`][super::tables],
//! against the parser the crate ships. This file covers what those four modules
//! happen not to contain: a variably sized record walked with
//! [`VariableSize`][super::parse::VariableSize], and a table mixing a plain
//! record slice with a wrapped one.
//!
//! Written the way codegen would emit it, so that when a table of either shape
//! is generated these can go.

#![allow(clippy::arithmetic_side_effects)]

use font_types::{BigEndian, F2Dot14, FixedSize, GlyphId16, Nullable, Offset16};

use super::parse::{Bytes, Resolve, Table, VariableSize, VariableSizeArray, WithParent};

// ---------------------------------------------------------------------------
// A leaf table, to have something for an offset to point at.
// ---------------------------------------------------------------------------

/// A `Device` / `VariationIndex` table, cut down to what these need.
#[derive(Clone, Copy)]
pub struct Device<'a> {
    data: Bytes<'a>,
}

impl<'a> Table<'a> for Device<'a> {
    type Args = ();
    const MIN_SIZE: usize = 6;

    #[inline]
    fn read_with_args(data: Bytes<'a>, _: ()) -> Option<Self> {
        (data.len() >= Self::MIN_SIZE).then_some(Self { data })
    }
}

impl Device<'_> {
    pub fn delta_format(&self) -> u16 {
        self.data.read_at(4).unwrap_or_default()
    }
}

// ---------------------------------------------------------------------------
// The rule for zerocopy records: a slice unless the record holds an offset.
// ---------------------------------------------------------------------------

/// A fixed-size record with no offsets. Nothing wraps it, ever.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, bytemuck::AnyBitPattern)]
#[repr(C, packed)]
pub struct RangeRecord {
    pub start_glyph_id: BigEndian<GlyphId16>,
    pub end_glyph_id: BigEndian<GlyphId16>,
    pub start_coverage_index: BigEndian<u16>,
}

impl FixedSize for RangeRecord {
    const RAW_BYTE_LEN: usize = GlyphId16::RAW_BYTE_LEN * 2 + u16::RAW_BYTE_LEN;
}

impl RangeRecord {
    pub fn start_glyph_id(&self) -> GlyphId16 {
        self.start_glyph_id.get()
    }

    pub fn end_glyph_id(&self) -> GlyphId16 {
        self.end_glyph_id.get()
    }
}

/// A fixed-size record that does hold an offset. This is the shape of a MATH
/// `MathValueRecord`, of which `MathConstants` embeds fifty one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, bytemuck::AnyBitPattern)]
#[repr(C, packed)]
pub struct MathValueRecord {
    pub value: BigEndian<i16>,
    pub device_offset: BigEndian<Nullable<Offset16>>,
}

impl FixedSize for MathValueRecord {
    const RAW_BYTE_LEN: usize = i16::RAW_BYTE_LEN + Offset16::RAW_BYTE_LEN;
}

impl MathValueRecord {
    pub fn value(&self) -> i16 {
        self.value.get()
    }

    pub fn device_offset(&self) -> Nullable<Offset16> {
        self.device_offset.get()
    }
}

impl<'a> WithParent<'a, MathValueRecord> {
    pub fn device(&self) -> Option<Device<'a>> {
        self.parent().resolve(self.device_offset())
    }
}

/// Shows both halves of the rule in one table.
///
/// `ranges` is a plain slice, because a `RangeRecord` needs nothing to be
/// interpreted. `min_connector_overlap` is wrapped, because it does. The
/// wrapping is per record type, decided by whether the record holds an offset,
/// and not by where the record appears.
#[derive(Clone, Copy)]
pub struct MixedRecords<'a> {
    data: Bytes<'a>,
}

impl<'a> Table<'a> for MixedRecords<'a> {
    type Args = ();
    const MIN_SIZE: usize = 2 + MathValueRecord::RAW_BYTE_LEN;

    #[inline]
    fn read_with_args(data: Bytes<'a>, _: ()) -> Option<Self> {
        (data.len() >= Self::MIN_SIZE).then_some(Self { data })
    }
}

impl<'a> MixedRecords<'a> {
    /// An embedded record that holds an offset, so it is wrapped.
    ///
    /// Optional, even though `MIN_SIZE` covers it. Nothing is fabricated for
    /// the case `MIN_SIZE` has ruled out: a caller who wants a value regardless
    /// asks for one, and gets to pick what it is.
    pub fn min_connector_overlap(&self) -> Option<WithParent<'a, MathValueRecord>> {
        WithParent::at(self.data, 2)
    }

    pub fn range_count(&self) -> u16 {
        self.data.read_at(0).unwrap_or_default()
    }

    /// A record with no offsets: a plain zerocopy slice, exactly as today.
    pub fn ranges(&self) -> &'a [RangeRecord] {
        let start = Self::MIN_SIZE;
        let end = start + self.range_count() as usize * RangeRecord::RAW_BYTE_LEN;
        self.data.read_array(start..end).unwrap_or_default()
    }
}

// ---------------------------------------------------------------------------
// The second kind of runtime-known size: variable, not uniform.
// ---------------------------------------------------------------------------

/// A fixed-size record, the elements of the array inside `SegmentMaps`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, bytemuck::AnyBitPattern)]
#[repr(C, packed)]
pub struct AxisValueMap {
    pub from_coordinate: BigEndian<F2Dot14>,
    pub to_coordinate: BigEndian<F2Dot14>,
}

impl FixedSize for AxisValueMap {
    const RAW_BYTE_LEN: usize = F2Dot14::RAW_BYTE_LEN * 2;
}

impl AxisValueMap {
    pub fn from_coordinate(&self) -> F2Dot14 {
        self.from_coordinate.get()
    }
}

/// `avar`'s segment map: declared a `record` in the DSL, but its size is read
/// from its own first field rather than computed from args.
///
/// It cannot implement [`ComputedSize`], because `computed_size` is handed only
/// args and there is nothing in them to read the count from. It need not:
/// it holds no offsets, so it needs nothing from its parent, and is read from a
/// slice at itself like any [`Table`]. Every variably sized thing in the
/// crate is like this.
#[derive(Clone, Copy)]
pub struct SegmentMaps<'a> {
    data: Bytes<'a>,
}

impl<'a> Table<'a> for SegmentMaps<'a> {
    type Args = ();
    const MIN_SIZE: usize = u16::RAW_BYTE_LEN;

    #[inline]
    fn read_with_args(data: Bytes<'a>, _: ()) -> Option<Self> {
        (data.len() >= Self::MIN_SIZE).then_some(Self { data })
    }
}

impl<'a> SegmentMaps<'a> {
    pub fn position_map_count(&self) -> u16 {
        self.data.read_at(0).unwrap_or_default()
    }

    pub fn axis_value_maps(&self) -> &'a [AxisValueMap] {
        let end = 2 + self.position_map_count() as usize * AxisValueMap::RAW_BYTE_LEN;
        self.data.read_array(2..end).unwrap_or_default()
    }
}

/// The third row of the taxonomy: the size is read from the element, so this is
/// what lets a run of them be walked. A [`ComputedSize`] element has no
/// equivalent and needs none.
impl VariableSize for SegmentMaps<'_> {
    fn len_at(data: Bytes, pos: usize) -> Option<usize> {
        let count: u16 = data.read_at(pos)?;
        (count as usize)
            .checked_mul(AxisValueMap::RAW_BYTE_LEN)?
            .checked_add(u16::RAW_BYTE_LEN)
    }
}

/// The `avar` axis segment map array, walked.
#[derive(Clone, Copy)]
pub struct Avar<'a> {
    data: Bytes<'a>,
}

impl<'a> Table<'a> for Avar<'a> {
    type Args = ();
    const MIN_SIZE: usize = 8;

    #[inline]
    fn read_with_args(data: Bytes<'a>, _: ()) -> Option<Self> {
        (data.len() >= Self::MIN_SIZE).then_some(Self { data })
    }
}

impl<'a> Avar<'a> {
    pub fn axis_count(&self) -> u16 {
        self.data.read_at(6).unwrap_or_default()
    }

    /// No `len()` on the result: the store cannot say how many there are
    /// without walking, and now the type says so.
    pub fn axis_segment_maps(&self) -> VariableSizeArray<'a, SegmentMaps<'a>> {
        VariableSizeArray::of_variable_size(self.data.split_off(8).unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data(bytes: &[u8]) -> Bytes<'_> {
        Bytes::new(bytes)
    }

    #[test]
    fn a_record_with_no_offsets_stays_a_slice_and_one_with_offsets_is_wrapped() {
        // range_count = 2, an embedded MathValueRecord, then two RangeRecords,
        // then the device table the embedded record points at
        #[rustfmt::skip]
        let bytes: Vec<u8> = vec![
            0, 2,                // 0:  range_count
            0, 10, 0, 18,        // 2:  min_connector_overlap: value 10, device at 18
            0, 1, 0, 3, 0, 0,    // 6:  ranges[0]
            0, 5, 0, 9, 0, 3,    // 12: ranges[1]
            0, 0, 0, 0, 0x80, 0, // 18: device, deltaFormat 0x8000 at 22
        ];
        let table = MixedRecords::read(data(&bytes)).unwrap();

        // no offsets: a plain slice, indexed and iterated as one
        let ranges = table.ranges();
        assert_eq!(ranges.len(), 2);
        assert_eq!(ranges[1].start_glyph_id(), GlyphId16::new(5));

        // holds an offset: wrapped, and resolves with no argument
        let overlap = table.min_connector_overlap().unwrap();
        assert_eq!(overlap.value(), 10);
        assert_eq!(overlap.device().unwrap().delta_format(), 0x8000);

        // a caller who would rather have a value than an `Option` picks the
        // fallback themselves; nothing is fabricated on our side
        let first: RangeRecord = ranges.first().copied().unwrap_or_default();
        assert_eq!(first.end_glyph_id(), GlyphId16::new(3));
        let past_end: RangeRecord = ranges.get(9).copied().unwrap_or_default();
        assert_eq!(past_end, RangeRecord::default());
    }

    #[test]
    fn an_absent_embedded_record_is_none_rather_than_zeroes() {
        // MIN_SIZE makes this unreachable through `read`; build the table
        // directly to reach it at all
        let table = MixedRecords { data: Bytes::EMPTY };
        assert!(table.min_connector_overlap().is_none());
        let overlap = table
            .min_connector_overlap()
            .map(|rec| *rec)
            .unwrap_or_default();
        assert_eq!(overlap.value(), 0);
    }

    #[test]
    fn variably_sized_elements_are_walked_and_have_no_len() {
        // an avar-shaped table: three segment maps of 3, 2 and 4 pairs
        let mut bytes: Vec<u8> = vec![0, 1, 0, 0, 0, 0, 0, 3];
        for count in [3u16, 2, 4] {
            bytes.extend_from_slice(&count.to_be_bytes());
            for i in 0..count {
                bytes.extend_from_slice(&(i as i16).to_be_bytes());
                bytes.extend_from_slice(&((i as i16) * 2).to_be_bytes());
            }
        }
        let avar = Avar::read(data(&bytes)).unwrap();
        assert_eq!(avar.axis_count(), 3);

        let maps = avar.axis_segment_maps();
        let counts: Vec<_> = maps
            .iter()
            .map(|m| m.map(|m| m.position_map_count()))
            .collect();
        assert_eq!(counts, vec![Some(3), Some(2), Some(4)]);

        // each element's own array is reachable, and each is a different size
        let lens: Vec<_> = maps
            .iter()
            .map(|m| m.map(|m| m.axis_value_maps().len()))
            .collect();
        assert_eq!(lens, vec![Some(3), Some(2), Some(4)]);

        // `get` walks, so it is O(n), but it works
        assert_eq!(maps.get(1).unwrap().unwrap().position_map_count(), 2);
        assert!(maps.get(3).is_none());
    }

    #[test]
    fn a_truncated_var_len_element_stops_the_walk() {
        // a count claiming four pairs with none of them present
        let bytes: Vec<u8> = vec![0, 1, 0, 0, 0, 0, 0, 1, 0, 4, 0, 0];
        let avar = Avar::read(data(&bytes)).unwrap();
        let found: Vec<_> = avar
            .axis_segment_maps()
            .iter()
            .map(|m| m.map(|m| m.axis_value_maps().len()))
            .collect();
        // the walk cannot complete the element, and says so rather than
        // guessing where the next one starts
        assert_eq!(found, vec![None]);
    }

    /// The numbers behind the `Option` choice, recorded so a regression shows.
    #[test]
    fn option_is_cheaper_than_result() {
        use crate::ReadError;
        use core::mem::size_of;

        assert_eq!(size_of::<Device>(), 16);
        assert_eq!(size_of::<Option<Device>>(), 16);
        assert_eq!(size_of::<Result<Device, ReadError>>(), 24);
        // the shape the nullable accessors have today
        assert_eq!(size_of::<Option<Result<Device, ReadError>>>(), 24);
        // a borrowed record plus its base, whatever the record's size
        assert_eq!(size_of::<WithParent<MathValueRecord>>(), 24);
        assert_eq!(size_of::<Option<WithParent<MathValueRecord>>>(), 24);
    }
}
