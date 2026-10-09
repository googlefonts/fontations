//! Resolve CFF2 charstring and Private DICT blends through the same region scalars.
use super::{
    blend::{Rebaser, Rounding},
    charstring::{self, CharStringActions, Command, Interpreter, Program, Recorder, Value},
    dict, encoding,
    source::{Cff2, CharStringSource, Source},
    subset::{self, Programs},
    Error, Result,
};
use crate::{
    instance::{AxisPlan, StorePlan},
    Plan, SubsetFlags,
};
use std::collections::BTreeSet;
use write_fonts::read::{
    collections::IntSet,
    tables::variations::{ItemVariationStore, VariationRegion},
    types::{F2Dot14, NameId},
    FontRef,
};

// HarfBuzz evaluates CFF2 blend scalars in float, then accumulates their
// products with operands in double. A 16.16 intermediate can move half-unit
// operands across the rounding boundary and shift the rest of a contour.
fn region_gain(region: &VariationRegion, coords: &[F2Dot14]) -> f64 {
    let mut gain = 1f32;
    for (i, axis) in region.region_axes().iter().enumerate() {
        let (start, peak, end) = (
            axis.start_coord().to_bits() as i32,
            axis.peak_coord().to_bits() as i32,
            axis.end_coord().to_bits() as i32,
        );
        let coord = coords.get(i).copied().unwrap_or_default().to_bits() as i32;
        if peak == 0 || coord == peak || start > peak || peak > end || start < 0 && end > 0 {
            continue;
        }
        if coord <= start || coord >= end {
            return 0.;
        }
        gain *= if coord < peak {
            (coord - start) as f32 / (peak - start) as f32
        } else {
            (end - coord) as f32 / (end - peak) as f32
        };
    }
    gain as f64
}

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
            Ok(region_gain(
                &regions.get(idx.get() as usize).map_err(|_| Error)?,
                &axes.coords,
            ))
        })
        .collect()
}
#[derive(Default)]
struct ComplexBlends(BTreeSet<usize>);
impl CharStringActions for ComplexBlends {
    fn token(&mut self, _: Program, _: usize, _: &[u8]) {}
    fn call(&mut self, _: Program, _: usize, _: Program, _: &Value) -> Result<()> {
        Ok(())
    }
    fn discard(&mut self, _: impl Iterator<Item = (Program, usize)>) {}
    fn command(&mut self, command: Command) {
        for value in command.args.iter().filter(|v| v.nested()) {
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

fn complex_blends(source: &Source) -> Result<BTreeSet<usize>> {
    let mut collector = ComplexBlends::default();
    for gid in 0..source.font.num_glyphs() as usize {
        let fd = source.fd(write_fonts::types::GlyphId::new(gid as u32))?;
        let mut interpreter =
            Interpreter::<Cff2, _, _>::new(source, &mut collector, fd, source.fds[fd].ivs, false);
        interpreter.run(
            Program::Glyph(gid),
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

pub(super) fn instance(font: &FontRef, axes: &AxisPlan) -> Result<Vec<u8>> {
    use write_fonts::read::TableProvider;
    let data = font
        .data_for_tag(write_fonts::types::Tag::new(b"CFF2"))
        .ok_or(Error)?;
    let mut source = Source::new(data.as_bytes())?;
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
    let constants = if let Some(plan) = &mut store_plan {
        plan.add_constant_region(&complex_blends(&source)?)
            .map_err(|_| Error)?
    } else {
        Default::default()
    };
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
        chars.push(charstring::flatten(
            &rec.commands,
            None,
            true,
            if partial { source.fds[fd].ivs } else { 0 },
        )?);
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
            entry.raw = raw;
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

#[cfg(test)]
mod tests {
    use super::*;
    use write_fonts::{
        read::{FontData, FontRead},
        tables::variations::*,
    };

    #[test]
    fn nested_private_dict_blends_use_the_shared_rebaser() {
        let font = FontRef::new(include_bytes!(
            "../../test-data/fonts/cff2-nested-blends.otf"
        ))
        .unwrap();
        use write_fonts::read::TableProvider;
        let mut source = Source::new(font.cff2().unwrap().offset_data().as_bytes()).unwrap();
        let mut raw = Vec::new();
        for number in [100., 0., 10., 20., 0., 1.] {
            encoding::encode(&mut raw, number, true).unwrap();
        }
        raw.push(23);
        raw.extend([140, 23, 10]);
        let entry = dict::parse(&raw).unwrap().remove(0);
        let values = private_values(&entry, &source, 0).unwrap();
        assert_eq!(values.len(), 1);
        assert!(values[0].nested());
        assert_eq!(
            values[0]
                .resolve(&[vec![0.6, 0.5]][..], &mut 200_000)
                .unwrap(),
            111.
        );

        let axes = AxisPlan::new(&font, &crate::parse_axis_limits("wdth=0.5").unwrap()).unwrap();
        let store = source.font.var_store().unwrap();
        let mut plan = StorePlan::new(store, &axes).unwrap();
        let constants = plan.add_constant_region(&[0].into()).unwrap();
        let gains = vec![gains(store, 0, &axes).unwrap()];
        let result = Rebaser::new(&plan, &gains, &constants)
            .value(&values[0], &mut Rounding::private_dict())
            .unwrap();
        // The retained wght region and the constant region are the two columns.
        assert_eq!(
            result.resolve(&[vec![0.25, 1.]][..], &mut 200_000).unwrap(),
            108.
        );
        let mut encoded = Vec::new();
        charstring::emit_value(
            &result,
            &mut encoded,
            true,
            Some(0),
            &mut 0,
            513,
            &mut 200_000,
        )
        .unwrap();
        encoded.push(10);
        let reread = private_values(&dict::parse(&encoded).unwrap().remove(0), &source, 0).unwrap();
        assert_eq!(
            reread[0]
                .resolve(&[vec![0.25, 1.]][..], &mut 200_000)
                .unwrap(),
            108.
        );

        // Exercise the real Private DICT assembly and VarData compaction, then
        // instance the remaining axis through the public font API.
        source.fds[0].private = vec![entry];
        let plan = Plan::new(
            &IntSet::all(),
            &IntSet::empty(),
            &font,
            SubsetFlags::SUBSET_FLAGS_NOTDEF_OUTLINE,
            &IntSet::empty(),
            &IntSet::all(),
            &IntSet::all(),
            &IntSet::<NameId>::all(),
            &IntSet::all(),
        );
        let cff = subset::assemble::<Cff2>(
            &source,
            &plan,
            Programs {
                chars: (0..source.font.num_glyphs() as usize)
                    .map(|gid| source.font.charstrings().get(gid).unwrap().to_vec())
                    .collect(),
                globals: vec![],
                locals: vec![vec![]; source.fds.len()],
            },
        )
        .unwrap();
        let mut builder = write_fonts::FontBuilder::new();
        builder.add_raw(write_fonts::types::Tag::new(b"CFF2"), cff);
        let input = builder.copy_missing_tables(font.clone()).build();
        let input = FontRef::new(&input).unwrap();
        let partial =
            crate::instance_font(&input, &crate::parse_axis_limits("wdth=0.5").unwrap()).unwrap();
        let partial = FontRef::new(&partial).unwrap();
        let full = crate::instance_font(&partial, &crate::parse_axis_limits("wght=0.25").unwrap())
            .unwrap();
        let full = FontRef::new(&full).unwrap();
        let output = Source::new(full.cff2().unwrap().offset_data().as_bytes()).unwrap();
        let private = output.fds[0].private.iter().find(|e| e.op == 10).unwrap();
        assert_eq!(private.args, [108.]);
    }

    #[test]
    fn cff_scalars_keep_half_unit_operands_on_the_correct_side() {
        let bytes = write_fonts::dump_table(&VariationRegionList::new(
            1,
            vec![write_fonts::tables::variations::VariationRegion::new(vec![
                RegionAxisCoordinates::new(F2Dot14::ZERO, F2Dot14::from_f64(0.75), F2Dot14::ONE),
            ])],
        ))
        .unwrap();
        let list =
            write_fonts::read::tables::variations::VariationRegionList::read(FontData::new(&bytes))
                .unwrap();
        let region = list.variation_regions().get(0).unwrap();
        let gain = region_gain(&region, &[F2Dot14::from_f64(0.25)]);
        let fold = |default, deltas: &[f64], gains: &[f64]| {
            Value::blended(
                &Value::plain(default),
                &deltas.iter().copied().map(Value::plain).collect::<Vec<_>>(),
                0,
            )
            .unwrap()
            .resolve(&[gains.to_vec()][..], &mut 200_000)
            .unwrap()
        };
        assert_eq!(fold(0., &[1.5], &[gain]), 1.);
        assert_eq!(fold(0., &[-1.5], &[gain]), -1.);
        assert_eq!(fold(0.25, &[0.5], &[gain]), 0.);
        assert_eq!(fold(0.25, &[1.], &[gain]), 1.);
    }
}
