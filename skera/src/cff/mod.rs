//! CFF1 and CFF2 subsetters. Shared interpreter policies mirror HarfBuzz's
//! template parameters without exposing them as a public extension API.

mod charstring;
mod dict;
mod encoding;
mod source;
mod subset;

#[derive(Debug, Clone, Copy)]
pub(super) struct Error;
type Result<T> = std::result::Result<T, Error>;

use crate::{Plan, SubsetError};
use read_fonts::{
    collections::IntSet,
    types::{GlyphId, Tag},
    FontRef, TableProvider,
};

pub(crate) fn subset_data(
    font: &FontRef,
    plan: &Plan,
    cff2: bool,
) -> std::result::Result<Vec<u8>, SubsetError> {
    let tag = if cff2 {
        Tag::new(b"CFF2")
    } else {
        Tag::new(b"CFF ")
    };
    (|| {
        let data = font.data_for_tag(tag).ok_or(Error)?;
        let source = source::Source::new(data.as_bytes())?;
        if cff2 {
            subset::assemble::<source::Cff2>(
                &source,
                plan,
                subset::programs::<source::Cff2>(&source, plan)?,
            )
        } else {
            subset::assemble::<source::Cff1>(
                &source,
                plan,
                subset::programs::<source::Cff1>(&source, plan)?,
            )
        }
    })()
    .map_err(|_| SubsetError::SubsetTableError(tag))
}

pub(crate) fn closure(font: &FontRef, glyphs: &mut IntSet<GlyphId>) {
    if let Some(data) = font.data_for_tag(Tag::new(b"CFF ")) {
        if let Ok(source) = source::Source::new(data.as_bytes()) {
            let _ = subset::closure(&source, glyphs);
        }
    }
}

fn charstring_may_end_in_seac(data: &[u8]) -> bool {
    let Some((&14, body)) = data.split_last() else {
        return false;
    };
    let Some(&last) = body.last() else {
        return false;
    };
    // Type 2 numbers are not self-synchronizing backwards. Interpret only
    // when a number encoding, or a call that can leave operands on the shared
    // stack, could immediately precede endchar.
    matches!(last, 32..=246 | 10 | 29)
        || (body.len() >= 2 && matches!(body[body.len() - 2], 247..=254))
        || (body.len() >= 3 && body[body.len() - 3] == 28)
        || (body.len() >= 5 && body[body.len() - 5] == 255)
}

#[cfg(test)]
mod tests {
    use super::charstring_may_end_in_seac;

    #[test]
    fn seac_precheck_accepts_only_possible_trailing_operands() {
        for data in [
            &[139, 14][..],
            &[247, 0, 14],
            &[28, 0, 0, 14],
            &[255, 0, 0, 0, 0, 14],
            &[10, 14],
            &[29, 14],
        ] {
            assert!(charstring_may_end_in_seac(data));
        }
        for data in [&[][..], &[14], &[139, 11], &[5, 14], &[247, 14]] {
            assert!(!charstring_may_end_in_seac(data));
        }
    }
}
