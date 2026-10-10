//! Convert resolved CFF2 programs to CID-keyed CFF1, including explicit widths.
use super::{
    charstring::{self, Command, Interpreter, Program, Recorder, Value},
    dict, encoding,
    source::{Cff2, Source},
    Error, Result,
};
use std::collections::BTreeMap;
use write_fonts::read::{
    types::{GlyphId, Tag},
    FontRef, TableProvider,
};

fn entry(out: &mut Vec<u8>, op: u16, args: &[f64]) -> Result<()> {
    for &v in args {
        encoding::encode(out, v, true)?;
    }
    encoding::emit_op(out, op);
    Ok(())
}
fn link(out: &mut Vec<u8>, op: u16, args: &[usize]) -> Result<()> {
    out.extend(dict::replacement(op, args)?);
    Ok(())
}
pub(super) fn convert(font: &FontRef) -> Result<Vec<u8>> {
    let data = font.data_for_tag(Tag::new(b"CFF2")).ok_or(Error)?;
    let source = Source::new(data.as_bytes())?;
    if source.font.var_store().is_some() {
        return Err(Error);
    }
    let count = font.maxp().map_err(|_| Error)?.num_glyphs() as usize;
    if count == 0 {
        return Err(Error);
    }
    let metrics = font.hmtx().map_err(|_| Error)?;
    let mut fd_map = BTreeMap::new();
    let mut widths = vec![Vec::new(); source.fds.len()];
    let mut old_fds = Vec::new();
    for gid in 0..count {
        let gid = GlyphId::new(gid as u32);
        let fd = source.fd(gid)?;
        fd_map.insert(fd, 0);
        old_fds.push(fd);
        widths[fd].push(metrics.advance(gid).ok_or(Error)?);
    }
    for (i, fd) in fd_map.values_mut().enumerate() {
        *fd = i;
    }
    if fd_map.len() > 256 {
        return Err(Error);
    }
    let mut defaults = Vec::new();
    let mut nominals = Vec::new();
    for values in &widths {
        let mut histogram = BTreeMap::new();
        for &v in values {
            *histogram.entry(v).or_insert(0usize) += 1;
        }
        let default = histogram
            .iter()
            .max_by_key(|(v, n)| (**n, std::cmp::Reverse(**v)))
            .map_or(0, |(&v, _)| v);
        defaults.push(default);
        let min = values.iter().copied().min().unwrap_or(0) as u32;
        let max = values.iter().copied().max().unwrap_or(0) as u32;
        nominals.push((min + max).div_ceil(2) as u16);
    }
    let mut chars = Vec::new();
    for (gid, &fd) in old_fds.iter().enumerate() {
        let mut rec = Recorder::default();
        let mut interp = Interpreter::<Cff2, _, _>::new(&source, &mut rec, fd, 0, false);
        interp.run(
            Program::Glyph(gid),
            source.font.charstrings().get(gid).unwrap_or(&[]),
        )?;
        let width = metrics.advance(GlyphId::new(gid as u32)).ok_or(Error)?;
        let width = (width != defaults[fd]).then_some(width as f64 - nominals[fd] as f64);
        let commands = split(&rec.commands, width.is_some())?;
        chars.push(charstring::flatten(&commands, width, false, 0)?);
    }
    let mut privates = Vec::new();
    for &fd in fd_map.keys() {
        let mut bytes = Vec::new();
        for e in &source.fds[fd].private {
            if !matches!(e.op, 19..=23) {
                bytes.extend(&e.raw);
            }
        }
        entry(&mut bytes, 20, &[defaults[fd] as f64])?;
        entry(&mut bytes, 21, &[nominals[fd] as f64])?;
        privates.push(bytes);
    }
    let name = encoding::index(&[b"CFF1Font".to_vec()], false)?;
    let strings = encoding::index(&[b"Adobe".to_vec(), b"Identity".to_vec()], false)?;
    let char_index = encoding::index(&chars, false)?;
    let fds: Vec<_> = old_fds.iter().map(|fd| fd_map[fd]).collect();
    let select = super::subset::fdselect(&fds, false)?;
    let mut charset = if count == 1 { vec![0] } else { vec![2, 0, 1] };
    if count > 1 {
        charset.extend(((count - 2) as u16).to_be_bytes());
    }
    let bbox = font.head().map_err(|_| Error)?;
    let make = |base: usize| -> Result<(Vec<u8>, Vec<u8>)> {
        let mut offsets = Vec::new();
        let mut pos = base;
        for private in &privates {
            offsets.push(pos);
            pos += private.len();
        }
        let cs = pos;
        pos += char_index.len();
        let fdselect = pos;
        pos += select.len();
        let charset_offset = pos;
        pos += charset.len();
        let fdarray_offset = pos;
        let mut dictionaries = Vec::new();
        for (new, &old) in fd_map.keys().enumerate() {
            let mut bytes = Vec::new();
            for e in &source.fds[old].entries {
                if e.op == 0x107 {
                    bytes.extend(&e.raw);
                }
            }
            link(&mut bytes, 18, &[privates[new].len(), offsets[new]])?;
            dictionaries.push(bytes);
        }
        let fdarray = encoding::index(&dictionaries, false)?;
        let mut top = Vec::new();
        entry(&mut top, 0x11e, &[391., 392., 0.])?;
        entry(&mut top, 0x122, &[count as f64])?;
        entry(
            &mut top,
            5,
            &[
                bbox.x_min() as f64,
                bbox.y_min() as f64,
                bbox.x_max() as f64,
                bbox.y_max() as f64,
            ],
        )?;
        for e in &source.top {
            if e.op == 0x107 {
                top.extend(&e.raw);
            }
        }
        for (op, offset) in [
            (17, cs),
            (15, charset_offset),
            (0x124, fdarray_offset),
            (0x125, fdselect),
        ] {
            link(&mut top, op, &[offset])?;
        }
        Ok((encoding::index(&[top], false)?, fdarray))
    };
    let (dummy, _) = make(0)?;
    let (top, fdarray) = make(4 + name.len() + strings.len() + dummy.len() + 2)?;
    let mut out = vec![1, 0, 4, 4];
    out.extend(name);
    out.extend(top);
    out.extend(strings);
    out.extend([0, 0]);
    for p in privates {
        out.extend(p);
    }
    out.extend(char_index);
    out.extend(select);
    out.extend(charset);
    out.extend(fdarray);
    Ok(out)
}

fn command(op: u16, args: Vec<Value>, mask: Vec<u8>) -> Command {
    Command { op, args, mask }
}
/// Split packed drawing operators at their natural groups. Hint positions are
/// relative to zero in each stem operator, so their first operands need rebasing.
fn split(commands: &[Command], width: bool) -> Result<Vec<Command>> {
    let mut out = Vec::new();
    let mut first = true;
    for cmd in commands {
        if cmd.args.iter().any(|v| v.blend.is_some()) {
            return Err(Error);
        }
        let limit = if first && width { 47 } else { 48 };
        if cmd.args.len() <= limit {
            out.push(cmd.clone());
            first = false;
            continue;
        }
        let mut op = cmd.op;
        let args = cmd.args.clone();
        let mut mask = cmd.mask.clone();
        if matches!(op, 19 | 20) {
            op = 23;
            mask.clear();
        }
        if op == 24 {
            let end = args.len().checked_sub(8).ok_or(Error)?;
            let mut done = 0;
            while done < end {
                let n = (if first && width { 42 } else { 48 }).min(end - done);
                out.push(command(8, args[done..done + n].to_vec(), vec![]));
                done += n;
                first = false;
            }
            out.push(command(24, args[end..].to_vec(), vec![]));
        } else if op == 25 {
            let end = args.len().checked_sub(8).ok_or(Error)?;
            for c in args[..end].chunks(46) {
                out.push(command(5, c.to_vec(), vec![]));
            }
            out.push(command(25, args[end..].to_vec(), vec![]));
        } else {
            let group = match op {
                1 | 3 | 18 | 23 | 5 => 2,
                6 | 7 => 1,
                8 => 6,
                26 | 27 | 30 | 31 => 4,
                _ => return Err(Error),
            };
            let optional = matches!(op, 26 | 27 | 30 | 31) && args.len() % 4 == 1;
            let mut done = 0;
            let mut hint_offset = 0.;
            while done < args.len() {
                let limit = if first && width { 47 } else { 48 };
                let prefix = usize::from(optional && done == 0 && matches!(op, 26 | 27));
                let remaining = args.len() - done;
                let n = if remaining <= limit {
                    remaining
                } else {
                    ((limit - prefix) / group) * group + prefix
                };
                if n == 0 {
                    return Err(Error);
                }
                let mut chunk = args[done..done + n].to_vec();
                if matches!(op, 1 | 3 | 18 | 23) {
                    chunk[0].default += hint_offset;
                    hint_offset += args[done..done + n].iter().map(|v| v.default).sum::<f64>();
                }
                out.push(command(op, chunk, vec![]));
                first = false;
                done += n;
                if matches!(op, 6 | 7) && n % 2 != 0 {
                    op = if op == 6 { 7 } else { 6 };
                }
                if matches!(op, 30 | 31) && ((n - prefix) / 4) % 2 != 0 {
                    op = if op == 30 { 31 } else { 30 };
                }
            }
        }
        if matches!(cmd.op, 19 | 20) {
            out.push(command(cmd.op, vec![], cmd.mask.clone()));
        } else if !mask.is_empty() {
            return Err(Error);
        }
        first = false;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use write_fonts::{
        read::{
            model::glyph::outline::PathElement,
            ps::{cff::CffFontRef, cs::CommandSink},
        },
        types::Fixed,
        FontBuilder,
    };
    fn font(program: Vec<u8>) -> Vec<u8> {
        let input = std::fs::read("test-data/fonts/Cantarell-VF-ABC.otf").unwrap();
        let source = FontRef::new(&input).unwrap();
        let chars = encoding::index(&[program.clone(), program], true).unwrap();
        let mut dummy = Vec::new();
        link(&mut dummy, 17, &[0]).unwrap();
        link(&mut dummy, 0x124, &[0]).unwrap();
        let start = 5 + dummy.len() + 4;
        let private = [139, 12, 17]; // LanguageGroup 0
        let mut fd = Vec::new();
        link(&mut fd, 18, &[private.len(), start]).unwrap();
        let fdarray = encoding::index(&[fd], true).unwrap();
        let char_start = start + private.len();
        let mut top = Vec::new();
        link(&mut top, 17, &[char_start]).unwrap();
        link(&mut top, 0x124, &[char_start + chars.len()]).unwrap();
        let mut cff = vec![2, 0, 5];
        cff.extend((top.len() as u16).to_be_bytes());
        cff.extend(top);
        cff.extend([0; 4]);
        cff.extend(private);
        cff.extend(chars);
        cff.extend(fdarray);
        let mut builder = FontBuilder::new();
        builder.add_raw(
            Tag::new(b"head"),
            source.data_for_tag(Tag::new(b"head")).unwrap(),
        );
        let mut hhea = source
            .data_for_tag(Tag::new(b"hhea"))
            .unwrap()
            .as_bytes()
            .to_vec();
        hhea[34..36].copy_from_slice(&2u16.to_be_bytes());
        builder.add_raw(Tag::new(b"hhea"), hhea);
        builder.add_raw(Tag::new(b"hmtx"), vec![2, 138, 0, 0, 2, 188, 0, 0]);
        builder.add_raw(Tag::new(b"maxp"), vec![0, 0, 0x50, 0, 0, 2]);
        builder.add_raw(Tag::new(b"CFF2"), cff);
        builder.build()
    }
    #[derive(Default, Debug, PartialEq)]
    struct Hints {
        stems: Vec<(bool, Fixed, Fixed)>,
        masks: Vec<(bool, Vec<u8>)>,
    }
    impl CommandSink for Hints {
        fn move_to(&mut self, _: Fixed, _: Fixed) {}
        fn line_to(&mut self, _: Fixed, _: Fixed) {}
        fn curve_to(&mut self, _: Fixed, _: Fixed, _: Fixed, _: Fixed, _: Fixed, _: Fixed) {}
        fn close(&mut self) {}
        fn hstem(&mut self, a: Fixed, b: Fixed) {
            self.stems.push((false, a, b));
        }
        fn vstem(&mut self, a: Fixed, b: Fixed) {
            self.stems.push((true, a, b));
        }
        fn hint_mask(&mut self, m: &[u8]) {
            self.masks.push((false, m.to_vec()));
        }
        fn counter_mask(&mut self, m: &[u8]) {
            self.masks.push((true, m.to_vec()));
        }
    }
    #[test]
    fn packed_operators_fit_cff1_and_preserve_geometry_hints_and_widths() {
        for (op, n) in [
            (1, 100),
            (3, 100),
            (18, 100),
            (23, 100),
            (19, 100),
            (20, 100),
            (5, 100),
            (6, 101),
            (7, 101),
            (8, 102),
            (24, 104),
            (25, 106),
            (26, 101),
            (27, 101),
            (30, 101),
            (31, 101),
        ] {
            let hint = matches!(op, 1 | 3 | 18 | 23 | 19 | 20);
            let mut program = Vec::new();
            if !hint {
                program.extend([139, 139, 21]);
            }
            for i in 0..n {
                encoding::encode(&mut program, 1. + (i % 3) as f64, false).unwrap();
            }
            encoding::emit_op(&mut program, op);
            if matches!(op, 19 | 20) {
                program.extend([0xff; 7]);
            }
            if hint {
                program.extend([139, 139, 21, 140, 140, 5]);
            }
            let data = font(program);
            let original = FontRef::new(&data).unwrap();
            let data = crate::downgrade_cff2(&original).unwrap();
            let converted = FontRef::new(&data).unwrap();
            let a = CffFontRef::new(
                original.data_for_tag(Tag::new(b"CFF2")).unwrap().as_bytes(),
                0,
                None,
            )
            .unwrap();
            let b = CffFontRef::new(
                converted
                    .data_for_tag(Tag::new(b"CFF "))
                    .unwrap()
                    .as_bytes(),
                0,
                None,
            )
            .unwrap();
            for (gid, width) in [(0, 650), (1, 700)] {
                let gid = GlyphId::new(gid);
                let sa = a.subfont(0, &[]).unwrap();
                let sb = b.subfont(0, &[]).unwrap();
                let mut pa = Vec::<PathElement>::new();
                let mut pb = Vec::<PathElement>::new();
                a.draw(&sa, gid, &[], None, &mut pa).unwrap();
                b.draw(&sb, gid, &[], None, &mut pb).unwrap();
                assert_eq!(pa, pb, "op={op}");
                let mut ha = Hints::default();
                let mut hb = Hints::default();
                a.evaluate_charstring(&sa, gid, &[], &mut ha).unwrap();
                b.evaluate_charstring(&sb, gid, &[], &mut hb).unwrap();
                assert_eq!(ha, hb, "op={op}");
                assert_eq!(
                    b.evaluate_width(&sb, gid, &[]).unwrap().unwrap().to_i32(),
                    width
                );
            }
        }
    }
}
