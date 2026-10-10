//! HarfBuzz's avar2 culling path: drop unreachable regions without rebasing
//! live tuples or changing variation indices.
use crate::SubsetError;
use std::collections::BTreeMap;
use write_fonts::{
    from_obj::ToOwnedTable,
    read::{FontRef, TableProvider},
    tables::{
        base::Base, colr::Colr, gdef::Gdef, hvar::Hvar, mvar::Mvar, variations::ItemVariationStore,
        vvar::Vvar,
    },
    types::Tag,
};

type Ranges = [Option<(i16, i16)>];
fn error(tag: Tag) -> SubsetError {
    SubsetError::SubsetTableError(tag)
}

fn dead(ranges: &Ranges, tents: impl IntoIterator<Item = (i16, i16, i16)>) -> bool {
    ranges
        .iter()
        .zip(tents)
        .any(|(&range, (start, peak, end))| {
            let Some((lo, hi)) = range else { return false };
            // Ignored tents are constant at runtime and cannot kill a region.
            peak != 0
                && start <= peak
                && peak <= end
                && !(start < 0 && end > 0)
                && (hi < start || lo > end)
        })
}

fn prune_store(
    store: &mut ItemVariationStore,
    ranges: &Ranges,
    tag: Tag,
) -> Result<bool, SubsetError> {
    if store.variation_region_list.axis_count as usize != ranges.len() {
        return Err(error(tag));
    }
    let keep: Vec<_> = store
        .variation_region_list
        .variation_regions
        .iter()
        .map(|r| {
            !dead(
                ranges,
                r.region_axes.iter().map(|a| {
                    (
                        a.start_coord.to_bits(),
                        a.peak_coord.to_bits(),
                        a.end_coord.to_bits(),
                    )
                }),
            )
        })
        .collect();
    if keep.iter().all(|&v| v) {
        return Ok(false);
    }
    let mut mapping = vec![0; keep.len()];
    let mut next = 0;
    for (old, &keep) in keep.iter().enumerate() {
        if keep {
            mapping[old] = next;
            next += 1;
        }
    }
    for data in &mut store.item_variation_data {
        let Some(data) = data.as_mut() else { continue };
        let wide = (data.word_delta_count & 0x7fff) as usize;
        let long = data.word_delta_count & 0x8000 != 0;
        if wide > data.region_indexes.len() {
            return Err(error(tag));
        }
        let mut row_size = 0usize;
        let mut columns = Vec::new();
        let mut kept_wide = 0;
        for (i, &region) in data.region_indexes.iter().enumerate() {
            let size = if i < wide {
                if long {
                    4
                } else {
                    2
                }
            } else if long {
                2
            } else {
                1
            };
            if *keep.get(region as usize).ok_or_else(|| error(tag))? {
                columns.push((mapping[region as usize], row_size, size));
                kept_wide += usize::from(i < wide);
            }
            row_size += size;
        }
        if data.delta_sets.len()
            < row_size
                .checked_mul(data.item_count as usize)
                .ok_or_else(|| error(tag))?
        {
            return Err(error(tag));
        }
        let mut deltas = Vec::new();
        for row in 0..data.item_count as usize {
            for &(_, offset, size) in &columns {
                let offset = row * row_size + offset;
                deltas.extend_from_slice(&data.delta_sets[offset..offset + size]);
            }
        }
        data.word_delta_count = kept_wide as u16 | if long { 0x8000 } else { 0 };
        data.region_indexes = columns.iter().map(|&(region, _, _)| region).collect();
        data.delta_sets = deltas;
    }
    let mut index = 0;
    store.variation_region_list.variation_regions.retain(|_| {
        let retain = keep[index];
        index += 1;
        retain
    });
    Ok(true)
}

pub(super) fn tables(
    font: &FontRef,
    ranges: &Ranges,
    tables: &mut BTreeMap<Tag, Vec<u8>>,
) -> Result<bool, SubsetError> {
    if ranges.iter().all(Option::is_none) {
        return Ok(false);
    }
    let mut changed = false;
    macro_rules! store {
        ($table:ident, $field:ident, required) => {
            Some($table.$field.as_mut())
        };
        ($table:ident, $field:ident, optional) => {
            $table.$field.as_mut()
        };
    }
    macro_rules! prune {
        ($access:ident, $owned:ty, $field:ident, $kind:ident, $tag:literal) => {
            let tag = Tag::new($tag);
            if font.data_for_tag(tag).is_some() {
                let source = font.$access().map_err(|_| error(tag))?;
                let mut table: $owned = source.to_owned_table();
                if let Some(store) = store!(table, $field, $kind) {
                    if prune_store(store, ranges, tag)? {
                        tables.insert(
                            tag,
                            write_fonts::dump_table(&table).map_err(|_| error(tag))?,
                        );
                        changed = true;
                    }
                }
            }
        };
    }
    prune!(hvar, Hvar, item_variation_store, required, b"HVAR");
    prune!(vvar, Vvar, item_variation_store, required, b"VVAR");
    prune!(mvar, Mvar, item_variation_store, optional, b"MVAR");
    prune!(gdef, Gdef, item_var_store, optional, b"GDEF");
    prune!(base, Base, item_var_store, optional, b"BASE");
    prune!(colr, Colr, item_variation_store, optional, b"COLR");
    let tag = Tag::new(b"gvar");
    if let Some(bytes) = tables.get(&tag) {
        if let Some(pruned) = prune_gvar(bytes, ranges)? {
            tables.insert(tag, pruned);
            changed = true;
        }
    }
    Ok(changed)
}

fn read16(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_be_bytes(
        bytes.get(offset..offset.checked_add(2)?)?.try_into().ok()?,
    ))
}
fn read32(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_be_bytes(
        bytes.get(offset..offset.checked_add(4)?)?.try_into().ok()?,
    ))
}

// Preserve the original shared points and each live header/data pair. The
// packed deltas need no decoding, just as in HB's cull_tuple_variations.
fn prune_tuples(bytes: &[u8], ranges: &Ranges, shared: &[u8]) -> Option<Vec<u8>> {
    let count = read16(bytes, 0)?;
    let data_offset = read16(bytes, 2)? as usize;
    let axes = ranges.len();
    let mut data = data_offset;
    if count & 0x8000 != 0 {
        let first = *bytes.get(data)?;
        data += 1;
        let points = if first & 0x80 != 0 {
            let n = ((first as usize & 0x7f) << 8) | *bytes.get(data)? as usize;
            data += 1;
            n
        } else {
            first as usize
        };
        let mut n = 0;
        while n < points {
            let run = *bytes.get(data)?;
            data += 1;
            n += (run as usize & 0x7f) + 1;
            if n > points {
                return None;
            }
            data += ((run as usize & 0x7f) + 1) * if run & 0x80 != 0 { 2 } else { 1 };
            bytes.get(..data)?;
        }
    }
    let shared_points = bytes.get(data_offset..data)?;
    let mut header = 4;
    let mut headers = Vec::new();
    let mut deltas = Vec::new();
    let mut kept = 0u16;
    for _ in 0..count & 0x0fff {
        let size = read16(bytes, header)? as usize;
        let index = read16(bytes, header + 2)?;
        let start_header = header;
        header += 4;
        let peak = if index & 0x8000 != 0 {
            let peak = bytes.get(header..header + axes * 2)?;
            header += axes * 2;
            peak
        } else {
            let offset = (index as usize & 0x0fff) * axes * 2;
            shared.get(offset..offset + axes * 2)?
        };
        let intermediate = if index & 0x4000 != 0 {
            let bounds = bytes.get(header..header + axes * 4)?;
            header += axes * 4;
            Some(bounds)
        } else {
            None
        };
        if header > data_offset {
            return None;
        }
        let tents = (0..axes).map(|i| {
            let p = read16(peak, i * 2).unwrap() as i16;
            intermediate.map_or((p.min(0), p, p.max(0)), |b| {
                (
                    read16(b, i * 2).unwrap() as i16,
                    p,
                    read16(b, (axes + i) * 2).unwrap() as i16,
                )
            })
        });
        let tuple_data = bytes.get(data..data.checked_add(size)?)?;
        data += size;
        if !dead(ranges, tents) {
            headers.extend_from_slice(&bytes[start_header..header]);
            deltas.extend_from_slice(tuple_data);
            kept += 1;
        }
    }
    if kept == count & 0x0fff {
        return None;
    }
    if kept == 0 {
        return Some(Vec::new());
    }
    let offset = u16::try_from(4 + headers.len()).ok()?;
    let mut out = (kept | count & 0x8000).to_be_bytes().to_vec();
    out.extend_from_slice(&offset.to_be_bytes());
    out.extend(headers);
    out.extend_from_slice(shared_points);
    out.extend(deltas);
    Some(out)
}

fn prune_gvar(bytes: &[u8], ranges: &Ranges) -> Result<Option<Vec<u8>>, SubsetError> {
    let err = || error(Tag::new(b"gvar"));
    let axes = read16(bytes, 4).ok_or_else(err)? as usize;
    if axes != ranges.len() {
        return Err(err());
    }
    let shared_count = read16(bytes, 6).ok_or_else(err)? as usize;
    let shared_offset = read32(bytes, 8).ok_or_else(err)? as usize;
    let glyph_count = read16(bytes, 12).ok_or_else(err)? as usize;
    let flags = read16(bytes, 14).ok_or_else(err)?;
    let data_offset = read32(bytes, 16).ok_or_else(err)? as usize;
    let shared_size = shared_count
        .checked_mul(axes)
        .and_then(|v| v.checked_mul(2))
        .ok_or_else(err)?;
    let shared = bytes
        .get(shared_offset..shared_offset.checked_add(shared_size).ok_or_else(err)?)
        .ok_or_else(err)?;
    let offset = |i| {
        if flags & 1 != 0 {
            read32(bytes, 20 + i * 4).map(|v| v as usize)
        } else {
            read16(bytes, 20 + i * 2).map(|v| v as usize * 2)
        }
    };
    let mut changed = false;
    let mut data = Vec::new();
    let mut offsets = Vec::new();
    for i in 0..glyph_count {
        offsets.push(u32::try_from(data.len()).map_err(|_| err())?);
        let start = data_offset
            .checked_add(offset(i).ok_or_else(err)?)
            .ok_or_else(err)?;
        let end = data_offset
            .checked_add(offset(i + 1).ok_or_else(err)?)
            .ok_or_else(err)?;
        let original = bytes.get(start..end).ok_or_else(err)?;
        if let Some(pruned) = prune_tuples(original, ranges, shared) {
            data.extend(pruned);
            changed = true;
        } else {
            data.extend_from_slice(original);
        }
        if data.len() % 2 != 0 {
            data.push(0);
        }
    }
    if !changed {
        return Ok(None);
    }
    offsets.push(u32::try_from(data.len()).map_err(|_| err())?);
    let short = data.len() <= 0x1fffe && flags & 1 == 0;
    let shared_offset = 20 + offsets.len() * if short { 2 } else { 4 };
    let data_offset = shared_offset + shared.len();
    let mut out = bytes[..8].to_vec();
    out.extend_from_slice(
        &u32::try_from(shared_offset)
            .map_err(|_| err())?
            .to_be_bytes(),
    );
    out.extend_from_slice(&(glyph_count as u16).to_be_bytes());
    out.extend_from_slice(&((flags & !1) | u16::from(!short)).to_be_bytes());
    out.extend_from_slice(&u32::try_from(data_offset).map_err(|_| err())?.to_be_bytes());
    for offset in offsets {
        if short {
            out.extend_from_slice(&((offset / 2) as u16).to_be_bytes());
        } else {
            out.extend_from_slice(&offset.to_be_bytes());
        }
    }
    out.extend_from_slice(shared);
    out.extend(data);
    Ok(Some(out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use write_fonts::{
        read::{tables::variations::ItemVariationStore as ReadStore, FontData, FontRead},
        tables::variations::{
            ItemVariationData, RegionAxisCoordinates, VariationRegion, VariationRegionList,
        },
        types::F2Dot14,
    };

    #[test]
    fn boundary_peaks_and_ignored_tents_remain_live() {
        let ranges = [Some((8192, 8192))];
        for tent in [
            (8192, 8192, 8192),
            (0, 8192, 16384),
            (0, 0, 0),
            (9000, 8000, 10000),
            (-1000, 8000, 9000),
        ] {
            assert!(!dead(&ranges, [tent]), "{tent:?}");
        }
        assert!(dead(&ranges, [(0, 8000, 8191)]));
        assert!(dead(&ranges, [(8193, 10000, 16384)]));
        assert!(!dead(&[None], [(8193, 10000, 16384)]));
    }

    fn regions() -> VariationRegionList {
        VariationRegionList::new(
            1,
            [
                (-16384, -16384, 0),
                (0, 16384, 16384),
                (0, 0, 0),
                (-1000, 8000, 10000),
            ]
            .map(|(a, b, c)| {
                VariationRegion::new(vec![RegionAxisCoordinates::new(
                    F2Dot14::from_bits(a),
                    F2Dot14::from_bits(b),
                    F2Dot14::from_bits(c),
                )])
            })
            .to_vec(),
        )
    }

    #[test]
    fn mixed_delta_widths_and_zero_column_rows_keep_their_indices() {
        for long in [false, true] {
            let values = [[-300i32, 200, -128, 127], [300, -200, 127, -128]];
            let mut bytes = Vec::new();
            for row in values {
                for (i, delta) in row.into_iter().enumerate() {
                    if i < 2 && long {
                        bytes.extend(delta.to_be_bytes());
                    } else if i < 2 || long {
                        bytes.extend((delta as i16).to_be_bytes());
                    } else {
                        bytes.push(delta as i8 as u8);
                    }
                }
            }
            let mut store = ItemVariationStore::new(
                regions(),
                vec![
                    Some(ItemVariationData {
                        item_count: 2,
                        word_delta_count: 2 | if long { 0x8000 } else { 0 },
                        region_indexes: vec![0, 1, 2, 3],
                        delta_sets: bytes,
                    }),
                    Some(ItemVariationData {
                        item_count: 3,
                        word_delta_count: 1,
                        region_indexes: vec![0],
                        delta_sets: vec![0; 6],
                    }),
                ],
            );
            let tag = Tag::new(b"HVAR");
            assert!(prune_store(&mut store, &[Some((8191, 8193))], tag).unwrap());
            assert_eq!(store.variation_region_list.variation_regions.len(), 3);
            let bytes = write_fonts::dump_table(&store).unwrap();
            let store = ReadStore::read(FontData::new(&bytes)).unwrap();
            let data = store.item_variation_data().get(0).unwrap().unwrap();
            assert_eq!(data.word_delta_count(), 1 | if long { 0x8000 } else { 0 });
            assert_eq!(data.item_count(), 2);
            assert_eq!(data.delta_set(0).collect::<Vec<_>>(), vec![200, -128, 127]);
            assert_eq!(data.delta_set(1).collect::<Vec<_>>(), vec![-200, 127, -128]);
            let data = store.item_variation_data().get(1).unwrap().unwrap();
            assert_eq!(data.item_count(), 3);
            assert_eq!(data.region_index_count(), 0);
            assert!(data.delta_set(2).next().is_none());
        }
    }

    #[test]
    fn live_tuple_headers_shared_points_and_packed_deltas_are_copied() {
        // Shared point numbers encode points 3 and 5; the first tuple uses a
        // shared negative peak, the second an embedded positive tent.
        let mut bytes = vec![0x80, 2, 0, 18, 0, 6, 0, 0, 0, 6, 0xc0, 0];
        bytes.extend_from_slice(&[0x40, 0, 0, 0, 0x40, 0]);
        bytes.extend_from_slice(&[2, 1, 3, 2]);
        bytes.extend_from_slice(&[1, 11, 12, 1, 13, 14, 1, 21, 22, 1, 23, 24]);
        let expected = [
            0x80, 1, 0, 14, 0, 6, 0xc0, 0, 0x40, 0, 0, 0, 0x40, 0, 2, 1, 3, 2, 1, 21, 22, 1, 23, 24,
        ];
        let shared = [0xc0, 0];
        assert_eq!(
            prune_tuples(&bytes, &[Some((8191, 8193))], &shared).unwrap(),
            expected
        );
        assert!(prune_tuples(&bytes, &[None], &shared).is_none());
        assert!(prune_tuples(&bytes[..25], &[Some((8191, 8193))], &shared).is_none());
        assert!(prune_tuples(&bytes, &[Some((-1, 1))], &shared).is_none());
        assert!(prune_tuples(&bytes, &[Some((17000, 17001))], &shared)
            .unwrap()
            .is_empty());
    }
}
