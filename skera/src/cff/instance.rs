//! Resolve CFF2 charstring and Private DICT blends through the same region scalars.
use super::{
    blend::{Rebaser, Rounding},
    charstring::{self, CharStringActions, Flattener, Interpreter, Program, Value},
    dict, encoding,
    source::{Cff2, CharStringSource, Source},
    subset::{self, Programs},
    Error, Result,
};
use crate::{
    instance::{AxisPlan, StorePlan},
    Plan, SubsetFlags,
};
use read_fonts::{tables::variations::ItemVariationStore, FontRef};
use std::collections::BTreeSet;

pub(super) fn gains(
    store: &ItemVariationStore,
    ivs: usize,
    coords: &[read_fonts::types::F2Dot14],
) -> Result<Vec<f64>> {
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
                .compute_scalar_f32(coords) as f64)
        })
        .collect()
}
#[derive(Default)]
struct ComplexBlends(BTreeSet<usize>);
impl CharStringActions<'_> for ComplexBlends {
    fn command(&mut self, _op: u16, args: &[Value], _mask: &[u8]) {
        for value in args.iter().filter(|v| v.nested()) {
            self.0.insert(value.blend.as_ref().unwrap().ivs);
        }
    }
}

fn private_values(entry: &dict::Entry, source: &Source, ivs: usize) -> Result<Vec<Value>> {
    let mut position = 0;
    let mut stack: Vec<Value> = Vec::new();
    while position < entry.raw.len() {
        if let Some(number) = encoding::number(&entry.raw, &mut position, true)? {
            if stack.len() >= 513 {
                return Err(Error);
            }
            stack.push(Value::plain(number));
            continue;
        }
        let op = encoding::op(&entry.raw, &mut position)?;
        if op != 23 {
            if op != entry.op || position != entry.raw.len() {
                return Err(Error);
            }
            return Ok(stack);
        }
        let count = usize::try_from(stack.pop().ok_or(Error)?.int()?).map_err(|_| Error)?;
        let regions = source.region_count(ivs)?;
        let start = stack
            .len()
            .checked_sub(count.checked_mul(regions + 1).ok_or(Error)?)
            .ok_or(Error)?;
        let values = stack.split_off(start);
        for i in 0..count {
            stack.push(Value::blended(
                &values[i],
                &values[count + i * regions..count + (i + 1) * regions],
                ivs,
            )?);
        }
    }
    Err(Error)
}

fn complex_blends(source: &Source, plan: &Plan) -> Result<BTreeSet<usize>> {
    let mut collector = ComplexBlends::default();
    let mut stack = charstring::operand_stack::<Cff2>();
    for &(_, old) in &plan.new_to_old_gid_list {
        let gid = old.to_u32() as usize;
        let fd = source.fd(read_fonts::types::GlyphId::new(gid as u32))?;
        let mut interpreter =
            Interpreter::<Cff2, _, _>::new(source, &mut collector, fd, false, &mut stack);
        interpreter.run(
            Program::Glyph(gid as u32),
            source.font.charstrings().get(gid).ok_or(Error)?,
        )?;
    }
    for fd in &source.fds {
        let mut ivs = 0;
        for entry in &fd.private {
            if entry.op == 22 {
                ivs = dict::uint(*entry.args.first().ok_or(Error)?)?;
            } else if entry.op != 19 {
                for value in private_values(entry, source, ivs)?
                    .iter()
                    .filter(|v| v.nested())
                {
                    collector.0.insert(value.blend.as_ref().unwrap().ivs);
                }
            }
        }
    }
    Ok(collector.0)
}

pub(super) fn instance_subset(
    _font: &FontRef,
    axes: &AxisPlan,
    table: &read_fonts::tables::cff2::Cff2,
    plan: &Plan,
) -> Result<Vec<u8>> {
    let mut source = Source::new(table.offset_data().as_bytes())?;
    let partial = !axes.all_pinned();
    let mut store_plan = if partial {
        source
            .font
            .var_store()
            .map(|s| StorePlan::new(s, axes))
            .transpose()
            .map_err(|_| Error)?
    } else {
        None
    };
    let constants = if let Some(store_plan) = &mut store_plan {
        store_plan
            .add_constant_region(&complex_blends(&source, plan)?)
            .map_err(|_| Error)?
    } else {
        Default::default()
    };
    let mut transforms = Vec::new();
    if let Some(store) = source.font.var_store() {
        for i in 0..store.item_variation_data_count() as usize {
            transforms.push(gains(store, i, &axes.coords)?);
        }
    }
    let mut chars = vec![vec![]; plan.num_output_glyphs];
    let mut stack = charstring::operand_stack::<Cff2>();
    for &(new, old) in &plan.new_to_old_gid_list {
        if old == read_fonts::types::GlyphId::NOTDEF
            && !plan
                .subset_flags
                .contains(SubsetFlags::SUBSET_FLAGS_NOTDEF_OUTLINE)
        {
            continue;
        }
        let gid = old.to_u32() as usize;
        let fd = source.fd(read_fonts::types::GlyphId::new(gid as u32))?;
        let mut rec = Flattener::default();
        let mut interp = Interpreter::<Cff2, _, _>::new(
            &source,
            &mut rec,
            fd,
            plan.subset_flags
                .contains(SubsetFlags::SUBSET_FLAGS_NO_HINTING),
            &mut stack,
        );
        interp.run(
            Program::Glyph(gid as u32),
            source.font.charstrings().get(gid).ok_or(Error)?,
        )?;
        let mut rebaser = store_plan
            .as_ref()
            .map(|s| Rebaser::new(s, &transforms, &constants));
        let mut remaining = 200_000;
        for command in &mut rec.commands {
            for value in &mut command.args {
                if partial {
                    if let Some(rebaser) = &mut rebaser {
                        *value = rebaser.value(value, &mut Rounding::default())?;
                    }
                } else {
                    value.default = value.resolve(transforms.as_slice(), &mut remaining)?;
                    value.blend = None;
                }
            }
        }
        chars[new.to_u32() as usize] = charstring::flatten(
            &rec.commands,
            None,
            true,
            if partial { source.fds[fd].ivs } else { 0 },
        )?;
    }
    // Parse before mutating source so DICT expressions use the original store.
    let mut privates = Vec::new();
    for fd in &source.fds {
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
            let mut values = private_values(entry, &source, ivs)?;
            let mut rounding = Rounding::private_dict();
            let mut rebaser = store_plan
                .as_ref()
                .map(|s| Rebaser::new(s, &transforms, &constants));
            let mut remaining = 200_000;
            for value in &mut values {
                if partial {
                    if let Some(rebaser) = &mut rebaser {
                        *value = rebaser.value(value, &mut rounding)?;
                    }
                } else {
                    value.default = value.resolve(transforms.as_slice(), &mut remaining)?;
                    value.blend = None;
                }
            }
            let mut raw = Vec::new();
            let mut occupied = 0;
            let mut remaining = 200_000;
            for value in &values {
                charstring::emit_value(
                    value,
                    &mut raw,
                    true,
                    Some(ivs),
                    &mut occupied,
                    513,
                    &mut remaining,
                )?;
            }
            encoding::emit_op(&mut raw, entry.op);
            let mut entry = entry.clone();
            entry.raw = raw.into();
            entries.push(entry);
        }
        privates.push(entries);
    }
    for (fd, private) in source.fds.iter_mut().zip(privates) {
        fd.private = private;
        if !partial {
            fd.ivs = 0;
        }
    }
    if partial {
        if let (Some(plan), Some(store)) = (&store_plan, source.font.var_store()) {
            let mut table = plan.rebuild(store).map_err(|_| Error)?;
            // CFF2 stores carry region indexes only; blend deltas live in
            // charstrings/DICTs, so both itemCount and wordDeltaCount are zero.
            for data in &mut table.item_variation_data {
                if let Some(data) = data.as_mut() {
                    data.word_delta_count = 0;
                }
            }
            let bytes = crate::instance::serialize::item_store(&table).map_err(|_| Error)?;
            // OFF 2026 5.3.3.11.2: larger stores use length=65535.
            let mut data = u16::try_from(bytes.len())
                .unwrap_or(u16::MAX)
                .to_be_bytes()
                .to_vec();
            data.extend(bytes);
            source.instanced_store = Some(data);
        }
    } else {
        source.top.retain(|entry| entry.op != 24);
    }
    let locals = vec![vec![]; source.fds.len()];
    chars.resize(plan.num_output_glyphs, vec![]);
    let bytes = subset::assemble::<Cff2>(
        &source,
        plan,
        Programs {
            chars,
            globals: vec![],
            locals,
        },
    )?;
    if partial {
        let compact_plan = crate::Plan::identity_glyphs(plan.num_output_glyphs, plan.subset_flags);
        compact(&bytes, &compact_plan)
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
    let mut stack = charstring::operand_stack::<Cff2>();
    for gid in 0..source.font.num_glyphs() as usize {
        let fd = source.fd(read_fonts::types::GlyphId::new(gid as u32))?;
        let mut recorder = Flattener::default();
        let mut interp =
            Interpreter::<Cff2, _, _>::new(&source, &mut recorder, fd, false, &mut stack);
        interp.run(
            Program::Glyph(gid as u32),
            source.font.charstrings().get(gid).ok_or(Error)?,
        )?;
        used.extend(
            recorder
                .commands
                .iter()
                .flat_map(|c| &c.args)
                .filter_map(|v| v.blend.as_ref().map(|b| b.ivs)),
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
        let data = crate::instance::serialize::item_store(&table).map_err(|_| Error)?;
        // The length field saturates; all store bytes must still be written.
        let mut bytes = u16::try_from(data.len())
            .unwrap_or(u16::MAX)
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
                    e.raw = dict::replacement(22, &[new])?.into();
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
                value.remap_ivs(&remap)?;
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
