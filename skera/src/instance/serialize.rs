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

#[cfg(test)]
mod tests {
    use super::*;
    use write_fonts::tables::variations::PackedDeltas;

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
}
