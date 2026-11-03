//! Reachability checks and retained raw tuple pruning for avar2.
type Ranges = [Option<(i16, i16)>];
pub(super) fn dead(ranges: &Ranges, tents: impl IntoIterator<Item = (i16, i16, i16)>) -> bool {
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

fn read16(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_be_bytes(
        bytes.get(offset..offset.checked_add(2)?)?.try_into().ok()?,
    ))
}
// Preserve the original shared points and each live header/data pair. The
// packed deltas need no decoding, just as in HB's cull_tuple_variations.
pub(crate) fn prune_tuples(bytes: &[u8], ranges: &Ranges, shared: &[u8]) -> Option<Vec<u8>> {
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

#[cfg(test)]
mod tests {
    use super::*;
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
