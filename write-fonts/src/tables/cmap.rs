//! the [cmap] table
//!
//! [cmap]: https://docs.microsoft.com/en-us/typography/opentype/spec/cmap

include!("../../generated/generated_cmap.rs");

// https://learn.microsoft.com/en-us/typography/opentype/spec/cmap#windows-platform-platform-id--3
const WINDOWS_BMP_ENCODING: u16 = 1;

// https://learn.microsoft.com/en-us/typography/opentype/spec/cmap#unicode-platform-platform-id--0
const UNICODE_BMP_ENCODING: u16 = 3;

fn size_of_cmap4(seg_count: u16, gid_count: u16) -> u16 {
    8 * 2  // 8 uint16's
    + 2 * seg_count * 4  // 4 parallel arrays of len seg_count, 2 bytes per entry
    + 2 * gid_count // 2 bytes per gid in glyphIdArray
}

impl CmapSubtable {
    fn create_format_4(
        lang: u16,
        end_code: Vec<u16>,
        start_code: Vec<u16>,
        id_deltas: Vec<i16>,
    ) -> Self {
        assert!(
            end_code.len() == start_code.len() && end_code.len() == id_deltas.len(),
            "uneven parallel arrays, very bad. Very very bad."
        );

        let seg_count: u16 = start_code.len().try_into().unwrap();
        // Spec: Log2 of the maximum power of 2 less than or equal to segCount (log2(searchRange/2),
        // which is equal to floor(log2(segCount)))
        let entry_selector = (seg_count as f32).log2().floor();

        // Spec: Maximum power of 2 less than or equal to segCount, times 2
        // ((2**floor(log2(segCount))) * 2, where “**” is an exponentiation operator)
        let search_range = 2u16.pow(entry_selector as u32).checked_mul(2).unwrap();

        // if 2^entry_selector*2 is a u16 then so is entry_selector
        let entry_selector = entry_selector as u16;
        let range_shift = seg_count * 2 - search_range;

        let id_range_offsets = vec![0; id_deltas.len()];
        CmapSubtable::Format4(Cmap4::new(
            size_of_cmap4(seg_count, 0),
            lang,
            seg_count * 2,
            search_range,
            entry_selector,
            range_shift,
            end_code,
            start_code,
            id_deltas,
            id_range_offsets,
            vec![], // becauseour idRangeOffset's are 0 glyphIdArray is unused
        ))
    }
}

impl Cmap {
    /// Generates a [cmap](https://learn.microsoft.com/en-us/typography/opentype/spec/cmap) that is expected to work in most modern environments.
    ///
    /// For the time being just emits [format 4](https://learn.microsoft.com/en-us/typography/opentype/spec/cmap#format-4-segment-mapping-to-delta-values)
    /// so we can drive towards compiling working fonts. In time we may wish to additionally emit format 12 to support
    /// novel codepoints.
    pub fn from_mappings(mappings: impl IntoIterator<Item = (char, GlyphId)>) -> Cmap {
        let mut end_code = Vec::new();
        let mut start_code = Vec::new();
        let mut id_deltas = Vec::new();

        let mut mappings: Vec<_> = mappings.into_iter().collect();
        mappings.sort();

        let mut prev = (u16::MAX - 1, u16::MAX - 1);
        for (cp, gid) in mappings.into_iter() {
            let gid = gid.to_u16();
            let cp = (cp as u32).try_into().unwrap();
            let next_in_run = (
                prev.0.checked_add(1).unwrap(),
                prev.1.checked_add(1).unwrap(),
            );
            let current = (cp, gid);
            // Codepoint and gid need to be continuous
            if current != next_in_run {
                // Start a new run
                start_code.push(cp);
                end_code.push(cp);

                // TIL Python % 0x10000 and Rust % 0x10000 do not mean the same thing.
                // rem_euclid is almost what we want, except as applied to small values
                // ex -10 rem_euclid 0x10000 = 65526
                let delta: i32 = gid as i32 - cp as i32;
                let delta = if let Ok(delta) = TryInto::<i16>::try_into(delta) {
                    delta
                } else {
                    delta.rem_euclid(0x10000).try_into().unwrap()
                };
                id_deltas.push(delta);
            } else {
                // Continue the prior run
                let last = end_code.last_mut().unwrap();
                *last = cp;
            }
            prev = current;
        }

        // close out
        start_code.push(0xFFFF);
        end_code.push(0xFFFF);
        id_deltas.push(1);

        // Absent a strong signal to do otherwise, match fontmake/fonttools
        // Since both tables use the same format4 subtable they are almost entirely byte-shared
        // See https://github.com/googlefonts/fontmake-rs/issues/251
        let win_bmp = EncodingRecord::new(
            PlatformId::Windows,
            WINDOWS_BMP_ENCODING,
            CmapSubtable::create_format_4(
                0, // set to zero for all 'cmap' subtables whose platform IDs are other than Macintosh
                end_code, start_code, id_deltas,
            ),
        );
        let mut unicode = win_bmp.clone();
        unicode.platform_id = PlatformId::Unicode;
        unicode.encoding_id = UNICODE_BMP_ENCODING;

        Cmap::new(vec![unicode, win_bmp])
    }
}

#[cfg(test)]
mod tests {
    use font_types::{BigEndian, GlyphId, Scalar};
    use read::{
        tables::cmap::{Cmap, CmapSubtable, PlatformId},
        FontData, FontRead,
    };

    use crate::{
        dump_table,
        tables::cmap::{self as write, UNICODE_BMP_ENCODING, WINDOWS_BMP_ENCODING},
    };

    fn to_vec<T: Scalar>(bees: &[BigEndian<T>]) -> Vec<T> {
        bees.iter().map(|be| be.get()).collect()
    }

    fn assert_generates_simple_cmap(mappings: Vec<(char, GlyphId)>) {
        let cmap = write::Cmap::from_mappings(mappings);

        let bytes = dump_table(&cmap).unwrap();
        let font_data = FontData::new(&bytes);
        let cmap = Cmap::read(font_data).unwrap();

        assert_eq!(2, cmap.encoding_records().len(), "{cmap:?}");
        assert_eq!(
            vec![
                (PlatformId::Unicode, UNICODE_BMP_ENCODING),
                (PlatformId::Windows, WINDOWS_BMP_ENCODING)
            ],
            cmap.encoding_records()
                .iter()
                .map(|er| (er.platform_id(), er.encoding_id()))
                .collect::<Vec<_>>(),
            "{cmap:?}"
        );

        for encoding_record in cmap.encoding_records() {
            let CmapSubtable::Format4(cmap4) = encoding_record.subtable(font_data).unwrap() else {
                panic!("Expected a cmap4 in {encoding_record:?}");
            };

            // The spec example says entry_selector 4 but the calculation it gives seems to yield 2 (?)
            assert_eq!(
                (8, 8, 2, 0),
                (
                    cmap4.seg_count_x2(),
                    cmap4.search_range(),
                    cmap4.entry_selector(),
                    cmap4.range_shift()
                )
            );
            assert_eq!(
                vec![10u16, 30u16, 153u16, 0xffffu16],
                to_vec(cmap4.start_code())
            );
            assert_eq!(
                vec![20u16, 90u16, 480u16, 0xffffu16],
                to_vec(cmap4.end_code())
            );
            // The example starts at gid 1, we're starting at 0
            assert_eq!(vec![-10i16, -19i16, -81i16, 1i16], to_vec(cmap4.id_delta()));
            assert_eq!(
                vec![0u16, 0u16, 0u16, 0u16],
                to_vec(cmap4.id_range_offsets())
            );
        }
    }

    fn simple_cmap_mappings() -> Vec<(char, GlyphId)> {
        (10..=20)
            .chain(30..=90)
            .chain(153..=480)
            .enumerate()
            .map(|(idx, codepoint)| (char::from_u32(codepoint).unwrap(), GlyphId::new(idx as u16)))
            .collect()
    }

    // https://learn.microsoft.com/en-us/typography/opentype/spec/cmap#format-4-segment-mapping-to-delta-values
    // "map characters 10-20, 30-90, and 153-480 onto a contiguous range of glyph indices"
    #[test]
    fn generate_simple_cmap4() {
        let mappings = simple_cmap_mappings();
        assert_generates_simple_cmap(mappings);
    }

    #[test]
    fn generate_cmap4_out_of_order_input() {
        let mut ordered = simple_cmap_mappings();
        let mut disordered = Vec::new();
        while !ordered.is_empty() {
            if ordered.len() % 2 == 0 {
                disordered.insert(0, ordered.remove(0));
            } else {
                disordered.push(ordered.remove(0));
            }
        }
        assert_ne!(ordered, disordered);
        assert_generates_simple_cmap(disordered);
    }

    #[test]
    fn generate_cmap4_large_values() {
        let mut mappings = simple_cmap_mappings();
        // Example from Texturina.
        let codepoint = char::from_u32(0xa78b).unwrap();
        let gid = GlyphId::new(153);
        mappings.push((codepoint, gid));

        let cmap = write::Cmap::from_mappings(mappings);

        let bytes = dump_table(&cmap).unwrap();
        let font_data = FontData::new(&bytes);
        let cmap = Cmap::read(font_data).unwrap();
        assert_eq!(cmap.map_codepoint(codepoint), Some(gid));
    }

    #[test]
    fn bytes_are_reused() {
        // We emit extra encoding records assuming it's cheap. Make sure.
        let mappings = simple_cmap_mappings();
        let cmap_both = write::Cmap::from_mappings(mappings);
        assert_eq!(2, cmap_both.encoding_records.len(), "{cmap_both:?}");

        let bytes_for_both = dump_table(&cmap_both).unwrap().len();

        for i in 0..cmap_both.encoding_records.len() {
            let mut cmap = cmap_both.clone();
            cmap.encoding_records.remove(i);
            let bytes_for_one = dump_table(&cmap).unwrap().len();
            assert_eq!(bytes_for_one + 8, bytes_for_both);
        }
    }
}
