//! the [cmap] table
//!
//! [cmap]: https://docs.microsoft.com/en-us/typography/opentype/spec/cmap

include!("../../generated/generated_cmap.rs");

use std::collections::HashMap;

use crate::util::SearchRange;

// https://learn.microsoft.com/en-us/typography/opentype/spec/cmap#windows-platform-platform-id--3
const WINDOWS_BMP_ENCODING: u16 = 1;
const WINDOWS_FULL_REPERTOIRE_ENCODING: u16 = 10;

// https://learn.microsoft.com/en-us/typography/opentype/spec/cmap#unicode-platform-platform-id--0
const UNICODE_BMP_ENCODING: u16 = 3;
const UNICODE_FULL_REPERTOIRE_ENCODING: u16 = 4;

fn size_of_cmap4(seg_count: u16, gid_count: u16) -> u16 {
    // https://learn.microsoft.com/en-us/typography/opentype/spec/cmap#format-4-segment-mapping-to-delta-values
    8 * 2  // 8 uint16's
    + 2 * seg_count * 4  // 4 parallel arrays of len seg_count, 2 bytes per entry
    + 2 * gid_count // 2 bytes per gid in glyphIdArray
}

fn size_of_cmap12(num_groups: u32) -> u32 {
    // https://learn.microsoft.com/en-us/typography/opentype/spec/cmap#format-12-segmented-coverage
    2 * 2 + 3 * 4  // 2 uint16's and 3 uint32's
    + num_groups * 3 * 4 // 3 unit32's per segment map group
}

impl CmapSubtable {
    /// Create a new format 4 `CmapSubtable` from a list of `(char, GlyphId)` pairs.
    ///
    /// The pairs are expected to be already sorted by codepoint.
    /// Characters beyond the BMP are ignored. If all characters are beyond the BMP
    /// then `None` is returned.
    fn create_format_4(mappings: &[(char, GlyphId)]) -> Option<Self> {
        let mut end_code = Vec::new();
        let mut start_code = Vec::new();
        let mut id_deltas = Vec::new();

        let mut prev = (u16::MAX - 1, u16::MAX - 1);
        for (cp, gid) in mappings {
            let gid = gid.to_u32();
            if gid > 0xFFFF {
                // Should we just fail here?
                continue;
            }
            let gid = gid as u16;
            if *cp > '\u{FFFF}' {
                // mappings is sorted, so the rest will be beyond the BMP too.
                break;
            }
            let cp = (*cp as u32).try_into().unwrap();
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

        if start_code.is_empty() {
            // No characters in the BMP
            return None;
        }

        // close out
        start_code.push(0xFFFF);
        end_code.push(0xFFFF);
        id_deltas.push(1);

        assert!(
            end_code.len() == start_code.len() && end_code.len() == id_deltas.len(),
            "uneven parallel arrays, very bad. Very very bad."
        );

        let seg_count: u16 = start_code.len().try_into().unwrap();

        let computed = SearchRange::compute(seg_count as _, u16::RAW_BYTE_LEN);
        let id_range_offsets = vec![0; id_deltas.len()];
        Some(CmapSubtable::format_4(
            size_of_cmap4(seg_count, 0),
            0, // 'lang' set to zero for all 'cmap' subtables whose platform IDs are other than Macintosh
            seg_count * 2,
            computed.search_range,
            computed.entry_selector,
            computed.range_shift,
            end_code,
            start_code,
            id_deltas,
            id_range_offsets,
            vec![], // because our idRangeOffset's are 0 glyphIdArray is unused
        ))
    }

    /// Create a new format 12 `CmapSubtable` from a list of `(char, GlyphId)` pairs.
    ///
    /// The pairs are expected to be already sorted by chars.
    /// In case of duplicate chars, the last one wins.
    fn create_format_12(mappings: &[(char, GlyphId)]) -> Self {
        let (mut char_codes, gids): (Vec<u32>, Vec<u32>) = mappings
            .iter()
            .map(|(cp, gid)| (*cp as u32, gid.to_u32()))
            .unzip();
        let cmap: HashMap<_, _> = char_codes.iter().cloned().zip(gids).collect();
        char_codes.dedup();

        // we know we have at least one non-BMP char_code > 0xFFFF so unwrap is safe
        let mut start_char_code = *char_codes.first().unwrap();
        let mut start_glyph_id = cmap[&start_char_code];
        let mut last_glyph_id = start_glyph_id.wrapping_sub(1);
        let mut last_char_code = start_char_code.wrapping_sub(1);
        let mut groups = Vec::new();
        for char_code in char_codes {
            let glyph_id = cmap[&char_code];
            if glyph_id != last_glyph_id.wrapping_add(1)
                || char_code != last_char_code.wrapping_add(1)
            {
                groups.push((start_char_code, last_char_code, start_glyph_id));
                start_char_code = char_code;
                start_glyph_id = glyph_id;
            }
            last_glyph_id = glyph_id;
            last_char_code = char_code;
        }
        groups.push((start_char_code, last_char_code, start_glyph_id));

        let num_groups: u32 = groups.len().try_into().unwrap();
        let seq_map_groups = groups
            .into_iter()
            .map(|(start_char, end_char, gid)| SequentialMapGroup::new(start_char, end_char, gid))
            .collect::<Vec<_>>();
        CmapSubtable::format_12(
            size_of_cmap12(num_groups),
            0, // 'lang' set to zero for all 'cmap' subtables whose platform IDs are other than Macintosh
            num_groups,
            seq_map_groups,
        )
    }
}

/// A conflicting Cmap definition, one char is mapped to multiple distinct GlyphIds.
///
/// If there are multiple conflicting mappings, one is chosen arbitrarily.
/// gid1 is less than gid2.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CmapConflict {
    ch: char,
    gid1: GlyphId,
    gid2: GlyphId,
}

impl std::fmt::Display for CmapConflict {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let ch32 = self.ch as u32;
        write!(
            f,
            "Cannot map {:?} (U+{ch32:04X}) to two different glyph ids: {} and {}",
            self.ch, self.gid1, self.gid2
        )
    }
}

impl std::error::Error for CmapConflict {}

impl Cmap {
    /// Generates a [cmap](https://learn.microsoft.com/en-us/typography/opentype/spec/cmap) that is expected to work in most modern environments.
    ///
    /// This emits [format 4](https://learn.microsoft.com/en-us/typography/opentype/spec/cmap#format-4-segment-mapping-to-delta-values)
    /// and [format 12](https://learn.microsoft.com/en-us/typography/opentype/spec/cmap#format-12-segmented-coverage)
    /// subtables, respectively for the Basic Multilingual Plane and Full Unicode Repertoire.
    ///
    /// Also see: <https://learn.microsoft.com/en-us/typography/opentype/spec/recom#cmap-table>
    pub fn from_mappings(
        mappings: impl IntoIterator<Item = (char, GlyphId)>,
    ) -> Result<Cmap, CmapConflict> {
        let mut mappings: Vec<_> = mappings.into_iter().collect();
        mappings.sort();
        mappings.dedup();
        if let Some((ch, gid1, gid2)) =
            mappings
                .iter()
                .zip(mappings.iter().skip(1))
                .find_map(|((c1, g1), (c2, g2))| {
                    (c1 == c2 && g1 != g2).then(|| (*c1, *g1.min(g2), *g1.max(g2)))
                })
        {
            return Err(CmapConflict { ch, gid1, gid2 });
        }

        let mut uni_records = Vec::new(); // platform 0
        let mut win_records = Vec::new(); // platform 3

        // if there are characters in the Unicode Basic Multilingual Plane (U+0000 to U+FFFF)
        // we need to emit format 4 subtables
        let bmp_subtable = CmapSubtable::create_format_4(&mappings);
        if let Some(bmp_subtable) = bmp_subtable {
            // Absent a strong signal to do otherwise, match fontmake/fonttools
            // Since both Windows and Unicode platform tables use the same subtable they are
            // almost entirely byte-shared
            // See https://github.com/googlefonts/fontmake-rs/issues/251
            uni_records.push(EncodingRecord::new(
                PlatformId::Unicode,
                UNICODE_BMP_ENCODING,
                bmp_subtable.clone(),
            ));
            win_records.push(EncodingRecord::new(
                PlatformId::Windows,
                WINDOWS_BMP_ENCODING,
                bmp_subtable,
            ));
        }

        // If there are any supplementary-plane characters (U+10000 to U+10FFFF) we also
        // emit format 12 subtables
        if mappings.iter().any(|(cp, _)| *cp > '\u{FFFF}') {
            let full_repertoire_subtable = CmapSubtable::create_format_12(&mappings);
            // format 12 subtables are also going to be byte-shared, just like above
            uni_records.push(EncodingRecord::new(
                PlatformId::Unicode,
                UNICODE_FULL_REPERTOIRE_ENCODING,
                full_repertoire_subtable.clone(),
            ));
            win_records.push(EncodingRecord::new(
                PlatformId::Windows,
                WINDOWS_FULL_REPERTOIRE_ENCODING,
                full_repertoire_subtable,
            ));
        }

        // put encoding records in order of (platform id, encoding id):
        // - Unicode (0), BMP (3)
        // - Unicode (0), full repertoire (4)
        // - Windows (3), BMP (1)
        // - Windows (3), full repertoire (10)
        Ok(Cmap::new(
            uni_records.into_iter().chain(win_records).collect(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use font_types::GlyphId;
    use read_fonts::{
        tables::cmap::{Cmap, CmapSubtable, PlatformId},
        FontData, FontRead,
    };

    use crate::{
        dump_table,
        tables::cmap::{
            self as write, CmapConflict, UNICODE_BMP_ENCODING, UNICODE_FULL_REPERTOIRE_ENCODING,
            WINDOWS_BMP_ENCODING, WINDOWS_FULL_REPERTOIRE_ENCODING,
        },
    };

    fn assert_generates_simple_cmap(mappings: Vec<(char, GlyphId)>) {
        let cmap = write::Cmap::from_mappings(mappings).unwrap();

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
            assert_eq!(cmap4.start_code(), &[10u16, 30u16, 153u16, 0xffffu16]);
            assert_eq!(cmap4.end_code(), &[20u16, 90u16, 480u16, 0xffffu16]);
            // The example starts at gid 1, we're starting at 0
            assert_eq!(cmap4.id_delta(), &[-10i16, -19i16, -81i16, 1i16]);
            assert_eq!(cmap4.id_range_offsets(), &[0u16, 0u16, 0u16, 0u16]);
        }
    }

    fn simple_cmap_mappings() -> Vec<(char, GlyphId)> {
        (10..=20)
            .chain(30..=90)
            .chain(153..=480)
            .enumerate()
            .map(|(idx, codepoint)| (char::from_u32(codepoint).unwrap(), GlyphId::new(idx as u32)))
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

        let cmap = write::Cmap::from_mappings(mappings).unwrap();

        let bytes = dump_table(&cmap).unwrap();
        let font_data = FontData::new(&bytes);
        let cmap = Cmap::read(font_data).unwrap();
        assert_eq!(cmap.map_codepoint(codepoint), Some(gid));
    }

    #[test]
    fn bytes_are_reused() {
        // We emit extra encoding records assuming it's cheap. Make sure.
        let mappings = simple_cmap_mappings();
        let cmap_both = write::Cmap::from_mappings(mappings).unwrap();
        assert_eq!(2, cmap_both.encoding_records.len(), "{cmap_both:?}");

        let bytes_for_both = dump_table(&cmap_both).unwrap().len();

        for i in 0..cmap_both.encoding_records.len() {
            let mut cmap = cmap_both.clone();
            cmap.encoding_records.remove(i);
            let bytes_for_one = dump_table(&cmap).unwrap().len();
            assert_eq!(bytes_for_one + 8, bytes_for_both);
        }
    }

    fn non_bmp_cmap_mappings() -> Vec<(char, GlyphId)> {
        // contains four sequential map groups
        vec![
            // first group
            ('\u{1f12f}', GlyphId::new(481)),
            ('\u{1f130}', GlyphId::new(482)),
            // char 0x1f131 skipped, starts second group
            ('\u{1f132}', GlyphId::new(483)),
            ('\u{1f133}', GlyphId::new(484)),
            // gid 485 skipped, starts third group
            ('\u{1f134}', GlyphId::new(486)),
            // char 0x1f135 skipped, starts fourth group. identical duplicate bindings are fine
            ('\u{1f136}', GlyphId::new(488)),
            ('\u{1f136}', GlyphId::new(488)),
        ]
    }

    fn bmp_and_non_bmp_cmap_mappings() -> Vec<(char, GlyphId)> {
        let mut mappings = simple_cmap_mappings();
        mappings.extend(non_bmp_cmap_mappings());
        mappings
    }

    fn assert_cmap12_groups(
        font_data: FontData,
        cmap: &Cmap,
        record_index: usize,
        expected: &[(u32, u32, u32)],
    ) {
        let rec = &cmap.encoding_records()[record_index];
        let CmapSubtable::Format12(subtable) = rec.subtable(font_data).unwrap() else {
            panic!("Expected a cmap12 in {rec:?}");
        };
        let groups = subtable
            .groups()
            .iter()
            .map(|g| (g.start_char_code(), g.end_char_code(), g.start_glyph_id()))
            .collect::<Vec<_>>();
        assert_eq!(groups.len(), expected.len());
        assert_eq!(groups, expected);
    }

    #[test]
    fn generate_cmap4_and_12() {
        let mappings = bmp_and_non_bmp_cmap_mappings();

        let cmap = write::Cmap::from_mappings(mappings).unwrap();

        let bytes = dump_table(&cmap).unwrap();
        let font_data = FontData::new(&bytes);
        let cmap = Cmap::read(font_data).unwrap();

        assert_eq!(4, cmap.encoding_records().len(), "{cmap:?}");
        assert_eq!(
            vec![
                (PlatformId::Unicode, UNICODE_BMP_ENCODING),
                (PlatformId::Unicode, UNICODE_FULL_REPERTOIRE_ENCODING),
                (PlatformId::Windows, WINDOWS_BMP_ENCODING),
                (PlatformId::Windows, WINDOWS_FULL_REPERTOIRE_ENCODING)
            ],
            cmap.encoding_records()
                .iter()
                .map(|er| (er.platform_id(), er.encoding_id()))
                .collect::<Vec<_>>(),
            "{cmap:?}"
        );

        let encoding_records = cmap.encoding_records();
        let first_rec = &encoding_records[0];
        assert!(
            matches!(
                first_rec.subtable(font_data).unwrap(),
                CmapSubtable::Format4(_)
            ),
            "Expected a cmap4 in {first_rec:?}"
        );

        // (start_char_code, end_char_code, start_glyph_id)
        let expected_groups = vec![
            (10, 20, 0),
            (30, 90, 11),
            (153, 480, 72),
            (0x1f12f, 0x1f130, 481),
            (0x1f132, 0x1f133, 483),
            (0x1f134, 0x1f134, 486),
            (0x1f136, 0x1f136, 488),
        ];
        assert_cmap12_groups(font_data, &cmap, 1, &expected_groups);
        assert_cmap12_groups(font_data, &cmap, 3, &expected_groups);
    }

    #[test]
    fn generate_cmap12_only() {
        let mappings = non_bmp_cmap_mappings();

        let cmap = write::Cmap::from_mappings(mappings).unwrap();

        let bytes = dump_table(&cmap).unwrap();
        let font_data = FontData::new(&bytes);
        let cmap = Cmap::read(font_data).unwrap();

        assert_eq!(2, cmap.encoding_records().len(), "{cmap:?}");
        assert_eq!(
            vec![
                (PlatformId::Unicode, UNICODE_FULL_REPERTOIRE_ENCODING),
                (PlatformId::Windows, WINDOWS_FULL_REPERTOIRE_ENCODING)
            ],
            cmap.encoding_records()
                .iter()
                .map(|er| (er.platform_id(), er.encoding_id()))
                .collect::<Vec<_>>(),
            "{cmap:?}"
        );

        // (start_char_code, end_char_code, start_glyph_id)
        let expected_groups = vec![
            (0x1f12f, 0x1f130, 481),
            (0x1f132, 0x1f133, 483),
            (0x1f134, 0x1f134, 486),
            (0x1f136, 0x1f136, 488),
        ];
        assert_cmap12_groups(font_data, &cmap, 0, &expected_groups);
        assert_cmap12_groups(font_data, &cmap, 1, &expected_groups);
    }

    #[test]
    fn multiple_mappings_fails() {
        let mut mappings = non_bmp_cmap_mappings();
        // add an additional mapping to a different glyphId
        let (ch, gid1) = mappings[0];
        let gid2 = GlyphId::new(gid1.to_u32() + 1);
        mappings.push((ch, gid2));

        let result = write::Cmap::from_mappings(mappings);

        assert_eq!(result, Err(CmapConflict { ch, gid1, gid2 }))
    }
}
