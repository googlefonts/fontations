//! Reads and applies font patches to font binaries.
//!
//! Font patch formats are defined as part of the incremental font transfer specification:
//! <https://w3c.github.io/IFT/Overview.html#font-patch-formats>
//!
//! Two main types of font patches are implemented:
//! 1. Brotli Based Patch - these patches contain a per table brotli binary patch to be applied
//!    to the input font.
//! 2. Glyph Keyed - these patches contain blobs of data associated with combinations of
//!    glyph id + table. The patch inserts these blobs into the table at the location for
//!    the corresponding glyph id.

use crate::patchmap::PatchEncoding;
use raw::tables::ift::TablePatchFlags;
use raw::types::Tag;
use raw::{FontData, FontRef};
use read_fonts::tables::ift::{PerTableBrotliPatch, TablePatch};
use read_fonts::{FontRead, ReadError};
use std::collections::BTreeSet;
use write_fonts::FontBuilder;

pub fn apply_patch(
    font: &FontRef,
    compatibility_id: &[u32; 4],
    patch_data: &[u8],
    encoding: PatchEncoding,
) -> Result<Vec<u8>, ReadError> {
    // TODO(garretrieger): Return a custom error type instead of ReadError?
    let patch = match encoding {
        PatchEncoding::Brotli => {
            todo!()
        }
        PatchEncoding::PerTableBrotli { .. } => parse_per_table_brotli_patch(patch_data)?,
        PatchEncoding::GlyphKeyed => {
            todo!()
        }
    };

    patch.apply(font, compatibility_id)
}

fn parse_per_table_brotli_patch<'a>(
    patch_data: &'a [u8],
) -> Result<impl FontPatch + 'a, ReadError> {
    PerTableBrotliPatch::read(FontData::new(patch_data))
}

trait FontPatch {
    fn apply(&self, font: &FontRef, compatibility_id: &[u32; 4]) -> Result<Vec<u8>, ReadError>;
}

impl<'a> FontPatch for PerTableBrotliPatch<'a> {
    fn apply(&self, font: &FontRef, compatibility_id: &[u32; 4]) -> Result<Vec<u8>, ReadError> {
        if self.compatibility_id() != compatibility_id {
            return Err(ReadError::ValidationError);
        }

        if self.format() != Tag::new(b"ifbt") {
            return Err(ReadError::ValidationError);
        }

        // TODO font builder
        let mut font_builder = FontBuilder::new();
        let mut processed_tables = BTreeSet::<Tag>::new();
        for i in 0..self.patches_count() {
            let i = i as usize;
            let next = i + 1;

            let table_patch = self.patchs().get(i)?;
            let (Some(offset), Some(next_offset)) =
                (self.patch_offsets().get(i), self.patch_offsets().get(next))
            else {
                return Err(ReadError::MalformedData("Missing patch offset."));
            };

            let offset = offset.get().to_u32();
            let next_offset = next_offset.get().to_u32();
            let Some(stream_length) = next_offset
                .checked_sub(offset)
                .and_then(|v| v.checked_sub(10))
            // brotli stream starts at the 10th byte
            else {
                return Err(ReadError::MalformedData(
                    "invalid patch offsets result in < 0 byte length brotli stream.",
                ));
            };

            let tag = table_patch.tag();
            if processed_tables.contains(&tag) {
                // Table has already been processed.
                continue;
            }

            processed_tables.insert(tag);
            if table_patch.flags().contains(TablePatchFlags::DROP_TABLE) {
                // Table will not be copied, skip any further processing.
                continue;
            }

            let replacement = table_patch.flags().contains(TablePatchFlags::REPLACE_TABLE);
            let new_table = apply_table_patch(font, table_patch, stream_length, replacement)?;
            font_builder.add_raw(tag, new_table);
        }

        font.table_directory
            .table_records()
            .iter()
            .map(|r| r.tag())
            .filter(|tag| !processed_tables.contains(tag))
            .filter_map(|tag| Some((tag, font.table_data(tag)?)))
            .for_each(|(tag, data)| {
                font_builder.add_raw(tag, data);
            });

        Ok(font_builder.build())
    }
}

fn apply_table_patch(
    font: &FontRef,
    table_patch: TablePatch,
    stream_length: u32,
    replacement: bool,
) -> Result<Vec<u8>, ReadError> {
    // TODO: decompress brotli stream as instructed by the flags (eg. with or without a base).
    todo!()
}

// TODO(garretrieger): add tests
