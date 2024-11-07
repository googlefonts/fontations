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

impl CmapSubtable {
    /// Create a new format 4 `CmapSubtable` from a list of `(char, GlyphId)` pairs.
    ///
    /// The pairs are expected to be already sorted and deduplicated.
    /// Characters beyond the BMP are ignored. If all characters are beyond the BMP
    /// then `None` is returned.
    fn create_format_4(mappings: &[(char, GlyphId)]) -> Option<Self> {
        let mut end_code = Vec::new();
        let mut start_code = Vec::new();
        let mut id_deltas = Vec::new();

        let mut prev = (u16::MAX - 1, u16::MAX - 1);
        for (cp, gid) in mappings {
            let Ok(gid) = u16::try_from(gid.to_u32()) else {
                // Should we just fail here?
                continue;
            };
            let Ok(cp) = u16::try_from(*cp as u32) else {
                // mappings is sorted, so the rest will be beyond the BMP too.
                break;
            };
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

        let id_range_offsets = vec![0; id_deltas.len()];
        Some(CmapSubtable::format_4(
            0, // 'lang' set to zero for all 'cmap' subtables whose platform IDs are other than Macintosh
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

        let seq_map_groups = groups
            .into_iter()
            .map(|(start_char, end_char, gid)| SequentialMapGroup::new(start_char, end_char, gid))
            .collect::<Vec<_>>();
        CmapSubtable::format_12(
            0, // 'lang' set to zero for all 'cmap' subtables whose platform IDs are other than Macintosh
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
    /// Generates a ['cmap'] that is expected to work in most modern environments.
    ///
    /// The input is not required to be sorted.
    ///
    /// This emits [format 4] and [format 12] subtables, respectively for the
    /// Basic Multilingual Plane and Full Unicode Repertoire.
    ///
    /// Also see: <https://learn.microsoft.com/en-us/typography/opentype/spec/recom#cmap-table>
    ///
    /// [`cmap`]: https://learn.microsoft.com/en-us/typography/opentype/spec/cmap
    /// [format 4]: https://learn.microsoft.com/en-us/typography/opentype/spec/cmap#format-4-segment-mapping-to-delta-values
    /// [format 12]: https://learn.microsoft.com/en-us/typography/opentype/spec/cmap#format-12-segmented-coverage
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
// a helper for iterating over ranges for cmap 4
struct Format4Ranges<'a> {
    mappings: &'a [(char, GlyphId)],
    seg_start: usize,
    currently_contiguous: bool,
}

#[derive(Clone, Copy, Debug)]
struct Format4Segment {
    // indices are into the source mappings
    start_ix: usize,
    end_ix: usize,
    id_delta: Option<i32>,
}

impl Format4Segment {
    fn len(&self) -> usize {
        self.end_ix - self.start_ix + 1
    }

    // cost in bytes of this segment
    fn cost(&self) -> usize {
        const BASE_COST: usize = 8; // 4 u16s for a new segment
        let glyph_id_cost = self
            .id_delta
            .is_none()
            .then(|| self.len() * u16::RAW_BYTE_LEN)
            .unwrap_or(0);
        BASE_COST + glyph_id_cost
    }

    fn combine(&self, other: Format4Segment) -> Format4Segment {
        assert_eq!(other.start_ix, self.end_ix + 1);
        Format4Segment {
            start_ix: self.start_ix,
            end_ix: other.end_ix,
            id_delta: None,
        }
    }
}

impl<'a> Format4Ranges<'a> {
    fn new(mappings: &'a [(char, GlyphId)]) -> Self {
        // ignore chars above BMP:
        let mappings = mappings
            .iter()
            .position(|(c, _)| u16::try_from(*c as u32).is_err())
            .map(|bad_idx| &mappings[..bad_idx])
            .unwrap_or(mappings);
        Self {
            mappings,
            seg_start: 0,
            currently_contiguous: false,
        }
    }

    /// a convenience method called from our iter in the various cases where
    /// we emit a segment.
    fn make_segment(&mut self, seg_len: usize) -> Format4Segment {
        let result = Format4Segment {
            start_ix: self.seg_start,
            end_ix: self.seg_start + seg_len,
            id_delta: self
                .currently_contiguous
                .then_some(
                    self.mappings
                        .get(self.seg_start)
                        .map(|(cp, gid)| gid.to_u32() as i32 - *cp as u32 as i32),
                )
                .flatten(),
        };
        self.seg_start += seg_len + 1;
        self.currently_contiguous = false;
        result
    }
}

// this iterator creates segments greedily, that is it will create a new segment
// at every opportunity. These are then recombined by the caller to generate
// the most efficient overall sequence of segments.
impl<'a> Iterator for Format4Ranges<'a> {
    type Item = Format4Segment;
    fn next(&mut self) -> Option<Format4Segment> {
        if self.seg_start == self.mappings.len() {
            return None;
        }

        let Some(((mut prev_cp, mut prev_gid), rest)) =
            self.mappings[self.seg_start..].split_first()
        else {
            // if this is the last element, make a final segment
            return Some(self.make_segment(0));
        };

        for (i, (cp, gid)) in rest.iter().enumerate() {
            // first: all segments must be a contiguous range of codepoints
            if *cp as u32 - prev_cp as u32 > 1 {
                return Some(self.make_segment(i));
            }
            let next_is_contiguous = gid.to_u32().saturating_sub(prev_gid.to_u32()) == 1;
            if !next_is_contiguous {
                // next: if prev gids were ordered but this one isn't, end prev segment
                if self.currently_contiguous {
                    return Some(self.make_segment(i));
                }
            // and the funny case:
            // if we were not previously contiguous but are now:
            // - if i == 0, then this is the first item in a new segment;
            //   set is_contiguous and continue
            // - if i > 0, we need to back up one
            } else if !self.currently_contiguous {
                if i == 0 {
                    self.currently_contiguous = true;
                } else {
                    return Some(self.make_segment(i - 1));
                }
            }
            prev_cp = *cp;
            prev_gid = *gid;
        }

        // if we're done looping then create the last segment:
        let last_idx = self.mappings.len() - 1;
        return Some(self.make_segment(last_idx - self.seg_start));
    }
}

/// Computes an efficient set of segments
fn compute_format_4_segments(mappings: &[(char, GlyphId)]) -> Vec<Format4Segment> {
    assert!(!mappings.is_empty());
    let mut iter = Format4Ranges::new(mappings).peekable();
    let Some(first) = iter.next() else {
        return Default::default();
    };

    let mut result = vec![first];

    // now we want to collect the segments, combining smaller segments where
    // that leads to a size savings.
    //
    // This differs from the python, which starts from larger segments and then
    // subdivides them, but the overall idea is the same.
    // (https://github.com/fonttools/fonttools/blob/f1d3e116d54f/Lib/fontTools/ttLib/tables/_c_m_a_p.py#L783)
    while let Some(segment) = iter.next() {
        let prev = result.last_mut().unwrap();
        let is_contiguous_with_prev =
            mappings[prev.end_ix].0 as u32 + 1 == mappings[segment.start_ix].0 as u32;
        let is_contiguous_with_next = iter
            .peek()
            .map(|next| mappings[segment.end_ix].0 as u32 + 1 == mappings[next.start_ix].0 as u32)
            .unwrap_or(false);
        // first: if segment is not contiguous with either prev or next, it can't be
        // combined, so just push and continue

        if !(is_contiguous_with_prev | is_contiguous_with_next) {
            result.push(segment);
            continue;
        }

        // next: if chars are contiguous and neither has a delta, we always combine
        // this will mostly happen if we've combined the previous, previously
        // delta-having segment
        if is_contiguous_with_prev && prev.id_delta.is_none() && segment.id_delta.is_none() {
            *prev = prev.combine(segment);
            continue;
        }
        // next: if contiguous, combine only if it saves bytes
        if is_contiguous_with_prev {
            let combined = prev.combine(segment);
            if combined.cost() < prev.cost() + segment.cost() {
                *prev = combined;
                continue;
            }
        }
        result.push(segment);
    }
    result
}

impl Cmap4 {
    fn compute_length(&self) -> u16 {
        // https://learn.microsoft.com/en-us/typography/opentype/spec/cmap#format-4-segment-mapping-to-delta-values
        // there are always 8 u16 fields
        const FIXED_SIZE: usize = 8 * u16::RAW_BYTE_LEN;
        const PER_SEGMENT_LEN: usize = 4 * u16::RAW_BYTE_LEN;

        let segment_len = self.end_code.len() * PER_SEGMENT_LEN;
        let gid_len = self.glyph_id_array.len() * u16::RAW_BYTE_LEN;

        (FIXED_SIZE + segment_len + gid_len)
            .try_into()
            .expect("cmap4 overflow")
    }

    fn compute_search_range(&self) -> u16 {
        SearchRange::compute(self.end_code.len(), u16::RAW_BYTE_LEN).search_range
    }

    fn compute_entry_selector(&self) -> u16 {
        SearchRange::compute(self.end_code.len(), u16::RAW_BYTE_LEN).entry_selector
    }

    fn compute_range_shift(&self) -> u16 {
        SearchRange::compute(self.end_code.len(), u16::RAW_BYTE_LEN).range_shift
    }
}

impl Cmap12 {
    fn compute_length(&self) -> u32 {
        // https://learn.microsoft.com/en-us/typography/opentype/spec/cmap#format-12-segmented-coverage
        const FIXED_SIZE: usize = 2 * u16::RAW_BYTE_LEN + 3 * u32::RAW_BYTE_LEN;
        const PER_SEGMENT_LEN: usize = 3 * u32::RAW_BYTE_LEN;

        (FIXED_SIZE + PER_SEGMENT_LEN * self.groups.len())
            .try_into()
            .unwrap()
    }
}

#[cfg(test)]
mod tests {
    use std::ops::RangeInclusive;

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

    // input is a sequence of ranges representing contiguous gids,
    // output is how they're grouped into segments
    fn compute_ranges(
        iter: impl IntoIterator<Item = RangeInclusive<char>>,
    ) -> Vec<RangeInclusive<char>> {
        let mut next_gid = 0u16;
        let mappings = iter
            .into_iter()
            .flat_map(|range| {
                // start of new range means we aren't contiguous:
                let start_gid = next_gid;
                next_gid += 1 + range.clone().count() as u16;
                range
                    .enumerate()
                    .map(move |(i, c)| (c, GlyphId::new((start_gid + i as u16) as _)))
            })
            .collect::<Vec<_>>();

        super::compute_format_4_segments(&mappings)
            .into_iter()
            .map(|seg| mappings[seg.start_ix].0..=mappings[seg.end_ix].0)
            .collect()
    }

    #[derive(Default)]
    struct MappingBuilder {
        mappings: Vec<(char, GlyphId)>,
        next_gid: u16,
    }

    impl MappingBuilder {
        fn extend(mut self, range: impl IntoIterator<Item = char>) -> Self {
            for c in range {
                let gid = GlyphId::new(self.next_gid as _);
                self.mappings.push((c, gid));
                self.next_gid += 1;
            }
            self
        }

        // compute the segments for the mapping
        fn compute(&mut self) -> Vec<RangeInclusive<char>> {
            self.mappings.sort();
            super::compute_format_4_segments(&self.mappings)
                .into_iter()
                .map(|seg| self.mappings[seg.start_ix].0..=self.mappings[seg.end_ix].0)
                .collect()
        }
    }

    #[test]
    fn f4_segments_simple() {
        let mut one_big_discontiguous_mapping = MappingBuilder::default().extend(('a'..='z').rev());
        assert_eq!(one_big_discontiguous_mapping.compute(), ['a'..='z']);
    }

    #[test]
    fn f4_segments_combine_small() {
        let mut mapping = MappingBuilder::default()
            // backwards so gids are not contiguous
            .extend(['e', 'd', 'c', 'b', 'a'])
            // these two contiguous ranges aren't worth the cost, should merge
            // into the first and last respectively
            .extend('f'..='g')
            .extend('m'..='n')
            .extend(('o'..='z').rev());

        assert_eq!(mapping.compute(), ['a'..='g', 'm'..='z']);
    }

    #[test]
    fn f4_segments_keep() {
        let mut mapping = MappingBuilder::default()
            .extend('a'..='m')
            .extend(['o', 'n']);

        assert_eq!(mapping.compute(), ['a'..='m', 'n'..='o']);
    }
}
