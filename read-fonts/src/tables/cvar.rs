//! The [cvar (CVT Variations)](https://learn.microsoft.com/en-us/typography/opentype/spec/cvar)
//! table

include!("../../generated/generated_cvar.rs");

use super::variations::{
    PackedPointNumbers, TupleVariationCount, TupleVariationData, TupleVariationHeader,
};

impl<'a> Cvar<'a> {
    pub fn variation_data(&self, axis_count: u16) -> Result<TupleVariationData<'a>, ReadError> {
        let count = self.tuple_variation_count();
        let data = self.data()?;
        let header_data = self.raw_tuple_header_data();
        // if there are shared point numbers, get them now
        let (shared_point_numbers, serialized_data) = if count.shared_point_numbers() {
            let (packed, data) = PackedPointNumbers::split_off_front(data);
            (Some(packed), data)
        } else {
            (None, data)
        };
        Ok(TupleVariationData {
            is_gvar: false,
            tuple_count: count,
            axis_count,
            shared_tuples: None,
            shared_point_numbers,
            header_data,
            serialized_data,
        })
    }

    fn raw_tuple_header_data(&self) -> FontData<'a> {
        let range = self.shape.tuple_variation_headers_byte_range();
        self.data.split_off(range.start).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use font_types::F2Dot14;

    use crate::{FontRef, TableProvider};

    #[test]
    fn cvar_deltas() {
        let font = FontRef::new(font_test_data::CVAR).unwrap();
        // Elements are ([coords], [deltas]) where deltas are fixed point
        // values
        let cases = &[
            (
                [0.5, 0.5],
                [
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 720896, 3276800, 1179648, 0, 0, 0, 0, 0,
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 622592, 0, 1179648, 0, 0, 0, 0, 0, 622592,
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                ],
            ),
            (
                [-0.5, 0.5],
                [
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -1441792, -2162688, -1277952, 0, 0, 0,
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -917504, 0, -1277952, 0, 0, 0, 0, 0,
                    -720896, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0,
                ],
            ),
            (
                [0.5, -0.5],
                [
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1900544, 2621440, 2129920, 0, 0, 0, 0,
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 360448, 0, 1015808, 0, 0, 0, 0, 0,
                    524288, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0,
                ],
            ),
            (
                [-0.5, -0.5],
                [
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -1212416, -2293760, -1130496, 0, 0, 0,
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -1097728, 0, -1277952, 0, 0, 0, 0, 0,
                    -737280, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0,
                ],
            ),
            (
                [-1.0, -1.0],
                [
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -2490368, -4325376, -2490368, 0, 0, 0,
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -1835008, 0, -2555904, 0, 0, 0, 0, 0,
                    -1441792, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0,
                ],
            ),
            (
                [1.0, 1.0],
                [
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1441792, 6553600, 2359296, 0, 0, 0, 0,
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1245184, 0, 2359296, 0, 0, 0, 0, 0,
                    1245184, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0,
                ],
            ),
            (
                [-1.0, 1.0],
                [
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -2883584, -4325376, -2555904, 0, 0, 0,
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -1835008, 0, -2555904, 0, 0, 0, 0, 0,
                    -1441792, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0,
                ],
            ),
            (
                [1.0, -1.0],
                [
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 5636096, 4456448, 5636096, 0, 0, 0, 0,
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 917504, 0, 1703936, 0, 0, 0, 0, 0,
                    917504, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0,
                ],
            ),
        ];
        let cvar = font.cvar().unwrap();
        let axis_count = font.fvar().unwrap().axis_count();
        let cvar_data = cvar.variation_data(axis_count).unwrap();
        for (coords, expected_deltas) in cases {
            let coords = coords.map(F2Dot14::from_f32);
            let mut deltas = vec![0; expected_deltas.len()];
            for (tuple, weight) in cvar_data.active_tuples_at(&coords) {
                for delta in tuple.deltas() {
                    let scaled_delta = delta.apply_scalar(weight).x;
                    deltas[delta.position as usize] += scaled_delta.to_bits();
                }
            }
            assert_eq!(&deltas, expected_deltas);
        }
    }
}
