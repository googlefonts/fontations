//! Implementation of Glyph Keyed patch application.
//!
//! Glyph Keyed patches are a type of incremental font patch which stores opaque data blobs
//! keyed by glyph id. Patch application places the data blobs into the appropriate place
//! in the base font based on the associated glyph id.
//!
//! Glyph Keyed patches are specified here:
//! <https://w3c.github.io/IFT/Overview.html#glyph-keyed>
use crate::patchmap::IftTableTag;
use crate::table_keyed::copy_unprocessed_tables;
use crate::{font_patch::PatchingError, patch_group::PatchInfo};

use font_types::Scalar;
use read_fonts::tables::ift::{IFTX_TAG, IFT_TAG};
use read_fonts::{
    collections::IntSet,
    tables::{
        ift::{GlyphKeyedPatch, GlyphPatches},
        loca::Loca,
    },
    types::Tag,
    FontData, FontRef, ReadError, TableProvider,
};

use shared_brotli_patch_decoder::shared_brotli_decode;
use skrifa::GlyphId;
use std::collections::{BTreeSet, HashMap};
use std::ops::RangeInclusive;

use write_fonts::FontBuilder;

pub(crate) fn apply_glyph_keyed_patches(
    patches: &[(&PatchInfo, GlyphKeyedPatch<'_>)],
    font: &FontRef,
) -> Result<Vec<u8>, PatchingError> {
    let mut decompression_buffer: Vec<Vec<u8>> = Vec::with_capacity(patches.len());

    for (_, patch) in patches {
        if patch.format() != Tag::new(b"ifgk") {
            return Err(PatchingError::InvalidPatch("Patch file tag is not 'ifgk'"));
        }

        decompression_buffer.push(
            shared_brotli_decode(
                patch.brotli_stream(),
                None,
                patch.max_uncompressed_length() as usize,
            )
            .map_err(PatchingError::from)?,
        );
    }

    let mut glyph_patches: Vec<GlyphPatches<'_>> = vec![];
    for (raw_data, patch) in decompression_buffer.iter().zip(patches) {
        glyph_patches.push(
            GlyphPatches::read(FontData::new(raw_data), patch.1.flags())
                .map_err(PatchingError::PatchParsingFailed)?,
        );
    }

    let num_glyphs = font
        .maxp()
        .map_err(PatchingError::FontParsingFailed)?
        .num_glyphs();

    let max_glyph_id = GlyphId::new(num_glyphs.checked_sub(1).ok_or(
        PatchingError::FontParsingFailed(ReadError::MalformedData("Font has no glyphs.")),
    )? as u32);

    // IFT and IFTX tables will be modified and then copied below.
    let mut processed_tables = BTreeSet::from([IFT_TAG, IFTX_TAG]);
    let mut font_builder = FontBuilder::new();

    for table_tag in table_tag_list(&glyph_patches)? {
        if table_tag == Tag::new(b"glyf") {
            let (Some(glyf), Ok(loca)) = (font.table_data(Tag::new(b"glyf")), font.loca(None))
            else {
                return Err(PatchingError::InvalidPatch(
                    "Trying to patch glyf/loca but base font doesn't have them.",
                ));
            };
            patch_glyf_and_loca(
                &glyph_patches,
                glyf.as_bytes(),
                loca,
                max_glyph_id,
                &mut font_builder,
            )?;
            // glyf patch application also generates a loca table.
            processed_tables.insert(table_tag);
            processed_tables.insert(Tag::new(b"loca"));
        } else if table_tag == Tag::new(b"CFF ")
            || table_tag == Tag::new(b"CFF2")
            || table_tag == Tag::new(b"gvar")
        {
            // TODO(garretrieger): add CFF, CFF2, and gvar support as well.
            return Err(PatchingError::InvalidPatch(
                "CFF, CFF2, and gvar patches are not yet supported.",
            ));
        } else {
            // All other table tags are ignored.
            continue;
        }
    }

    // Mark patches applied in IFT and IFTX as needed, copy the modified tables into the font builder.
    let mut new_itf_data = font
        .table_data(IFT_TAG)
        .map(|data| data.as_bytes().to_vec());
    let mut new_itfx_data = font
        .table_data(IFTX_TAG)
        .map(|data| data.as_bytes().to_vec());
    for (info, _) in patches {
        let data = match info.tag() {
            IftTableTag::Ift(_) => new_itf_data.as_mut().ok_or(PatchingError::InternalError)?,
            IftTableTag::Iftx(_) => new_itfx_data.as_mut().ok_or(PatchingError::InternalError)?,
        };
        let byte_index = info.application_flag_bit_index() / 8;
        let bit_index = (info.application_flag_bit_index() % 8) as u8;
        let byte = data
            .get_mut(byte_index)
            .ok_or(PatchingError::InternalError)?;
        *byte |= 1 << bit_index;
    }

    if let Some(data) = new_itf_data {
        font_builder.add_raw(IFT_TAG, data);
    }
    if let Some(data) = new_itfx_data {
        font_builder.add_raw(IFTX_TAG, data);
    }

    copy_unprocessed_tables(font, processed_tables, &mut font_builder);

    Ok(font_builder.build())
}

fn table_tag_list(glyph_patches: &[GlyphPatches]) -> Result<BTreeSet<Tag>, PatchingError> {
    for patches in glyph_patches {
        if patches
            .tables()
            .iter()
            .zip(patches.tables().iter().skip(1))
            .any(|(prev_tag, next_tag)| next_tag <= prev_tag)
        {
            return Err(PatchingError::InvalidPatch(
                "Duplicate or unsorted table tag.",
            ));
        }
    }

    Ok(glyph_patches
        .iter()
        .flat_map(|patch| patch.tables())
        .map(|tag| tag.get())
        .collect::<BTreeSet<Tag>>())
}

fn dedup_gid_replacement_data<'a>(
    glyph_patches: impl Iterator<Item = &'a GlyphPatches<'a>>,
    table_tag: Tag,
) -> Result<(IntSet<GlyphId>, Vec<&'a [u8]>), ReadError> {
    // Since the specification allows us to freely choose patch application order (for groups of glyph keyed patches,
    // see: https://w3c.github.io/IFT/Overview.html#extend-font-subset) if two patches affect the same gid we can choose
    // one arbitrarily to remain applied. In this case we choose the first applied patch for each gid be the one that takes
    // priority.
    let mut gids: IntSet<GlyphId> = IntSet::default();
    let mut data_for_gid: HashMap<GlyphId, &'a [u8]> = HashMap::default();
    for glyph_patch in glyph_patches {
        let Some((table_index, _)) = glyph_patch
            .tables()
            .iter()
            .enumerate()
            .find(|(_, tag)| tag.get() == table_tag)
        else {
            continue;
        };

        glyph_patch
            .glyph_data_for_table(table_index)
            .try_for_each(|result| {
                let (gid, data) = result?;
                data_for_gid.entry(gid).or_insert(data);
                gids.insert(gid);
                Ok(())
            })?;
    }

    let mut deduped: Vec<&'a [u8]> = Vec::with_capacity(data_for_gid.len());
    gids.iter().for_each(|gid| {
        // in the above loop for each gid in gids there is always an  entry in data_for_gid
        deduped.push(data_for_gid.get(&gid).unwrap());
    });

    Ok((gids, deduped))
}

fn retained_glyphs_in_font(
    replace_gids: &IntSet<GlyphId>,
    max_glyph_id: GlyphId,
) -> impl Iterator<Item = RangeInclusive<GlyphId>> + '_ {
    replace_gids
        .iter_excluded_ranges()
        .filter_map(move |range| {
            // Filter out values beyond max_glyph_id.
            if *range.start() > max_glyph_id {
                return None;
            }
            if *range.end() > max_glyph_id {
                return Some(*range.start()..=max_glyph_id);
            }
            Some(range)
        })
}

fn retained_glyphs_total_size(
    gids: &IntSet<GlyphId>,
    loca: &Loca,
    max_glyph_id: GlyphId,
) -> Result<u64, PatchingError> {
    let mut total_size = 0u64;
    for keep_range in retained_glyphs_in_font(gids, max_glyph_id) {
        let start = keep_range.start();
        let end = keep_range.end();

        let start_offset = loca
            .get_raw(start.to_u32() as usize)
            .ok_or(PatchingError::InvalidPatch("Start loca entry is missing."))?;
        let end_offset = loca
            .get_raw(end.to_u32() as usize + 1)
            .ok_or(PatchingError::InvalidPatch("End loca entry is missing."))?;

        total_size += end_offset
            .checked_sub(start_offset) // TODO: this can be removed if we pre-verify ascending order
            .ok_or(PatchingError::FontParsingFailed(ReadError::MalformedData(
                "loca entries are not in ascending order",
            )))? as u64;
    }

    Ok(total_size)
}

trait LocaOffset {
    fn write_to(self, dest: &mut [u8]);
}

impl LocaOffset for u32 {
    fn write_to(self, dest: &mut [u8]) {
        let data: [u8; 4] = self.to_raw();
        dest[..4].copy_from_slice(&data);
    }
}

impl LocaOffset for u16 {
    fn write_to(self, dest: &mut [u8]) {
        let data: [u8; 2] = (self / 2).to_raw();
        dest[..2].copy_from_slice(&data);
    }
}

fn synthesize_glyf_and_loca<OffsetType: LocaOffset + TryFrom<usize>>(
    gids: &IntSet<GlyphId>,
    max_glyph_id: GlyphId,
    replacement_data: &[&[u8]],
    glyf: &[u8],
    loca: &Loca<'_>,
    new_glyf: &mut [u8],
    new_loca: &mut [u8],
) -> Result<(), PatchingError> {
    if !loca.all_offsets_are_ascending() {
        return Err(PatchingError::FontParsingFailed(ReadError::MalformedData(
            "loca contains unordered offsets.",
        )));
    }

    let mut replace_it = gids.iter_ranges().peekable();
    let mut keep_it = retained_glyphs_in_font(gids, max_glyph_id).peekable();
    let mut replacement_data_it = replacement_data.iter();
    let mut write_index = 0;
    let is_short_loca = match loca {
        Loca::Short(_) => true,
        Loca::Long(_) => false,
    };
    let off_size = std::mem::size_of::<OffsetType>();

    loop {
        let (range, replace) = match (replace_it.peek(), keep_it.peek()) {
            (Some(replace), Some(keep)) => {
                if replace.start() <= keep.start() {
                    (replace_it.next().unwrap(), true)
                } else {
                    (keep_it.next().unwrap(), false)
                }
            }
            (Some(_), None) => (replace_it.next().unwrap(), true),
            (None, Some(_)) => (keep_it.next().unwrap(), false),
            (None, None) => break,
        };

        let (start, end) = (
            range.start().to_u32() as usize,
            range.end().to_u32() as usize,
        );

        if replace {
            for gid in start..=end {
                let data = *replacement_data_it
                    .next()
                    .ok_or(PatchingError::InternalError)?;

                new_glyf
                    .get_mut(write_index..write_index + data.len())
                    .ok_or(PatchingError::InternalError)?
                    .copy_from_slice(data);

                let loca_off: OffsetType = write_index
                    .try_into()
                    .map_err(|_| PatchingError::InternalError)?;

                loca_off.write_to(
                    new_loca
                        .get_mut(gid * off_size..)
                        .ok_or(PatchingError::InternalError)?,
                );

                write_index += data.len();
                if is_short_loca {
                    // Add padding for short loca.
                    write_index += data.len() % 2;
                }
            }
        } else {
            let start_off = loca.get_raw(start).ok_or(PatchingError::InternalError)? as usize;
            let end_off = loca.get_raw(end + 1).ok_or(PatchingError::InternalError)? as usize;
            let len = end_off
                .checked_sub(start_off)
                .ok_or(PatchingError::InternalError)?;
            new_glyf
                .get_mut(write_index..write_index + len)
                .ok_or(PatchingError::InternalError)?
                .copy_from_slice(
                    glyf.get(start_off..end_off)
                        .ok_or(PatchingError::InternalError)?,
                );

            for gid in start..=end {
                let cur_off = loca.get_raw(gid).ok_or(PatchingError::InternalError)? as usize;
                let new_off = cur_off - start_off + write_index;

                let new_off: OffsetType = new_off
                    .try_into()
                    .map_err(|_| PatchingError::InternalError)?;
                new_off.write_to(
                    new_loca
                        .get_mut(gid * off_size..)
                        .ok_or(PatchingError::InternalError)?,
                );
            }

            write_index += len;
        }
    }

    // Write the last loca offset
    let loca_off: OffsetType = write_index
        .try_into()
        .map_err(|_| PatchingError::InternalError)?;
    loca_off.write_to(
        new_loca
            .get_mut(new_loca.len() - 2..)
            .ok_or(PatchingError::InternalError)?,
    );

    Ok(())
}

fn patch_glyf_and_loca<'a>(
    glyph_patches: &'a [GlyphPatches<'a>],
    glyf: &[u8],
    loca: Loca<'a>,
    max_glyph_id: GlyphId,
    font_builder: &mut FontBuilder,
) -> Result<(), PatchingError> {
    // TODO(garretrieger) using traits, generalize this approach to any of the supported table types.

    let is_short = match loca {
        Loca::Short(_) => true,
        Loca::Long(_) => false,
    };

    // Step 0: merge the individual patches into a list of replacement data for gid.
    // TODO(garretrieger): special case where gids is empty, just returned umodified copy of glyf + loca?
    let (gids, replacement_data) =
        dedup_gid_replacement_data(glyph_patches.iter(), Tag::new(b"glyf"))
            .map_err(PatchingError::PatchParsingFailed)?;

    // Step 1: determine the new total size of glyf
    let mut total_glyf_size = retained_glyphs_total_size(&gids, &loca, max_glyph_id)?;
    for data in replacement_data.iter() {
        let len = data.len() as u64;
        // note: include padding as needed for short loca
        total_glyf_size += len + if is_short { len % 2 } else { 0 };
    }

    // TODO(garretrieger): pre-check loca has all ascending offsets.
    // TODO(garretrieger): check if loca format will need to switch, if so that's an error.

    if gids.last().unwrap_or(GlyphId::new(0)) > max_glyph_id {
        return Err(PatchingError::InvalidPatch(
            "Patch would add a glyph beyond this fonts maximum.",
        ));
    }

    // Step 2: patch together the new glyf (by copying in ranges of data in the correct order).
    let loca_size = (max_glyph_id.to_u32() as usize + 2) * if is_short { 2 } else { 4 };
    let mut new_glyf = vec![0u8; total_glyf_size as usize];
    let mut new_loca = vec![0u8; loca_size];
    if is_short {
        synthesize_glyf_and_loca::<u16>(
            &gids,
            max_glyph_id,
            &replacement_data,
            glyf,
            &loca,
            new_glyf.as_mut_slice(),
            new_loca.as_mut_slice(),
        )?;
    } else {
        synthesize_glyf_and_loca::<u32>(
            &gids,
            max_glyph_id,
            &replacement_data,
            glyf,
            &loca,
            new_glyf.as_mut_slice(),
            new_loca.as_mut_slice(),
        )?;
    }

    // Step 3: add new tables to the output builder
    font_builder.add_raw(Tag::new(b"glyf"), new_glyf);
    font_builder.add_raw(Tag::new(b"loca"), new_loca);

    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use std::{
        collections::{BTreeSet, HashMap},
        io::Write,
    };

    use brotlic::CompressorWriter;
    use read_fonts::{
        tables::ift::{CompatibilityId, GlyphKeyedPatch, IFTX_TAG, IFT_TAG},
        test_helpers::BeBuffer,
        FontData, FontRead, ReadError, TableProvider,
    };

    use font_test_data::ift::{
        glyf_and_gvar_u16_glyph_patches, glyf_u16_glyph_patches, glyf_u16_glyph_patches_2,
        glyph_keyed_patch_header, noop_glyf_glyph_patches, test_font_for_patching,
        test_font_for_patching_with_loca_mod,
    };
    use skrifa::{FontRef, Tag};

    use crate::{
        font_patch::PatchingError,
        glyph_keyed::apply_glyph_keyed_patches,
        patchmap::{PatchFormat, PatchUri},
    };

    use super::{IftTableTag, PatchInfo};

    pub(crate) fn assemble_glyph_keyed_patch(mut header: BeBuffer, payload: BeBuffer) -> BeBuffer {
        let payload_data: &[u8] = &payload;
        let mut compressor = CompressorWriter::new(Vec::new());
        compressor.write_all(payload_data).unwrap();
        let compressed = compressor.into_inner().unwrap();

        header.write_at("max_uncompressed_length", payload_data.len() as u32);
        header.extend(compressed)
    }

    fn check_tables_equal(a: &FontRef, b: &FontRef, excluding: BTreeSet<Tag>) {
        let it_a = a
            .table_directory
            .table_records()
            .iter()
            .map(|r| r.tag())
            .filter(|tag| !excluding.contains(tag));
        let it_b = b
            .table_directory
            .table_records()
            .iter()
            .map(|r| r.tag())
            .filter(|tag| !excluding.contains(tag));

        for (tag_a, tag_b) in it_a.zip(it_b) {
            assert_eq!(tag_a, tag_b);
            let data_a = a.table_data(tag_a).unwrap();
            let data_b = b.table_data(tag_b).unwrap();
            assert_eq!(data_a.as_bytes(), data_b.as_bytes(), "{}", tag_a);
        }
    }

    fn patch_info(tag: Tag, bit_index: usize) -> PatchInfo {
        let source = match &tag.to_be_bytes() {
            b"IFT " => IftTableTag::Ift(CompatibilityId::from_u32s([0, 0, 0, 0])),
            b"IFTX" => IftTableTag::Iftx(CompatibilityId::from_u32s([0, 0, 0, 0])),
            _ => panic!("Unexpected tag value."),
        };
        PatchUri::from_index(
            "",
            0,
            source,
            bit_index,
            PatchFormat::GlyphKeyed,
            Default::default(),
        )
        .into()
    }

    #[test]
    fn noop_glyph_keyed() {
        let patch =
            assemble_glyph_keyed_patch(glyph_keyed_patch_header(), noop_glyf_glyph_patches());
        let patch: &[u8] = &patch;
        let patch = GlyphKeyedPatch::read(FontData::new(patch)).unwrap();

        let font = test_font_for_patching();
        let font = FontRef::new(&font).unwrap();

        let patch_info = patch_info(IFT_TAG, 4);

        let patched = apply_glyph_keyed_patches(&[(&patch_info, patch)], &font).unwrap();
        let patched = FontRef::new(&patched).unwrap();

        // Application bit will be set in the patched font.
        let expected_font = test_font_for_patching_with_loca_mod(
            |_| {},
            HashMap::from([(IFT_TAG, vec![0b0001_0000, 0, 0, 0].as_slice())]),
        );
        let expected_font = FontRef::new(&expected_font).unwrap();
        check_tables_equal(&expected_font, &patched, BTreeSet::default());
    }

    #[test]
    fn basic_glyph_keyed() {
        let patch =
            assemble_glyph_keyed_patch(glyph_keyed_patch_header(), glyf_u16_glyph_patches());
        let patch: &[u8] = &patch;
        let patch = GlyphKeyedPatch::read(FontData::new(patch)).unwrap();

        let font = test_font_for_patching();
        let font = FontRef::new(&font).unwrap();

        let patch_info = patch_info(IFT_TAG, 28);
        let patched = apply_glyph_keyed_patches(&[(&patch_info, patch)], &font).unwrap();
        let patched = FontRef::new(&patched).unwrap();

        let new_ift: &[u8] = patched.table_data(IFT_TAG).unwrap().as_bytes();
        assert_eq!(&[0, 0, 0, 0b0001_0000], new_ift);

        let new_glyf: &[u8] = patched.table_data(Tag::new(b"glyf")).unwrap().as_bytes();
        assert_eq!(
            &[
                1, 2, 3, 4, 5, 0, // gid 0
                6, 7, 8, 0, // gid 1
                b'a', b'b', b'c', 0, // gid2
                b'd', b'e', b'f', b'g', // gid 7
                b'h', b'i', b'j', b'k', b'l', 0, // gid 8 + 9
                b'm', b'n', // gid 13
            ],
            new_glyf
        );

        let new_loca = patched.loca(None).unwrap();
        let indices: Vec<u32> = (0..=15).map(|gid| new_loca.get_raw(gid).unwrap()).collect();

        assert_eq!(
            vec![
                0,  // gid 0
                6,  // gid 1
                10, // gid 2
                14, // gid 3
                14, // gid 4
                14, // gid 5
                14, // gid 6
                14, // gid 7
                18, // gid 8
                18, // gid 9
                24, // gid 10
                24, // gid 11
                24, // gid 12
                24, // gid 13
                26, // gid 14
                26, // end
            ],
            indices
        );

        check_tables_equal(
            &font,
            &patched,
            [IFT_TAG, Tag::new(b"glyf"), Tag::new(b"loca")].into(),
        );
    }

    #[test]
    fn multiple_glyph_keyed() {
        let patch =
            assemble_glyph_keyed_patch(glyph_keyed_patch_header(), glyf_u16_glyph_patches());
        let patch: &[u8] = &patch;
        let patch1 = GlyphKeyedPatch::read(FontData::new(patch)).unwrap();
        let patch_info_1 = patch_info(IFTX_TAG, 13);

        let patch =
            assemble_glyph_keyed_patch(glyph_keyed_patch_header(), glyf_u16_glyph_patches_2());
        let patch: &[u8] = &patch;
        let patch2 = GlyphKeyedPatch::read(FontData::new(patch)).unwrap();
        let patch_info_2 = patch_info(IFTX_TAG, 28);

        let font = test_font_for_patching_with_loca_mod(
            |_| {},
            HashMap::from([(IFTX_TAG, vec![0, 0, 0, 0].as_slice())]),
        );
        let font = FontRef::new(&font).unwrap();

        let patched =
            apply_glyph_keyed_patches(&[(&patch_info_2, patch2), (&patch_info_1, patch1)], &font)
                .unwrap();
        let patched = FontRef::new(&patched).unwrap();

        let new_ift: &[u8] = patched.table_data(IFTX_TAG).unwrap().as_bytes();
        assert_eq!(&[0, 0b0010_0000, 0, 0b0001_0000], new_ift);

        let new_glyf: &[u8] = patched.table_data(Tag::new(b"glyf")).unwrap().as_bytes();
        assert_eq!(
            &[
                1, 2, 3, 4, 5, 0, // gid 0
                6, 7, 8, 0, // gid 1
                b'a', b'b', b'c', 0, // gid2
                b'q', b'r', // gid 7
                b'h', b'i', b'j', b'k', b'l', 0, // gid 8 + 9
                b's', b't', b'u', 0, // gid 12
                b'm', b'n', // gid 13
                b'v', 0, // gid 14
            ],
            new_glyf
        );

        let new_loca = patched.loca(None).unwrap();
        let indices: Vec<u32> = (0..=15).map(|gid| new_loca.get_raw(gid).unwrap()).collect();

        assert_eq!(
            vec![
                0,  // gid 0
                6,  // gid 1
                10, // gid 2
                14, // gid 3
                14, // gid 4
                14, // gid 5
                14, // gid 6
                14, // gid 7
                16, // gid 8
                16, // gid 9
                22, // gid 10
                22, // gid 11
                22, // gid 12
                26, // gid 13
                28, // gid 14
                30, // end
            ],
            indices
        );

        check_tables_equal(
            &font,
            &patched,
            [Tag::new(b"glyf"), Tag::new(b"loca"), IFTX_TAG].into(),
        );
    }

    #[test]
    fn glyph_keyed_bad_format() {
        let mut header_builder = glyph_keyed_patch_header();
        header_builder.write_at("format", Tag::new(b"iftk"));
        let patch = assemble_glyph_keyed_patch(header_builder, glyf_u16_glyph_patches());
        let patch: &[u8] = &patch;
        let patch = GlyphKeyedPatch::read(FontData::new(patch)).unwrap();
        let patch_info = patch_info(IFT_TAG, 0);

        let font = test_font_for_patching();
        let font = FontRef::new(&font).unwrap();

        assert_eq!(
            apply_glyph_keyed_patches(&[(&patch_info, patch)], &font),
            Err(PatchingError::InvalidPatch("Patch file tag is not 'ifgk'"))
        );
    }

    #[test]
    fn glyph_keyed_unsupported_table() {
        let patch = assemble_glyph_keyed_patch(
            glyph_keyed_patch_header(),
            glyf_and_gvar_u16_glyph_patches(),
        );
        let patch: &[u8] = &patch;
        let patch = GlyphKeyedPatch::read(FontData::new(patch)).unwrap();
        let patch_info = patch_info(IFT_TAG, 0);

        let font = test_font_for_patching();
        let font = FontRef::new(&font).unwrap();

        assert_eq!(
            apply_glyph_keyed_patches(&[(&patch_info, patch)], &font),
            Err(PatchingError::InvalidPatch(
                "CFF, CFF2, and gvar patches are not yet supported."
            ))
        );
    }

    #[test]
    fn glyph_keyed_unknown_table() {
        let mut builder = glyf_and_gvar_u16_glyph_patches();
        builder.write_at("gvar_tag", Tag::new(b"hijk"));

        let patch = assemble_glyph_keyed_patch(glyph_keyed_patch_header(), builder);
        let patch: &[u8] = &patch;
        let patch = GlyphKeyedPatch::read(FontData::new(patch)).unwrap();
        let patch_info = patch_info(IFT_TAG, 0);

        let font = test_font_for_patching();
        let font = FontRef::new(&font).unwrap();

        let patched = apply_glyph_keyed_patches(&[(&patch_info, patch)], &font).unwrap();
        let patched = FontRef::new(&patched).unwrap();

        let new_glyf: &[u8] = patched.table_data(Tag::new(b"glyf")).unwrap().as_bytes();
        assert_eq!(
            &[
                1, 2, 3, 4, 5, 0, // gid 0
                6, 7, 8, 0, // gid 1
                b'a', b'b', b'c', 0, // gid2
                b'd', b'e', b'f', b'g', // gid 7
                b'h', b'i', b'j', b'k', b'l', 0, // gid 8
            ],
            new_glyf
        );

        let new_loca = patched.loca(None).unwrap();
        let indices: Vec<u32> = (0..=15).map(|gid| new_loca.get_raw(gid).unwrap()).collect();

        assert_eq!(
            vec![
                0,  // gid 0
                6,  // gid 1
                10, // gid 2
                14, // gid 3
                14, // gid 4
                14, // gid 5
                14, // gid 6
                14, // gid 7
                18, // gid 8
                24, // gid 9
                24, // gid 10
                24, // gid 11
                24, // gid 12
                24, // gid 13
                24, // gid 14
                24, // end
            ],
            indices
        );

        check_tables_equal(
            &font,
            &patched,
            [Tag::new(b"glyf"), Tag::new(b"loca"), IFT_TAG].into(),
        );
    }

    #[test]
    fn glyph_keyed_unsorted_tables() {
        let mut builder = glyf_and_gvar_u16_glyph_patches();
        builder.write_at("gvar_tag", Tag::new(b"glye"));
        let patch = assemble_glyph_keyed_patch(glyph_keyed_patch_header(), builder);
        let patch: &[u8] = &patch;
        let patch = GlyphKeyedPatch::read(FontData::new(patch)).unwrap();
        let patch_info = patch_info(IFT_TAG, 0);

        let font = test_font_for_patching();
        let font = FontRef::new(&font).unwrap();

        assert_eq!(
            apply_glyph_keyed_patches(&[(&patch_info, patch)], &font),
            Err(PatchingError::InvalidPatch(
                "Duplicate or unsorted table tag."
            ))
        );
    }

    #[test]
    fn glyph_keyed_duplicate_tables() {
        let mut builder = glyf_and_gvar_u16_glyph_patches();
        builder.write_at("gvar_tag", Tag::new(b"glyf"));
        let patch = assemble_glyph_keyed_patch(glyph_keyed_patch_header(), builder);
        let patch: &[u8] = &patch;
        let patch = GlyphKeyedPatch::read(FontData::new(patch)).unwrap();
        let patch_info = patch_info(IFT_TAG, 0);

        let font = test_font_for_patching();
        let font = FontRef::new(&font).unwrap();

        assert_eq!(
            apply_glyph_keyed_patches(&[(&patch_info, patch)], &font),
            Err(PatchingError::InvalidPatch(
                "Duplicate or unsorted table tag."
            ))
        );
    }

    #[test]
    fn glyph_keyed_unsorted_gids() {
        let mut builder = glyf_u16_glyph_patches();
        builder.write_at("gid_8", 6);
        let patch = assemble_glyph_keyed_patch(glyph_keyed_patch_header(), builder);
        let patch: &[u8] = &patch;
        let patch = GlyphKeyedPatch::read(FontData::new(patch)).unwrap();
        let patch_info = patch_info(IFT_TAG, 0);

        let font = test_font_for_patching();
        let font = FontRef::new(&font).unwrap();

        assert_eq!(
            apply_glyph_keyed_patches(&[(&patch_info, patch)], &font),
            Err(PatchingError::PatchParsingFailed(ReadError::MalformedData(
                "Glyph IDs are unsorted or duplicated."
            ))),
        );
    }

    #[test]
    fn glyph_keyed_duplicate_gids() {
        let mut builder = glyf_u16_glyph_patches();
        builder.write_at("gid_8", 7);
        let patch = assemble_glyph_keyed_patch(glyph_keyed_patch_header(), builder);
        let patch: &[u8] = &patch;
        let patch = GlyphKeyedPatch::read(FontData::new(patch)).unwrap();
        let patch_info = patch_info(IFT_TAG, 0);

        let font = test_font_for_patching();
        let font = FontRef::new(&font).unwrap();

        assert_eq!(
            apply_glyph_keyed_patches(&[(&patch_info, patch)], &font),
            Err(PatchingError::PatchParsingFailed(ReadError::MalformedData(
                "Glyph IDs are unsorted or duplicated."
            ))),
        );
    }

    #[test]
    fn glyph_keyed_uncompressed_length_to_small() {
        let len = glyf_u16_glyph_patches().as_slice().len();
        let mut patch =
            assemble_glyph_keyed_patch(glyph_keyed_patch_header(), glyf_u16_glyph_patches());
        patch.write_at("max_uncompressed_length", len as u32 - 1);
        let patch: &[u8] = &patch;
        let patch = GlyphKeyedPatch::read(FontData::new(patch)).unwrap();
        let patch_info = patch_info(IFT_TAG, 0);

        let font = test_font_for_patching();
        let font = FontRef::new(&font).unwrap();

        assert_eq!(
            apply_glyph_keyed_patches(&[(&patch_info, patch)], &font),
            Err(PatchingError::InvalidPatch("Max size exceeded.")),
        );
    }

    #[test]
    fn glyph_keyed_max_glyph_exceeded() {
        let mut builder = glyf_u16_glyph_patches();
        builder.write_at("gid_13", 15u16);
        let patch = assemble_glyph_keyed_patch(glyph_keyed_patch_header(), builder);
        let patch: &[u8] = &patch;
        let patch = GlyphKeyedPatch::read(FontData::new(patch)).unwrap();
        let patch_info = patch_info(IFT_TAG, 0);

        let font = test_font_for_patching();
        let font = FontRef::new(&font).unwrap();

        assert_eq!(
            apply_glyph_keyed_patches(&[(&patch_info, patch)], &font),
            Err(PatchingError::InvalidPatch(
                "Patch would add a glyph beyond this fonts maximum."
            )),
        );
    }

    #[test]
    fn glyph_keyed_unordered_loca_offsets() {
        let patch =
            assemble_glyph_keyed_patch(glyph_keyed_patch_header(), glyf_u16_glyph_patches());
        let patch: &[u8] = &patch;
        let patch = GlyphKeyedPatch::read(FontData::new(patch)).unwrap();
        let patch_info = patch_info(IFT_TAG, 0);

        // unorder offsets related to a glyph not being replaced
        let font = test_font_for_patching_with_loca_mod(
            |loca| {
                let loca_gid_1 = loca[1];
                let loca_gid_2 = loca[2];
                loca[1] = loca_gid_2;
                loca[2] = loca_gid_1;
            },
            Default::default(),
        );

        let font = FontRef::new(&font).unwrap();

        assert_eq!(
            apply_glyph_keyed_patches(&[(&patch_info, patch)], &font),
            Err(PatchingError::FontParsingFailed(ReadError::MalformedData(
                "loca contains unordered offsets."
            ))),
        );
    }

    // TODO test of invalid cases:
    // - patch data offsets unordered.
    // - loca offset type switch required.
    // - glyph keyed test with large number of offsets to check type conversion on (glyphCount * tableCount)
    // - test that glyph keyed patches are idempotent.
}
