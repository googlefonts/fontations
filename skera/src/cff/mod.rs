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
