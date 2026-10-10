//! Flat variation streams appended directly to the destination buffer.
//! Complex table models and offset graphs remain in write-fonts.

pub(super) fn packed_deltas(values: &[i32], out: &mut Vec<u8>) {
    let kind = |value: i32| {
        if value == 0 {
            0x80
        } else if i8::try_from(value).is_ok() {
            0
        } else if i16::try_from(value).is_ok() {
            0x40
        } else {
            0xc0
        }
    };
    let mut start = 0;
    while start < values.len() {
        let flags = kind(values[start]);
        let mut end = start + 1;
        while end < values.len() && end - start < 64 {
            let current = kind(values[end]);
            let next = values.get(end + 1).map(|&v| kind(v));
            let stop = match flags {
                0x80 => current != 0x80,
                0 => current == 0x40 || current == 0xc0 || current == 0x80 && next == Some(0x80),
                0x40 => {
                    current == 0x80
                        || current == 0xc0
                        || current == 0 && matches!(next, Some(0 | 0x80))
                }
                _ => current != 0xc0,
            };
            if stop {
                break;
            }
            end += 1;
        }
        out.push(flags | (end - start - 1) as u8);
        for &v in &values[start..end] {
            match flags {
                0 => out.push(v as u8),
                0x40 => out.extend((v as i16).to_be_bytes()),
                0xc0 => out.extend(v.to_be_bytes()),
                _ => {}
            }
        }
        start = end;
    }
}

pub(super) fn packed_points(points: &[u16], out: &mut Vec<u8>) {
    if points.len() < 128 {
        out.push(points.len() as u8);
    } else {
        out.extend((points.len() as u16 | 0x8000).to_be_bytes());
    }
    let mut start = 0;
    let mut previous = 0;
    while start < points.len() {
        let wide = points[start] - previous > 255;
        let mut end = start;
        let mut last = previous;
        while end < points.len() && end - start < 128 && (points[end] - last > 255) == wide {
            last = points[end];
            end += 1;
        }
        out.push((u8::from(wide) * 0x80) | (end - start - 1) as u8);
        for &point in &points[start..end] {
            let delta = point - previous;
            previous = point;
            if wide {
                out.extend(delta.to_be_bytes());
            } else {
                out.push(delta as u8);
            }
        }
        start = end;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use write_fonts::tables::variations::{PackedDeltas, PackedPointNumbers};

    #[test]
    fn delta_streams_match_writer_at_run_and_integer_boundaries() {
        let values = [
            i32::MIN,
            -32769,
            -32768,
            -129,
            -128,
            -1,
            0,
            1,
            127,
            128,
            32767,
            32768,
            i32::MAX,
        ];
        for &a in &values {
            for &b in &values {
                for len in [0, 1, 2, 63, 64, 65, 128, 129] {
                    let values: Vec<_> = (0..len).map(|i| if i % 3 == 0 { a } else { b }).collect();
                    let mut bytes = Vec::new();
                    packed_deltas(&values, &mut bytes);
                    assert_eq!(
                        bytes,
                        write_fonts::dump_table(&PackedDeltas::new(values)).unwrap()
                    );
                }
            }
        }
    }

    #[test]
    fn point_streams_match_writer_at_count_and_distance_boundaries() {
        for points in [
            vec![],
            vec![0],
            vec![255, 256, 511, 767, 768, 65535],
            (0..127).collect(),
            (0..128).collect(),
            (0..129).collect(),
            (0..255).map(|v| v * 256).collect(),
            (0..=32766).collect(),
        ] {
            let mut bytes = Vec::new();
            packed_points(&points, &mut bytes);
            assert_eq!(
                bytes,
                write_fonts::dump_table(&PackedPointNumbers::Some(points)).unwrap()
            );
        }
    }
}
