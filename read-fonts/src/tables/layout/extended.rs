//! Accessors for the ISO OFF extended coverage and class definitions.

use super::*;

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
