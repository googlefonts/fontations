//! Outline instancing and the variation tables that share its axes.
mod avar2;
mod axes;
mod color;
mod cull;
mod iup;
mod layout;
mod metadata;
mod metrics;
mod optimize;
mod rebase;
pub(crate) mod scalars;
mod serialize;
mod stat;
mod store;
mod table;
mod truetype;
mod tuple;
pub(crate) use axes::AxisPlan;
pub use axes::{parse_axis_limits, parse_axis_limits_for_font, AxisLimits};
pub(crate) use store::StorePlan;
pub(crate) use table::{FontContext, Instance, TableChange};

use crate::{cff, SubsetError, SubsetFlags};
use read_fonts::collections::IntSet;
use read_fonts::{types::Tag, FontRef, TableProvider};
use std::collections::BTreeMap;
use write_fonts::FontBuilder;

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
    if axes.values.is_empty() {
        return Ok(copy_font(font));
    }
    let mut tables = FontOutput::default();
    if font.data_for_tag(Tag::new(b"VARC")).is_some() {
        let source = font
            .varc()
            .map_err(|_| SubsetError::SubsetTableError(Tag::new(b"VARC")))?;
        let mut context = FontContext {
            font,
            axes: &axes,
            flags,
        };
        tables.apply(Tag::new(b"VARC"), source.instance(&mut context)?);
    }
    let final_axes = axes.coupled.then(|| axes.final_space(font));
    // Cull in the original final-coordinate space, before removing any
    // self-contained axes. avar's own store operates in intermediate space.
    let pruned = if axes.coupled {
        let changes = cull::collect(font, &axes.reachable)?;
        if changes.is_empty() {
            None
        } else {
            for (tag, change) in changes {
                tables.apply(tag, change);
            }
            Some(tables.build(font))
        }
    } else {
        None
    };
    let variation_font = pruned
        .as_deref()
        .map(FontRef::new)
        .transpose()
        .map_err(|_| SubsetError::SubsetTableError(Tag::new(b"avar")))?;
    let variation_font = variation_font.as_ref().unwrap_or(font);
    let mut metrics = metrics::MetricTables::new(font);
    if !axes.coupled || axes.pinned.iter().any(|&p| p) {
        let axes = final_axes.as_ref().unwrap_or(&axes);
        let mut context = FontContext {
            font: variation_font,
            axes,
            flags,
        };
        if is_truetype {
            let glyf = variation_font
                .glyf()
                .map_err(|_| SubsetError::SubsetTableError(Tag::new(b"glyf")))?;
            let mut glyph_context = truetype::TrueTypeContext {
                input: &context,
                metrics: &mut metrics,
            };
            let output = glyf.instance(&mut glyph_context)?;
            tables.apply(Tag::new(b"glyf"), TableChange::Replace(output.glyf));
            tables.apply(Tag::new(b"loca"), TableChange::Replace(output.loca));
            tables.apply(Tag::new(b"gvar"), output.gvar);
            tables.apply(Tag::new(b"cvar"), output.cvar);
            if let Some(bytes) = output.cvt {
                tables.apply(Tag::new(b"cvt "), TableChange::Replace(bytes));
            }
        } else {
            let cff2 = variation_font
                .cff2()
                .map_err(|_| SubsetError::SubsetTableError(Tag::new(b"CFF2")))?;
            tables.apply(
                Tag::new(b"CFF2"),
                TableChange::Replace(cff2.instance(&mut context)?),
            );
            metrics::cff2_metrics(variation_font, axes, &mut metrics)?;
        }
        let layout = layout::instance_layout(variation_font, axes)?;
        for (tag, data) in [
            (b"GPOS", layout.gpos),
            (b"GSUB", layout.gsub),
            (b"MATH", layout.math),
            (b"JSTF", layout.jstf),
            (b"GDEF", layout.gdef),
            (b"BASE", layout.base),
        ] {
            if let Some(data) = data {
                tables.apply(Tag::new(tag), TableChange::Replace(data));
            }
        }
        if let Ok(colr) = variation_font.colr() {
            tables.apply(Tag::new(b"COLR"), colr.instance(&mut context)?);
        }
    }
    if axes.coupled {
        metrics::update_os2(&axes, &mut metrics);
    }
    for (tag, change) in metrics.into_changes() {
        tables.apply(tag, change);
    }
    if axes.all_pinned() {
        for tag in [b"fvar", b"avar", b"HVAR", b"VVAR", b"MVAR"] {
            tables.apply(Tag::new(tag), TableChange::Remove);
        }
    } else {
        let mut context = FontContext {
            font,
            axes: &axes,
            flags,
        };
        let fvar = font
            .fvar()
            .map_err(|_| SubsetError::SubsetTableError(Tag::new(b"fvar")))?;
        let fvar = fvar.instance(&mut context)?;
        tables.apply(Tag::new(b"fvar"), TableChange::Replace(fvar.bytes));
        let mut avar_context = metadata::AvarContext {
            input: &context,
            original_axes: &fvar.original_axes,
        };
        let avar = if let Ok(avar) = font.avar() {
            avar.instance(&mut avar_context)?
        } else {
            metadata::rebuild_avar(None, &avar_context)?
        };
        tables.apply(Tag::new(b"avar"), TableChange::Replace(avar));
    }
    if font.data_for_tag(Tag::new(b"STAT")).is_some() {
        let stat = font
            .stat()
            .map_err(|_| SubsetError::SubsetTableError(Tag::new(b"STAT")))?;
        let mut context = FontContext {
            font,
            axes: &axes,
            flags,
        };
        tables.apply(
            Tag::new(b"STAT"),
            TableChange::Replace(stat.instance(&mut context)?),
        );
    }
    tables.apply(Tag::new(b"DSIG"), TableChange::Remove);
    let bytes = tables.build(font);
    if !is_truetype && axes.all_pinned() && flags.contains(SubsetFlags::SUBSET_FLAGS_DOWNGRADE_CFF2)
    {
        downgrade_cff2(
            &FontRef::new(&bytes).map_err(|_| SubsetError::SubsetTableError(Tag::new(b"CFF2")))?,
        )
    } else {
        Ok(bytes)
    }
}
/// Only the font coordinator can replace or remove arbitrary output tables.
#[derive(Default)]
struct FontOutput {
    replacements: BTreeMap<Tag, Vec<u8>>,
    removed: IntSet<Tag>,
}
impl FontOutput {
    fn apply(&mut self, tag: Tag, change: TableChange) {
        match change {
            TableChange::Keep => {}
            TableChange::Replace(bytes) => {
                self.removed.remove(tag);
                self.replacements.insert(tag, bytes);
            }
            TableChange::Remove => {
                self.replacements.remove(&tag);
                self.removed.insert(tag);
            }
        }
    }
    fn build(&self, source: &FontRef) -> Vec<u8> {
        let mut builder = FontBuilder::new();
        for record in source.table_directory().table_records() {
            let tag = record.tag();
            if !self.removed.contains(tag) && !self.replacements.contains_key(&tag) {
                if let Some(data) = source.data_for_tag(tag) {
                    builder.add_raw(tag, data.as_bytes());
                }
            }
        }
        for (tag, data) in &self.replacements {
            builder.add_raw(*tag, data.as_slice());
        }
        builder.build()
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
