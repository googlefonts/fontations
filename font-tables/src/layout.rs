//! OpenType Layout

pub mod gdef;
pub mod gpos;

#[cfg(test)]
#[path = "./tests/layout.rs"]
mod tests;

include!("../generated/layout.rs");

/// A typed lookup table.
///
/// Our generated code doesn't handle generics, so we define this ourselves.
pub struct TypedLookup<'a, T> {
    inner: Lookup<'a>,
    phantom: std::marker::PhantomData<T>,
}

impl<'a, T: FontRead<'a>> TypedLookup<'a, T> {
    pub(crate) fn new(inner: Lookup<'a>) -> Self {
        TypedLookup {
            inner,
            phantom: std::marker::PhantomData,
        }
    }

    pub fn get_subtable(&self, offset: Offset16) -> Result<T, ReadError> {
        self.inner.resolve_offset(offset)
    }
}

impl<'a, T> std::ops::Deref for TypedLookup<'a, T> {
    type Target = Lookup<'a>;
    fn deref(&self) -> &Self::Target {
        &self.inner
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

impl FeatureTableSubstitutionRecord {
    pub fn alternate_feature<'a>(&self, data: &FontData<'a>) -> Result<Feature<'a>, ReadError> {
        self.alternate_feature_offset()
            .resolve_with_args(data, &Tag::new(b"NULL"))
    }
}

impl CoverageTable<'_> {
    pub fn iter(&self) -> impl Iterator<Item = GlyphId> + '_ {
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
}

impl RangeRecord {
    fn iter(&self) -> impl Iterator<Item = GlyphId> + '_ {
        (self.start_glyph_id().to_u16()..=self.end_glyph_id().to_u16()).map(GlyphId::new)
    }
}

impl Default for DeltaFormat {
    fn default() -> Self {
        DeltaFormat::Local2BitDeltas
    }
}

fn delta_value_count(start_size: u16, end_size: u16, delta_format: DeltaFormat) -> usize {
    let range_len = end_size.saturating_add(1).saturating_sub(start_size) as usize;
    let val_per_word = match delta_format {
        DeltaFormat::Local2BitDeltas => 8,
        DeltaFormat::Local4BitDeltas => 4,
        DeltaFormat::Local8BitDeltas => 2,
        _ => return 0,
    };

    let count = range_len / val_per_word;
    let extra = (range_len % val_per_word).min(1);
    count + extra
}

fn minus_one(val: impl Into<usize>) -> usize {
    val.into().saturating_sub(1)
}
