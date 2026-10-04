//! cmap test data for scenarios not readily produced with ttx

use crate::{be_buffer, bebuffer::BeBuffer};

/// Builds a minimal SFNT with optional cmap and DMAP tables.
pub fn font_with_cmaps(cmap: Option<&[u8]>, dmap: Option<&[u8]>) -> Vec<u8> {
    let maxp = [0, 0, 0x50, 0, 0xff, 0xff];
    let mut tables = vec![(*b"maxp", maxp.as_slice())];
    if let Some(cmap) = cmap {
        tables.push((*b"cmap", cmap));
    }
    if let Some(dmap) = dmap {
        tables.push((*b"DMAP", dmap));
    }
    tables.sort_by_key(|(tag, _)| *tag);
    let mut data = vec![0; 12 + 16 * tables.len()];
    data[..4].copy_from_slice(&0x00010000u32.to_be_bytes());
    data[4..6].copy_from_slice(&(tables.len() as u16).to_be_bytes());
    for (i, (tag, table)) in tables.into_iter().enumerate() {
        let record = 12 + i * 16;
        let offset = data.len() as u32;
        data[record..record + 4].copy_from_slice(&tag);
        data[record + 8..record + 12].copy_from_slice(&offset.to_be_bytes());
        data[record + 12..record + 16].copy_from_slice(&(table.len() as u32).to_be_bytes());
        data.extend_from_slice(table);
        data.resize((data.len() + 3) & !3, 0);
    }
    data
}

/// Builds a cmap-compatible table from encoding records and subtable bytes.
pub fn table(subtables: &[(u16, u16, &[u8])]) -> Vec<u8> {
    let mut data = vec![0; 4 + 8 * subtables.len()];
    data[2..4].copy_from_slice(&(subtables.len() as u16).to_be_bytes());
    for (i, (platform, encoding, subtable)) in subtables.iter().enumerate() {
        let record = 4 + i * 8;
        let offset = data.len() as u32;
        data[record..record + 2].copy_from_slice(&platform.to_be_bytes());
        data[record + 2..record + 4].copy_from_slice(&encoding.to_be_bytes());
        data[record + 4..record + 8].copy_from_slice(&offset.to_be_bytes());
        data.extend_from_slice(subtable);
    }
    data
}

/// Builds a format 12 subtable with one group per mapping.
pub fn format12(mappings: &[(u32, u32)]) -> Vec<u8> {
    let mut data = vec![0, 12, 0, 0];
    data.extend_from_slice(&(16 + 12 * mappings.len() as u32).to_be_bytes());
    data.extend_from_slice(&0u32.to_be_bytes());
    data.extend_from_slice(&(mappings.len() as u32).to_be_bytes());
    for (codepoint, glyph) in mappings {
        data.extend_from_slice(&codepoint.to_be_bytes());
        data.extend_from_slice(&codepoint.to_be_bytes());
        data.extend_from_slice(&glyph.to_be_bytes());
    }
    data
}

/// Builds a format 14 subtable with a single variation selector.
pub fn format14(selector: u32, defaults: &[u32], variants: &[(u32, u16)]) -> Vec<u8> {
    let mut data = vec![0; 21];
    data[..2].copy_from_slice(&14u16.to_be_bytes());
    data[6..10].copy_from_slice(&1u32.to_be_bytes());
    data[10..13].copy_from_slice(&selector.to_be_bytes()[1..]);
    if !defaults.is_empty() {
        data[13..17].copy_from_slice(&21u32.to_be_bytes());
        data.extend_from_slice(&(defaults.len() as u32).to_be_bytes());
        for codepoint in defaults {
            data.extend_from_slice(&codepoint.to_be_bytes()[1..]);
            data.push(0);
        }
    }
    if !variants.is_empty() {
        let offset = data.len() as u32;
        data[17..21].copy_from_slice(&offset.to_be_bytes());
        data.extend_from_slice(&(variants.len() as u32).to_be_bytes());
        for (codepoint, glyph) in variants {
            data.extend_from_slice(&codepoint.to_be_bytes()[1..]);
            data.extend_from_slice(&glyph.to_be_bytes());
        }
    }
    let length = data.len() as u32;
    data[2..6].copy_from_slice(&length.to_be_bytes());
    data
}

/// Contains two codepoint ranges, both [6, 64]. Surely you don't duplicate them?
pub fn repetitive_cmap4() -> BeBuffer {
    // <https://learn.microsoft.com/en-us/typography/opentype/spec/cmap#format-4-segment-mapping-to-delta-values>
    be_buffer! {
      4_u16,                      // uint16	format
      0_u16,                      // uint16	length, unused
      0_u16,                      // uint16	language, unused
      4_u16,                      // uint16	segCountX2, 2 * 2 segments
      0_u16,                      // uint16	searchRange, unused
      0_u16,                      // uint16	entrySelector, unused
      0_u16,                      // uint16	rangeShift, unused
      // segCount endCode entries
      64_u16,                    // uint16	endCode[0]
      64_u16,                    // uint16	endCode[1]

      0_u16,                      // uint16	reservedPad, unused

      // segCount startCode entries
      6_u16,                      // uint16	startCode[0]
      6_u16,                      // uint16	startCode[1]

      // segCount idDelta entries
      0_u16,                      // uint16	idDelta[0]
      0_u16,                      // uint16	idDelta[1]

      // segCount idRangeOffset entries
      0_u16,                      // uint16	idRangeOffset[0]
      0_u16                       // uint16	idRangeOffset[1]

      // no glyphIdArray entries
    }
}
