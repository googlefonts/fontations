/// Implementation of Glyph Keyed patch application.
///
/// Glyph Keyed patches are a type of incremental font patch which stores opaque data blobs
/// keyed by glyph id. Patch application places the data blobs into the appropriate place
/// in the base font based on the associated glyph id.
///
/// Glyph Keyed patches are specified here:
/// <https://w3c.github.io/IFT/Overview.html#glyph-keyed>
use crate::font_patch::copy_unprocessed_tables;
use crate::font_patch::PatchingError;

use font_types::Scalar;
use read_fonts::collections::IntSet;
use read_fonts::tables::ift::{GlyphKeyedPatch, GlyphPatches};
use read_fonts::tables::loca::Loca;
use read_fonts::types::Tag;
use read_fonts::ReadError;
use read_fonts::{FontData, FontRef, TableProvider};
use shared_brotli_patch_decoder::shared_brotli_decode;
use skrifa::GlyphId;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::ops::RangeInclusive;

use write_fonts::FontBuilder;

pub(crate) fn apply_glyph_keyed_patches(
    patches: &[GlyphKeyedPatch<'_>],
    font: &FontRef,
) -> Result<Vec<u8>, PatchingError> {
    let mut decompression_buffer: Vec<Vec<u8>> = Vec::with_capacity(patches.len());

    for patch in patches {
        if patch.format() != Tag::new(b"ifgk") {
            return Err(PatchingError::InvalidPatch("Patch file tag is not 'iftk'"));
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
            GlyphPatches::read(FontData::new(raw_data), patch.flags())
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

    let mut processed_tables = BTreeSet::<Tag>::new();
    let mut font_builder = FontBuilder::new();

    let mut prev_table_tag: Option<Tag> = None;
    for table_tag in table_tag_list(&glyph_patches) {
        if let Some(prev_table_tag) = prev_table_tag {
            if table_tag <= prev_table_tag {
                return Err(PatchingError::InvalidPatch(
                    "Table tags are unsorted or contain duplicate entries.",
                ));
            }
        }
        prev_table_tag = Some(table_tag);

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

    // TODO(garretrieger): mark the patch applied in the appropriate IFT table.

    copy_unprocessed_tables(font, processed_tables, &mut font_builder);

    Ok(font_builder.build())
}

fn table_tag_list(glyph_patches: &[GlyphPatches]) -> BTreeSet<Tag> {
    glyph_patches
        .iter()
        .flat_map(|patch| patch.tables())
        .map(|tag| tag.get())
        .collect::<BTreeSet<Tag>>()
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
mod tests {
    use std::{collections::BTreeSet, io::Write};

    use brotlic::CompressorWriter;
    use read_fonts::{
        tables::ift::GlyphKeyedPatch, test_helpers::BeBuffer, FontData, FontRead, TableProvider,
    };

    use font_test_data::ift::{
        glyf_u16_glyph_patches, glyf_u16_glyph_patches_2, glyph_keyed_patch_header,
        noop_glyf_glyph_patches, test_font_for_patching,
    };
    use skrifa::{FontRef, Tag};

    use crate::glyph_keyed::apply_glyph_keyed_patches;

    fn assemble_patch(mut header: BeBuffer, payload: BeBuffer) -> BeBuffer {
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

    #[test]
    fn noop_glyph_keyed() {
        let patch = assemble_patch(glyph_keyed_patch_header(), noop_glyf_glyph_patches());
        let patch: &[u8] = &patch;
        let patch = GlyphKeyedPatch::read(FontData::new(patch)).unwrap();

        let font = test_font_for_patching();
        let font = FontRef::new(&font).unwrap();

        let patched = apply_glyph_keyed_patches(&[patch], &font).unwrap();
        let patched = FontRef::new(&patched).unwrap();

        check_tables_equal(&font, &patched, BTreeSet::default());
    }

    #[test]
    fn basic_glyph_keyed() {
        let patch = assemble_patch(glyph_keyed_patch_header(), glyf_u16_glyph_patches());
        let patch: &[u8] = &patch;
        let patch = GlyphKeyedPatch::read(FontData::new(patch)).unwrap();

        let font = test_font_for_patching();
        let font = FontRef::new(&font).unwrap();

        let patched = apply_glyph_keyed_patches(&[patch], &font).unwrap();
        let patched = FontRef::new(&patched).unwrap();

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
            [Tag::new(b"glyf"), Tag::new(b"loca")].into(),
        );
    }

    #[test]
    fn multiple_glyph_keyed() {
        let patch = assemble_patch(glyph_keyed_patch_header(), glyf_u16_glyph_patches());
        let patch: &[u8] = &patch;
        let patch1 = GlyphKeyedPatch::read(FontData::new(patch)).unwrap();

        let patch = assemble_patch(glyph_keyed_patch_header(), glyf_u16_glyph_patches_2());
        let patch: &[u8] = &patch;
        let patch2 = GlyphKeyedPatch::read(FontData::new(patch)).unwrap();

        let font = test_font_for_patching();
        let font = FontRef::new(&font).unwrap();

        let patched = apply_glyph_keyed_patches(&[patch2, patch1], &font).unwrap();
        let patched = FontRef::new(&patched).unwrap();

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
            [Tag::new(b"glyf"), Tag::new(b"loca")].into(),
        );
    }

    // TODO test of invalid cases:
    // - bad format value
    // - ignore unsupported tables.
    // - table tags unordered
    // - loca offsets unordered
    // - patch data offsets unordered.
    // - bad decompressed length.
    // - gid tags unordered
    // - loca offset type switch required.
    // TODO glyph keyed test with large number of offsets to check type conversion on (glyphCount * tableCount)
}
