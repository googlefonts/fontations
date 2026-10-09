//! Resolve CFF2 charstring and Private DICT blends through the same region scalars.
use super::{
    charstring::{self, Interpreter, Program, Recorder},
    dict, encoding,
    source::{Cff2, Source},
    subset::{self, Programs},
    Error, Result,
};
use crate::{instance::AxisPlan, Plan, SubsetFlags};
use write_fonts::read::{
    collections::IntSet, tables::variations::ItemVariationStore, types::NameId, FontRef,
};

fn gains(store: &ItemVariationStore, ivs: usize, axes: &AxisPlan) -> Result<Vec<f64>> {
    let data = store
        .item_variation_data()
        .get(ivs)
        .ok_or(Error)?
        .map_err(|_| Error)?;
    let regions = store
        .variation_region_list()
        .map_err(|_| Error)?
        .variation_regions();
    data.region_indexes()
        .iter()
        .map(|idx| {
            Ok(regions
                .get(idx.get() as usize)
                .map_err(|_| Error)?
                .compute_scalar(&axes.coords)
                .to_f64())
        })
        .collect()
}
fn fold(default: f64, deltas: &[f64], gains: &[f64]) -> Result<f64> {
    if deltas.len() != gains.len() {
        return Err(Error);
    }
    Ok(default
        + deltas
            .iter()
            .zip(gains)
            .map(|(d, g)| d * g)
            .sum::<f64>()
            .round())
}

pub(super) fn instance(font: &FontRef, axes: &AxisPlan) -> Result<Vec<u8>> {
    use write_fonts::read::TableProvider;
    let data = font
        .data_for_tag(write_fonts::types::Tag::new(b"CFF2"))
        .ok_or(Error)?;
    let mut source = Source::new(data.as_bytes())?;
    let mut transforms = Vec::new();
    if let Some(store) = source.font.var_store() {
        for i in 0..store.item_variation_data_count() as usize {
            transforms.push(gains(store, i, axes)?);
        }
    }
    let mut chars = Vec::new();
    for gid in 0..source.font.num_glyphs() as usize {
        let fd = source.fd(write_fonts::types::GlyphId::new(gid as u32))?;
        let mut rec = Recorder::default();
        let mut interp =
            Interpreter::<Cff2, _, _>::new(&source, &mut rec, fd, source.fds[fd].ivs, false);
        interp.run(
            Program::Glyph(gid),
            source.font.charstrings().get(gid).ok_or(Error)?,
        )?;
        for command in &mut rec.commands {
            for value in &mut command.args {
                if let Some((ivs, deltas)) = value.blend.take() {
                    value.default =
                        fold(value.default, &deltas, transforms.get(ivs).ok_or(Error)?)?;
                }
            }
        }
        chars.push(charstring::flatten(&rec.commands, None, true, 0)?);
    }
    for fd in &mut source.fds {
        let mut ivs = 0;
        let mut entries = Vec::new();
        for entry in &fd.private {
            if entry.op == 19 {
                continue;
            }
            if entry.op == 22 {
                ivs = dict::uint(*entry.args.first().ok_or(Error)?)?;
                continue;
            }
            let mut p = 0;
            let mut stack = Vec::new();
            let mut raw = Vec::new();
            while p < entry.raw.len() {
                if let Some(v) = encoding::number(&entry.raw, &mut p, true)? {
                    stack.push(v);
                    continue;
                }
                let op = encoding::op(&entry.raw, &mut p)?;
                if op == 23 {
                    let n = dict::uint(stack.pop().ok_or(Error)?)?;
                    let g = transforms.get(ivs).ok_or(Error)?;
                    let k = g.len();
                    let start = stack
                        .len()
                        .checked_sub(n.checked_mul(k + 1).ok_or(Error)?)
                        .ok_or(Error)?;
                    let values = stack.split_off(start);
                    for i in 0..n {
                        stack.push(
                            (values[i]
                                + values[n + i * k..n + (i + 1) * k]
                                    .iter()
                                    .zip(g)
                                    .map(|(d, s)| d * s)
                                    .sum::<f64>())
                            .round(),
                        );
                    }
                } else {
                    for v in stack.drain(..) {
                        encoding::encode(&mut raw, v, true)?;
                    }
                    encoding::emit_op(&mut raw, op);
                }
            }
            let mut e = entry.clone();
            e.raw = raw;
            entries.push(e);
        }
        fd.private = entries;
        fd.ivs = 0;
    }
    source.top.retain(|entry| entry.op != 24);
    let plan = Plan::new(
        &IntSet::all(),
        &IntSet::empty(),
        font,
        SubsetFlags::SUBSET_FLAGS_NOTDEF_OUTLINE,
        &IntSet::empty(),
        &IntSet::all(),
        &IntSet::all(),
        &IntSet::<NameId>::all(),
        &IntSet::all(),
    );
    let locals = vec![vec![]; source.fds.len()];
    subset::assemble::<Cff2>(
        &source,
        &plan,
        Programs {
            chars,
            globals: vec![],
            locals,
        },
    )
}
