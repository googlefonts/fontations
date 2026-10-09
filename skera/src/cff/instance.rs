//! Resolve CFF2 charstring and Private DICT blends through the same region scalars.
use super::{
    charstring::{self, Interpreter, Program, Recorder, Value},
    dict, encoding,
    source::{Cff2, Source},
    subset::{self, Programs},
    Error, Result,
};
use crate::{
    instance::{AxisPlan, StorePlan},
    Plan, SubsetFlags,
};
use write_fonts::read::{
    collections::IntSet, tables::variations::ItemVariationStore, types::NameId, FontRef,
};

fn gains(store: &ItemVariationStore, ivs: usize, axes: &AxisPlan) -> Result<Vec<f64>> {
    let data = store.item_variation_data().get(ivs);
    let Some(data) = data else {
        return Ok(vec![]);
    };
    let data = data.map_err(|_| Error)?;
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
    let partial = !axes.all_pinned();
    let store_plan = source
        .font
        .var_store()
        .map(|s| StorePlan::new(s, axes))
        .transpose()
        .map_err(|_| Error)?;
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
                    if partial {
                        let transform = store_plan
                            .as_ref()
                            .ok_or(Error)?
                            .transforms
                            .get(ivs)
                            .ok_or(Error)?;
                        value.default += transform.fold(&deltas).map_err(|_| Error)?.round();
                        let residual: Vec<_> = transform
                            .residual(&deltas)
                            .map_err(|_| Error)?
                            .into_iter()
                            .map(f64::round)
                            .collect();
                        if residual.iter().any(|v| *v != 0.) {
                            value.blend = Some((ivs, residual));
                        }
                    } else {
                        value.default =
                            fold(value.default, &deltas, transforms.get(ivs).ok_or(Error)?)?;
                    }
                }
            }
        }
        chars.push(charstring::flatten(
            &rec.commands,
            None,
            true,
            if partial { source.fds[fd].ivs } else { 0 },
        )?);
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
                if partial {
                    entries.push(entry.clone());
                }
                continue;
            }
            let mut p = 0;
            let mut stack: Vec<Value> = Vec::new();
            let mut raw = Vec::new();
            let mut fold_exact = 0.;
            let mut fold_emitted = 0.;
            while p < entry.raw.len() {
                if let Some(v) = encoding::number(&entry.raw, &mut p, true)? {
                    stack.push(Value::plain(v));
                    continue;
                }
                let op = encoding::op(&entry.raw, &mut p)?;
                if op == 23 {
                    let n = dict::uint(stack.pop().ok_or(Error)?.default)?;
                    let g = transforms.get(ivs).ok_or(Error)?;
                    let k = g.len();
                    let start = stack
                        .len()
                        .checked_sub(n.checked_mul(k + 1).ok_or(Error)?)
                        .ok_or(Error)?;
                    let values = stack.split_off(start);
                    for i in 0..n {
                        let deltas: Vec<_> = values[n + i * k..n + (i + 1) * k]
                            .iter()
                            .map(|v| v.default)
                            .collect();
                        let mut value = values[i].clone();
                        if partial {
                            let t = store_plan
                                .as_ref()
                                .ok_or(Error)?
                                .transforms
                                .get(ivs)
                                .ok_or(Error)?;
                            let f = t.fold(&deltas).map_err(|_| Error)?;
                            fold_exact += f;
                            let emitted = fold_exact.round();
                            value.default += emitted - fold_emitted;
                            fold_emitted = emitted;
                            let residual: Vec<_> = t
                                .residual(&deltas)
                                .map_err(|_| Error)?
                                .into_iter()
                                .map(f64::round)
                                .collect();
                            if residual.iter().any(|v| *v != 0.) {
                                value.blend = Some((ivs, residual));
                            }
                        } else {
                            value.default = (value.default
                                + deltas.iter().zip(g).map(|(d, s)| d * s).sum::<f64>())
                            .round();
                        }
                        stack.push(value);
                    }
                } else {
                    let k = stack
                        .iter()
                        .find_map(|v| v.blend.as_ref().map(|(_, d)| d.len()));
                    if let Some(k) = k {
                        let mut done = 0;
                        while done < stack.len() {
                            let count = ((512 - done) / (k + 1)).min(stack.len() - done);
                            if count == 0 {
                                return Err(Error);
                            }
                            for v in &stack[done..done + count] {
                                encoding::encode(&mut raw, v.default, true)?;
                            }
                            for v in &stack[done..done + count] {
                                for i in 0..k {
                                    encoding::encode(
                                        &mut raw,
                                        v.blend.as_ref().map_or(0., |(_, d)| d[i]),
                                        true,
                                    )?;
                                }
                            }
                            encoding::encode(&mut raw, count as f64, true)?;
                            raw.push(23);
                            done += count;
                        }
                    } else {
                        for v in &stack {
                            encoding::encode(&mut raw, v.default, true)?;
                        }
                    }
                    stack.clear();
                    encoding::emit_op(&mut raw, op);
                }
            }
            let mut e = entry.clone();
            e.raw = raw;
            entries.push(e);
        }
        fd.private = entries;
        if !partial {
            fd.ivs = 0;
        }
    }
    if partial {
        if let (Some(plan), Some(store)) = (&store_plan, source.font.var_store()) {
            let table = plan.rebuild(store).map_err(|_| Error)?;
            let bytes = write_fonts::dump_table(&table).map_err(|_| Error)?;
            let mut data = u16::try_from(bytes.len())
                .map_err(|_| Error)?
                .to_be_bytes()
                .to_vec();
            data.extend(bytes);
            source.instanced_store = Some(data);
        }
    } else {
        source.top.retain(|entry| entry.op != 24);
    }
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
    chars.resize(plan.num_output_glyphs, vec![]);
    let bytes = subset::assemble::<Cff2>(
        &source,
        &plan,
        Programs {
            chars,
            globals: vec![],
            locals,
        },
    )?;
    if partial {
        compact(&bytes, &plan)
    } else {
        Ok(bytes)
    }
}

/// Flattening leaves a self-contained table, so a second symbolic pass can
/// compact VarData and region indices without touching subroutine namespaces.
fn compact(bytes: &[u8], plan: &Plan) -> Result<Vec<u8>> {
    use std::collections::{BTreeMap, BTreeSet};
    use write_fonts::from_obj::ToOwnedTable;
    let mut source = Source::new(bytes)?;
    let Some(store) = source.font.var_store() else {
        return Ok(bytes.to_vec());
    };
    let mut used = BTreeSet::new();
    let mut commands = Vec::new();
    for gid in 0..source.font.num_glyphs() as usize {
        let fd = source.fd(write_fonts::types::GlyphId::new(gid as u32))?;
        let mut recorder = Recorder::default();
        let mut interp =
            Interpreter::<Cff2, _, _>::new(&source, &mut recorder, fd, source.fds[fd].ivs, false);
        interp.run(
            Program::Glyph(gid),
            source.font.charstrings().get(gid).ok_or(Error)?,
        )?;
        used.extend(
            recorder
                .commands
                .iter()
                .flat_map(|c| &c.args)
                .filter_map(|v| v.blend.as_ref().map(|b| b.0)),
        );
        commands.push((fd, recorder.commands));
    }
    for fd in &source.fds {
        let mut ivs = 0;
        for e in &fd.private {
            if e.op == 22 {
                ivs = dict::uint(*e.args.first().ok_or(Error)?)?;
            }
            let mut p = 0;
            while p < e.raw.len() {
                if encoding::number(&e.raw, &mut p, true)?.is_none()
                    && encoding::op(&e.raw, &mut p)? == 23
                {
                    used.insert(ivs);
                }
            }
        }
    }
    let remap: BTreeMap<_, _> = used
        .iter()
        .enumerate()
        .map(|(new, &old)| (old, new))
        .collect();
    let mut table: write_fonts::tables::variations::ItemVariationStore = store.to_owned_table();
    if used.is_empty() {
        source.top.retain(|e| e.op != 24);
        source.instanced_store = None;
    } else {
        table.item_variation_data = used
            .iter()
            .map(|&i| table.item_variation_data.get(i).cloned().ok_or(Error))
            .collect::<Result<Vec<_>>>()?;
        let indices: BTreeSet<_> = table
            .item_variation_data
            .iter()
            .flat_map(|d| {
                d.as_ref()
                    .into_iter()
                    .flat_map(|d| d.region_indexes.iter().copied())
            })
            .collect();
        let regions: BTreeMap<_, _> = indices
            .iter()
            .enumerate()
            .map(|(new, &old)| (old, new as u16))
            .collect();
        table.variation_region_list.variation_regions = indices
            .iter()
            .map(|&i| {
                table
                    .variation_region_list
                    .variation_regions
                    .get(i as usize)
                    .cloned()
                    .ok_or(Error)
            })
            .collect::<Result<Vec<_>>>()?;
        for d in &mut table.item_variation_data {
            if let Some(d) = d.as_mut() {
                for r in &mut d.region_indexes {
                    *r = *regions.get(r).ok_or(Error)?;
                }
            }
        }
        let data = write_fonts::dump_table(&table).map_err(|_| Error)?;
        let mut bytes = u16::try_from(data.len())
            .map_err(|_| Error)?
            .to_be_bytes()
            .to_vec();
        bytes.extend(data);
        source.instanced_store = Some(bytes);
    }
    for fd in &mut source.fds {
        fd.ivs = remap.get(&fd.ivs).copied().unwrap_or(0);
        let mut private = Vec::new();
        for entry in &fd.private {
            if entry.op == 22 {
                let old = dict::uint(*entry.args.first().ok_or(Error)?)?;
                if let Some(&new) = remap.get(&old) {
                    let mut e = entry.clone();
                    e.args = vec![new as f64];
                    e.raw = dict::replacement(22, &[new])?;
                    private.push(e);
                }
            } else {
                private.push(entry.clone());
            }
        }
        fd.private = private;
    }
    let mut chars = Vec::new();
    for (fd, mut commands) in commands {
        for command in &mut commands {
            for value in &mut command.args {
                if let Some((ivs, _)) = &mut value.blend {
                    *ivs = *remap.get(ivs).ok_or(Error)?;
                }
            }
        }
        chars.push(charstring::flatten(
            &commands,
            None,
            true,
            source.fds[fd].ivs,
        )?);
    }
    let locals = vec![vec![]; source.fds.len()];
    subset::assemble::<Cff2>(
        &source,
        plan,
        Programs {
            chars,
            globals: vec![],
            locals,
        },
    )
}
