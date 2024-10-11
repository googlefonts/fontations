//! TODO write me.

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
use std::cmp::min;
use std::collections::BTreeSet;
use std::collections::HashMap;

use write_fonts::FontBuilder;

pub(crate) fn apply_glyph_keyed_patch(
    patch: &GlyphKeyedPatch<'_>,
    font: &FontRef,
) -> Result<Vec<u8>, PatchingError> {
    if patch.format() != Tag::new(b"ifgk") {
        return Err(PatchingError::InvalidPatch("Patch file tag is not 'iftk'"));
    }

    let raw_data = shared_brotli_decode(
        patch.brotli_stream(),
        None,
        patch.max_uncompressed_length() as usize,
    )
    .map_err(PatchingError::from)?;
    let glyph_patches = GlyphPatches::read(FontData::new(&raw_data), patch.flags())
        .map_err(PatchingError::PatchParsingFailed)?;

    let mut processed_tables = BTreeSet::<Tag>::new();
    let mut font_builder = FontBuilder::new();
    for table_tag in glyph_patches.tables().iter() {
        let table_tag = table_tag.get();
        // TODO(garretrieger): add CFF, CFF2, and gvar support as well.
        if table_tag == Tag::new(b"glyf") {
            let (Some(glyf), Ok(loca)) = (font.table_data(Tag::new(b"glyf")), font.loca(None))
            else {
                return Err(PatchingError::InvalidPatch(
                    "Trying to patch glyf/loca but base font doesn't have them.",
                ));
            };
            patch_glyf_and_loca(
                &[glyph_patches.clone()],
                glyf.as_bytes(),
                loca,
                GlyphId::new(0),
                &mut font_builder,
            )?;
        } else {
            return Err(PatchingError::InvalidPatch(
                "Glyph keyed patch against unsupported table.",
            ));
        }

        processed_tables.insert(table_tag);
    }

    // TODO(garretrieger): mark the patch applied in the appropriate IFT table.

    copy_unprocessed_tables(font, processed_tables, &mut font_builder);

    Ok(font_builder.build())
}

fn table_tag_list<'a>(glyph_patches: impl Iterator<Item = &'a GlyphPatches<'a>>) -> BTreeSet<Tag> {
    let mut result = BTreeSet::new();
    for glyph_patch in glyph_patches {
        result.extend(glyph_patch.tables().iter().map(|tag| tag.get()));
    }
    result
}

fn dedup_gid_replacement_data<'a>(
    glyph_patches: impl Iterator<Item = &'a GlyphPatches<'a>>,
    table_tag: Tag,
) -> Result<(IntSet<GlyphId>, Vec<&'a [u8]>), ReadError> {
    // TODO consider making return an iterator?
    // TODO in the spec require sorted glyph ids?

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

fn retained_glyph_total_size<'a>(
    gids: &IntSet<GlyphId>,
    loca: &Loca<'a>,
    max_glyph_id: GlyphId,
) -> Result<u64, PatchingError> {
    let mut total_size = 0u64;
    for keep_range in gids.iter_excluded_ranges() {
        if *keep_range.start() > max_glyph_id {
            break;
        }

        let start = keep_range.start();
        let end = min(*keep_range.end(), max_glyph_id);

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
        let data: [u8; 2] = self.to_raw();
        dest[..2].copy_from_slice(&data);
    }
}

fn synthesize_glyf_and_loca<OffsetType: LocaOffset + TryFrom<usize>>(
    gids: &IntSet<GlyphId>,
    replacement_data: &[&[u8]],
    glyf: &[u8],
    loca: &Loca<'_>,
    new_glyf: &mut [u8],
    new_loca: &mut [u8],
) -> Result<(), PatchingError> {
    let mut replace_it = gids.iter_ranges().peekable();
    let mut keep_it = gids.iter_excluded_ranges().peekable();
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
            let end_off = loca.get_raw(end).ok_or(PatchingError::InternalError)? as usize;
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

// TODO: Idea - can we actually construct the new glyf table on the fly during the brotli decompression process?
//              ie. eliminate the intermediate buffer that stores GlyphPatches?

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

    // Step 0: merge the invidual patches into a list of replacement data for gid.
    // TODO(garretrieger): special case where gids is empty, just returned umodified copy of glyf + loca?
    let (gids, replacement_data) =
        dedup_gid_replacement_data(glyph_patches.iter(), Tag::new(b"glyf"))
            .map_err(PatchingError::PatchParsingFailed)?;

    // Step 1: determine the new total size of glyf
    let mut total_glyf_size = retained_glyph_total_size(&gids, &loca, max_glyph_id)?;
    for data in replacement_data.iter() {
        total_glyf_size += data.len() as u64;
    }

    // TODO(garretrieger): check if loca format will need to switch, if so that's an error.

    if gids.last().unwrap_or(GlyphId::new(0)) > max_glyph_id {
        return Err(PatchingError::InvalidPatch(
            "Patch would add a glyph beyond this fonts maximum.",
        ));
    }

    // Step 2: patch together the new glyf (by copying in ranges of data in the correct order).
    let loca_size = (max_glyph_id.to_u32() as usize + 1) * if is_short { 2 } else { 4 };
    let mut new_glyf = vec![0u8; total_glyf_size as usize];
    let mut new_loca = vec![0u8; loca_size];
    if is_short {
        synthesize_glyf_and_loca::<u16>(
            &gids,
            &replacement_data,
            glyf,
            &loca,
            new_glyf.as_mut_slice(),
            new_loca.as_mut_slice(),
        )?;
    } else {
        synthesize_glyf_and_loca::<u32>(
            &gids,
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
    #[test]
    fn basic_glyph_keyed() {
        todo!()
    }
}
