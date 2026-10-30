//! Shared subset planning and format-specific table assembly.

use super::{
    charstring::{self, CharStringActions, Flattener, Interpreter, Program, Recorder, Value},
    dict::{self, Entry},
    encoding,
    source::{CffFlavor, Source},
    Error, Result,
};
use crate::{FastHashMap, Plan, SubsetFlags};
use read_fonts::{collections::IntSet, ps::PredefinedEncoding, types::GlyphId};
use std::collections::{BTreeMap, BTreeSet};

fn gid_list(plan: &Plan) -> Vec<usize> {
    (0..plan.num_output_glyphs)
        .map(|i| {
            plan.reverse_glyph_map
                .get(&GlyphId::new(i as u32))
                .map_or(i, |g| g.to_u32() as usize)
        })
        .collect()
}

pub(super) fn closure(source: &Source, glyphs: &mut IntSet<GlyphId>) -> Result<()> {
    if source.cid || source.font.version() == 2 {
        return Ok(());
    }
    let charset = source.font.charset().ok_or(Error)?;
    let mut pending: Vec<_> = glyphs.iter().collect();
    let mut visited = BTreeSet::new();
    while let Some(gid) = pending.pop() {
        if !visited.insert(gid.to_u32()) {
            continue;
        }
        let fd = source.fd(gid)?;
        let data = source
            .font
            .charstrings()
            .get(gid.to_u32() as usize)
            .ok_or(Error)?;
        if !super::charstring_may_end_in_seac(data) {
            continue;
        }
        let mut rec = ();
        let mut interp =
            Interpreter::<super::source::Cff1, _, _>::new(source, &mut rec, fd, 0, false);
        interp.run(Program::Glyph(gid.to_u32()), data)?;
        if let Some((base, accent)) = interp.seac {
            for code in [base, accent] {
                let sid = PredefinedEncoding::Standard.sid(code).ok_or(Error)?;
                let component = charset.glyph_id(sid).ok_or(Error)?;
                if glyphs.insert(component) {
                    pending.push(component);
                }
            }
        }
    }
    Ok(())
}

pub(super) struct Programs {
    pub chars: Vec<Vec<u8>>,
    pub globals: Vec<Vec<u8>>,
    pub locals: Vec<Vec<Vec<u8>>>,
}

fn copy_index(index: &read_fonts::ps::cff::index::Index<'_>) -> Result<Vec<Vec<u8>>> {
    (0..index.count() as usize)
        .map(|i| index.get(i).map(<[u8]>::to_vec).ok_or(Error))
        .collect()
}

fn can_copy_identity_programs(source: &Source, plan: &Plan) -> bool {
    let flags = plan.subset_flags;
    plan.has_identity_glyph_map()
        && flags.contains(SubsetFlags::SUBSET_FLAGS_NOTDEF_OUTLINE)
        && !flags.contains(SubsetFlags::SUBSET_FLAGS_DESUBROUTINIZE)
        && !flags.contains(SubsetFlags::SUBSET_FLAGS_NO_HINTING)
        && source.font.charstrings().count() as usize == plan.num_output_glyphs
}

fn copy_identity_programs(source: &Source) -> Result<Programs> {
    Ok(Programs {
        chars: copy_index(source.font.charstrings())?,
        globals: copy_index(source.font.global_subrs())?,
        locals: source
            .fds
            .iter()
            .map(|fd| copy_index(&fd.subrs))
            .collect::<Result<_>>()?,
    })
}

struct IdentityProgramCollector<'a, 's> {
    source: &'a Source<'s>,
    globals: IntSet<u32>,
    locals: Vec<IntSet<u32>>,
    copyable: bool,
}

impl<'s> IdentityProgramCollector<'_, 's> {
    fn source_program(&self, key: Program) -> Option<&'s [u8]> {
        match key {
            Program::Glyph(index) => self.source.font.charstrings().get(index as usize),
            Program::Local(fd, index) => {
                self.source.fds.get(fd as usize)?.subrs.get(index as usize)
            }
            Program::Global(_, index) => self.source.font.global_subrs().get(index as usize),
        }
    }
}

impl<'s> CharStringActions<'s> for IdentityProgramCollector<'_, 's> {
    const MATERIALIZE_BLENDS: bool = false;

    fn program(&mut self, key: Program, _data: &'s [u8]) {
        match key {
            Program::Global(_, index) => {
                self.globals.insert(index);
            }
            Program::Local(fd, index) => {
                if let Some(locals) = self.locals.get_mut(fd as usize) {
                    locals.insert(index);
                } else {
                    self.copyable = false;
                }
            }
            Program::Glyph(_) => {}
        }
    }

    fn call(
        &mut self,
        key: Program,
        start: usize,
        end: usize,
        target: Program,
        operand: &Value,
    ) -> Result<()> {
        let Some(origin) = operand.origin() else {
            self.copyable = false;
            return Ok(());
        };
        let (op, count, index) = match target {
            Program::Local(fd, index) => (
                10,
                self.source.fds.get(fd as usize).ok_or(Error)?.subrs.count() as usize,
                index,
            ),
            Program::Global(_, index) => {
                (29, self.source.font.global_subrs().count() as usize, index)
            }
            Program::Glyph(_) => return Err(Error),
        };
        let (expected, expected_len) = encoding::integer_bytes(
            i32::try_from(index).map_err(|_| Error)? - encoding::bias(count),
        );
        let span = origin.span();
        self.copyable &= origin.program == key
            && span.end == start
            && self
                .source_program(key)
                .and_then(|data| data.get(span.start..end))
                .is_some_and(|bytes| {
                    bytes.len() == expected_len + 1
                        && bytes[..expected_len] == expected[..expected_len]
                        && bytes[expected_len] == op
                });
        Ok(())
    }
}

fn all_source_programs_reachable<F: CffFlavor>(source: &Source, plan: &Plan) -> Result<bool> {
    let mut collector = IdentityProgramCollector {
        source,
        globals: IntSet::empty(),
        locals: vec![IntSet::empty(); source.fds.len()],
        copyable: true,
    };
    for &(_, old) in &plan.new_to_old_gid_list {
        let orig = old.to_u32();
        let data = source.font.charstrings().get(orig as usize).ok_or(Error)?;
        let fd = source.fd(old)?;
        let mut interpreter =
            Interpreter::<F, _, _>::new(source, &mut collector, fd, source.fds[fd].ivs, false);
        interpreter.run(Program::Glyph(orig), data)?;
    }
    Ok(collector.copyable
        && collector.globals.len() == source.font.global_subrs().count() as u64
        && collector
            .locals
            .iter()
            .zip(&source.fds)
            .all(|(used, fd)| used.len() == fd.subrs.count() as u64))
}

fn remap_order(program: Program) -> (u8, u32, u32) {
    match program {
        Program::Glyph(index) => (0, index, 0),
        Program::Local(fd, index) => (1, fd, index),
        // HarfBuzz assigns global subroutine numbers in source order. The FD
        // is only internal context for globals whose local calls depend on it.
        Program::Global(fd, index) => (2, index, fd),
    }
}

pub(super) fn programs<F: CffFlavor>(source: &Source, plan: &Plan) -> Result<Programs> {
    if can_copy_identity_programs(source, plan) && all_source_programs_reachable::<F>(source, plan)?
    {
        return copy_identity_programs(source);
    }
    let desub = plan
        .subset_flags
        .contains(SubsetFlags::SUBSET_FLAGS_DESUBROUTINIZE);
    let nohint = plan
        .subset_flags
        .contains(SubsetFlags::SUBSET_FLAGS_NO_HINTING);
    // Most selected glyphs reach a small number of subroutines. Use a bounded
    // estimate and let the map grow rather than reserving for every source
    // subroutine in every FD. Desubroutinization does not use a recorder.
    const MAX_INITIAL_PROGRAM_CAPACITY: usize = 4096;
    let mut rec = (!desub).then(|| {
        Recorder::with_capacity(
            plan.new_to_old_gid_list
                .len()
                .saturating_mul(4)
                .min(MAX_INITIAL_PROGRAM_CAPACITY),
        )
    });
    let mut widths = BTreeMap::new();
    let mut roots = Vec::new();
    let mut chars = vec![F::empty_glyph(); plan.num_output_glyphs];
    for &(new, old) in &plan.new_to_old_gid_list {
        let gid = new.to_u32() as usize;
        let orig = old.to_u32();
        if gid == 0
            && !plan
                .subset_flags
                .contains(SubsetFlags::SUBSET_FLAGS_NOTDEF_OUTLINE)
        {
            continue;
        }
        let fd = source.fd(old)?;
        let data = source.font.charstrings().get(orig as usize).unwrap_or(&[]);
        let key = Program::Glyph(orig);
        if desub {
            chars[gid] = flatten_glyph::<F>(source, key, fd, data, nohint)?;
        } else {
            let mut glyph = Recorder::with_capacity(16);
            let mut interpreter =
                Interpreter::<F, _, _>::new(source, &mut glyph, fd, source.fds[fd].ivs, nohint);
            interpreter.run(key, data)?;
            let width = interpreter.width;
            let rec = rec.as_mut().ok_or(Error)?;
            if !rec.merge(glyph) {
                chars[gid] = flatten_glyph::<F>(source, key, fd, data, nohint)?;
            } else {
                roots.push(key);
                widths.insert(orig, width);
            }
        }
    }
    let mut globals = Vec::new();
    let mut locals = vec![Vec::new(); source.fds.len()];
    if let Some(mut rec) = rec {
        if nohint {
            rec.remove_hint_calls();
        }
        let mut used = rec.closure(&roots)?;
        used.sort_unstable_by_key(|program| remap_order(*program));
        let mut remap = FastHashMap::default();
        let mut shared_globals: BTreeMap<u32, Vec<Program>> = BTreeMap::new();
        for key in &used {
            match *key {
                Program::Global(_, index) => {
                    if rec.fd_dependent(*key) {
                        remap.insert(*key, globals.len());
                        globals.push(vec![]);
                        continue;
                    }
                    let candidates = shared_globals.entry(index).or_default();
                    if let Some(shared) = candidates
                        .iter()
                        .find(|&&other| rec.share_global(*key, other))
                    {
                        remap.insert(*key, remap[shared]);
                    } else {
                        candidates.push(*key);
                        remap.insert(*key, globals.len());
                        globals.push(vec![]);
                    }
                }
                Program::Local(fd, _) => {
                    remap.insert(*key, locals[fd as usize].len());
                    locals[fd as usize].push(vec![]);
                }
                _ => (),
            }
        }
        let counts: Vec<_> = locals.iter().map(Vec::len).collect();
        let mut written_globals = vec![false; globals.len()];
        for (&key, &new) in &remap {
            if matches!(key, Program::Global(..)) {
                if written_globals[new] {
                    continue;
                }
                written_globals[new] = true;
            }
            let bytes = rec.encode(key, &remap, globals.len(), &counts)?;
            match key {
                Program::Global(_, _) => globals[new] = bytes,
                Program::Local(fd, _) => locals[fd as usize][new] = bytes,
                _ => (),
            }
        }
        for &(new, old) in &plan.new_to_old_gid_list {
            let orig = old.to_u32();
            if !widths.contains_key(&orig) {
                continue;
            }
            let mut out = Vec::new();
            if nohint {
                if let Some(width) = widths[&orig] {
                    encoding::encode(&mut out, width, false)?;
                }
            }
            out.extend(rec.encode(Program::Glyph(orig), &remap, globals.len(), &counts)?);
            chars[new.to_u32() as usize] = out;
        }
    }
    Ok(Programs {
        chars,
        globals,
        locals,
    })
}

fn flatten_glyph<F: CffFlavor>(
    source: &Source,
    key: Program,
    fd: usize,
    data: &[u8],
    nohint: bool,
) -> Result<Vec<u8>> {
    let mut actions = Flattener::default();
    let mut interpreter =
        Interpreter::<F, _, _>::new(source, &mut actions, fd, source.fds[fd].ivs, nohint);
    interpreter.run(key, data)?;
    let width = interpreter.width;
    charstring::flatten(&actions.commands, width, F::CFF2, source.fds[fd].ivs)
}

fn sid_args(op: u16) -> usize {
    if matches!(op, 0..=4 | 0x100 | 0x115 | 0x116 | 0x126) {
        1
    } else if op == 0x11e {
        2
    } else {
        0
    }
}
fn map_sids<'a>(
    entries: &[Entry<'a>],
    map: &mut BTreeMap<usize, usize>,
    strings: &mut Vec<Vec<u8>>,
    source: &Source,
) -> Result<Vec<Entry<'a>>> {
    let mut out = entries.to_vec();
    for e in &mut out {
        let count = sid_args(e.op);
        if count == 0 {
            continue;
        }
        for i in 0..count {
            let sid = dict::uint(*e.args.get(i).ok_or(Error)?)?;
            let new = remap_sid(sid, map, strings, source)?;
            e.args[i] = new as f64;
        }
        let mut raw = Vec::new();
        for &v in &e.args {
            encoding::encode(&mut raw, v, true)?;
        }
        encoding::emit_op(&mut raw, e.op);
        e.raw = raw.into();
    }
    Ok(out)
}
fn remap_sid(
    sid: usize,
    map: &mut BTreeMap<usize, usize>,
    strings: &mut Vec<Vec<u8>>,
    source: &Source,
) -> Result<usize> {
    if sid < 391 {
        return Ok(sid);
    }
    if let Some(&new) = map.get(&sid) {
        return Ok(new);
    }
    let new = strings
        .len()
        .checked_add(391)
        .filter(|&i| i <= 65535)
        .ok_or(Error)?;
    strings.push(source.strings.get(sid - 391).ok_or(Error)?.to_vec());
    map.insert(sid, new);
    Ok(new)
}

fn charset(codes: &[usize]) -> Result<Vec<u8>> {
    let mut raw = vec![0];
    for &sid in codes.iter().skip(1) {
        raw.extend(u16::try_from(sid).map_err(|_| Error)?.to_be_bytes());
    }
    for format in [1, 2] {
        let mut ranges = vec![format];
        let mut i = 1;
        while i < codes.len() {
            let start = i;
            let max = if format == 1 { 255 } else { 65535 };
            while i + 1 < codes.len() && codes[i + 1] == codes[i] + 1 && i - start < max {
                i += 1;
            }
            ranges.extend(
                u16::try_from(codes[start])
                    .map_err(|_| Error)?
                    .to_be_bytes(),
            );
            if format == 1 {
                ranges.push((i - start) as u8);
            } else {
                ranges.extend(((i - start) as u16).to_be_bytes());
            }
            i += 1;
        }
        if ranges.len() < raw.len() {
            raw = ranges;
        }
    }
    Ok(raw)
}

fn encoding(source: &Source, old_gids: &[usize], codes: &[usize]) -> Result<Option<Vec<u8>>> {
    let offset = dict::offset(&source.top, 16, 0)?.unwrap_or(0);
    if offset <= 1 {
        return Ok(None);
    }
    let enc = source.font.encoding().ok_or(Error)?;
    let mut by_gid: BTreeMap<usize, Vec<u8>> = BTreeMap::new();
    for code in 0..=255 {
        if let Some(gid) = enc.map(code) {
            by_gid.entry(gid.to_u32() as usize).or_default().push(code);
        }
    }
    let mut primary = Vec::new();
    let mut supplements = Vec::new();
    for (gid, &old) in old_gids.iter().enumerate().skip(1) {
        let Some(chars) = by_gid.get(&old) else {
            continue;
        };
        let first = if primary.len() == gid - 1 && primary.len() < 255 {
            primary.push(chars[0]);
            1
        } else {
            0
        };
        for &code in &chars[first..] {
            supplements.push((code, codes[gid]));
        }
    }
    let mut out = vec![
        if supplements.is_empty() { 0 } else { 128 },
        primary.len() as u8,
    ];
    out.extend(primary);
    if !supplements.is_empty() {
        out.push(u8::try_from(supplements.len()).map_err(|_| Error)?);
        for (code, sid) in supplements {
            out.push(code);
            out.extend(u16::try_from(sid).map_err(|_| Error)?.to_be_bytes());
        }
    }
    Ok(Some(out))
}

fn fdselect(fds: &[usize], cff2: bool) -> Result<Vec<u8>> {
    let mut ranges = Vec::new();
    for (i, &fd) in fds.iter().enumerate() {
        if i == 0 || fd != fds[i - 1] {
            ranges.push((i, fd));
        }
    }
    if fds.iter().any(|&fd| fd > 255) || fds.len() > 65535 || ranges.len() > 65535 {
        if !cff2 {
            return Err(Error);
        }
        let mut out = vec![4];
        out.extend(
            u32::try_from(ranges.len())
                .map_err(|_| Error)?
                .to_be_bytes(),
        );
        for (gid, fd) in ranges {
            out.extend(u32::try_from(gid).map_err(|_| Error)?.to_be_bytes());
            out.extend(u16::try_from(fd).map_err(|_| Error)?.to_be_bytes());
        }
        out.extend(u32::try_from(fds.len()).map_err(|_| Error)?.to_be_bytes());
        return Ok(out);
    }
    // HarfBuzz never emits format 0 (CFF_SERIALIZE_FDSELECT_0 is disabled), even
    // when it would be smaller, so always use format 3 to match its output.
    let mut out = vec![3];
    out.extend((ranges.len() as u16).to_be_bytes());
    for (gid, fd) in ranges {
        out.extend((gid as u16).to_be_bytes());
        out.push(fd as u8);
    }
    out.extend((fds.len() as u16).to_be_bytes());
    Ok(out)
}

fn rewrite(entries: &[Entry], replacements: &[(u16, Option<Vec<u8>>)]) -> Vec<u8> {
    let mut out = Vec::new();
    for e in entries {
        if !replacements.iter().any(|r| r.0 == e.op) {
            out.extend(e.raw.as_ref());
        }
    }
    for (_, bytes) in replacements {
        if let Some(bytes) = bytes {
            out.extend(bytes);
        }
    }
    out
}

pub(super) fn assemble<F: CffFlavor>(
    source: &Source,
    plan: &Plan,
    programs: Programs,
) -> Result<Vec<u8>> {
    let nohint = plan
        .subset_flags
        .contains(SubsetFlags::SUBSET_FLAGS_NO_HINTING);
    let old_gids = gid_list(plan);
    let mut fd_map = BTreeMap::new();
    let old_fds = old_gids
        .iter()
        .map(|&g| source.fd(GlyphId::new(g as u32)))
        .collect::<Result<Vec<_>>>()?;
    for &fd in &old_fds {
        fd_map.insert(fd, 0);
    }
    for (new, (_, idx)) in fd_map.iter_mut().enumerate() {
        *idx = new;
    }
    let fds: Vec<_> = old_fds.iter().map(|fd| fd_map[fd]).collect();
    let mut strings = Vec::new();
    let mut sid_map = BTreeMap::new();
    let mut codes = vec![0];
    if !F::CFF2 {
        let cs = source.font.charset().ok_or(Error)?;
        // Range-based charsets require a linear scan for each random lookup.
        // Decode them once because old_gids can be in arbitrary output order.
        let source_codes: Vec<_> = cs.iter().map(|(_, sid)| sid.to_u16() as usize).collect();
        for &old in old_gids.iter().skip(1) {
            let code = *source_codes.get(old).ok_or(Error)?;
            codes.push(if source.cid {
                code
            } else {
                remap_sid(code, &mut sid_map, &mut strings, source)?
            });
        }
    }
    let top = if F::CFF2 {
        source.top.clone()
    } else {
        map_sids(&source.top, &mut sid_map, &mut strings, source)?
    };
    let font_dicts = fd_map
        .keys()
        .map(|&fd| map_sids(&source.fds[fd].entries, &mut sid_map, &mut strings, source))
        .collect::<Result<Vec<_>>>()?;
    let name_index = encoding::IndexLayout::new(&source.names, false)?;
    let string_index = encoding::IndexLayout::new(&strings, false)?;
    let global_index = encoding::IndexLayout::new(&programs.globals, F::CFF2)?;
    let char_index = encoding::IndexLayout::new(&programs.chars, F::CFF2)?;
    // FDSelect is optional in CFF2 when there is a single Font DICT. Like
    // HarfBuzz, only emit it if the source has one. CFF1 CID fonts require it.
    let has_fdselect = !F::CFF2 || dict::offset(&source.top, 0x125, 0)?.is_some();
    let select = if has_fdselect {
        fdselect(&fds, F::CFF2)?
    } else {
        Vec::new()
    };
    let charset = if F::CFF2 { vec![] } else { charset(&codes)? };
    let enc = if F::CFF2 || source.cid {
        None
    } else {
        encoding(source, &old_gids, &codes)?
    };
    let mut privates = Vec::new();
    let mut private_sizes = Vec::new();
    for &fd in fd_map.keys() {
        let entries: Vec<_> = source.fds[fd]
            .private
            .iter()
            .filter(|e| !nohint || !dict::is_hint(e.op))
            .cloned()
            .collect();
        let has_subrs = !programs.locals[fd].is_empty();
        let base = rewrite(&entries, &[(19, None)]);
        let mut bytes = base;
        if has_subrs {
            let size = bytes.len().checked_add(6).ok_or(Error)?;
            bytes.extend(dict::replacement(19, &[size])?);
        }
        private_sizes.push(bytes.len());
        if has_subrs {
            encoding::IndexLayout::new(&programs.locals[fd], F::CFF2)?
                .write(&mut bytes, &programs.locals[fd]);
        }
        privates.push(bytes);
    }
    let var_store = if F::CFF2 {
        dict::offset(&top, 24, 0)?
            .map(|offset| {
                let data = source.font.data();
                let size = u16::from_be_bytes(
                    data.get(offset..offset + 2)
                        .ok_or(Error)?
                        .try_into()
                        .map_err(|_| Error)?,
                ) as usize;
                data.get(offset..offset.checked_add(size + 2).ok_or(Error)?)
                    .ok_or(Error)
            })
            .transpose()?
    } else {
        None
    };
    // Fixed-width DICT links make the second pass independent of offset magnitudes.
    let make_dicts = |base: usize| -> Result<(Vec<u8>, Vec<u8>, usize)> {
        let mut offsets = Vec::new();
        let mut pos = base;
        for private in &privates {
            offsets.push(pos);
            pos = pos.checked_add(private.len()).ok_or(Error)?;
        }
        let cs = pos;
        pos = pos.checked_add(char_index.size).ok_or(Error)?;
        let fdselect_pos = pos;
        pos = pos.checked_add(select.len()).ok_or(Error)?;
        let charset_pos = pos;
        pos = pos.checked_add(charset.len()).ok_or(Error)?;
        let enc_pos = pos;
        pos = pos
            .checked_add(enc.as_ref().map_or(0, Vec::len))
            .ok_or(Error)?;
        let vstore_pos = pos;
        pos = pos
            .checked_add(var_store.as_ref().map_or(0, |v| v.len()))
            .ok_or(Error)?;
        let fdarray_pos = pos;
        let mut fdarray = Vec::new();
        for ((_, &new), entries) in fd_map.iter().zip(&font_dicts) {
            fdarray.push(rewrite(
                entries,
                &[(
                    18,
                    Some(dict::replacement(18, &[private_sizes[new], offsets[new]])?),
                )],
            ));
        }
        let fdarray = if F::CFF2 || source.cid {
            encoding::index(&fdarray, F::CFF2)?
        } else {
            vec![]
        };
        let mut replacements = vec![(17, Some(dict::replacement(17, &[cs])?))];
        if F::CFF2 || source.cid {
            replacements.extend([
                (0x124, Some(dict::replacement(0x124, &[fdarray_pos])?)),
                (
                    0x125,
                    has_fdselect
                        .then(|| dict::replacement(0x125, &[fdselect_pos]))
                        .transpose()?,
                ),
            ]);
        } else {
            replacements.push((
                18,
                Some(dict::replacement(18, &[private_sizes[0], offsets[0]])?),
            ));
        }
        if !F::CFF2 {
            replacements.push((15, Some(dict::replacement(15, &[charset_pos])?)));
            if enc.is_some() {
                replacements.push((16, Some(dict::replacement(16, &[enc_pos])?)));
            }
        }
        if var_store.is_some() {
            replacements.push((24, Some(dict::replacement(24, &[vstore_pos])?)));
        }
        let size = pos.checked_add(fdarray.len()).ok_or(Error)?;
        Ok((rewrite(&top, &replacements), fdarray, size))
    };
    let (dummy, _, _) = make_dicts(0)?;
    let base = if F::CFF2 {
        5 + dummy.len() + global_index.size
    } else {
        4 + name_index.size
            + encoding::index(&[dummy], false)?.len()
            + string_index.size
            + global_index.size
    };
    let (top, fdarray, size) = make_dicts(base)?;
    let mut out = Vec::with_capacity(size);
    if F::CFF2 {
        out.extend([2, 0, 5]);
        out.extend(u16::try_from(top.len()).map_err(|_| Error)?.to_be_bytes());
        out.extend(top);
    } else {
        out.extend([1, 0, 4, 4]);
        name_index.write(&mut out, &source.names);
        out.extend(encoding::index(&[top], false)?);
        string_index.write(&mut out, &strings);
    }
    global_index.write(&mut out, &programs.globals);
    for bytes in privates {
        out.extend(bytes);
    }
    char_index.write(&mut out, &programs.chars);
    out.extend(select);
    out.extend(charset);
    if let Some(enc) = enc {
        out.extend(enc);
    }
    if let Some(v) = var_store {
        out.extend(v);
    }
    out.extend(fdarray);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::{
        all_source_programs_reachable, can_copy_identity_programs, copy_identity_programs,
        remap_order, Program,
    };
    use crate::{
        cff::source::{Cff1, Source},
        Plan, SubsetFlags,
    };
    use read_fonts::{types::GlyphId, FontRef, TableProvider};

    fn identity_plan(glyph_count: u32) -> Plan {
        let mut plan = Plan {
            font_num_glyphs: glyph_count as usize,
            num_output_glyphs: glyph_count as usize,
            subset_flags: SubsetFlags::SUBSET_FLAGS_NOTDEF_OUTLINE,
            ..Default::default()
        };
        plan.new_to_old_gid_list = (0..glyph_count)
            .map(|gid| {
                let gid = GlyphId::new(gid);
                (gid, gid)
            })
            .collect();
        plan
    }

    #[test]
    fn global_remapping_is_source_index_major() {
        let mut programs = [
            Program::Global(0, 8),
            Program::Global(3, 2),
            Program::Global(1, 2),
        ];
        programs.sort_unstable_by_key(|program| remap_order(*program));
        assert_eq!(
            programs,
            [
                Program::Global(1, 2),
                Program::Global(3, 2),
                Program::Global(0, 8),
            ]
        );
    }

    #[test]
    fn identity_program_copy_is_limited_to_unmodified_full_subsets() {
        for (font_bytes, tag) in [
            (
                include_bytes!("../../test-data/fonts/SourceSansPro-Regular.otf").as_slice(),
                read_fonts::types::Tag::new(b"CFF "),
            ),
            (
                include_bytes!("../../test-data/fonts/AdobeVFPrototype.otf").as_slice(),
                read_fonts::types::Tag::new(b"CFF2"),
            ),
        ] {
            let font = FontRef::new(font_bytes).unwrap();
            let source = Source::new(font.data_for_tag(tag).unwrap().as_bytes()).unwrap();
            let mut plan = identity_plan(source.font.num_glyphs());

            assert!(can_copy_identity_programs(&source, &plan));
            let copied = copy_identity_programs(&source).unwrap();
            assert_eq!(
                copied.chars.len(),
                source.font.charstrings().count() as usize
            );
            assert_eq!(
                copied.globals.len(),
                source.font.global_subrs().count() as usize
            );
            assert_eq!(copied.locals.len(), source.fds.len());

            plan.subset_flags |= SubsetFlags::SUBSET_FLAGS_NO_HINTING;
            assert!(!can_copy_identity_programs(&source, &plan));
            plan.subset_flags =
                SubsetFlags::SUBSET_FLAGS_NOTDEF_OUTLINE | SubsetFlags::SUBSET_FLAGS_DESUBROUTINIZE;
            assert!(!can_copy_identity_programs(&source, &plan));
            plan.subset_flags = SubsetFlags::default();
            assert!(!can_copy_identity_programs(&source, &plan));
        }
    }

    #[test]
    fn identity_program_copy_supports_multiple_font_dicts() {
        let font = FontRef::new(include_bytes!(
            "../../test-data/fonts/SourceHanSans-Regular_subset.otf"
        ))
        .unwrap();
        let source = Source::new(font.cff().unwrap().offset_data().as_bytes()).unwrap();
        assert!(source.fds.len() > 1);

        let plan = identity_plan(source.font.num_glyphs());
        assert!(all_source_programs_reachable::<Cff1>(&source, &plan).unwrap());
    }
}
