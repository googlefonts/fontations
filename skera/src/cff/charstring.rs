//! A Type 2 interpreter parameterized by its source, format, and actions.
//!
//! Operand provenance lets hint removal follow values across subroutine calls.
//! The flattened command stream retains operators and symbolic CFF2 blends.

use super::{
    encoding,
    source::{CffFlavor, CharStringSource},
    Error, Result,
};
use std::collections::{BTreeMap, BTreeSet};
use std::marker::PhantomData;
use std::rc::Rc;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub(super) enum Program {
    Glyph(usize),
    Local(usize, usize),
    Global(usize, usize),
}
type Origin = (Program, usize);

#[derive(Clone, Default)]
pub(super) struct Value {
    pub default: f64,
    pub blend: Option<Rc<Blend>>,
    origins: BTreeSet<Origin>,
}
/// Origin-free expression nodes preserve higher-order blends without cloning
/// their trees when values cross subroutines. Construction bounds nesting.
#[derive(Clone)]
pub(super) struct Blend {
    pub ivs: usize,
    pub base: Value,
    pub deltas: Vec<Value>,
    depth: usize,
    expanded_size: usize,
}

pub(super) trait BlendScalars {
    fn scalars(&self, ivs: usize) -> Result<&[f64]>;
}
impl BlendScalars for [Vec<f64>] {
    fn scalars(&self, ivs: usize) -> Result<&[f64]> {
        self.get(ivs).map(Vec::as_slice).ok_or(Error)
    }
}

impl Value {
    pub fn plain(default: f64) -> Self {
        Self {
            default,
            ..Self::default()
        }
    }
    fn number(v: f64, origin: Origin) -> Self {
        Self {
            default: v,
            blend: None,
            origins: [origin].into(),
        }
    }
    pub fn expression(&self) -> Self {
        Self {
            default: self.default,
            blend: self.blend.clone(),
            origins: BTreeSet::new(),
        }
    }
    pub fn blended(base: &Self, deltas: &[Self], ivs: usize) -> Result<Self> {
        let depth = std::iter::once(base)
            .chain(deltas)
            .filter_map(|v| v.blend.as_ref().map(|b| b.depth))
            .max()
            .unwrap_or(0)
            + 1;
        if depth > 64 {
            return Err(Error);
        }
        // Selectors can share a subtree. Bound its expanded size as well as
        // depth so evaluation and serialization cannot grow exponentially.
        let expanded_size = std::iter::once(base)
            .chain(deltas)
            .try_fold(1usize, |size, v| {
                let size = size.checked_add(v.blend.as_ref().map_or(1, |b| b.expanded_size))?;
                (size <= 200_000).then_some(size)
            })
            .ok_or(Error)?;
        Ok(Self {
            default: base.default,
            blend: Some(Rc::new(Blend {
                ivs,
                base: base.expression(),
                deltas: deltas.iter().map(Self::expression).collect(),
                depth,
                expanded_size,
            })),
            origins: BTreeSet::new(),
        })
    }
    pub fn nested(&self) -> bool {
        self.blend
            .as_ref()
            .is_some_and(|b| b.base.blend.is_some() || b.deltas.iter().any(|v| v.blend.is_some()))
    }
    pub fn resolve<S: BlendScalars + ?Sized>(
        &self,
        scalars: &S,
        remaining: &mut usize,
    ) -> Result<f64> {
        *remaining = remaining.checked_sub(1).ok_or(Error)?;
        let Some(blend) = &self.blend else {
            return Ok(self.default);
        };
        let gains = scalars.scalars(blend.ivs)?;
        if gains.len() != blend.deltas.len() {
            return Err(Error);
        }
        let mut delta_sum = 0.;
        for (delta, gain) in blend.deltas.iter().zip(gains) {
            delta_sum += delta.resolve(scalars, remaining)? * gain;
        }
        Ok((blend.base.resolve(scalars, remaining)? + delta_sum).round())
    }
    pub fn remap_ivs(&mut self, map: &BTreeMap<usize, usize>) -> Result<()> {
        if let Some(blend) = &mut self.blend {
            let blend = Rc::make_mut(blend);
            blend.ivs = *map.get(&blend.ivs).ok_or(Error)?;
            blend.base.remap_ivs(map)?;
            for delta in &mut blend.deltas {
                delta.remap_ivs(map)?;
            }
        }
        Ok(())
    }
    pub fn int(&self) -> Result<i32> {
        if self.blend.is_some()
            || self.default.fract() != 0.
            || self.default < i32::MIN as f64
            || self.default > i32::MAX as f64
        {
            return Err(Error);
        }
        Ok(self.default as i32)
    }
}

#[derive(Clone)]
pub(super) struct Command {
    pub op: u16,
    pub args: Vec<Value>,
    pub mask: Vec<u8>,
}

pub(super) trait CharStringActions {
    fn token(&mut self, program: Program, start: usize, bytes: &[u8]);
    fn call(
        &mut self,
        program: Program,
        start: usize,
        target: Program,
        operand: &Value,
    ) -> Result<()>;
    fn discard(&mut self, origins: impl Iterator<Item = Origin>);
    fn command(&mut self, command: Command);
}

#[derive(Default, PartialEq, Eq)]
pub(super) struct Piece {
    original: Vec<u8>,
    bytes: Vec<u8>,
    removed: bool,
    call: Option<Program>,
}
#[derive(Default)]
pub(super) struct Recorder {
    programs: BTreeMap<Program, BTreeMap<usize, Piece>>,
    pub commands: Vec<Command>,
    drawing_origins: BTreeSet<Origin>,
    pub needs_flattening: bool,
}
impl CharStringActions for Recorder {
    fn token(&mut self, program: Program, start: usize, bytes: &[u8]) {
        let piece = self
            .programs
            .entry(program)
            .or_default()
            .entry(start)
            .or_insert_with(|| Piece {
                original: bytes.to_vec(),
                bytes: bytes.to_vec(),
                ..Default::default()
            });
        self.needs_flattening |= piece.original != bytes;
    }
    fn call(
        &mut self,
        program: Program,
        start: usize,
        target: Program,
        operand: &Value,
    ) -> Result<()> {
        let pieces = self.programs.get_mut(&program).ok_or(Error)?;
        // The common immediate-operand case needs only a replacement number.
        let operand_origin = operand.origins.first().copied().ok_or(Error)?;
        let immediate = operand.origins.len() == 1
            && operand_origin.0 == program
            && pieces
                .get(&operand_origin.1)
                .is_some_and(|p| operand_origin.1 + p.original.len() == start);
        if immediate {
            pieces.get_mut(&operand_origin.1).ok_or(Error)?.removed = true;
        }
        let piece = pieces.get_mut(&start).ok_or(Error)?;
        let prefix = if immediate { vec![] } else { vec![12, 18] };
        self.needs_flattening |= piece
            .call
            .is_some_and(|old| old != target || piece.bytes != prefix);
        piece.call = Some(target);
        // Otherwise retain the operand computation and consume its result.
        piece.bytes = prefix;
        Ok(())
    }
    fn discard(&mut self, origins: impl Iterator<Item = Origin>) {
        for (key, offset) in origins {
            self.needs_flattening |= self.drawing_origins.contains(&(key, offset));
            if let Some(piece) = self.programs.get_mut(&key).and_then(|p| p.get_mut(&offset)) {
                piece.removed = true;
            }
        }
    }
    fn command(&mut self, command: Command) {
        for origin in command.args.iter().flat_map(|v| &v.origins) {
            self.needs_flattening |= self
                .programs
                .get(&origin.0)
                .and_then(|p| p.get(&origin.1))
                .is_some_and(|p| p.removed);
            self.drawing_origins.insert(*origin);
        }
        self.commands.push(command);
    }
}

impl Recorder {
    pub fn merge(&mut self, other: Self) {
        self.needs_flattening |= other.needs_flattening;
        for (key, pieces) in other.programs {
            if let Some(old) = self.programs.get(&key) {
                self.needs_flattening |= *old != pieces;
            } else {
                self.programs.insert(key, pieces);
            }
        }
    }
    fn only_hints(&self, key: Program, visiting: &mut BTreeSet<Program>) -> bool {
        if !visiting.insert(key) {
            return false;
        }
        let result = self.programs.get(&key).is_some_and(|pieces| {
            pieces.values().all(|p| {
                p.removed || p.bytes == [11] || p.call.is_some_and(|t| self.only_hints(t, visiting))
            })
        });
        visiting.remove(&key);
        result
    }
    pub fn remove_hint_calls(&mut self) {
        let dead: BTreeSet<_> = self
            .programs
            .keys()
            .copied()
            .filter(|&p| {
                !matches!(p, Program::Glyph(_)) && self.only_hints(p, &mut BTreeSet::new())
            })
            .collect();
        for pieces in self.programs.values_mut() {
            for piece in pieces.values_mut() {
                if piece.call.is_some_and(|p| dead.contains(&p)) {
                    // A computed call operand must still be consumed.
                    piece.call = None;
                    piece.removed = piece.bytes.is_empty();
                }
            }
        }
    }
    pub fn closure(&self, roots: &[Program]) -> Result<BTreeSet<Program>> {
        let mut used = BTreeSet::new();
        let mut pending = roots.to_vec();
        while let Some(key) = pending.pop() {
            if !used.insert(key) {
                continue;
            }
            for p in self.programs.get(&key).ok_or(Error)?.values() {
                if !p.removed {
                    if let Some(target) = p.call {
                        pending.push(target);
                    }
                }
            }
        }
        Ok(used)
    }
    pub fn encode(
        &self,
        key: Program,
        remap: &BTreeMap<Program, usize>,
        global_count: usize,
        local_counts: &[usize],
    ) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        for p in self.programs.get(&key).ok_or(Error)?.values() {
            if p.removed {
                continue;
            }
            out.extend(&p.bytes);
            if let Some(target) = p.call {
                let (op, count) = match target {
                    Program::Local(fd, _) => (10, local_counts[fd]),
                    Program::Global(_, _) => (29, global_count),
                    _ => return Err(Error),
                };
                encoding::integer(
                    &mut out,
                    i32::try_from(*remap.get(&target).ok_or(Error)?).map_err(|_| Error)?
                        - encoding::bias(count),
                );
                out.push(op);
            }
        }
        Ok(out)
    }
}

pub(super) struct Interpreter<'a, F, S, A> {
    source: &'a S,
    actions: &'a mut A,
    fd: usize,
    stack: Vec<Value>,
    transient: Vec<Value>,
    hints: usize,
    ivs: usize,
    seen_ivs: bool,
    seen_blend: bool,
    width_read: bool,
    pub width: Option<f64>,
    pub seac: Option<(u8, u8)>,
    remaining: usize,
    drop_hints: bool,
    flavor: PhantomData<F>,
}
impl<'a, F: CffFlavor, S: CharStringSource, A: CharStringActions> Interpreter<'a, F, S, A> {
    pub fn new(source: &'a S, actions: &'a mut A, fd: usize, ivs: usize, drop_hints: bool) -> Self {
        Self {
            source,
            actions,
            fd,
            stack: vec![],
            transient: vec![Value::default(); 32],
            hints: 0,
            ivs,
            seen_ivs: false,
            seen_blend: false,
            width_read: F::CFF2,
            width: None,
            seac: None,
            remaining: 200_000,
            drop_hints,
            flavor: PhantomData,
        }
    }
    fn pop(&mut self) -> Result<Value> {
        self.stack.pop().ok_or(Error)
    }
    fn push(&mut self, v: Value) -> Result<()> {
        if self.stack.len() >= F::STACK_LIMIT || !v.default.is_finite() {
            return Err(Error);
        }
        self.stack.push(v);
        Ok(())
    }
    fn width(&mut self, op: u16) -> Result<()> {
        if self.width_read {
            return Ok(());
        }
        let have = match op {
            1 | 3 | 18 | 19 | 20 | 23 | 14 => self.stack.len() % 2 == 1,
            4 | 22 => self.stack.len() > 1,
            21 => self.stack.len() > 2,
            _ => return Ok(()),
        };
        self.width_read = true;
        if have {
            let value = self.stack.remove(0);
            self.width = Some(value.default);
            // Flattening emits the width separately, while recording keeps its original location.
            if self.drop_hints {
                self.actions.discard(value.origins.iter().copied());
            }
        }
        Ok(())
    }
    pub fn run(&mut self, key: Program, data: &[u8]) -> Result<()> {
        self.execute(key, data, 0).map(|_| ())
    }
    fn execute(&mut self, key: Program, data: &[u8], depth: usize) -> Result<bool> {
        if depth > 10 {
            return Err(Error);
        }
        let mut p = 0;
        // Empty CFF2 programs are valid, including retained-gid holes.
        if data.is_empty() {
            self.actions.token(key, 0, &[]);
        }
        while p < data.len() {
            if self.remaining == 0 {
                return Err(Error);
            }
            self.remaining -= 1;
            let start = p;
            if let Some(value) = encoding::number(data, &mut p, false)? {
                self.actions.token(key, start, &data[start..p]);
                self.push(Value::number(value, (key, start)))?;
                continue;
            }
            let op = encoding::op(data, &mut p)?;
            let mut mask = Vec::new();
            self.width(op)?;
            if matches!(op, 1 | 3 | 18 | 23 | 19 | 20) {
                if self.stack.len() % 2 != 0 {
                    return Err(Error);
                }
                self.hints = self.hints.checked_add(self.stack.len() / 2).ok_or(Error)?;
                if self.hints > 96 {
                    return Err(Error);
                }
                if matches!(op, 19 | 20) {
                    let end = p.checked_add(self.hints.div_ceil(8)).ok_or(Error)?;
                    mask.extend(data.get(p..end).ok_or(Error)?);
                    p = end;
                }
            }
            self.actions.token(key, start, &data[start..p]);
            match op {
                10 | 29 => {
                    let value = self.pop()?;
                    let global = op == 29;
                    let index = value
                        .int()?
                        .checked_add(encoding::bias(self.source.subr_count(global, self.fd)))
                        .ok_or(Error)?;
                    let index = usize::try_from(index).map_err(|_| Error)?;
                    let target = if global {
                        Program::Global(self.fd, index)
                    } else {
                        Program::Local(self.fd, index)
                    };
                    self.actions.call(key, start, target, &value)?;
                    if self.execute(
                        target,
                        self.source.program(global, self.fd, index)?,
                        depth + 1,
                    )? {
                        return Ok(true);
                    }
                }
                11 => {
                    if depth == 0 && !F::CFF2 {
                        return Err(Error);
                    }
                    return Ok(false);
                }
                15 if F::CFF2 => {
                    if self.seen_ivs || self.seen_blend {
                        return Err(Error);
                    }
                    self.ivs = usize::try_from(self.pop()?.int()?).map_err(|_| Error)?;
                    self.source.region_count(self.ivs)?;
                    self.seen_ivs = true;
                }
                16 if F::CFF2 => {
                    self.seen_blend = true;
                    let count_value = self.pop()?;
                    let n = usize::try_from(count_value.int()?).map_err(|_| Error)?;
                    let k = self.source.region_count(self.ivs)?;
                    let count = n.checked_mul(k + 1).ok_or(Error)?;
                    let group_start = self.stack.len().checked_sub(count).ok_or(Error)?;
                    let values = self.stack.split_off(group_start);
                    for i in 0..n {
                        let deltas = &values[n + i * k..n + (i + 1) * k];
                        let mut value = Value::blended(&values[i], deltas, self.ivs)?;
                        value.origins.extend(&values[i].origins);
                        for delta in deltas {
                            value.origins.extend(&delta.origins);
                        }
                        value.origins.extend(&count_value.origins);
                        value.origins.insert((key, start));
                        self.push(value)?;
                    }
                }
                0x103..=0x105
                | 0x109..=0x10c
                | 0x10e..=0x10f
                | 0x112
                | 0x114..=0x118
                | 0x11a..=0x11e => self.arithmetic(op, (key, start))?,
                1
                | 3
                | 4
                | 5
                | 6
                | 7
                | 8
                | 14
                | 18
                | 19
                | 20
                | 21
                | 22
                | 23
                | 24
                | 25
                | 26
                | 27
                | 30
                | 31
                | 0x100
                | 0x122..=0x125 => {
                    if op == 14 && !F::CFF2 && self.stack.len() == 4 {
                        self.seac = Some((
                            u8::try_from(self.stack[2].int()?).map_err(|_| Error)?,
                            u8::try_from(self.stack[3].int()?).map_err(|_| Error)?,
                        ));
                    }
                    let hint = matches!(op, 1 | 3 | 18 | 19 | 20 | 23 | 0x100);
                    if hint && self.drop_hints {
                        self.actions.discard([(key, start)].into_iter());
                        for v in &self.stack {
                            self.actions.discard(v.origins.iter().copied());
                        }
                    } else if !(F::CFF2 && op == 14) {
                        self.actions.command(Command {
                            op,
                            args: self.stack.clone(),
                            mask,
                        });
                    }
                    self.stack.clear();
                    if op == 14 {
                        return Ok(true);
                    }
                }
                // Reserved operators are ignored and clear the stack, as by other consumers.
                _ => self.stack.clear(),
            }
        }
        if !F::CFF2 {
            return Err(Error);
        }
        if depth == 0 && !self.stack.is_empty() {
            return Err(Error);
        }
        Ok(false)
    }
    fn arithmetic(&mut self, op: u16, origin: Origin) -> Result<()> {
        let mut a = self.pop()?;
        if a.blend.is_some() {
            return Err(Error);
        }
        match op {
            0x109 => a.default = a.default.abs(),
            0x10e => a.default = -a.default,
            0x105 => a.default = f64::from(a.default == 0.),
            0x11a => {
                if a.default < 0. {
                    return Err(Error);
                }
                a.default = a.default.sqrt();
            }
            0x112 => return Ok(()),
            0x11b => {
                self.push(a.clone())?;
            }
            0x114 => {
                let idx = usize::try_from(a.int()?).map_err(|_| Error)?;
                let mut v = self.pop()?;
                v.origins.extend(a.origins);
                v.origins.insert(origin);
                *self.transient.get_mut(idx).ok_or(Error)? = v;
                return Ok(());
            }
            0x115 => {
                let selector = a.origins.clone();
                a = self
                    .transient
                    .get(usize::try_from(a.int()?).map_err(|_| Error)?)
                    .ok_or(Error)?
                    .clone();
                a.origins.extend(selector);
            }
            0x11d => {
                let idx = a.int()?.max(0) as usize;
                let selector = a.origins.clone();
                a = self
                    .stack
                    .get(
                        self.stack
                            .len()
                            .checked_sub(idx.min(self.stack.len().saturating_sub(1)) + 1)
                            .ok_or(Error)?,
                    )
                    .ok_or(Error)?
                    .clone();
                a.origins.extend(selector);
            }
            0x11c => {
                let b = self.pop()?;
                a.origins.insert(origin);
                self.push(a)?;
                a = b;
            }
            0x11e => {
                let shift = a.int()?;
                let count = self.pop()?;
                let n = usize::try_from(count.int()?).map_err(|_| Error)?;
                let start = self.stack.len().checked_sub(n).ok_or(Error)?;
                if n > 0 {
                    self.stack[start..].rotate_right(shift.rem_euclid(n as i32) as usize);
                    for value in &mut self.stack[start..] {
                        value.origins.extend(&a.origins);
                        value.origins.extend(&count.origins);
                        value.origins.insert(origin);
                    }
                }
                return Ok(());
            }
            0x116 => {
                let b = self.pop()?;
                let v2 = self.pop()?;
                let v1 = self.pop()?;
                let mut origins = a.origins.clone();
                origins.extend(&b.origins);
                origins.extend(&v1.origins);
                origins.extend(&v2.origins);
                a = if b.default <= a.default { v1 } else { v2 };
                a.origins = origins;
            }
            0x117 => {
                return Err(Error);
            } // Random-dependent programs cannot be deterministically flattened.
            _ => {
                let mut b = self.pop()?;
                if b.blend.is_some() {
                    return Err(Error);
                }
                b.origins.extend(&a.origins);
                b.default = match op {
                    0x103 => f64::from(b.default != 0. && a.default != 0.),
                    0x104 => f64::from(b.default != 0. || a.default != 0.),
                    0x10a => b.default + a.default,
                    0x10b => b.default - a.default,
                    0x10c => {
                        if a.default == 0. {
                            return Err(Error);
                        }
                        b.default / a.default
                    }
                    0x10f => f64::from(b.default == a.default),
                    0x118 => b.default * a.default,
                    _ => return Err(Error),
                };
                a = b;
            }
        }
        a.origins.insert(origin);
        self.push(a)
    }
}

pub(super) fn emit_value(
    value: &Value,
    out: &mut Vec<u8>,
    dict: bool,
    active: Option<usize>,
    occupied: &mut usize,
    limit: usize,
    remaining: &mut usize,
) -> Result<()> {
    *remaining = remaining.checked_sub(1).ok_or(Error)?;
    if let Some(blend) = &value.blend {
        if Some(blend.ivs) != active {
            return Err(Error);
        }
        emit_value(&blend.base, out, dict, active, occupied, limit, remaining)?;
        for delta in &blend.deltas {
            emit_value(delta, out, dict, active, occupied, limit, remaining)?;
        }
        if *occupied >= limit {
            return Err(Error);
        }
        out.extend([140, if dict { 23 } else { 16 }]);
        *occupied -= blend.deltas.len();
    } else {
        if *occupied >= limit {
            return Err(Error);
        }
        encoding::encode(out, value.default, dict)?;
        *occupied += 1;
    }
    Ok(())
}

pub(super) fn flatten(
    commands: &[Command],
    width: Option<f64>,
    cff2: bool,
    inherited_ivs: usize,
) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut remaining = 200_000;
    if let Some(width) = width {
        encoding::encode(&mut out, width, false)?;
    }
    let active = commands
        .iter()
        .flat_map(|c| &c.args)
        .find_map(|v| v.blend.as_ref().map(|b| b.ivs));
    if let Some(ivs) = active.filter(|&ivs| ivs != inherited_ivs) {
        encoding::integer(&mut out, i32::try_from(ivs).map_err(|_| Error)?);
        out.push(15);
    }
    for (index, cmd) in commands.iter().enumerate() {
        let mut occupied = usize::from(width.is_some() && index == 0);
        for value in &cmd.args {
            emit_value(
                value,
                &mut out,
                false,
                active,
                &mut occupied,
                if cff2 { 513 } else { 48 },
                &mut remaining,
            )?;
        }
        encoding::emit_op(&mut out, cmd.op);
        out.extend(&cmd.mask);
    }
    if !cff2 && out.last() != Some(&14) {
        out.push(14);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cff::source::{Cff1, Cff2};
    struct Source(Vec<Vec<u8>>);
    impl CharStringSource for Source {
        fn program(&self, _: bool, _: usize, i: usize) -> Result<&[u8]> {
            self.0.get(i).map(Vec::as_slice).ok_or(Error)
        }
        fn subr_count(&self, _: bool, _: usize) -> usize {
            self.0.len()
        }
        fn region_count(&self, _: usize) -> Result<usize> {
            Ok(1)
        }
    }
    #[test]
    fn nested_blends_keep_intermediate_rounding_and_bound_depth() {
        let source = Source(vec![]);
        let program = [139, 139, 21, 139, 140, 140, 16, 140, 140, 16, 6];
        let mut recorder = Recorder::default();
        Interpreter::<Cff2, _, _>::new(&source, &mut recorder, 0, 0, false)
            .run(Program::Glyph(0), &program)
            .unwrap();
        let value = &recorder.commands[1].args[0];
        assert!(value.nested());
        assert_eq!(value.resolve(&[vec![0.6]][..], &mut 200_000).unwrap(), 2.);
        let flattened = flatten(&recorder.commands, None, true, 0).unwrap();
        let mut reread = Recorder::default();
        Interpreter::<Cff2, _, _>::new(&source, &mut reread, 0, 0, false)
            .run(Program::Glyph(0), &flattened)
            .unwrap();
        assert_eq!(
            reread.commands[1].args[0]
                .resolve(&[vec![0.6]][..], &mut 200_000)
                .unwrap(),
            2.
        );
        let mut value = Value::plain(0.);
        for _ in 0..64 {
            value = Value::blended(&value, &[Value::plain(1.)], 0).unwrap();
        }
        assert!(Value::blended(&value, &[Value::plain(1.)], 0).is_err());
    }

    #[test]
    fn shared_blends_bound_expansion_and_total_work() {
        let source = Source(vec![]);
        // `index` shares the current expression before using both copies as
        // the next blend's default and delta. Its expansion doubles each time.
        let mut program = vec![139, 140, 140, 16];
        for _ in 0..32 {
            program.extend([139, 12, 29, 140, 16]);
        }
        let mut recorder = Recorder::default();
        assert!(
            Interpreter::<Cff2, _, _>::new(&source, &mut recorder, 0, 0, false)
                .run(Program::Glyph(0), &program)
                .is_err()
        );

        let mut value = Value::plain(0.);
        for _ in 0..16 {
            value = Value::blended(&value, &[value.clone()], 0).unwrap();
        }
        assert!(Value::blended(&value, &[value.clone()], 0).is_err());
        assert!(value.resolve(&[vec![0.5]][..], &mut 100).is_err());
        let command = Command {
            op: 6,
            args: vec![value],
            mask: vec![],
        };
        assert!(flatten(&[command.clone(), command], None, true, 0).is_err());
    }

    #[test]
    fn hint_operands_cross_subroutine_boundaries() {
        let source = Source(vec![vec![1, 11]]);
        let mut recorder = Recorder::default();
        let mut interpreter = Interpreter::<Cff1, _, _>::new(&source, &mut recorder, 0, 0, true);
        interpreter
            .run(Program::Glyph(0), &[139, 149, 32, 10, 139, 139, 21, 14])
            .unwrap();
        recorder.remove_hint_calls();
        assert_eq!(
            recorder.closure(&[Program::Glyph(0)]).unwrap(),
            [Program::Glyph(0)].into()
        );
        assert_eq!(
            recorder
                .encode(Program::Glyph(0), &BTreeMap::new(), 0, &[0])
                .unwrap(),
            vec![139, 139, 21, 14]
        );
    }
    #[test]
    fn removing_blended_hints_removes_count_and_blend() {
        let source = Source(vec![]);
        let mut recorder = Recorder::default();
        let mut interpreter = Interpreter::<Cff2, _, _>::new(&source, &mut recorder, 0, 0, true);
        interpreter
            .run(
                Program::Glyph(0),
                &[139, 149, 140, 141, 141, 16, 1, 139, 139, 21],
            )
            .unwrap();
        assert_eq!(
            recorder
                .encode(Program::Glyph(0), &BTreeMap::new(), 0, &[0])
                .unwrap(),
            vec![139, 139, 21]
        );
    }
    #[test]
    fn computed_hint_calls_consume_their_operand() {
        let source = Source(vec![vec![1, 11]]);
        let mut recorder = Recorder::default();
        let mut interpreter = Interpreter::<Cff1, _, _>::new(&source, &mut recorder, 0, 0, true);
        interpreter
            .run(
                Program::Glyph(0),
                &[139, 149, 32, 139, 12, 10, 10, 139, 139, 21, 149, 139, 5, 14],
            )
            .unwrap();
        recorder.remove_hint_calls();
        let bytes = recorder
            .encode(Program::Glyph(0), &BTreeMap::new(), 0, &[0])
            .unwrap();
        let mut result = Recorder::default();
        let mut decoded = Interpreter::<Cff1, _, _>::new(&source, &mut result, 0, 0, false);
        decoded.run(Program::Glyph(0), &bytes).unwrap();
        assert_eq!(decoded.width, None);
        assert_eq!(
            result.commands.iter().map(|c| c.op).collect::<Vec<_>>(),
            [21, 5, 14]
        );
        assert_eq!(result.commands[1].args[0].default, 10.);
    }

    #[test]
    fn transient_and_selector_operands_are_removed_with_hints() {
        let source = Source(vec![]);
        let mut recorder = Recorder::default();
        let mut interpreter = Interpreter::<Cff1, _, _>::new(&source, &mut recorder, 0, 0, true);
        interpreter
            .run(
                Program::Glyph(0),
                &[159, 139, 12, 20, 139, 12, 21, 139, 1, 139, 139, 21, 14],
            )
            .unwrap();
        assert_eq!(
            recorder
                .encode(Program::Glyph(0), &BTreeMap::new(), 0, &[0])
                .unwrap(),
            [139, 139, 21, 14]
        );
    }

    #[test]
    fn shared_hint_and_drawing_operands_require_flattening() {
        let source = Source(vec![]);
        let mut recorder = Recorder::default();
        let mut interpreter = Interpreter::<Cff1, _, _>::new(&source, &mut recorder, 0, 0, true);
        interpreter
            .run(
                Program::Glyph(0),
                &[
                    139, 149, 1, 159, 12, 27, 139, 12, 20, 139, 1, 139, 12, 21, 139, 21, 14,
                ],
            )
            .unwrap();
        assert!(recorder.needs_flattening);
        assert_eq!(
            flatten(&recorder.commands, None, false, 0).unwrap(),
            [159, 139, 21, 14]
        );
    }

    #[test]
    fn duplicated_provenance_does_not_expand_exponentially() {
        let source = Source(vec![]);
        let mut program = vec![139];
        for _ in 0..80 {
            program.extend([12, 27, 12, 10]);
        }
        program.extend([139, 1, 139, 139, 21, 14]);
        let mut recorder = Recorder::default();
        let mut interpreter = Interpreter::<Cff1, _, _>::new(&source, &mut recorder, 0, 0, true);
        interpreter.run(Program::Glyph(0), &program).unwrap();
        assert_eq!(
            recorder
                .encode(Program::Glyph(0), &BTreeMap::new(), 0, &[0])
                .unwrap(),
            [139, 139, 21, 14]
        );
    }

    #[test]
    fn recursion_and_truncated_operands_are_errors() {
        let source = Source(vec![vec![32, 10, 11]]);
        let mut recorder = Recorder::default();
        let mut interpreter = Interpreter::<Cff1, _, _>::new(&source, &mut recorder, 0, 0, false);
        assert!(interpreter.run(Program::Glyph(0), &[32, 10, 14]).is_err());
        for bytes in [vec![28], vec![255, 0], vec![139, 19], vec![32, 10]] {
            let mut rec = Recorder::default();
            let mut i = Interpreter::<Cff1, _, _>::new(&source, &mut rec, 0, 0, false);
            assert!(i.run(Program::Glyph(0), &bytes).is_err());
        }
    }
}
