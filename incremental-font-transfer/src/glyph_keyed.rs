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
use read_fonts::tables::glyf::Glyf;
use read_fonts::tables::gvar::{Gvar, GvarFlags};
use read_fonts::tables::ift::{IFTX_TAG, IFT_TAG};
use read_fonts::TopLevelTable;
use read_fonts::{
    collections::IntSet,
    tables::{
        ift::{GlyphKeyedPatch, GlyphPatches},
        loca::Loca,
    },
    types::Tag,
    FontData, FontRef, ReadError, TableProvider,
};

use klippa::serialize::{OffsetWhence, Serializer};
use shared_brotli_patch_decoder::shared_brotli_decode;
use skrifa::GlyphId;
use std::borrow::Cow;
use std::collections::{BTreeSet, HashMap};
use std::ops::{Range, RangeInclusive};

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
        if table_tag == Glyf::TAG {
            let (Some(glyf), Ok(loca)) = (font.table_data(Glyf::TAG), font.loca(None)) else {
                return Err(PatchingError::InvalidPatch(
                    "Trying to patch glyf/loca but base font doesn't have them.",
                ));
            };
            patch_offset_array(
                Glyf::TAG,
                &glyph_patches,
                GlyfAndLoca {
                    loca,
                    glyf: glyf.as_bytes(),
                },
                max_glyph_id,
                &mut font_builder,
            )?;
            // glyf patch application also generates a loca table.
            processed_tables.insert(table_tag);
            processed_tables.insert(Loca::TAG);
        } else if table_tag == Gvar::TAG {
            let Ok(gvar) = font.gvar() else {
                return Err(PatchingError::InvalidPatch(
                    "Trying to patch gvar but base font doesn't have them.",
                ));
            };
            patch_offset_array(
                Gvar::TAG,
                &glyph_patches,
                gvar,
                max_glyph_id,
                &mut font_builder,
            )?;
            processed_tables.insert(Gvar::TAG);
        } else if table_tag == Tag::new(b"CFF ") || table_tag == Tag::new(b"CFF2") {
            // TODO(garretrieger): add CFF and CFF2 support as well.
            return Err(PatchingError::InvalidPatch(
                "CFF and CFF2 patches are not yet supported.",
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

fn retained_glyphs_total_size<T: GlyphDataOffsetArray>(
    gids: &IntSet<GlyphId>,
    offsets: &T,
    max_glyph_id: GlyphId,
) -> Result<u64, PatchingError> {
    let mut total_size = 0u64;
    for keep_range in retained_glyphs_in_font(gids, max_glyph_id) {
        let start = *keep_range.start();
        let end: GlyphId = keep_range
            .end()
            .to_u32()
            .checked_add(1)
            .ok_or(PatchingError::InternalError)?
            .into();

        let start_offset = offsets.offset_for(start)?;
        let end_offset = offsets.offset_for(end)?;

        total_size += end_offset
            .checked_sub(start_offset) // TODO: this can be removed if we pre-verify ascending order
            .ok_or(PatchingError::FontParsingFailed(ReadError::MalformedData(
                "offset entries are not in ascending order",
            )))? as u64;
    }

    Ok(total_size)
}

/// Objects with this trait can be written to bytes.
///
/// The offset value will NOT be divided prior to conversion to bytes.
trait WritableOffset {
    fn write_to(self, dest: &mut [u8]);
}

impl WritableOffset for u32 {
    fn write_to(self, dest: &mut [u8]) {
        let data: [u8; 4] = self.to_raw();
        dest[..4].copy_from_slice(&data);
    }
}

impl WritableOffset for u16 {
    fn write_to(self, dest: &mut [u8]) {
        let data: [u8; 2] = self.to_raw();
        dest[..2].copy_from_slice(&data);
    }
}

fn synthesize_offset_array<
    const DIV: usize,
    OffsetType: WritableOffset + TryFrom<usize>,
    T: GlyphDataOffsetArray,
>(
    gids: &IntSet<GlyphId>,
    max_glyph_id: GlyphId,
    replacement_data: &[&[u8]],
    offset_array: &T,
    new_data: &mut [u8],
    new_offsets: &mut [u8],
) -> Result<(), PatchingError> {
    if !offset_array.all_offsets_are_ascending() {
        return Err(PatchingError::FontParsingFailed(ReadError::MalformedData(
            "offset array contains unordered offsets.",
        )));
    }

    let mut replace_it = gids.iter_ranges().peekable();
    let mut keep_it = retained_glyphs_in_font(gids, max_glyph_id).peekable();
    let mut replacement_data_it = replacement_data.iter();
    let mut write_index = 0;
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

        let (start, end) = (range.start().to_u32(), range.end().to_u32());

        if replace {
            for gid in start..=end {
                let data = *replacement_data_it
                    .next()
                    .ok_or(PatchingError::InternalError)?;

                new_data
                    .get_mut(write_index..write_index + data.len())
                    .ok_or(PatchingError::InternalError)?
                    .copy_from_slice(data);

                let new_off: OffsetType = (write_index / DIV)
                    .try_into()
                    .map_err(|_| PatchingError::InternalError)?;

                new_off.write_to(
                    new_offsets
                        .get_mut(gid as usize * off_size..)
                        .ok_or(PatchingError::InternalError)?,
                );

                write_index += data.len();
                // Add padding if the offset gets divided
                if DIV > 1 {
                    write_index += data.len() % DIV;
                }
            }
        } else {
            let start_off = offset_array.offset_for(start.into())? as usize;
            let end_off = offset_array.offset_for(
                end.checked_add(1)
                    .ok_or(PatchingError::InternalError)?
                    .into(),
            )? as usize;

            let len = end_off
                .checked_sub(start_off)
                .ok_or(PatchingError::InternalError)?;
            new_data
                .get_mut(write_index..write_index + len)
                .ok_or(PatchingError::InternalError)?
                .copy_from_slice(offset_array.get(start_off..end_off)?);

            for gid in start..=end {
                let cur_off = offset_array.offset_for(gid.into())? as usize;
                let new_off = cur_off - start_off + write_index;

                let new_off: OffsetType = (new_off / DIV)
                    .try_into()
                    .map_err(|_| PatchingError::InternalError)?;
                new_off.write_to(
                    new_offsets
                        .get_mut(gid as usize * off_size..)
                        .ok_or(PatchingError::InternalError)?,
                );
            }

            write_index += len;
        }
    }

    // Write the last offset
    let new_off: OffsetType = (write_index / DIV)
        .try_into()
        .map_err(|_| PatchingError::InternalError)?;
    new_off.write_to(
        new_offsets
            .get_mut(new_offsets.len() - off_size..)
            .ok_or(PatchingError::InternalError)?,
    );

    Ok(())
}

fn patch_offset_array<'a, T: GlyphDataOffsetArray>(
    table_tag: Tag,
    glyph_patches: &'a [GlyphPatches<'a>],
    offset_array: T,
    max_glyph_id: GlyphId,
    font_builder: &mut FontBuilder,
) -> Result<(), PatchingError> {
    // Step 0: merge the individual patches into a list of replacement data for gid.
    // TODO(garretrieger): special case where gids is empty, just returned umodified copy of glyf + loca?
    let offset_type = offset_array.offset_type();
    let (gids, replacement_data) = dedup_gid_replacement_data(glyph_patches.iter(), table_tag)
        .map_err(PatchingError::PatchParsingFailed)?;

    // Step 1: determine the new total size of the data portion.
    let mut total_data_size = retained_glyphs_total_size(&gids, &offset_array, max_glyph_id)?;
    for data in replacement_data.iter() {
        let len = data.len() as u64;
        // note: include padding when needed (if offsets are divided for storage)
        total_data_size += len + (len % offset_type.offset_divisor());
    }

    // TODO(garretrieger): pre-check loca has all ascending offsets.
    // TODO(garretrieger): check if loca format will need to switch, if so that's an error.

    if gids.last().unwrap_or(GlyphId::new(0)) > max_glyph_id {
        return Err(PatchingError::InvalidPatch(
            "Patch would add a glyph beyond this fonts maximum.",
        ));
    }

    // Step 2: patch together the new data array (by copying in ranges of data in the correct order).
    // Note: max_glyph_id + 2 here because we want num glyphs + 1
    let offsets_size = (max_glyph_id.to_u32() as usize + 2) * offset_type.offset_width();
    let mut new_data = vec![0u8; total_data_size as usize];
    let mut new_offsets = vec![0u8; offsets_size];
    match offset_type {
        OffsetType::ShortDivByTwo => synthesize_offset_array::<2, u16, _>(
            &gids,
            max_glyph_id,
            &replacement_data,
            &offset_array,
            new_data.as_mut_slice(),
            new_offsets.as_mut_slice(),
        )?,
        OffsetType::Long => synthesize_offset_array::<1, u32, _>(
            &gids,
            max_glyph_id,
            &replacement_data,
            &offset_array,
            new_data.as_mut_slice(),
            new_offsets.as_mut_slice(),
        )?,
    }

    // Step 3: add new tables to the output builder
    offset_array.add_to_font(font_builder, new_data, new_offsets)?;

    Ok(())
}

/// Classifies the different style of offsets that can be used in a data offset array.
enum OffsetType {
    Long,
    ShortDivByTwo,
}

impl OffsetType {
    fn offset_width(&self) -> usize {
        match self {
            Self::Long => 4,
            Self::ShortDivByTwo => 2,
        }
    }

    fn offset_divisor(&self) -> u64 {
        match self {
            Self::Long => 1,
            Self::ShortDivByTwo => 2,
        }
    }
}

struct GlyfAndLoca<'a> {
    loca: Loca<'a>,
    glyf: &'a [u8],
}

/// Abstraction of a table which has blocks of data located by an array of ascending offsets (eg. glyf + loca)
trait GlyphDataOffsetArray {
    fn offset_type(&self) -> OffsetType;

    /// Returns the offset associated with a specific gid.
    ///
    /// This is the offset at which data for that glyph starts.
    fn offset_for(&self, gid: GlyphId) -> Result<u32, PatchingError>;

    /// Checks that all offsets are in ascending order.
    fn all_offsets_are_ascending(&self) -> bool;

    fn get(&self, range: Range<usize>) -> Result<&[u8], PatchingError>;

    fn add_to_font<'a>(
        &self,
        font_builder: &mut FontBuilder<'a>,
        data: impl Into<Cow<'a, [u8]>>,
        offsets: impl Into<Cow<'a, [u8]>>,
    ) -> Result<(), PatchingError>;
}

impl GlyphDataOffsetArray for GlyfAndLoca<'_> {
    fn offset_type(&self) -> OffsetType {
        match self.loca {
            Loca::Short(_) => OffsetType::ShortDivByTwo,
            Loca::Long(_) => OffsetType::Long,
        }
    }

    fn offset_for(&self, gid: GlyphId) -> Result<u32, PatchingError> {
        self.loca
            // Note: get_raw(...) applies the * 2 for short loca when needed.
            .get_raw(gid.to_u32() as usize)
            .ok_or(PatchingError::InvalidPatch("Start loca entry is missing."))
    }

    fn all_offsets_are_ascending(&self) -> bool {
        self.loca.all_offsets_are_ascending()
    }

    fn get(&self, range: Range<usize>) -> Result<&[u8], PatchingError> {
        self.glyf
            .get(range)
            .ok_or(PatchingError::from(ReadError::OutOfBounds))
    }

    fn add_to_font<'a>(
        &self,
        font_builder: &mut FontBuilder<'a>,
        data: impl Into<Cow<'a, [u8]>>,
        offsets: impl Into<Cow<'a, [u8]>>,
    ) -> Result<(), PatchingError> {
        font_builder.add_raw(Glyf::TAG, data);
        font_builder.add_raw(Loca::TAG, offsets);
        Ok(())
    }
}

impl GlyphDataOffsetArray for Gvar<'_> {
    fn offset_type(&self) -> OffsetType {
        if self.flags().contains(GvarFlags::LONG_OFFSETS) {
            OffsetType::Long
        } else {
            OffsetType::ShortDivByTwo
        }
    }

    fn offset_for(&self, gid: GlyphId) -> Result<u32, PatchingError> {
        Ok(self
            .glyph_variation_data_offsets()
            .get(gid.to_u32() as usize)
            .map_err(PatchingError::FontParsingFailed)?
            .get())
    }

    fn all_offsets_are_ascending(&self) -> bool {
        let mut prev: Option<u32> = None;
        for v in self.glyph_variation_data_offsets().iter() {
            let Ok(v) = v else {
                return false;
            };
            let v = v.get();
            if let Some(prev_value) = prev {
                if prev_value > v {
                    return false;
                }
            }
            prev = Some(v);
        }
        true
    }

    fn get(&self, range: Range<usize>) -> Result<&[u8], PatchingError> {
        self.glyph_variation_data_for_range(range)
            .map_err(PatchingError::FontParsingFailed)
            .map(|fd| fd.as_bytes())
    }

    fn add_to_font<'a>(
        &self,
        font_builder: &mut FontBuilder<'a>,
        data: impl Into<Cow<'a, [u8]>>,
        offsets: impl Into<Cow<'a, [u8]>>,
    ) -> Result<(), PatchingError> {
        // typical gvar layout (see: https://learn.microsoft.com/en-us/typography/opentype/spec/gvar):
        //   Part 1 - Header
        //   Part 2 - glyphVariationDataOffsets[glyphCount + 1]
        //   Part 3 - Shared Tuples
        //   Part 4 - Array of per glyph variation data
        //
        // When constructing a new gvar from the newly synthesized data we'll be replacing
        // Part 2 and 4 with 'offsets' and 'data' respectively. In order to be consistent with the open type
        // spec which prescribes the above ordering we always output the parts in the spec ordering
        // regardless of how they were ordered in the original table. Additionally this will correctly resolve cases
        // where the original table had overlapping shared tuple and glyph variation data.
        // However, as a result we may need to change offsets in part 1 if ordering gets modified. The klippa serializer
        // is used to recalculate offsets as needed.
        let orig_bytes = self.as_bytes();
        let orig_size = orig_bytes.len();
        let part2_offsets = offsets.into();
        let part4_data = data.into();

        let original_offsets_range = self.shape().glyph_variation_data_offsets_byte_range();
        if part2_offsets.len() != original_offsets_range.end - original_offsets_range.start {
            // computed offsets array length should not have changed from the original offsets array length
            return Err(PatchingError::InternalError);
        }

        let part1_header = orig_bytes
            .get(0..original_offsets_range.start)
            .ok_or(PatchingError::InternalError)?;

        let max_new_size = orig_size + part4_data.len();
        let mut serializer = Serializer::new(max_new_size);

        // part 1 and 2 - write gvar header and offsets
        serializer
            .start_serialize()
            .and(serializer.embed_bytes(part1_header))
            .and(serializer.embed_bytes(&part2_offsets))
            .map_err(PatchingError::from)?;

        // part 4 - write new glyph variation data
        serializer
            .push()
            .and(serializer.embed_bytes(&part4_data))
            .map_err(PatchingError::from)?;

        let glyph_data_obj = serializer
            .pop_pack(false)
            .ok_or_else(|| PatchingError::SerializationError(serializer.error()))?;

        // part 3 - write shared tuples.
        let shared_tuples = self.shared_tuples().map_err(PatchingError::from)?;

        let shared_tuples_bytes = shared_tuples
            .offset_data()
            .as_bytes()
            .get(shared_tuples.shape().tuples_byte_range())
            .ok_or_else(|| PatchingError::SerializationError(serializer.error()))?;

        let shared_tuples_obj = if !shared_tuples_bytes.is_empty() {
            serializer
                .push()
                .and(serializer.embed_bytes(shared_tuples_bytes))
                .map_err(PatchingError::from)?;

            // The spec says that shared tuple data should come before glyph variation data so use a virtual link to
            // ensure the correct ordering.
            serializer.add_virtual_link(glyph_data_obj);

            serializer
                .pop_pack(false)
                .ok_or_else(|| PatchingError::SerializationError(serializer.error()))?
        } else {
            // If there's no shared tuples just point the shared tuples offset to the start of glyph_data_obj
            // (since it can't be null).
            glyph_data_obj
        };

        // Set up offsets to shared tuples and glyph data.
        serializer
            .add_link(
                self.shape().shared_tuples_offset_byte_range(),
                shared_tuples_obj,
                OffsetWhence::Head,
                0,
                false,
            )
            .and(serializer.add_link(
                self.shape().glyph_variation_data_array_offset_byte_range(),
                glyph_data_obj,
                OffsetWhence::Head,
                0,
                false,
            ))
            .map_err(PatchingError::from)?;

        // Generate the final output
        serializer.end_serialize();
        let new_gvar = serializer.copy_bytes();
        font_builder.add_raw(Gvar::TAG, new_gvar);
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::{
        collections::{BTreeSet, HashMap},
        io::Write,
    };

    use brotlic::CompressorWriter;
    use read_fonts::{
        tables::{
            glyf::Glyf,
            gvar::Gvar,
            ift::{CompatibilityId, GlyphKeyedPatch, IFTX_TAG, IFT_TAG},
            loca::Loca,
        },
        FontData, FontRead, ReadError, TableProvider, TopLevelTable,
    };

    use font_test_data::{
        bebuffer::BeBuffer,
        ift::{
            glyf_and_gvar_u16_glyph_patches, glyf_u16_glyph_patches, glyf_u16_glyph_patches_2,
            glyph_keyed_patch_header, long_gvar_with_shared_tuples, noop_glyf_glyph_patches,
            out_of_order_gvar_with_shared_tuples, short_gvar_with_no_shared_tuples,
            short_gvar_with_shared_tuples,
        },
    };
    use skrifa::{FontRef, Tag};

    use crate::{
        font_patch::PatchingError,
        glyph_keyed::apply_glyph_keyed_patches,
        patchmap::{PatchFormat, PatchUri},
        testdata::{test_font_for_patching, test_font_for_patching_with_loca_mod},
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
        .try_into()
        .unwrap()
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
            true,
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

        let new_glyf: &[u8] = patched.table_data(Glyf::TAG).unwrap().as_bytes();
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

        check_tables_equal(&font, &patched, [IFT_TAG, Glyf::TAG, Loca::TAG].into());
    }

    #[test]
    fn basic_glyph_keyed_with_long_loca() {
        let patch =
            assemble_glyph_keyed_patch(glyph_keyed_patch_header(), glyf_u16_glyph_patches());
        let patch: &[u8] = &patch;
        let patch = GlyphKeyedPatch::read(FontData::new(patch)).unwrap();

        let font = test_font_for_patching_with_loca_mod(
            false, // force long loca
            |_| {},
            HashMap::from([(IFT_TAG, vec![0, 0, 0, 0].as_slice())]),
        );
        let font = FontRef::new(&font).unwrap();

        let patch_info = patch_info(IFT_TAG, 28);
        let patched = apply_glyph_keyed_patches(&[(&patch_info, patch)], &font).unwrap();
        let patched = FontRef::new(&patched).unwrap();

        let new_ift: &[u8] = patched.table_data(IFT_TAG).unwrap().as_bytes();
        assert_eq!(&[0, 0, 0, 0b0001_0000], new_ift);

        let new_glyf: &[u8] = patched.table_data(Glyf::TAG).unwrap().as_bytes();
        assert_eq!(
            &[
                1, 2, 3, 4, 5, 0, // gid 0
                6, 7, 8, 0, // gid 1
                b'a', b'b', b'c', // gid2
                b'd', b'e', b'f', b'g', // gid 7
                b'h', b'i', b'j', b'k', b'l', // gid 8 + 9
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
                13, // gid 3
                13, // gid 4
                13, // gid 5
                13, // gid 6
                13, // gid 7
                17, // gid 8
                17, // gid 9
                22, // gid 10
                22, // gid 11
                22, // gid 12
                22, // gid 13
                24, // gid 14
                24, // end
            ],
            indices
        );

        check_tables_equal(&font, &patched, [IFT_TAG, Glyf::TAG, Loca::TAG].into());
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
            true,
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

        let new_glyf: &[u8] = patched.table_data(Glyf::TAG).unwrap().as_bytes();
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

        check_tables_equal(&font, &patched, [Glyf::TAG, Loca::TAG, IFTX_TAG].into());
    }

    #[test]
    fn glyph_keyed_glyf_and_gvar() {
        let patch = assemble_glyph_keyed_patch(
            glyph_keyed_patch_header(),
            glyf_and_gvar_u16_glyph_patches(),
        );
        let patch: &[u8] = &patch;
        let patch = GlyphKeyedPatch::read(FontData::new(patch)).unwrap();
        let patch_info = patch_info(IFT_TAG, 0);

        let gvar = short_gvar_with_shared_tuples();

        let font = test_font_for_patching_with_loca_mod(
            true,
            |_| {},
            HashMap::from([
                (Gvar::TAG, gvar.as_slice()),
                (Tag::new(b"IFT "), vec![0, 0, 0, 0].as_slice()),
            ]),
        );
        let font = FontRef::new(&font).unwrap();

        let patched = apply_glyph_keyed_patches(&[(&patch_info, patch)], &font).unwrap();
        let patched = FontRef::new(&patched).unwrap();

        let new_gvar: &[u8] = patched.table_data(Gvar::TAG).unwrap().as_bytes();

        let mut expected_gvar: Vec<u8> = vec![];

        let change_start = gvar.offset_for("glyph_offset[3]");

        expected_gvar.extend_from_slice(gvar.get(0..change_start).unwrap());
        // Offsets
        expected_gvar.extend_from_slice(&[
            0x00, 0x03, // gid 3
            0x00, 0x03, // gid 4
            0x00, 0x03, // gid 5
            0x00, 0x03, // gid 6
            0x00, 0x03, // gid 7
            0x00, 0x05, // gid 8
            0x00, 0x06, // gid 9
            0x00, 0x06, // gid 10
            0x00, 0x06, // gid 11
            0x00, 0x06, // gid 12
            0x00, 0x06, // gid 13
            0x00, 0x06, // gid 14
            0x00, 0x06u8, // trailing
        ]);
        // Shared tuples
        expected_gvar.extend_from_slice(&[0, 42, 0, 13, 0, 25u8]);
        // Data
        expected_gvar.extend_from_slice(&[
            1, 2, 3, 4, // gid 0
            b'm', b'n', // gid 2
            b'o', b'p', b'q', 0, // gid 7
            b'r', 0u8, // gid 8
        ]);
        assert_eq!(&expected_gvar, new_gvar);
    }

    #[test]
    fn glyph_keyed_glyf_and_long_gvar() {
        let patch = assemble_glyph_keyed_patch(
            glyph_keyed_patch_header(),
            glyf_and_gvar_u16_glyph_patches(),
        );
        let patch: &[u8] = &patch;
        let patch = GlyphKeyedPatch::read(FontData::new(patch)).unwrap();
        let patch_info = patch_info(IFT_TAG, 0);

        let gvar = long_gvar_with_shared_tuples();

        let font = test_font_for_patching_with_loca_mod(
            true,
            |_| {},
            HashMap::from([
                (Gvar::TAG, gvar.as_slice()),
                (Tag::new(b"IFT "), vec![0, 0, 0, 0].as_slice()),
            ]),
        );
        let font = FontRef::new(&font).unwrap();

        let patched = apply_glyph_keyed_patches(&[(&patch_info, patch)], &font).unwrap();
        let patched = FontRef::new(&patched).unwrap();

        let new_gvar: &[u8] = patched.table_data(Gvar::TAG).unwrap().as_bytes();

        let mut expected_gvar: Vec<u8> = vec![];

        let change_start = gvar.offset_for("glyph_offset[3]");

        expected_gvar.extend_from_slice(gvar.get(0..change_start).unwrap());
        // Offsets
        expected_gvar.extend_from_slice(&[
            0x00, 0x00, 0x00, 0x06, // gid 3
            0x00, 0x00, 0x00, 0x06, // gid 4
            0x00, 0x00, 0x00, 0x06, // gid 5
            0x00, 0x00, 0x00, 0x06, // gid 6
            0x00, 0x00, 0x00, 0x06, // gid 7
            0x00, 0x00, 0x00, 0x09, // gid 8
            0x00, 0x00, 0x00, 0x0A, // gid 9
            0x00, 0x00, 0x00, 0x0A, // gid 10
            0x00, 0x00, 0x00, 0x0A, // gid 11
            0x00, 0x00, 0x00, 0x0A, // gid 12
            0x00, 0x00, 0x00, 0x0A, // gid 13
            0x00, 0x00, 0x00, 0x0A, // gid 14
            0x00, 0x00, 0x00, 0x0Au8, // trailing
        ]);
        // Shared tuples
        expected_gvar.extend_from_slice(&[0, 42, 0, 13, 0, 25u8]);
        // Data
        expected_gvar.extend_from_slice(&[
            1, 2, 3, 4, // gid 0
            b'm', b'n', // gid 2
            b'o', b'p', b'q', // gid 7
            b'r', // gid 8
        ]);
        assert_eq!(&expected_gvar, new_gvar);
    }

    #[test]
    fn glyph_keyed_glyf_and_gvar_no_shared_tuples() {
        let patch = assemble_glyph_keyed_patch(
            glyph_keyed_patch_header(),
            glyf_and_gvar_u16_glyph_patches(),
        );
        let patch: &[u8] = &patch;
        let patch = GlyphKeyedPatch::read(FontData::new(patch)).unwrap();
        let patch_info = patch_info(IFT_TAG, 0);

        let gvar = short_gvar_with_no_shared_tuples();

        let font = test_font_for_patching_with_loca_mod(
            true,
            |_| {},
            HashMap::from([
                (Gvar::TAG, gvar.as_slice()),
                (Tag::new(b"IFT "), vec![0, 0, 0, 0].as_slice()),
            ]),
        );
        let font = FontRef::new(&font).unwrap();

        let patched = apply_glyph_keyed_patches(&[(&patch_info, patch)], &font).unwrap();
        let patched = FontRef::new(&patched).unwrap();

        let new_gvar: &[u8] = patched.table_data(Gvar::TAG).unwrap().as_bytes();

        let mut expected_gvar: Vec<u8> = vec![];

        let change_start = gvar.offset_for("glyph_offset[3]");

        expected_gvar.extend_from_slice(gvar.get(0..change_start).unwrap());
        // Offsets
        expected_gvar.extend_from_slice(&[
            0x00, 0x03, // gid 3
            0x00, 0x03, // gid 4
            0x00, 0x03, // gid 5
            0x00, 0x03, // gid 6
            0x00, 0x03, // gid 7
            0x00, 0x05, // gid 8
            0x00, 0x06, // gid 9
            0x00, 0x06, // gid 10
            0x00, 0x06, // gid 11
            0x00, 0x06, // gid 12
            0x00, 0x06, // gid 13
            0x00, 0x06, // gid 14
            0x00, 0x06u8, // trailing
        ]);
        // Data
        expected_gvar.extend_from_slice(&[
            1, 2, 3, 4, // gid 0
            b'm', b'n', // gid 2
            b'o', b'p', b'q', 0, // gid 7
            b'r', 0u8, // gid 8
        ]);
        assert_eq!(&expected_gvar, new_gvar);
    }

    #[test]
    fn glyph_keyed_glyf_and_gvar_overlapping_shared_tuples() {
        let patch = assemble_glyph_keyed_patch(
            glyph_keyed_patch_header(),
            glyf_and_gvar_u16_glyph_patches(),
        );
        let patch: &[u8] = &patch;
        let patch = GlyphKeyedPatch::read(FontData::new(patch)).unwrap();
        let patch_info = patch_info(IFT_TAG, 0);

        let mut gvar = short_gvar_with_no_shared_tuples();
        gvar.write_at("shared_tuple_count", 2u16);

        let font = test_font_for_patching_with_loca_mod(
            true,
            |_| {},
            HashMap::from([
                (Gvar::TAG, gvar.as_slice()),
                (Tag::new(b"IFT "), vec![0, 0, 0, 0].as_slice()),
            ]),
        );
        let font = FontRef::new(&font).unwrap();

        let patched = apply_glyph_keyed_patches(&[(&patch_info, patch)], &font).unwrap();
        let patched = FontRef::new(&patched).unwrap();

        let new_gvar: &[u8] = patched.table_data(Gvar::TAG).unwrap().as_bytes();

        let mut expected_gvar: Vec<u8> = vec![];

        let change_start = gvar.offset_for("glyph_offset[3]");

        gvar.write_at(
            "glyph_variation_data_offset",
            (gvar.offset_for("glyph_0") + 4) as u32,
        ); // glyph variation data gets shifted by 4 bytes due to duplication of 4 bytes of shared tuple data.
        expected_gvar.extend_from_slice(gvar.get(0..change_start).unwrap());
        // Offsets
        expected_gvar.extend_from_slice(&[
            0x00, 0x03, // gid 3
            0x00, 0x03, // gid 4
            0x00, 0x03, // gid 5
            0x00, 0x03, // gid 6
            0x00, 0x03, // gid 7
            0x00, 0x05, // gid 8
            0x00, 0x06, // gid 9
            0x00, 0x06, // gid 10
            0x00, 0x06, // gid 11
            0x00, 0x06, // gid 12
            0x00, 0x06, // gid 13
            0x00, 0x06, // gid 14
            0x00, 0x06u8, // trailing
        ]);
        // Shared tuples
        expected_gvar.extend_from_slice(&[1, 2, 3, 4u8]); // overlapping portion is duplicated into its own region.
                                                          // Data
        expected_gvar.extend_from_slice(&[
            1, 2, 3, 4, // gid 0
            b'm', b'n', // gid 2
            b'o', b'p', b'q', 0, // gid 7
            b'r', 0u8, // gid 8
        ]);
        assert_eq!(&expected_gvar, new_gvar);
    }

    #[test]
    fn glyph_keyed_out_of_order_gvar() {
        let patch = assemble_glyph_keyed_patch(
            glyph_keyed_patch_header(),
            glyf_and_gvar_u16_glyph_patches(),
        );
        let patch: &[u8] = &patch;
        let patch = GlyphKeyedPatch::read(FontData::new(patch)).unwrap();
        let patch_info = patch_info(IFT_TAG, 0);

        let gvar = out_of_order_gvar_with_shared_tuples();

        let font = test_font_for_patching_with_loca_mod(
            true,
            |_| {},
            HashMap::from([
                (Gvar::TAG, gvar.as_slice()),
                (Tag::new(b"IFT "), vec![0, 0, 0, 0].as_slice()),
            ]),
        );
        let font = FontRef::new(&font).unwrap();

        let patched = apply_glyph_keyed_patches(&[(&patch_info, patch)], &font).unwrap();
        let patched = FontRef::new(&patched).unwrap();

        let new_gvar: &[u8] = patched.table_data(Gvar::TAG).unwrap().as_bytes();

        let mut expected_gvar: Vec<u8> = vec![];

        // Patching will reorder the gvar table to the expected spec ordering, so for expected compare to the
        // correctly ordered version.
        let gvar = short_gvar_with_shared_tuples();
        let change_start = gvar.offset_for("glyph_offset[3]");

        expected_gvar.extend_from_slice(gvar.get(0..change_start).unwrap());
        // Offsets
        expected_gvar.extend_from_slice(&[
            0x00, 0x03, // gid 3
            0x00, 0x03, // gid 4
            0x00, 0x03, // gid 5
            0x00, 0x03, // gid 6
            0x00, 0x03, // gid 7
            0x00, 0x05, // gid 8
            0x00, 0x06, // gid 9
            0x00, 0x06, // gid 10
            0x00, 0x06, // gid 11
            0x00, 0x06, // gid 12
            0x00, 0x06, // gid 13
            0x00, 0x06, // gid 14
            0x00, 0x06u8, // trailing
        ]);
        // Shared tuples
        expected_gvar.extend_from_slice(&[0, 42, 0, 13, 0, 25u8]);
        // Data
        expected_gvar.extend_from_slice(&[
            1, 2, 3, 4, // gid 0
            b'm', b'n', // gid 2
            b'o', b'p', b'q', 0, // gid 7
            b'r', 0u8, // gid 8
        ]);
        assert_eq!(&expected_gvar, new_gvar);
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
        let mut patch = glyf_and_gvar_u16_glyph_patches();
        patch.write_at("glyf_tag", Tag::new(b"CFF "));
        let patch = assemble_glyph_keyed_patch(glyph_keyed_patch_header(), patch);
        let patch: &[u8] = &patch;
        let patch = GlyphKeyedPatch::read(FontData::new(patch)).unwrap();
        let patch_info = patch_info(IFT_TAG, 0);

        let font = test_font_for_patching();
        let font = FontRef::new(&font).unwrap();

        assert_eq!(
            apply_glyph_keyed_patches(&[(&patch_info, patch)], &font),
            Err(PatchingError::InvalidPatch(
                "CFF and CFF2 patches are not yet supported."
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

        let new_glyf: &[u8] = patched.table_data(Glyf::TAG).unwrap().as_bytes();
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

        check_tables_equal(&font, &patched, [Glyf::TAG, Loca::TAG, IFT_TAG].into());
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
        builder.write_at("gvar_tag", Glyf::TAG);
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
            true,
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
                "offset array contains unordered offsets."
            ))),
        );
    }

    // TODO test of invalid cases:
    // - patch data offsets unordered.
    // - loca offset type switch required.
    // - glyph keyed test with large number of offsets to check type conversion on (glyphCount * tableCount)
    // - test that glyph keyed patches are idempotent.
}
