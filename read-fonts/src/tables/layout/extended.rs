//! Accessors shared by the ISO OFF extended layout formats.

use super::*;

/// An offset array whose serialized offsets have one of the layout widths.
#[derive(Clone)]
pub enum LayoutOffsetArray<'a, T: FontRead<'a, Args = ()>> {
    Offset16(ArrayOfOffsets<'a, T, Offset16>),
    Offset24(ArrayOfOffsets<'a, T, Offset24>),
    Offset32(ArrayOfOffsets<'a, T, Offset32>),
}

impl<'a, T: FontRead<'a, Args = ()> + 'a> LayoutOffsetArray<'a, T> {
    pub fn len(&self) -> usize {
        match self {
            Self::Offset16(array) => array.len(),
            Self::Offset24(array) => array.len(),
            Self::Offset32(array) => array.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn get(&self, index: usize) -> Result<T, ReadError> {
        match self {
            Self::Offset16(array) => array.get(index),
            Self::Offset24(array) => array.get(index),
            Self::Offset32(array) => array.get(index),
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = Result<T, ReadError>> + 'a {
        let (one, two, three) = match self {
            Self::Offset16(array) => (Some(array.iter()), None, None),
            Self::Offset24(array) => (None, Some(array.iter()), None),
            Self::Offset32(array) => (None, None, Some(array.iter())),
        };
        one.into_iter()
            .flatten()
            .chain(two.into_iter().flatten())
            .chain(three.into_iter().flatten())
    }

    pub fn iter_as_nullable(&self) -> impl Iterator<Item = Option<Result<T, ReadError>>> + 'a {
        self.iter().map(|result| match result {
            Err(ReadError::NullOffset) => None,
            other => Some(other),
        })
    }
}

/// LookupList or LookupList2, selected by the enclosing GSUB/GPOS header.
#[derive(Clone)]
pub enum LookupListTable<'a, T: FontRead<'a, Args = ()>> {
    Offset16(LookupList<'a, T>),
    Offset32(LookupList2<'a, T>),
}

impl<'a, T: FontRead<'a, Args = ()>> From<LookupList<'a, T>> for LookupListTable<'a, T> {
    fn from(list: LookupList<'a, T>) -> Self {
        Self::Offset16(list)
    }
}

impl<'a, T: FontRead<'a, Args = ()>> From<LookupList2<'a, T>> for LookupListTable<'a, T> {
    fn from(list: LookupList2<'a, T>) -> Self {
        Self::Offset32(list)
    }
}

impl<'a, T: FontRead<'a, Args = ()> + 'a> LookupListTable<'a, T> {
    pub fn lookup_count(&self) -> u16 {
        match self {
            Self::Offset16(table) => table.lookup_count(),
            Self::Offset32(table) => table.lookup_count(),
        }
    }

    pub fn lookups(&self) -> LayoutOffsetArray<'a, T> {
        match self {
            Self::Offset16(table) => LayoutOffsetArray::Offset16(table.lookups()),
            Self::Offset32(table) => LayoutOffsetArray::Offset32(table.lookups()),
        }
    }

    pub fn offset_data(&self) -> FontData<'a> {
        match self {
            Self::Offset16(table) => table.offset_data(),
            Self::Offset32(table) => table.offset_data(),
        }
    }
}

/// The offset a header field resolves through, for headers that carry both
/// an original 16-bit offset and an extended 32-bit one.
///
/// The extended offset applies when the header version has it and it is
/// nonzero; otherwise the original one does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HeaderOffset {
    Original(Offset32),
    Extended(Offset32),
}

impl HeaderOffset {
    /// Chooses between the two offsets of a header field. An extended header
    /// too short to hold its extended offset is an error.
    pub(crate) fn select(
        original: Nullable<Offset16>,
        extended: Option<Nullable<Offset32>>,
        has_extended: bool,
    ) -> Result<Self, ReadError> {
        if has_extended {
            let extended = *extended.ok_or(ReadError::OutOfBounds)?.offset();
            if !extended.is_null() {
                return Ok(Self::Extended(extended));
            }
        }
        Ok(Self::Original(Offset32::new(original.offset().to_u32())))
    }

    pub(crate) fn offset(self) -> Offset32 {
        match self {
            Self::Original(offset) | Self::Extended(offset) => offset,
        }
    }

    pub(crate) fn is_null(self) -> bool {
        self.offset().is_null()
    }

    /// Resolves the offset, or returns `None` if it is null.
    pub(crate) fn resolve<'a, T: FontRead<'a, Args = ()>>(
        self,
        data: FontData<'a>,
    ) -> Option<Result<T, ReadError>> {
        (!self.is_null()).then(|| self.offset().resolve(data))
    }
}

impl CoverageFormat3<'_> {
    pub fn get(&self, gid: impl Into<GlyphId>) -> Option<u32> {
        let gid = GlyphId24::checked_new(gid.into().to_u32())?;
        self.glyph_array()
            .binary_search(&BigEndian::from(gid))
            .ok()
            .map(|index| index as u32)
    }
}

impl CoverageFormat4<'_> {
    pub fn get(&self, gid: impl Into<GlyphId>) -> Option<u32> {
        let gid = gid.into().to_u32();
        let records = self.range_records();
        let index = records
            .binary_search_by(|r| {
                if r.end_glyph_id().to_u32() < gid {
                    Ordering::Less
                } else if r.start_glyph_id().to_u32() > gid {
                    Ordering::Greater
                } else {
                    Ordering::Equal
                }
            })
            .ok()?;
        let record = &records[index];
        record
            .start_coverage_index()
            .to_u32()
            .checked_add(gid.checked_sub(record.start_glyph_id().to_u32())?)
            .filter(|index| *index <= Uint24::MAX.to_u32())
    }
}

impl RangeRecord2 {
    pub fn iter(&self) -> impl Iterator<Item = GlyphId> + '_ {
        (self.start_glyph_id().to_u32()..=self.end_glyph_id().to_u32()).map(GlyphId::new)
    }

    pub fn population(&self) -> usize {
        self.end_glyph_id()
            .to_u32()
            .checked_sub(self.start_glyph_id().to_u32())
            .map_or(0, |n| n as usize + 1)
    }
}

impl ClassDefFormat3<'_> {
    pub fn get(&self, gid: impl Into<GlyphId>) -> u32 {
        let gid = gid.into().to_u32();
        if gid > Uint24::MAX.to_u32() {
            return 0;
        }
        gid.checked_sub(self.start_glyph_id().to_u32())
            .and_then(|index| self.class_value_array().get(index as usize))
            .map_or(0, |class| class.get().to_u32())
    }
}

impl ClassDefFormat4<'_> {
    pub fn get(&self, gid: impl Into<GlyphId>) -> u32 {
        let gid = gid.into().to_u32();
        let records = self.class_range_records();
        let index = records
            .binary_search_by(|r| r.start_glyph_id().to_u32().cmp(&gid))
            .unwrap_or_else(|i| i.saturating_sub(1));
        records
            .get(index)
            .filter(|r| (r.start_glyph_id().to_u32()..=r.end_glyph_id().to_u32()).contains(&gid))
            .map_or(0, |r| u32::from(r.class()))
    }
}

impl ClassRangeRecord2 {
    pub fn population(&self) -> usize {
        self.end_glyph_id()
            .to_u32()
            .checked_sub(self.start_glyph_id().to_u32())
            .map_or(0, |n| n as usize + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn original(offset: u16) -> Nullable<Offset16> {
        Scalar::from_raw(offset.to_be_bytes())
    }

    fn extended(offset: u32) -> Option<Nullable<Offset32>> {
        Some(Scalar::from_raw(offset.to_be_bytes()))
    }

    #[test]
    fn header_offset_prefers_nonzero_extended_offset() {
        assert_eq!(
            HeaderOffset::select(original(10), extended(70000), true),
            Ok(HeaderOffset::Extended(Offset32::new(70000)))
        );
    }

    #[test]
    fn header_offset_falls_back_to_original_offset() {
        // A zero extended offset defers to the original one.
        assert_eq!(
            HeaderOffset::select(original(10), extended(0), true),
            Ok(HeaderOffset::Original(Offset32::new(10)))
        );
        // A header version without the extended field ignores it.
        assert_eq!(
            HeaderOffset::select(original(10), extended(70000), false),
            Ok(HeaderOffset::Original(Offset32::new(10)))
        );
        assert!(HeaderOffset::select(original(0), extended(0), true)
            .unwrap()
            .is_null());
    }

    #[test]
    fn header_offset_requires_extended_field_in_extended_header() {
        assert_eq!(
            HeaderOffset::select(original(10), None, true),
            Err(ReadError::OutOfBounds)
        );
        assert_eq!(
            HeaderOffset::select(original(10), None, false),
            Ok(HeaderOffset::Original(Offset32::new(10)))
        );
    }

    #[test]
    fn coverage_24bit_glyphs_and_indices() {
        // Format 3: three 24-bit glyph IDs, including both width boundaries.
        let bytes = [0, 3, 0, 0, 3, 0, 0xFF, 0xFF, 1, 0, 0, 0xFF, 0xFF, 0xFF];
        let coverage = CoverageTable::read(FontData::new(&bytes)).unwrap();
        assert_eq!(
            coverage.iter().map(GlyphId::to_u32).collect::<Vec<_>>(),
            [65535, 65536, 0xFFFFFF]
        );
        assert_eq!(coverage.get(GlyphId::new(65536)), Some(1));
        assert_eq!(coverage.get(GlyphId::new(0x1000000)), None);

        // Format 4: a range with a coverage index above 65535.
        let bytes = [0, 4, 0, 0, 1, 1, 0, 0, 1, 0, 2, 1, 0, 0];
        let coverage = CoverageTable::read(FontData::new(&bytes)).unwrap();
        assert_eq!(coverage.get(GlyphId::new(65538)), Some(65538));
        assert_eq!(coverage.get(GlyphId::new(65539)), None);
        assert_eq!(coverage.population(), 3);
    }

    #[test]
    fn class_widths_follow_the_format() {
        // Format 3 uses 24-bit class values, not just 24-bit glyph IDs.
        let bytes = [0, 3, 1, 0, 0, 0, 0, 2, 1, 0, 0, 0xFF, 0xFF, 0xFF];
        let class = ClassDef::read(FontData::new(&bytes)).unwrap();
        assert_eq!(class.get(GlyphId::new(65536)), 65536);
        assert_eq!(class.get(GlyphId::new(65537)), 0xFFFFFF);
        assert_eq!(class.get(GlyphId::new(65538)), 0);

        // Format 4's range records still store a 16-bit class value.
        let bytes = [0, 4, 0, 0, 1, 1, 0, 0, 1, 0, 2, 0xFF, 0xFF];
        let class = ClassDef::read(FontData::new(&bytes)).unwrap();
        assert_eq!(class.get(GlyphId::new(65538)), 65535);
        assert_eq!(class.get(GlyphId::new(65539)), 0);
        assert_eq!(class.iter().count(), 3);
    }

    #[test]
    fn truncated_wide_arrays_do_not_match() {
        for bytes in [&[0, 3, 0, 0, 1][..], &[0, 4, 0, 0, 1][..]] {
            let coverage = CoverageTable::read(FontData::new(bytes)).unwrap();
            assert_eq!(coverage.get(GlyphId::new(0)), None);
        }
        for bytes in [&[0, 3, 1, 0, 0, 0, 0, 1][..], &[0, 4, 0, 0, 1][..]] {
            let class = ClassDef::read(FontData::new(bytes)).unwrap();
            assert_eq!(class.get(GlyphId::new(65536)), 0);
        }
    }

    #[cfg(feature = "std")]
    #[test]
    fn wide_class_zero_includes_unassigned_glyphs() {
        let bytes = [0, 4, 0, 0, 1, 1, 0, 0, 1, 0, 2, 0, 1];
        let class = ClassDef::read(FontData::new(&bytes)).unwrap();
        let glyphs: IntSet<GlyphId> = [65535, 65536, 65538, 65539]
            .map(GlyphId::new)
            .into_iter()
            .collect();
        assert_eq!(
            class.intersect_classes(&glyphs).iter().collect::<Vec<_>>(),
            [0, 1]
        );
        assert_eq!(
            class
                .intersected_class_glyphs(&glyphs, 0)
                .iter()
                .map(GlyphId::to_u32)
                .collect::<Vec<_>>(),
            [65535, 65539]
        );
    }
}
