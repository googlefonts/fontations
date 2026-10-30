//! Flat variation streams appended directly to the destination buffer.
//! Complex table models and offset graphs remain in write-fonts.

pub(super) fn packed_deltas(values: &[i32], out: &mut Vec<u8>) {
    write_fonts::tables::variations::PackedDeltas::extend(values, out);
}

pub(super) fn packed_points(points: &[u16], out: &mut Vec<u8>) {
    write_fonts::tables::variations::PackedPointNumbers::extend_some(points, out);
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
