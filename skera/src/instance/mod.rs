//! CFF2 instancing and the variation tables that share its axes.
mod avar2;
mod axes;
mod color;
mod layout;
mod metrics;
mod rebase;
mod store;
pub(crate) use axes::AxisPlan;
pub use axes::{parse_axis_limits, AxisLimits};
pub(crate) use store::StorePlan;

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
/// Coupled avar2 pins remain hidden axes until full instancing; restricted
/// ranges can introduce F2Dot14 rounding differences in the axis mapping.
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
    let mut tables: BTreeMap<Tag, Vec<u8>> = font
        .table_directory()
        .table_records()
        .iter()
        .filter_map(|r| Some((r.tag(), font.data_for_tag(r.tag())?.as_bytes().to_vec())))
        .collect();
    if !axes.coupled {
        let cff2 = cff::instance(font, &axes)?;
        tables.insert(Tag::new(b"CFF2"), cff2);
        metrics::instance(font, &axes, &mut tables)?;
        layout::instance(font, &axes, &mut tables)?;
        color::instance(font, &axes, &mut tables)?;
    }
    if axes.all_pinned() {
        for tag in [b"fvar", b"avar", b"HVAR", b"VVAR", b"MVAR"] {
            tables.remove(&Tag::new(tag));
        }
    } else {
        axes.update_tables(font, &mut tables)?;
    }
    for tag in [b"STAT", b"DSIG"] {
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

/// Convert a fully instantiated CFF2 font to CID-keyed CFF1 outlines.
///
/// Glyph IDs and metrics are preserved. Variable fonts must first be fully
/// instantiated with `instance_font`.
pub fn downgrade_cff2(font: &FontRef) -> Result<Vec<u8>, SubsetError> {
    if font.fvar().is_ok() {
        return Err(SubsetError::InvalidAxis(
            "CFF2 downgrade requires a full instance".into(),
        ));
    }
    let data = cff::downgrade(font)?;
    let mut builder = FontBuilder::new();
    for r in font.table_directory().table_records() {
        if r.tag() != Tag::new(b"CFF2") {
            if let Some(data) = font.data_for_tag(r.tag()) {
                builder.add_raw(r.tag(), data);
            }
        }
    }
    builder.add_raw(Tag::new(b"CFF "), data);
    Ok(builder.build())
}
