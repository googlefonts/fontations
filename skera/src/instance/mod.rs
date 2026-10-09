//! CFF2 instancing and the variation tables that share its axes.
mod axes;
mod layout;
mod metrics;
pub(crate) use axes::AxisPlan;
pub use axes::{parse_axis_limits, AxisLimits};

use crate::{cff, SubsetError};
use std::collections::BTreeMap;
use write_fonts::{
    read::{FontRef, TableProvider},
    types::Tag,
    FontBuilder,
};

/// Instantiate a CFF2 font at the requested axis locations.
///
/// Glyph IDs are preserved. Unspecified axes are retained. To subset the
/// instance, create a `Plan` from the returned font and call `subset_font`.
pub fn instance_font(font: &FontRef, limits: &[AxisLimits]) -> Result<Vec<u8>, SubsetError> {
    if limits.is_empty() {
        return Ok(copy_font(font));
    }
    if font.cff2().is_err() {
        return Err(SubsetError::InvalidAxis(
            "instancing requires CFF2 outlines".into(),
        ));
    }
    let axes = AxisPlan::new(font, limits)?;
    if !axes.all_pinned() {
        return Err(SubsetError::InvalidAxis(
            "partial instancing is not available yet".into(),
        ));
    }
    let mut tables: BTreeMap<Tag, Vec<u8>> = font
        .table_directory()
        .table_records()
        .iter()
        .filter_map(|r| Some((r.tag(), font.data_for_tag(r.tag())?.as_bytes().to_vec())))
        .collect();
    let cff2 = cff::instance(font, &axes)?;
    tables.insert(Tag::new(b"CFF2"), cff2);
    metrics::instance(font, &axes, &mut tables)?;
    layout::instance(font, &axes, &mut tables)?;
    for tag in [
        b"fvar", b"avar", b"HVAR", b"VVAR", b"MVAR", b"STAT", b"DSIG",
    ] {
        tables.remove(&Tag::new(tag));
    }
    let mut builder = FontBuilder::new();
    for (tag, data) in tables {
        builder.add_raw(tag, data);
    }
    Ok(builder.build())
}
fn copy_font(font: &FontRef) -> Vec<u8> {
    let mut builder = FontBuilder::new();
    for record in font.table_directory().table_records() {
        if let Some(data) = font.data_for_tag(record.tag()) {
            builder.add_raw(record.tag(), data);
        }
    }
    builder.build()
}
