//! Type 2 interpretation with separate closure, retained, and flattened actions.
//!
//! Retained programs borrow source bytes and record only edits. Literal spans
//! are sufficient for the common case. Retained calls require immediate literals;
//! dropped hint operands are deleted from whichever program pushed them, and
//! glyphs whose deletions conflict in a shared program are flattened.

use super::{
    encoding,
    source::{CffFlavor, CharStringSource},
    Error, Result,
};
use crate::FastHashMap;
use std::collections::{BTreeMap, BTreeSet};
use std::marker::PhantomData;
use std::rc::Rc;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub(super) enum Program {
    Glyph(usize),
    Local(usize, usize),
    Global(usize, usize),
}
type Origin = (Program, usize, usize);

#[derive(Clone, Default)]
pub(super) struct Value {
    pub default: f64,
    pub blend: Option<Rc<Blend>>,
    origin: Option<Origin>,
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
    fn finish_blend(&self, value: f64) -> f64 {
        value.round()
    }
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
            origin: Some(origin),
        }
    }
    pub fn expression(&self) -> Self {
        Self {
            default: self.default,
            blend: self.blend.clone(),
            origin: None,
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
            origin: None,
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
        self.resolve_with_rounding(scalars, remaining, true)
    }
    pub fn resolve_continuous<S: BlendScalars + ?Sized>(
        &self,
        scalars: &S,
        remaining: &mut usize,
    ) -> Result<f64> {
        self.resolve_with_rounding(scalars, remaining, false)
    }
    fn resolve_with_rounding<S: BlendScalars + ?Sized>(
        &self,
        scalars: &S,
        remaining: &mut usize,
        round: bool,
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
            delta_sum += delta.resolve_with_rounding(scalars, remaining, round)? * gain;
        }
        let result = blend
            .base
            .resolve_with_rounding(scalars, remaining, round)?
            + delta_sum;
        Ok(if round {
            scalars.finish_blend(result)
        } else {
            result
        })
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

    pub(super) fn origin(&self) -> Option<(Program, usize, usize)> {
        self.origin
    }
}

#[derive(Clone)]
pub(super) struct Command {
    pub op: u16,
    pub args: Vec<Value>,
    pub mask: Vec<u8>,
}

pub(super) trait CharStringActions<'s> {
    // Retaining original bytes and finding closure need blend identity and
    // validation, while consumers that emit values need the actual deltas.
    const MATERIALIZE_BLENDS: bool = true;
    fn program(&mut self, _key: Program, _data: &'s [u8]) {}
    fn call(
        &mut self,
        _key: Program,
        _start: usize,
        _end: usize,
        _target: Program,
        _operand: &Value,
    ) -> Result<()> {
        Ok(())
    }
    fn width(&mut self, _origin: Option<Origin>) {}
    fn mask(&mut self, _key: Program, _start: usize, _end: usize) {}
    fn hint(&mut self, _key: Program, _start: usize, _end: usize, _args: &[Value]) {}
    fn command(&mut self, _op: u16, _args: &[Value], _mask: &[u8]) {}
}
// seac closure only needs the interpreter's result.
impl CharStringActions<'_> for () {
    const MATERIALIZE_BLENDS: bool = false;
}

#[derive(Default)]
pub(super) struct Flattener {
    pub commands: Vec<Command>,
}
impl CharStringActions<'_> for Flattener {
    fn command(&mut self, op: u16, args: &[Value], mask: &[u8]) {
        self.commands.push(Command {
            op,
            args: args.to_vec(),
            mask: mask.to_vec(),
        });
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Edit {
    end: usize,
    call: Option<Program>,
}
struct RecordedProgram<'s> {
    id: usize,
    data: &'s [u8],
    edits: BTreeMap<usize, Edit>,
    masks: BTreeMap<usize, usize>,
}
impl PartialEq for RecordedProgram<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.data == other.data && self.edits == other.edits && self.masks == other.masks
    }
}
impl Eq for RecordedProgram<'_> {}
#[derive(Default)]
pub(super) struct Recorder<'s> {
    programs: FastHashMap<Program, RecordedProgram<'s>>,
    pub needs_flatten: bool,
}
impl<'s> CharStringActions<'s> for Recorder<'s> {
    const MATERIALIZE_BLENDS: bool = false;

    fn program(&mut self, key: Program, data: &'s [u8]) {
        let id = self.programs.len();
        self.programs.entry(key).or_insert_with(|| RecordedProgram {
            id,
            data,
            edits: BTreeMap::new(),
            masks: BTreeMap::new(),
        });
    }
    fn call(
        &mut self,
        key: Program,
        start: usize,
        end: usize,
        target: Program,
        operand: &Value,
    ) -> Result<()> {
        if let Some((producer, number_start, number_end)) = operand.origin {
            if producer == key && number_end == start {
                self.edit(
                    key,
                    number_start,
                    Edit {
                        end,
                        call: Some(target),
                    },
                );
                return Ok(());
            }
        }
        // Retained subroutine calls require a literal in the same program.
        // A caller-supplied target cannot be remapped at this shared offset.
        Err(Error)
    }
    fn width(&mut self, origin: Option<Origin>) {
        // Like HarfBuzz, remove the width literal from whichever program
        // pushed it, often a subroutine shared with the glyph's hint operands.
        // merge() flattens glyphs whose deletion conflicts with other callers.
        if let Some((key, start, end)) = origin {
            self.edit(key, start, Edit { end, call: None });
        } else {
            self.needs_flatten = true;
        }
    }
    fn mask(&mut self, key: Program, start: usize, end: usize) {
        let old = self
            .programs
            .get_mut(&key)
            .unwrap()
            .masks
            .entry(start)
            .or_insert(end);
        self.needs_flatten |= *old != end;
    }
    fn hint(&mut self, key: Program, start: usize, end: usize, args: &[Value]) {
        // Hint operators consume the whole stack. HarfBuzz drops each operand
        // together with the operator, even when a caller or another subroutine
        // pushed it, so delete every literal from the program that produced it.
        // Operands that immediately precede the operator join its deletion.
        let mut first = start;
        let mut detached = Vec::new();
        for value in args.iter().rev() {
            let Some((producer, from, to)) = value.origin else {
                self.needs_flatten = true;
                return;
            };
            if producer == key && to == first {
                first = from;
            } else {
                detached.push((producer, from, to));
            }
        }
        self.edit(key, first, Edit { end, call: None });
        for (producer, from, to) in detached {
            self.edit(
                producer,
                from,
                Edit {
                    end: to,
                    call: None,
                },
            );
        }
    }
}
impl<'s> Recorder<'s> {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            programs: FastHashMap::with_capacity_and_hasher(
                capacity,
                foldhash::fast::FixedState::default(),
            ),
            needs_flatten: false,
        }
    }

    pub fn merge(&mut self, other: Self) -> bool {
        // Validate the entire glyph before modifying any shared program.
        // Flattening must discard every staged edit, including later edits
        // made after the interpreter first discovered an incompatible mask.
        if other.needs_flatten
            || other
                .programs
                .iter()
                .any(|(key, program)| self.programs.get(key).is_some_and(|old| old != program))
        {
            return false;
        }
        for (key, mut program) in other.programs {
            let id = self.programs.len();
            self.programs.entry(key).or_insert_with(|| {
                program.id = id;
                program
            });
        }
        true
    }

    fn edit(&mut self, key: Program, start: usize, edit: Edit) {
        let edits = &mut self.programs.get_mut(&key).unwrap().edits;
        // A mask's byte length can depend on its caller's stem count, too.
        if let Some(previous) = edits.get(&start) {
            if *previous != edit {
                self.needs_flatten = true;
            }
        } else {
            edits.insert(start, edit);
        }
    }
    fn only_hints(&self, key: Program, visiting: &mut BTreeSet<Program>) -> bool {
        if !visiting.insert(key) {
            return false;
        }
        let result = self.programs.get(&key).is_some_and(|p| {
            let mut pos = 0;
            for (&start, edit) in &p.edits {
                if start < pos
                    || p.data[pos..start].iter().any(|&b| b != 11)
                    || edit.call.is_some_and(|t| !self.only_hints(t, visiting))
                {
                    return false;
                }
                pos = edit.end;
            }
            p.data[pos..].iter().all(|&b| b == 11)
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
        for p in self.programs.values_mut() {
            for edit in p.edits.values_mut() {
                if edit.call.is_some_and(|p| dead.contains(&p)) {
                    edit.call = None;
                }
            }
        }
    }
    pub fn closure(&self, roots: &[Program]) -> Result<Vec<Program>> {
        let mut used = vec![false; self.programs.len()];
        let mut result = Vec::new();
        let mut pending = roots.to_vec();
        while let Some(key) = pending.pop() {
            let program = self.programs.get(&key).ok_or(Error)?;
            if std::mem::replace(&mut used[program.id], true) {
                continue;
            }
            result.push(key);
            for edit in program.edits.values() {
                if let Some(target) = edit.call {
                    pending.push(target);
                }
            }
        }
        Ok(result)
    }
    pub fn fd_dependent(&self, key: Program) -> bool {
        match key {
            Program::Local(..) => true,
            _ => self.programs[&key]
                .edits
                .values()
                .filter_map(|e| e.call)
                .any(|t| self.fd_dependent(t)),
        }
    }
    pub fn share_global(&self, a: Program, b: Program) -> bool {
        if self.fd_dependent(a) || self.fd_dependent(b) {
            return false;
        }
        let a = &self.programs[&a];
        let b = &self.programs[&b];
        a.data == b.data
            && a.edits.len() == b.edits.len()
            && a.edits.iter().zip(&b.edits).all(|((s, a), (t, b))| {
                s == t
                    && a.end == b.end
                    && match (a.call, b.call) {
                        (None, None) => true,
                        (Some(a @ Program::Global(_, i)), Some(b @ Program::Global(_, j))) => {
                            i == j && self.share_global(a, b)
                        }
                        _ => false,
                    }
            })
    }
    pub fn encode(
        &self,
        key: Program,
        remap: &FastHashMap<Program, usize>,
        global_count: usize,
        local_counts: &[usize],
    ) -> Result<Vec<u8>> {
        let program = self.programs.get(&key).ok_or(Error)?;
        let mut out = Vec::with_capacity(program.data.len());
        let mut pos = 0;
        for (&start, edit) in &program.edits {
            out.extend(program.data.get(pos..start).ok_or(Error)?);
            if let Some(target) = edit.call {
                let (op, count) = match target {
                    Program::Local(fd, _) => (10, *local_counts.get(fd).ok_or(Error)?),
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
            pos = edit.end;
        }
        out.extend(program.data.get(pos..).ok_or(Error)?);
        Ok(out)
    }
}

pub(super) struct Interpreter<'a, 's, F, S, A> {
    source: &'s S,
    actions: &'a mut A,
    fd: usize,
    stack: Vec<Value>,
    transient: Vec<Value>,
    hints: usize,
    ivs: usize,
    seen_ivs: bool,
    seen_blend: bool,
    blend_identity: Option<Rc<Blend>>,
    width_read: bool,
    pub width: Option<f64>,
    pub seac: Option<(u8, u8)>,
    remaining: usize,
    drop_hints: bool,
    flavor: PhantomData<F>,
}
impl<'a, 's, F: CffFlavor, S: CharStringSource, A: CharStringActions<'s>>
    Interpreter<'a, 's, F, S, A>
{
    pub fn new(source: &'s S, actions: &'a mut A, fd: usize, ivs: usize, drop_hints: bool) -> Self {
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
            blend_identity: None,
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
    #[inline]
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
                self.actions.width(value.origin);
            }
        }
        Ok(())
    }
    pub fn run(&mut self, key: Program, data: &'s [u8]) -> Result<()> {
        self.execute(key, data, 0).map(|_| ())
    }
    fn execute(&mut self, key: Program, data: &'s [u8], depth: usize) -> Result<bool> {
        if depth > 10 {
            return Err(Error);
        }
        let mut p = 0;
        self.actions.program(key, data);
        while p < data.len() {
            if self.remaining == 0 {
                return Err(Error);
            }
            self.remaining -= 1;
            let start = p;
            let byte = *data.get(p).ok_or(Error)?;
            if byte >= 28 {
                if let Some(value) = encoding::number(data, &mut p, false)? {
                    self.push(Value::number(value, (key, start, p)))?;
                    continue;
                }
            }
            let op = encoding::op(data, &mut p)?;
            let mut mask = &[][..];
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
                    mask = data.get(p..end).ok_or(Error)?;
                    p = end;
                    self.actions.mask(key, start, p);
                }
            }
            match op {
                10 | 29 => {
                    let value = self.pop()?;
                    let global = op == 29;
                    let index = value
                        .int()?
                        .checked_add(self.source.subr_bias(global, self.fd))
                        .ok_or(Error)?;
                    let index = usize::try_from(index).map_err(|_| Error)?;
                    let target = if global {
                        Program::Global(self.fd, index)
                    } else {
                        Program::Local(self.fd, index)
                    };
                    self.actions.call(key, start, p, target, &value)?;
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
                    let n = usize::try_from(self.pop()?.int()?).map_err(|_| Error)?;
                    let k = self.source.region_count(self.ivs)?;
                    let count = n.checked_mul(k.checked_add(1).ok_or(Error)?).ok_or(Error)?;
                    let group_start = self.stack.len().checked_sub(count).ok_or(Error)?;
                    for i in 0..n {
                        let value = if A::MATERIALIZE_BLENDS {
                            let deltas =
                                &self.stack[group_start + n + i * k..group_start + n + (i + 1) * k];
                            Value::blended(&self.stack[group_start + i], deltas, self.ivs)?
                        } else {
                            // Retained bytes need only a nonconstant identity.
                            // Share one marker instead of allocating each expression.
                            let identity = self.blend_identity.get_or_insert_with(|| {
                                Rc::new(Blend {
                                    ivs: 0,
                                    base: Value::default(),
                                    deltas: Vec::new(),
                                    depth: 0,
                                    expanded_size: 1,
                                })
                            });
                            Value {
                                default: self.stack[group_start + i].default,
                                blend: Some(identity.clone()),
                                origin: None,
                            }
                        };
                        self.stack[group_start + i] = value;
                    }
                    self.stack.truncate(group_start + n);
                }
                0x100 => {
                    // dotsection is a deprecated no-op; operands stay live.
                    if self.drop_hints {
                        self.actions.hint(key, start, p, &[]);
                    }
                }
                0x103..=0x105
                | 0x109..=0x10c
                | 0x10e..=0x10f
                | 0x112
                | 0x114..=0x118
                | 0x11a..=0x11e => self.arithmetic(op)?,
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
                | 0x122..=0x125 => {
                    if op == 14 && !F::CFF2 && self.stack.len() == 4 {
                        self.seac = Some((
                            u8::try_from(self.stack[2].int()?).map_err(|_| Error)?,
                            u8::try_from(self.stack[3].int()?).map_err(|_| Error)?,
                        ));
                    }
                    let hint = matches!(op, 1 | 3 | 18 | 19 | 20 | 23);
                    if hint && self.drop_hints {
                        self.actions.hint(key, start, p, &self.stack);
                    } else if !(F::CFF2 && op == 14) {
                        self.actions.command(op, &self.stack, mask);
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
    fn arithmetic(&mut self, op: u16) -> Result<()> {
        // Stack/transient operations may share values. Their producer is no
        // longer a removable literal, even if the numeric result is unchanged.
        for value in self.stack.iter_mut().chain(&mut self.transient) {
            value.origin = None;
        }
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
                let v = self.pop()?;
                *self.transient.get_mut(idx).ok_or(Error)? = v;
                return Ok(());
            }
            0x115 => {
                a = self
                    .transient
                    .get(usize::try_from(a.int()?).map_err(|_| Error)?)
                    .ok_or(Error)?
                    .clone();
            }
            0x11d => {
                let idx = a.int()?.max(0) as usize;
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
            }
            0x11c => {
                let b = self.pop()?;
                self.push(a)?;
                a = b;
            }
            0x11e => {
                let shift = a.int()?;
                let n = usize::try_from(self.pop()?.int()?).map_err(|_| Error)?;
                let start = self.stack.len().checked_sub(n).ok_or(Error)?;
                if n > 0 {
                    self.stack[start..].rotate_right(shift.rem_euclid(n as i32) as usize);
                }
                return Ok(());
            }
            0x116 => {
                let b = self.pop()?;
                let v2 = self.pop()?;
                let v1 = self.pop()?;
                a = if b.default <= a.default { v1 } else { v2 };
            }
            0x117 => {
                return Err(Error);
            } // Random-dependent programs cannot be deterministically flattened.
            _ => {
                let mut b = self.pop()?;
                if b.blend.is_some() {
                    return Err(Error);
                }
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
    let mut remaining: usize = 200_000;
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
        let mut i = 0;
        let limit: usize = if cff2 { 513 } else { 48 };
        while i < cmd.args.len() {
            let value = &cmd.args[i];
            if let Some(blend) = value.blend.as_ref().filter(|_| !value.nested()) {
                if Some(blend.ivs) != active {
                    return Err(Error);
                }
                let room = limit.checked_sub(occupied + 1).ok_or(Error)? / (blend.deltas.len() + 1);
                let n = cmd.args[i..]
                    .iter()
                    .take(room)
                    .take_while(|v| {
                        !v.nested()
                            && v.blend.as_ref().is_some_and(|b| {
                                b.ivs == blend.ivs && b.deltas.len() == blend.deltas.len()
                            })
                    })
                    .count();
                if n == 0 {
                    return Err(Error);
                }
                remaining = remaining
                    .checked_sub(n * (blend.deltas.len() + 2))
                    .ok_or(Error)?;
                for v in &cmd.args[i..i + n] {
                    encoding::encode(&mut out, v.default, false)?;
                }
                for v in &cmd.args[i..i + n] {
                    for delta in &v.blend.as_ref().ok_or(Error)?.deltas {
                        encoding::encode(&mut out, delta.default, false)?;
                    }
                }
                encoding::integer(&mut out, n as i32);
                out.push(16);
                occupied += n;
                i += n;
            } else {
                emit_value(
                    value,
                    &mut out,
                    false,
                    active,
                    &mut occupied,
                    limit,
                    &mut remaining,
                )?;
                i += 1;
            }
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
    fn repeated_dup_add_has_bounded_state() {
        let source = Source(vec![]);
        let mut data = vec![139];
        for _ in 0..40 {
            data.extend([12, 27, 12, 10]);
        }
        data.extend([149, 1, 139, 139, 21, 14]);
        let mut rec = Recorder::default();
        Interpreter::<Cff1, _, _>::new(&source, &mut rec, 0, 0, true)
            .run(Program::Glyph(0), &data)
            .unwrap();
        assert!(rec.needs_flatten);
        let mut flat = Flattener::default();
        Interpreter::<Cff1, _, _>::new(&source, &mut flat, 0, 0, true)
            .run(Program::Glyph(0), &data)
            .unwrap();
        assert_eq!(
            flatten(&flat.commands, None, false, 0).unwrap(),
            [139, 139, 21, 14]
        );
    }
    #[test]
    fn hint_operands_from_subroutines_are_deleted_there() {
        // The local subroutine pushes the hstem operands for glyph 0, but the
        // rmoveto operands for glyph 1.
        let source = Source(vec![vec![139, 149, 11]]);
        fn record<'s>(source: &'s Source, glyph: &'s [u8], gid: usize) -> Recorder<'s> {
            let mut rec = Recorder::default();
            Interpreter::<Cff1, _, _>::new(source, &mut rec, 0, 0, true)
                .run(Program::Glyph(gid), glyph)
                .unwrap();
            rec
        }
        let mut merged = Recorder::default();
        let hinted = record(&source, &[32, 10, 1, 139, 139, 21, 14], 0);
        assert!(!hinted.needs_flatten);
        assert!(merged.merge(hinted));
        merged.remove_hint_calls();
        // The operands are deleted from the subroutine, which is then only a
        // return, so the call is dropped as well.
        assert_eq!(
            merged.closure(&[Program::Glyph(0)]).unwrap(),
            [Program::Glyph(0)]
        );
        assert_eq!(
            merged
                .encode(Program::Glyph(0), &FastHashMap::default(), 0, &[1])
                .unwrap(),
            [139, 139, 21, 14]
        );
        // Glyph 1 needs the subroutine's literals, which conflicts with the
        // deletions already merged, so it must be flattened instead.
        assert!(!merged.merge(record(&source, &[32, 10, 21, 14], 1)));
    }
    #[test]
    fn hints_with_local_literals_remain_subroutinized() {
        let source = Source(vec![vec![139, 149, 1, 11]]);
        let mut rec = Recorder::default();
        Interpreter::<Cff1, _, _>::new(&source, &mut rec, 0, 0, true)
            .run(Program::Glyph(0), &[32, 10, 139, 139, 21, 14])
            .unwrap();
        assert!(!rec.needs_flatten);
        rec.remove_hint_calls();
        assert_eq!(
            rec.closure(&[Program::Glyph(0)]).unwrap(),
            [Program::Glyph(0)]
        );
        assert_eq!(
            rec.encode(Program::Glyph(0), &FastHashMap::default(), 0, &[0])
                .unwrap(),
            [139, 139, 21, 14]
        );
    }
    #[test]
    fn caller_supplied_subroutine_operand_is_rejected() {
        let source = Source(vec![vec![10, 11]]);
        let mut rec = Recorder::default();
        assert!(
            Interpreter::<Cff1, _, _>::new(&source, &mut rec, 0, 0, false)
                .run(Program::Glyph(0), &[32, 32, 10, 14])
                .is_err()
        );
    }
    #[test]
    fn flatten_keeps_blend_groups() {
        let source = Source(vec![]);
        let mut flat = Flattener::default();
        let data = [139, 149, 140, 141, 141, 16, 21];
        Interpreter::<Cff2, _, _>::new(&source, &mut flat, 0, 0, false)
            .run(Program::Glyph(0), &data)
            .unwrap();
        assert_eq!(flatten(&flat.commands, None, true, 0).unwrap(), data);
    }
    #[test]
    fn recursion_and_truncated_operands_are_errors() {
        let source = Source(vec![vec![32, 10, 11]]);
        for bytes in [
            vec![32, 10, 14],
            vec![28],
            vec![255, 0],
            vec![139, 19],
            vec![32, 10],
        ] {
            assert!(
                Interpreter::<Cff1, _, _>::new(&source, &mut (), 0, 0, false)
                    .run(Program::Glyph(0), &bytes)
                    .is_err()
            );
        }
    }
    #[test]
    fn computed_hint_calls_consume_their_operand() {
        let source = Source(vec![vec![1, 11]]);
        let data = [139, 149, 32, 139, 12, 10, 10, 139, 139, 21, 149, 139, 5, 14];
        assert!(
            Interpreter::<Cff1, _, _>::new(&source, &mut Recorder::default(), 0, 0, true)
                .run(Program::Glyph(0), &data)
                .is_err()
        );
        let mut flat = Flattener::default();
        Interpreter::<Cff1, _, _>::new(&source, &mut flat, 0, 0, true)
            .run(Program::Glyph(0), &data)
            .unwrap();
        let bytes = flatten(&flat.commands, None, false, 0).unwrap();
        let mut result = Flattener::default();
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
        let data = [159, 139, 12, 20, 139, 12, 21, 139, 1, 139, 139, 21, 14];
        let mut flat = Flattener::default();
        Interpreter::<Cff1, _, _>::new(&source, &mut flat, 0, 0, true)
            .run(Program::Glyph(0), &data)
            .unwrap();
        assert_eq!(
            flatten(&flat.commands, None, false, 0).unwrap(),
            [139, 139, 21, 14]
        );
    }
    #[test]
    fn shared_hint_and_drawing_operands_require_flattening() {
        let source = Source(vec![]);
        let data = [
            139, 149, 1, 159, 12, 27, 139, 12, 20, 139, 1, 139, 12, 21, 139, 21, 14,
        ];
        let mut rec = Recorder::default();
        Interpreter::<Cff1, _, _>::new(&source, &mut rec, 0, 0, true)
            .run(Program::Glyph(0), &data)
            .unwrap();
        assert!(rec.needs_flatten);
        let mut flat = Flattener::default();
        Interpreter::<Cff1, _, _>::new(&source, &mut flat, 0, 0, true)
            .run(Program::Glyph(0), &data)
            .unwrap();
        assert_eq!(
            flatten(&flat.commands, None, false, 0).unwrap(),
            [159, 139, 21, 14]
        );
    }
    #[test]
    fn duplicated_provenance_does_not_expand_exponentially() {
        let source = Source(vec![]);
        let mut data = vec![139];
        for _ in 0..80 {
            data.extend([12, 27, 12, 10]);
        }
        data.extend([139, 1, 139, 139, 21, 14]);
        let mut flat = Flattener::default();
        Interpreter::<Cff1, _, _>::new(&source, &mut flat, 0, 0, true)
            .run(Program::Glyph(0), &data)
            .unwrap();
        assert_eq!(
            flatten(&flat.commands, None, false, 0).unwrap(),
            [139, 139, 21, 14]
        );
    }
    #[test]
    fn nested_blends_keep_intermediate_rounding_and_bound_depth() {
        let source = Source(vec![]);
        let program = [139, 139, 21, 139, 140, 140, 16, 140, 140, 16, 6];
        let mut recorder = Flattener::default();
        Interpreter::<Cff2, _, _>::new(&source, &mut recorder, 0, 0, false)
            .run(Program::Glyph(0), &program)
            .unwrap();
        let value = &recorder.commands[1].args[0];
        assert!(value.nested());
        assert_eq!(value.resolve(&[vec![0.6]][..], &mut 200_000).unwrap(), 2.);
        assert_eq!(
            value
                .resolve_continuous(&[vec![0.6]][..], &mut 200_000)
                .unwrap(),
            1.2
        );
        let flattened = flatten(&recorder.commands, None, true, 0).unwrap();
        let mut reread = Flattener::default();
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
        let mut recorder = Flattener::default();
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
}
