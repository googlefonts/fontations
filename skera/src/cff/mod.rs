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

use crate::{
    serialize::{SerializeErrorFlags, Serializer},
    Plan, SubsetError,
};
use write_fonts::read::{
    collections::IntSet,
    types::{GlyphId, Tag},
    FontRef, TableProvider,
};

pub(crate) fn subset(
    font: &FontRef,
    plan: &Plan,
    s: &mut Serializer,
    cff2: bool,
) -> std::result::Result<(), SubsetError> {
    let tag = if cff2 {
        Tag::new(b"CFF2")
    } else {
        Tag::new(b"CFF ")
    };
    let result = (|| {
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
    })();
    match result {
        Ok(data) => s
            .embed_bytes(&data)
            .map(|_| ())
            .map_err(|_| SubsetError::SubsetTableError(tag)),
        Err(_) => {
            s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR);
            Err(SubsetError::SubsetTableError(tag))
        }
    }
}

pub(crate) fn closure(font: &FontRef, glyphs: &mut IntSet<GlyphId>) {
    if let Some(data) = font.data_for_tag(Tag::new(b"CFF ")) {
        if let Ok(source) = source::Source::new(data.as_bytes()) {
            let _ = subset::closure(&source, glyphs);
        }
    }
}
