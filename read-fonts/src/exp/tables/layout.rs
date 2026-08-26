//! Layout common tables, generated into the reworked framework.

use super::super::prelude::*;

include!("../../../generated/exp/generated_layout.rs");

// Helper the generated `#[count(..)]` expression calls; copied from the
// hand-written half of `read-fonts/src/tables/layout.rs`.
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

// `extern scalar LookupFlag`: a plain scalar wrapper, reused as-is.
pub use crate::tables::layout::LookupFlag;

/// The target of [`Feature::feature_params_offset`], chosen by the feature tag.
///
/// Hand-written, as it is today: which table is present is decided by a tag
/// held by an ancestor rather than by a format word, so it is not a format
/// group. Under the new framework it is an ordinary `Table` with args.
#[derive(Clone, Copy)]
pub enum FeatureParams<'a> {
    StylisticSet(StylisticSetParams<'a>),
    Size(SizeParams<'a>),
    CharacterVariant(CharacterVariantParams<'a>),
}

impl<'a> Table<'a> for FeatureParams<'a> {
    type Args = Tag;

    const MIN_SIZE: usize = {
        let a = <StylisticSetParams as Table>::MIN_SIZE;
        let b = <SizeParams as Table>::MIN_SIZE;
        let c = <CharacterVariantParams as Table>::MIN_SIZE;
        let min = if a < b { a } else { b };
        if min < c {
            min
        } else {
            c
        }
    };

    fn read_with_args(data: Bytes<'a>, tag: Tag) -> Option<Self> {
        match tag {
            t if t == Tag::new(b"size") => SizeParams::read(data).map(Self::Size),
            t if &t.to_raw()[..2] == b"ss" => {
                StylisticSetParams::read(data).map(Self::StylisticSet)
            }
            t if &t.to_raw()[..2] == b"cv" => {
                CharacterVariantParams::read(data).map(Self::CharacterVariant)
            }
            // an offset exists but the tag does not say what it points at
            _ => None,
        }
    }
}

#[cfg(feature = "fast_sanitize")]
impl<'a> FastSanitize<'a> for FeatureParams<'a> {
    fn fast_sanitize_in(&self, ctx: &mut FastSanitizeContext) -> bool {
        match self {
            Self::StylisticSet(t) => t.fast_sanitize_in(ctx),
            Self::Size(t) => t.fast_sanitize_in(ctx),
            Self::CharacterVariant(t) => t.fast_sanitize_in(ctx),
        }
    }
}

#[cfg(feature = "sanitize")]
impl<'a> Sanitize<'a> for FeatureParams<'a> {
    const TYPE_NAME: &'static str = "FeatureParams";

    fn sanitize_in(&self, ctx: &mut SanitizeContext) {
        match self {
            Self::StylisticSet(t) => t.sanitize_in(ctx),
            Self::Size(t) => t.sanitize_in(ctx),
            Self::CharacterVariant(t) => t.sanitize_in(ctx),
        }
    }
}
