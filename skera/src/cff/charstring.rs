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
    pub blend: Option<(usize, Vec<f64>)>,
    origins: Vec<Origin>,
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
            origins: vec![origin],
        }
    }
    fn int(&self) -> Result<i32> {
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
    fn discard(&mut self, origins: &[Origin]);
    fn command(&mut self, command: Command);
}

#[derive(Default)]
pub(super) struct Piece {
    bytes: Vec<u8>,
    removed: bool,
    call: Option<Program>,
}
#[derive(Default)]
pub(super) struct Recorder {
    programs: BTreeMap<Program, BTreeMap<usize, Piece>>,
    pub commands: Vec<Command>,
}
impl CharStringActions for Recorder {
    fn token(&mut self, program: Program, start: usize, bytes: &[u8]) {
        self.programs
            .entry(program)
            .or_default()
            .entry(start)
            .or_insert_with(|| Piece {
                bytes: bytes.to_vec(),
                ..Default::default()
            });
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
        let immediate = operand.origins.len() == 1
            && operand.origins[0].0 == program
            && pieces
                .get(&operand.origins[0].1)
                .is_some_and(|p| operand.origins[0].1 + p.bytes.len() == start);
        if immediate {
            pieces.get_mut(&operand.origins[0].1).ok_or(Error)?.removed = true;
        }
        let piece = pieces.get_mut(&start).ok_or(Error)?;
        piece.call = Some(target);
        // Otherwise retain the operand computation and consume its result.
        piece.bytes = if immediate { vec![] } else { vec![12, 18] };
        Ok(())
    }
    fn discard(&mut self, origins: &[Origin]) {
        for &(key, offset) in origins {
            if let Some(piece) = self.programs.get_mut(&key).and_then(|p| p.get_mut(&offset)) {
                piece.removed = true;
            }
        }
    }
    fn command(&mut self, command: Command) {
        self.commands.push(command);
    }
}

impl Recorder {
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
                    piece.removed = true;
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
                self.actions.discard(&value.origins);
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
                        let mut value = values[i].clone();
                        if value.blend.is_some() {
                            return Err(Error);
                        }
                        let deltas = &values[n + i * k..n + (i + 1) * k];
                        if deltas.iter().any(|v| v.blend.is_some()) {
                            return Err(Error);
                        }
                        value.blend = Some((self.ivs, deltas.iter().map(|v| v.default).collect()));
                        for delta in deltas {
                            value.origins.extend(&delta.origins);
                        }
                        value.origins.extend(&count_value.origins);
                        value.origins.push((key, start));
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
                        self.actions.discard(&[(key, start)]);
                        for v in &self.stack {
                            self.actions.discard(&v.origins);
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
        a.origins.push(origin);
        self.push(a)
    }
}

pub(super) fn flatten(
    commands: &[Command],
    width: Option<f64>,
    cff2: bool,
    inherited_ivs: usize,
) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    if let Some(width) = width {
        encoding::encode(&mut out, width, false)?;
    }
    let active = commands
        .iter()
        .flat_map(|c| &c.args)
        .find_map(|v| v.blend.as_ref().map(|b| b.0));
    if let Some(ivs) = active.filter(|&ivs| ivs != inherited_ivs) {
        encoding::integer(&mut out, i32::try_from(ivs).map_err(|_| Error)?);
        out.push(15);
    }
    for cmd in commands {
        for (i, v) in cmd.args.iter().enumerate() {
            if let Some((vs, deltas)) = &v.blend {
                if Some(*vs) != active || i + deltas.len() + 2 > 513 {
                    return Err(Error);
                }
                encoding::encode(&mut out, v.default, false)?;
                for &delta in deltas {
                    encoding::encode(&mut out, delta, false)?;
                }
                out.extend([140, 16]);
            } else {
                encoding::encode(&mut out, v.default, false)?;
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
