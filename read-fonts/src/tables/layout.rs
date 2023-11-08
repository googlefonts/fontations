//! OpenType Layout common table formats

#[path = "./lookupflag.rs"]
mod lookupflag;

use core::cmp::Ordering;

pub use lookupflag::LookupFlag;

#[cfg(test)]
#[path = "../tests/layout.rs"]
mod spec_tests;

include!("../../generated/generated_layout.rs");

impl<'a, T: FontRead<'a>> Lookup<'a, T> {
    pub fn get_subtable(&self, offset: Offset16) -> Result<T, ReadError> {
        self.resolve_offset(offset)
    }

    #[cfg(feature = "traversal")]
    fn traverse_lookup_flag(&self) -> traversal::FieldType<'a> {
        self.lookup_flag().to_bits().into()
    }
}

/// An enum for different possible tables referenced by [Feature::feature_params_offset]
pub enum FeatureParams<'a> {
    StylisticSet(StylisticSetParams<'a>),
    Size(SizeParams<'a>),
    CharacterVariant(CharacterVariantParams<'a>),
}

impl ReadArgs for FeatureParams<'_> {
    type Args = Tag;
}

impl<'a> FontReadWithArgs<'a> for FeatureParams<'a> {
    fn read_with_args(bytes: FontData<'a>, args: &Tag) -> Result<FeatureParams<'a>, ReadError> {
        match *args {
            t if t == Tag::new(b"size") => SizeParams::read(bytes).map(Self::Size),
            // to whoever is debugging this dumb bug I wrote: I'm sorry.
            t if &t.to_raw()[..2] == b"ss" => {
                StylisticSetParams::read(bytes).map(Self::StylisticSet)
            }
            t if &t.to_raw()[..2] == b"cv" => {
                CharacterVariantParams::read(bytes).map(Self::CharacterVariant)
            }
            // NOTE: what even is our error condition here? an offset exists but
            // we don't know the tag?
            _ => Err(ReadError::InvalidFormat(0xdead)),
        }
    }
}

#[cfg(feature = "traversal")]
impl<'a> SomeTable<'a> for FeatureParams<'a> {
    fn type_name(&self) -> &str {
        match self {
            FeatureParams::StylisticSet(table) => table.type_name(),
            FeatureParams::Size(table) => table.type_name(),
            FeatureParams::CharacterVariant(table) => table.type_name(),
        }
    }

    fn get_field(&self, idx: usize) -> Option<Field<'a>> {
        match self {
            FeatureParams::StylisticSet(table) => table.get_field(idx),
            FeatureParams::Size(table) => table.get_field(idx),
            FeatureParams::CharacterVariant(table) => table.get_field(idx),
        }
    }
}

impl FeatureTableSubstitutionRecord {
    pub fn alternate_feature<'a>(&self, data: FontData<'a>) -> Result<Feature<'a>, ReadError> {
        self.alternate_feature_offset()
            .resolve_with_args(data, &Tag::new(b"NULL"))
    }
}

impl<'a> CoverageTable<'a> {
    pub fn iter(&self) -> impl Iterator<Item = GlyphId> + 'a {
        // all one expression so that we have a single return type
        let (iter1, iter2) = match self {
            CoverageTable::Format1(t) => (Some(t.glyph_array().iter().map(|g| g.get())), None),
            CoverageTable::Format2(t) => {
                let iter = t.range_records().iter().flat_map(RangeRecord::iter);
                (None, Some(iter))
            }
        };

        iter1
            .into_iter()
            .flatten()
            .chain(iter2.into_iter().flatten())
    }

    /// If this glyph is in the coverage table, returns its index
    pub fn get(&self, gid: GlyphId) -> Option<u16> {
        match self {
            CoverageTable::Format1(sub) => sub.get(gid),
            CoverageTable::Format2(sub) => sub.get(gid),
        }
    }
}

impl CoverageFormat1<'_> {
    /// If this glyph is in the coverage table, returns its index
    pub fn get(&self, gid: GlyphId) -> Option<u16> {
        let be_glyph: BigEndian<GlyphId> = gid.into();
        self.glyph_array()
            .binary_search(&be_glyph)
            .ok()
            .map(|idx| idx as _)
    }
}

impl CoverageFormat2<'_> {
    /// If this glyph is in the coverage table, returns its index
    pub fn get(&self, gid: GlyphId) -> Option<u16> {
        self.range_records()
            .binary_search_by(|rec| {
                if rec.end_glyph_id() < gid {
                    Ordering::Less
                } else if rec.start_glyph_id() > gid {
                    Ordering::Greater
                } else {
                    Ordering::Equal
                }
            })
            .ok()
            .map(|idx| {
                let rec = &self.range_records()[idx];
                rec.start_coverage_index() + gid.to_u16() - rec.start_glyph_id().to_u16()
            })
    }
}

impl RangeRecord {
    fn iter(&self) -> impl Iterator<Item = GlyphId> + '_ {
        (self.start_glyph_id().to_u16()..=self.end_glyph_id().to_u16()).map(GlyphId::new)
    }
}

impl DeltaFormat {
    pub(crate) fn value_count(self, start_size: u16, end_size: u16) -> usize {
        let range_len = end_size.saturating_add(1).saturating_sub(start_size) as usize;
        let val_per_word = match self {
            DeltaFormat::Local2BitDeltas => 8,
            DeltaFormat::Local4BitDeltas => 4,
            DeltaFormat::Local8BitDeltas => 2,
            _ => return 0,
        };

        let count = range_len / val_per_word;
        let extra = (range_len % val_per_word).min(1);
        count + extra
    }
}

// we as a 'format' in codegen, and the generic error type for an invalid format
// stores the value as an i64, so we need this conversion.
impl From<DeltaFormat> for i64 {
    fn from(value: DeltaFormat) -> Self {
        value as u16 as _
    }
}

impl<'a> ClassDefFormat1<'a> {
    /// Get the class for this glyph id
    pub fn get(&self, gid: GlyphId) -> u16 {
        if gid < self.start_glyph_id() {
            return 0;
        }
        let idx = gid.to_u16() - self.start_glyph_id().to_u16();
        self.class_value_array()
            .get(idx as usize)
            .map(|x| x.get())
            .unwrap_or(0)
    }

    /// Iterate over each glyph and its class.
    pub fn iter(&self) -> impl Iterator<Item = (GlyphId, u16)> + 'a {
        let start = self.start_glyph_id();
        self.class_value_array()
            .iter()
            .enumerate()
            .map(move |(i, val)| {
                let gid = start.to_u16().saturating_add(i as u16);
                (GlyphId::new(gid), val.get())
            })
    }
}

impl<'a> ClassDefFormat2<'a> {
    /// Get the class for this glyph id
    pub fn get(&self, gid: GlyphId) -> u16 {
        self.class_range_records()
            .iter()
            .find_map(|record| {
                (record.start_glyph_id() >= gid && record.end_glyph_id() <= gid)
                    .then_some(record.class())
            })
            .unwrap_or(0)
    }

    /// Iterate over each glyph and its class.
    pub fn iter(&self) -> impl Iterator<Item = (GlyphId, u16)> + 'a {
        self.class_range_records().iter().flat_map(|range| {
            let start = range.start_glyph_id().to_u16();
            let end = range.end_glyph_id().to_u16();
            (start..=end).map(|gid| (GlyphId::new(gid), range.class()))
        })
    }
}

impl ClassDef<'_> {
    /// Get the class for this glyph id
    pub fn get(&self, gid: GlyphId) -> u16 {
        match self {
            ClassDef::Format1(table) => table.get(gid),
            ClassDef::Format2(table) => table.get(gid),
        }
    }

    /// Iterate over each glyph and its class.
    ///
    /// This will not include class 0 unless it has been explicitly assigned.
    pub fn iter(&self) -> impl Iterator<Item = (GlyphId, u16)> + '_ {
        let (one, two) = match self {
            ClassDef::Format1(inner) => (Some(inner.iter()), None),
            ClassDef::Format2(inner) => (None, Some(inner.iter())),
        };
        one.into_iter().flatten().chain(two.into_iter().flatten())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coverage_get_format1() {
        // manually generated, corresponding to the glyphs (1, 7, 13, 27, 44);
        const COV1_DATA: FontData = FontData::new(&[0, 1, 0, 5, 0, 1, 0, 7, 0, 13, 0, 27, 0, 44]);

        let coverage = CoverageFormat1::read(COV1_DATA).unwrap();
        assert_eq!(coverage.get(GlyphId::new(1)), Some(0));
        assert_eq!(coverage.get(GlyphId::new(2)), None);
        assert_eq!(coverage.get(GlyphId::new(7)), Some(1));
        assert_eq!(coverage.get(GlyphId::new(27)), Some(3));
        assert_eq!(coverage.get(GlyphId::new(45)), None);
    }

    #[test]
    fn coverage_get_format2() {
        // manually generated, corresponding to glyphs (5..10) and (30..40).
        const COV2_DATA: FontData =
            FontData::new(&[0, 2, 0, 2, 0, 5, 0, 9, 0, 0, 0, 30, 0, 39, 0, 5]);
        let coverage = CoverageFormat2::read(COV2_DATA).unwrap();
        assert_eq!(coverage.get(GlyphId::new(2)), None);
        assert_eq!(coverage.get(GlyphId::new(7)), Some(2));
        assert_eq!(coverage.get(GlyphId::new(9)), Some(4));
        assert_eq!(coverage.get(GlyphId::new(10)), None);
        assert_eq!(coverage.get(GlyphId::new(32)), Some(7));
        assert_eq!(coverage.get(GlyphId::new(39)), Some(14));
        assert_eq!(coverage.get(GlyphId::new(40)), None);
    }
}
