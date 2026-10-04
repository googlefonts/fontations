//! Regression tests for extended contextual glyph and lookup closure.

use super::*;
use crate::FontData;
use font_test_data::bebuffer::BeBuffer;
use font_types::{GlyphId24, Uint24};

fn context(chain: bool, records: &[(u16, u16)]) -> Vec<u8> {
    let mut bytes = BeBuffer::new()
        .push(4u16)
        .push(12u32)
        .push(Uint24::new(1))
        .push(Uint24::new(20))
        .push(3u16)
        .push(Uint24::new(1))
        .push(GlyphId24::new(65536))
        .push(1u16);
    if chain {
        bytes = bytes
            .push(Uint24::new(5))
            .push(1u16)
            .push(GlyphId24::new(65535))
            .push(2u16)
            .push(GlyphId24::new(70000))
            .push(1u16)
            .push(GlyphId24::new(0xffffff))
            .push(records.len() as u16);
    } else {
        bytes = bytes
            .push(4u16)
            .push(2u16)
            .push(records.len() as u16)
            .push(GlyphId24::new(70000));
    }
    for &(sequence, lookup) in records {
        bytes = bytes.push(sequence).push(lookup);
    }
    bytes.to_vec()
}

fn font(kind: u16, context: &[u8]) -> Vec<u8> {
    let single = BeBuffer::new()
        .push(4u16)
        .push(15u32)
        .push(Uint24::new(2))
        .extend([70001, 70002].map(GlyphId24::new))
        .push(3u16)
        .push(Uint24::new(2))
        .extend([65536, 70000].map(GlyphId24::new));
    layout_font(kind, context, &single)
}

fn layout_font(kind: u16, context: &[u8], nested: &[u8]) -> Vec<u8> {
    BeBuffer::new()
        .push(1u16)
        .push(2u16)
        .extend([0u16; 3])
        .extend([0u32; 3])
        .push(26u32)
        .push(2u16)
        .push(10u32)
        .push(18u32 + context.len() as u32)
        .push(kind)
        .push(0u16)
        .push(1u16)
        .push(8u16)
        .extend(context.iter().copied())
        .push(1u16)
        .push(0u16)
        .push(1u16)
        .push(8u16)
        .extend(nested.iter().copied())
        .to_vec()
}

fn close(kind: u16, context: &[u8], inputs: &[u32]) -> Result<(Vec<u32>, Vec<u16>), ReadError> {
    let bytes = font(kind, context);
    let gsub = Gsub::read(FontData::new(&bytes)).unwrap();
    let mut glyphs = inputs.iter().copied().map(GlyphId::new).collect();
    let mut lookups = [0].into_iter().collect();
    gsub.closure_lookups(&glyphs, &mut lookups)?;
    // Only the contextual lookup is a feature root. Nested lookups must keep
    // the context's active-glyph restriction, rather than run independently.
    gsub.closure_glyphs(&[0].into_iter().collect(), &mut glyphs)?;
    Ok((
        glyphs.iter().map(GlyphId::to_u32).collect(),
        lookups.iter().collect(),
    ))
}

#[test]
fn wide_glyph_context_closure_restricts_nested_lookup_to_its_sequence_position() {
    for (chain, kind, inputs) in [
        (false, 5, vec![65536, 70000]),
        (true, 6, vec![65535, 65536, 70000, 0xffffff]),
    ] {
        for (records, outputs) in [
            (vec![(0, 1)], vec![70001]),
            (vec![(1, 1)], vec![70002]),
            (vec![(1, 1), (1, 1)], vec![70001, 70002]),
            (vec![(2, 1)], vec![]),
        ] {
            let (glyphs, lookups) = close(kind, &context(chain, &records), &inputs).unwrap();
            let mut expected = inputs.clone();
            expected.extend(outputs);
            expected.sort_unstable();
            assert_eq!(glyphs, expected);
            assert_eq!(lookups, [0, 1]);
        }
    }
}

#[test]
fn wide_glyph_context_closure_requires_every_context_glyph() {
    for (chain, kind, inputs) in [
        (false, 5, vec![65536, 70000]),
        (true, 6, vec![65535, 65536, 70000, 0xffffff]),
    ] {
        for excluded in &inputs {
            let inputs: Vec<_> = inputs
                .iter()
                .copied()
                .filter(|gid| gid != excluded)
                .collect();
            let (glyphs, lookups) = close(kind, &context(chain, &[(1, 1)]), &inputs).unwrap();
            assert_eq!(glyphs, inputs);
            assert!(lookups.is_empty());
        }
    }
}

#[test]
fn wide_glyph_context_closure_handles_null_invalid_offsets_and_cycles() {
    for (chain, kind) in [(false, 5), (true, 6)] {
        let inputs = [65535, 65536, 70000, 0xffffff];
        let base = context(chain, &[(1, 1)]);
        for range in [2..6, 9..12, if chain { 22..25 } else { 22..24 }] {
            let mut bytes = base.clone();
            bytes[range.clone()].fill(0);
            let (glyphs, lookups) = close(kind, &bytes, &inputs).unwrap();
            assert_eq!(glyphs, inputs);
            assert!(lookups.is_empty());
            bytes[range].fill(0xff);
            assert!(close(kind, &bytes, &inputs).is_err());
        }
        let (glyphs, lookups) = close(kind, &context(chain, &[(1, 0)]), &inputs).unwrap();
        assert_eq!(glyphs, inputs);
        assert_eq!(lookups, [0]);
    }
}

#[test]
fn wide_glyph_context_closure_keeps_large_set_indices_and_offsets() {
    let count = 65537u32;
    for (chain, kind) in [(false, 5), (true, 6)] {
        let base = context(chain, &[(1, 1)]);
        let coverage_offset = 9 + count * 3;
        let set_offset = coverage_offset + 14;
        let bytes = BeBuffer::new()
            .push(4u16)
            .push(coverage_offset)
            .push(Uint24::new(count))
            .extend(
                (0..count).map(|index| Uint24::new(if index == 65536 { set_offset } else { 0 })),
            )
            .push(4u16)
            .push(Uint24::new(1))
            .extend([0, 65536, 0].map(Uint24::new))
            .extend(base[20..].iter().copied());
        let (glyphs, lookups) = close(kind, &bytes, &[65535, 65536, 70000, 0xffffff]).unwrap();
        assert_eq!(glyphs, [65535, 65536, 70000, 70002, 0xffffff]);
        assert_eq!(lookups, [0, 1]);
    }
}

#[test]
fn wide_glyph_context_lookup_closure_also_handles_gpos() {
    use crate::tables::gpos::Gpos;
    let single = BeBuffer::new()
        .push(1u16)
        .push(8u16)
        .push(4u16)
        .push(10i16)
        .push(3u16)
        .push(Uint24::new(2))
        .extend([65536, 70000].map(GlyphId24::new));
    for (chain, kind, inputs) in [
        (false, 7, vec![65536, 70000]),
        (true, 8, vec![65535, 65536, 70000, 0xffffff]),
    ] {
        let bytes = layout_font(kind, &context(chain, &[(1, 1)]), &single);
        let gpos = Gpos::read(FontData::new(&bytes)).unwrap();
        let glyphs = inputs.iter().copied().map(GlyphId::new).collect();
        let mut lookups = [0].into_iter().collect();
        gpos.closure_lookups(&glyphs, &mut lookups).unwrap();
        assert_eq!(lookups.iter().collect::<Vec<_>>(), [0, 1]);
        for excluded in &inputs {
            let glyphs = inputs
                .iter()
                .copied()
                .filter(|gid| gid != excluded)
                .map(GlyphId::new)
                .collect();
            let mut lookups = [0].into_iter().collect();
            gpos.closure_lookups(&glyphs, &mut lookups).unwrap();
            assert!(lookups.is_empty());
        }
    }
}
