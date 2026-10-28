//! Outline instancing and the variation tables that share its axes.
#![allow(dead_code)]
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
    let outlines = if font.glyf().is_ok() {
        OutlineKind::TrueType
    } else if font.cff2().is_ok() {
        OutlineKind::Cff2
    } else {
        return Err(SubsetError::InvalidAxis(
            "instancing requires CFF2 or TrueType outlines".into(),
        ));
    };
    let axes = AxisPlan::new(font, limits)?;
    if axes.values.is_empty() {
        return Ok(copy_font(font));
    }
    // These families select their uppercase tables independently. Updating only
    // a shadowed original would leave authoritative outlines or metrics stale.
    for tag in [
        b"GLYF", b"LOCA", b"GVAR", b"MAXP", b"HHEA", b"HMTX", b"VHEA", b"VMTX",
    ] {
        let tag = Tag::new(tag);
        if font.data_for_tag(tag).is_some() {
            return Err(SubsetError::InvalidAxis(format!(
                "instancing the {tag} table is not supported"
            )));
        }
    }
    let mut tables = FontOutput::default();
    let mut source = FontContext {
        font,
        axes: &axes,
        flags,
    };
    tables.instance_varc(&mut source)?;
    let final_axes = axes.coupled.then(|| axes.final_space(font));
    // Cull in the original final-coordinate space, before removing any
    // self-contained axes. avar's own store operates in intermediate space.
    let pruned = if axes.coupled {
        let changes = cull::collect(font, &axes.reachable)?;
        if changes.is_empty() {
            None
        } else {
            tables.apply_all(changes);
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
        tables.instance_outlines(&mut context, &mut metrics, outlines)?;
        tables.instance_layout(&mut context)?;
        tables.instance_color(&mut context)?;
    }
    if axes.coupled {
        metrics::update_os2(&axes, &mut metrics);
    }
    tables.apply_all(metrics.into_changes());
    tables.finish_metadata(&mut source)?;
    let bytes = tables.build(font);
    if axes.all_pinned() && flags.contains(SubsetFlags::SUBSET_FLAGS_DOWNGRADE_CFF2) {
        downgrade_cff2(
            &FontRef::new(&bytes).map_err(|_| SubsetError::SubsetTableError(Tag::new(b"CFF2")))?,
        )
    } else {
        Ok(bytes)
    }
}
#[derive(Clone, Copy)]
enum OutlineKind {
    TrueType,
    Cff2,
}
/// Only the font coordinator can replace or remove arbitrary output tables.
/// One entry per tag makes removal and replacement mutually exclusive.
#[derive(Default)]
struct FontOutput {
    changes: BTreeMap<Tag, TableChange>,
}
impl FontOutput {
    fn apply(&mut self, tag: Tag, change: TableChange) {
        // Keep retains a previous phase's candidate, including a removal.
        if !matches!(change, TableChange::Keep) {
            self.changes.insert(tag, change);
        }
    }
    fn apply_all(&mut self, changes: impl IntoIterator<Item = (Tag, TableChange)>) {
        for (tag, change) in changes {
            self.apply(tag, change);
        }
    }
    fn build(&self, source: &FontRef) -> Vec<u8> {
        let mut builder = FontBuilder::new();
        for record in source.table_directory().table_records() {
            let tag = record.tag();
            if !self.changes.contains_key(&tag) {
                if let Some(data) = source.data_for_tag(tag) {
                    builder.add_raw(tag, data.as_bytes());
                }
            }
        }
        for (tag, change) in &self.changes {
            if let TableChange::Replace(data) = change {
                builder.add_raw(*tag, data.as_slice());
            }
        }
        builder.build()
    }
    fn instance_varc(&mut self, context: &mut FontContext<'_>) -> Result<(), SubsetError> {
        let font = context.font;
        if font.data_for_tag(Tag::new(b"VARC")).is_some() {
            let source = font
                .varc()
                .map_err(|_| SubsetError::SubsetTableError(Tag::new(b"VARC")))?;
            self.apply(Tag::new(b"VARC"), source.instance(context)?);
        }
        Ok(())
    }
    fn instance_outlines(
        &mut self,
        context: &mut FontContext<'_>,
        metrics: &mut metrics::MetricTables,
        outlines: OutlineKind,
    ) -> Result<(), SubsetError> {
        let font = context.font;
        let axes = context.axes;
        match outlines {
            OutlineKind::TrueType => {
                let glyf = font
                    .glyf()
                    .map_err(|_| SubsetError::SubsetTableError(Tag::new(b"glyf")))?;
                let mut glyph_context = truetype::TrueTypeContext {
                    input: context,
                    metrics,
                };
                let output = glyf.instance(&mut glyph_context)?;
                self.apply(Tag::new(b"glyf"), TableChange::Replace(output.glyf));
                self.apply(Tag::new(b"loca"), TableChange::Replace(output.loca));
                self.apply(Tag::new(b"gvar"), output.gvar);
                self.apply(Tag::new(b"cvar"), output.cvar);
                if let Some(bytes) = output.cvt {
                    self.apply(Tag::new(b"cvt "), TableChange::Replace(bytes));
                }
            }
            OutlineKind::Cff2 => {
                let cff2 = font
                    .cff2()
                    .map_err(|_| SubsetError::SubsetTableError(Tag::new(b"CFF2")))?;
                self.apply(
                    Tag::new(b"CFF2"),
                    TableChange::Replace(cff2.instance(context)?),
                );
                metrics::cff2_metrics(font, axes, metrics)?;
            }
        }
        Ok(())
    }
    fn instance_layout(&mut self, context: &mut FontContext<'_>) -> Result<(), SubsetError> {
        let layout = layout::instance_layout(context.font, context.axes)?;
        for (tag, data) in [
            (b"GPOS", layout.gpos),
            (b"GSUB", layout.gsub),
            (b"GDEF", layout.gdef),
            (b"BASE", layout.base),
        ] {
            if let Some(data) = data {
                self.apply(Tag::new(tag), TableChange::Replace(data));
            }
        }
        Ok(())
    }
    fn instance_color(&mut self, context: &mut FontContext<'_>) -> Result<(), SubsetError> {
        if let Ok(colr) = context.font.colr() {
            self.apply(Tag::new(b"COLR"), colr.instance(context)?);
        }
        Ok(())
    }
    fn finish_metadata(&mut self, context: &mut FontContext<'_>) -> Result<(), SubsetError> {
        let font = context.font;
        let axes = context.axes;
        if axes.all_pinned() {
            for tag in [b"fvar", b"avar", b"HVAR", b"VVAR", b"MVAR"] {
                self.apply(Tag::new(tag), TableChange::Remove);
            }
        } else {
            let fvar = font
                .fvar()
                .map_err(|_| SubsetError::SubsetTableError(Tag::new(b"fvar")))?;
            let fvar = fvar.instance(context)?;
            self.apply(Tag::new(b"fvar"), TableChange::Replace(fvar.bytes));
            let mut avar_context = metadata::AvarContext {
                input: context,
                original_axes: &fvar.original_axes,
            };
            let avar = if let Ok(avar) = font.avar() {
                avar.instance(&mut avar_context)?
            } else {
                metadata::rebuild_avar(None, &avar_context)?
            };
            self.apply(Tag::new(b"avar"), TableChange::Replace(avar));
        }
        if font.data_for_tag(Tag::new(b"STAT")).is_some() {
            let stat = font
                .stat()
                .map_err(|_| SubsetError::SubsetTableError(Tag::new(b"STAT")))?;
            self.apply(
                Tag::new(b"STAT"),
                TableChange::Replace(stat.instance(context)?),
            );
        }
        self.apply(Tag::new(b"DSIG"), TableChange::Remove);
        Ok(())
    }
}

// HarfBuzz's roundf keeps ties toward positive infinity. Promote only at
// the rounding boundary: float addition can turn 0.5's predecessor into a
// tie, or change an already integral value above 2^23.
fn round_f32(value: f32) -> f64 {
    (value as f64 + 0.5).floor()
}

fn iup_delta(v: f32, a: f32, b: f32, da: f32, db: f32) -> f32 {
    if a == b {
        return if da == db { da } else { 0. };
    }
    if v <= a.min(b) {
        if a < b {
            da
        } else {
            db
        }
    } else if v >= a.max(b) {
        if a > b {
            da
        } else {
            db
        }
    } else {
        let r = (v - a) / (b - a);
        da + r * (db - da)
    }
}

fn copy_font(font: &FontRef) -> Vec<u8> {
    FontOutput::default().build(font)
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

#[cfg(any())]
mod tests {
    use super::*;

    #[test]
    fn table_changes_keep_the_latest_candidate_and_can_restore_removed_tables() {
        let tag = Tag::new(b"test");
        let unchanged = Tag::new(b"keep");
        let added = Tag::new(b"new!");
        let mut builder = FontBuilder::new();
        builder.add_raw(tag, vec![1]);
        builder.add_raw(unchanged, vec![4]);
        let bytes = builder.build();
        let font = FontRef::new(&bytes).unwrap();
        let snapshot = |output: &FontOutput| {
            let bytes = output.build(&font);
            let font = FontRef::new(&bytes).unwrap();
            [tag, unchanged, added]
                .map(|tag| font.data_for_tag(tag).map(|data| data.as_bytes().to_vec()))
        };
        let mut output = FontOutput::default();
        assert_eq!(snapshot(&output), [Some(vec![1]), Some(vec![4]), None]);
        output.apply(tag, TableChange::Replace(vec![2]));
        output.apply(tag, TableChange::Keep);
        assert_eq!(snapshot(&output), [Some(vec![2]), Some(vec![4]), None]);
        output.apply(tag, TableChange::Remove);
        output.apply(tag, TableChange::Keep);
        assert_eq!(snapshot(&output), [None, Some(vec![4]), None]);
        output.apply(tag, TableChange::Replace(vec![3]));
        output.apply(added, TableChange::Remove);
        output.apply(added, TableChange::Replace(vec![9]));
        assert_eq!(
            snapshot(&output),
            [Some(vec![3]), Some(vec![4]), Some(vec![9])]
        );
    }

    #[test]
    fn referenced_varc_axes_are_rejected_instead_of_copying_stale_axes() {
        let bytes = std::fs::read("test-data/fonts/varc-unrelated-axis.ttf").unwrap();
        let font = FontRef::new(&bytes).unwrap();
        assert!(matches!(
            instance_font(&font,&parse_axis_limits("wght=400").unwrap()),
            Err(SubsetError::SubsetTableError(tag)) if tag == Tag::new(b"VARC")
        ));
    }
    #[test]
    fn unsupported_extended_families_cannot_leave_authoritative_tables_stale() {
        for fixture in ["Roboto-Variable.ABC.ttf", "AdobeVFPrototype.otf"] {
            let bytes = std::fs::read(format!("test-data/fonts/{fixture}")).unwrap();
            let source = FontRef::new(&bytes).unwrap();
            for tag in [
                b"GLYF", b"LOCA", b"GVAR", b"MAXP", b"HHEA", b"HMTX", b"VHEA", b"VMTX",
            ] {
                let tag = Tag::new(tag);
                let mut builder = FontBuilder::new();
                for record in source.table_directory().table_records() {
                    builder.add_raw(record.tag(), source.data_for_tag(record.tag()).unwrap());
                }
                // An opaque extended table must never be ignored in favor of
                // the valid original tables already present in these fixtures.
                builder.add_raw(tag, vec![0xff]);
                let bytes = builder.build();
                let font = FontRef::new(&bytes).unwrap();
                assert!(
                    matches!(
                        instance_font(&font, &parse_axis_limits("wght=700").unwrap()),
                        Err(SubsetError::InvalidAxis(message)) if message.contains("not supported")
                    ),
                    "{fixture}: {tag}"
                );
                let copied = instance_font(&font, &[]).unwrap();
                let copied = FontRef::new(&copied).unwrap();
                assert_eq!(copied.data_for_tag(tag).unwrap().as_bytes(), &[0xff]);
            }
        }
    }
}
