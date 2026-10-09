//! DICTs retain their original bytes unless an entry needs rewriting.

use super::{
    encoding::{self, emit_op},
    Error, Result,
};

#[derive(Clone)]
pub(super) struct Entry {
    pub op: u16,
    pub args: Vec<f64>,
    pub raw: Vec<u8>,
}

pub(super) fn parse(data: &[u8]) -> Result<Vec<Entry>> {
    let mut out = Vec::new();
    let mut p = 0;
    let mut start = 0;
    let mut args = Vec::new();
    while p < data.len() {
        if let Some(v) = encoding::number(data, &mut p, true)? {
            args.push(v);
        } else {
            let op = encoding::op(data, &mut p)?;
            // blend leaves its results on the stack; its arguments are interpreted later.
            if op == 23 {
                continue;
            }
            out.push(Entry {
                op,
                args: std::mem::take(&mut args),
                raw: data[start..p].to_vec(),
            });
            start = p;
        }
        if args.len() > 513 {
            return Err(Error);
        }
    }
    if start != p {
        return Err(Error);
    }
    Ok(out)
}

pub(super) fn arg(entries: &[Entry], op: u16, index: usize) -> Option<f64> {
    entries
        .iter()
        .rev()
        .find(|e| e.op == op)?
        .args
        .get(index)
        .copied()
}

pub(super) fn uint(v: f64) -> Result<usize> {
    if v < 0. || v > i32::MAX as f64 || v.fract() != 0. {
        return Err(Error);
    }
    Ok(v as usize)
}

pub(super) fn offset(entries: &[Entry], op: u16, index: usize) -> Result<Option<usize>> {
    arg(entries, op, index).map(uint).transpose()
}

pub(super) fn replacement(op: u16, args: &[usize]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    for &arg in args {
        encoding::long(&mut out, i32::try_from(arg).map_err(|_| Error)?);
    }
    emit_op(&mut out, op);
    Ok(out)
}

pub(super) fn is_hint(op: u16) -> bool {
    matches!(op, 6..=11 | 0x109..=0x10e | 0x112)
}
