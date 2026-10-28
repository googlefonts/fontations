//! CFF1 and CFF2 subsetters. Shared interpreter policies mirror HarfBuzz's
//! template parameters without exposing them as a public extension API.

mod blend;
mod bounds;
mod charstring;
mod dict;
mod downgrade;
mod encoding;
mod instance;
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
    if data.iter().any(|byte| matches!(byte, 10 | 29)) {
        return true;
    }
    let Some((&14, body)) = data.split_last() else {
        return false;
    };
    let Some(&last) = body.last() else {
        return false;
    };
    matches!(last, 32..=246)
        || (body.len() >= 2 && body[body.len() - 2] == 12)
        || (body.len() >= 2 && matches!(body[body.len() - 2], 247..=254))
        || (body.len() >= 3 && body[body.len() - 3] == 28)
        || (body.len() >= 5 && body[body.len() - 5] == 255)
}

impl crate::instance::Instance<crate::instance::FontContext<'_>>
    for read_fonts::tables::cff2::Cff2<'_>
{
    type Output = Vec<u8>;
    fn instance(
        &self,
        context: &mut crate::instance::FontContext<'_>,
    ) -> std::result::Result<Self::Output, SubsetError> {
        instance::instance(context.font, context.axes, self)
            .map_err(|_| SubsetError::SubsetTableError(Tag::new(b"CFF2")))
    }
}

pub(crate) fn instance_bounds(
    font: &FontRef,
    axes: &crate::instance::AxisPlan,
) -> std::result::Result<Vec<Option<[f64; 4]>>, SubsetError> {
    bounds::bounds(font, axes).map_err(|_| SubsetError::SubsetTableError(Tag::new(b"CFF2")))
}

pub(crate) fn downgrade(font: &FontRef) -> std::result::Result<Vec<u8>, SubsetError> {
    downgrade::convert(font).map_err(|_| SubsetError::SubsetTableError(Tag::new(b"CFF2")))
}
