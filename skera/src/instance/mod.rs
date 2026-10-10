//! Outline instancing and the variation tables that share its axes.
mod avar2;
mod axes;
mod color;
mod cull;
mod iup;
mod layout;
mod metrics;
mod optimize;
mod rebase;
pub(crate) mod scalars;
mod stat;
mod store;
mod truetype;
mod tuple;
pub(crate) use axes::AxisPlan;
pub use axes::{parse_axis_limits, parse_axis_limits_for_font, AxisLimits};
pub(crate) use store::StorePlan;

use crate::{cff, SubsetError, SubsetFlags};
use std::collections::BTreeMap;
use write_fonts::{
    read::{FontRef, TableProvider},
    types::Tag,
    FontBuilder,
};

/// Instantiate a CFF2 or TrueType font at the requested axis locations.
///
/// Glyph IDs are preserved. Unspecified axes are retained. To subset the
/// instance, create a `Plan` from the returned font and call `subset_font`.
/// TrueType avar2 pins with constant final coordinates are removed. Other
/// coupled pins remain hidden; restricted ranges can introduce F2Dot14
/// rounding differences in the axis mapping.
/// Unreachable final-space variation regions and glyph tuples are removed.
/// Axes referenced by retained VARC data cannot be instanced. Other axes can
/// be pinned or restricted; VARC's retained axis references are remapped.
pub fn instance_font(font: &FontRef, limits: &[AxisLimits]) -> Result<Vec<u8>, SubsetError> {
    instance_font_with_flags(font, limits, SubsetFlags::default())
}

/// Instantiate a font, applying instancing options from `flags`.
///
/// `SUBSET_FLAGS_OPTIMIZE_IUP_DELTAS` optimizes residual TrueType tuples after
/// rebasing and merging. `SUBSET_FLAGS_DOWNGRADE_CFF2` converts full CFF2
/// instances to CFF1. Other subsetting flags apply when `subset_font` is
/// called on the resulting font. See `instance_font` for axis semantics.
pub fn instance_font_with_flags(
    font: &FontRef,
    limits: &[AxisLimits],
    flags: SubsetFlags,
) -> Result<Vec<u8>, SubsetError> {
    if limits.is_empty() {
        return Ok(copy_font(font));
    }
    let is_truetype = font.glyf().is_ok();
    if font.cff2().is_err() && !is_truetype {
        return Err(SubsetError::InvalidAxis(
            "instancing requires CFF2 or TrueType outlines".into(),
        ));
    }
    let axes = AxisPlan::new(font, limits)?;
    let mut tables: BTreeMap<Tag, Vec<u8>> = font
        .table_directory()
        .table_records()
        .iter()
        .filter_map(|r| Some((r.tag(), font.data_for_tag(r.tag())?.as_bytes().to_vec())))
        .collect();
    if font.data_for_tag(Tag::new(b"VARC")).is_some() {
        let bytes = crate::varc::instance(font, &axes)?;
        if bytes.is_empty() {
            tables.remove(&Tag::new(b"VARC"));
        } else {
            tables.insert(Tag::new(b"VARC"), bytes);
        }
    }
    let final_axes = axes.coupled.then(|| axes.final_space(font));
    // Cull in the original final-coordinate space, before removing any
    // self-contained axes. avar's own store operates in intermediate space.
    let pruned = if axes.coupled && cull::tables(font, &axes.reachable, &mut tables)? {
        let mut builder = FontBuilder::new();
        for (tag, data) in &tables {
            builder.add_raw(*tag, data);
        }
        Some(builder.build())
    } else {
        None
    };
    let variation_font = pruned
        .as_deref()
        .map(FontRef::new)
        .transpose()
        .map_err(|_| SubsetError::SubsetTableError(Tag::new(b"avar")))?;
    let variation_font = variation_font.as_ref().unwrap_or(font);
    if !axes.coupled || axes.pinned.iter().any(|&p| p) {
        let axes = final_axes.as_ref().unwrap_or(&axes);
        if is_truetype {
            truetype::instance(
                variation_font,
                axes,
                &mut tables,
                flags.contains(SubsetFlags::SUBSET_FLAGS_OPTIMIZE_IUP_DELTAS),
            )?;
        } else {
            let cff2 = cff::instance(variation_font, axes)?;
            tables.insert(Tag::new(b"CFF2"), cff2);
            metrics::instance(variation_font, axes, &mut tables)?;
        }
        layout::instance(variation_font, axes, &mut tables)?;
        color::instance(variation_font, axes, &mut tables)?;
    }
    if axes.coupled {
        metrics::update_os2(&axes, &mut tables);
    }
    if axes.all_pinned() {
        for tag in [b"fvar", b"avar", b"HVAR", b"VVAR", b"MVAR"] {
            tables.remove(&Tag::new(tag));
        }
    } else {
        axes.update_tables(font, &mut tables)?;
    }
    stat::instance(font, &axes, &mut tables)?;
    tables.remove(&Tag::new(b"DSIG"));
    let mut builder = FontBuilder::new();
    for (tag, data) in tables {
        builder.add_raw(tag, data);
    }
    let bytes = builder.build();
    if !is_truetype && axes.all_pinned() && flags.contains(SubsetFlags::SUBSET_FLAGS_DOWNGRADE_CFF2)
    {
        downgrade_cff2(
            &FontRef::new(&bytes).map_err(|_| SubsetError::SubsetTableError(Tag::new(b"CFF2")))?,
        )
    } else {
        Ok(bytes)
    }
}
// HarfBuzz's roundf keeps ties toward positive infinity. Promote only at
// the rounding boundary: float addition can turn 0.5's predecessor into a
// tie, or change an already integral value above 2^23.
fn round_f32(value: f32) -> f64 {
    (value as f64 + 0.5).floor()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn referenced_varc_axes_are_rejected_instead_of_copying_stale_axes() {
        let bytes = std::fs::read("test-data/fonts/varc-unrelated-axis.ttf").unwrap();
        let font = FontRef::new(&bytes).unwrap();
        assert!(matches!(
            instance_font(&font,&parse_axis_limits("wght=400").unwrap()),
            Err(SubsetError::SubsetTableError(tag)) if tag == Tag::new(b"VARC")
        ));
    }
}
