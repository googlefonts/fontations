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
use read_fonts::tables::ift::{TableKeyedPatch, TablePatch};
use read_fonts::{FontRead, ReadError};
use shared_brotli::{shared_brotli_decode, DecodeError};
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
        PatchEncoding::TableKeyed { .. } => parse_per_table_brotli_patch(patch_data)?,
        PatchEncoding::GlyphKeyed => {
            todo!()
        }
    };

    patch.apply(font, compatibility_id)
}

fn parse_per_table_brotli_patch<'a>(
    patch_data: &'a [u8],
) -> Result<impl FontPatch + 'a, ReadError> {
    TableKeyedPatch::read(FontData::new(patch_data))
}

trait FontPatch {
    fn apply(&self, font: &FontRef, compatibility_id: &[u32; 4]) -> Result<Vec<u8>, ReadError>;
}

impl<'a> FontPatch for TableKeyedPatch<'a> {
    fn apply(&self, font: &FontRef, compatibility_id: &[u32; 4]) -> Result<Vec<u8>, ReadError> {
        if self.compatibility_id() != compatibility_id {
            return Err(ReadError::ValidationError);
        }

        if self.format() != Tag::new(b"iftk") {
            return Err(ReadError::ValidationError);
        }

        let mut font_builder = FontBuilder::new();
        let mut processed_tables = BTreeSet::<Tag>::new();
        // TODO(garretrieger): enforce a max combined size of all decoded tables? say something in the spec about this?
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
                .and_then(|v| v.checked_sub(9))
            // brotli stream starts at the (u32 tag + u8 flags + u32 length) = 9th byte
            else {
                // TODO(garretrieger): update spec to clarify this case is an error.
                return Err(ReadError::MalformedData(
                    "invalid patch offsets result in < 0 byte length brotli stream.",
                ));
            };

            let tag = table_patch.tag();
            if !processed_tables.insert(tag) {
                // Table has already been processed.
                continue;
            }

            if table_patch.flags().contains(TablePatchFlags::DROP_TABLE) {
                // TODO(garretrieger): spec needs to be clarified on what happens when DROP_TABLE and REPLACE_TABLE are
                // both set.

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
    // TODO(garretrieger): spec needs to be clarified what happens when replacement = false, but the source table
    // does not exist in the font.
    let stream_length = stream_length as usize;
    let base_data = font.table_data(table_patch.tag());
    let stream = if table_patch.brotli_stream().len() >= stream_length {
        &table_patch.brotli_stream()[..stream_length]
    } else {
        // TODO(garretrieger): update spec to clarify this case is an error.
        return Err(ReadError::OutOfBounds);
    };
    let r = match (base_data, replacement) {
        (Some(base_data), false) => shared_brotli_decode(
            stream,
            Some(base_data.as_bytes()),
            table_patch.max_uncompressed_length(),
        ),
        _ => shared_brotli_decode(stream, None, table_patch.max_uncompressed_length()),
    };

    r.map_err(|decode_error| match decode_error {
        DecodeError::InitFailure => ReadError::MalformedData("Failure to init brotli encoder."),
        DecodeError::InvalidStream => ReadError::MalformedData("Malformed brotli stream."),
        DecodeError::InvalidDictionary => ReadError::MalformedData("Malformed dictionary."),
        DecodeError::MaxSizeExceeded => ReadError::OutOfBounds,
        DecodeError::ExcessInputData => {
            ReadError::MalformedData("Input brotli stream has excess bytes.")
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use font_test_data::ift::table_keyed_patch;
    use raw::FontRef;
    use read_fonts::ReadError;
    use write_fonts::FontBuilder;

    fn test_font() -> Vec<u8> {
        let mut font_builder = FontBuilder::new();
        font_builder.add_raw(Tag::new(b"tab1"), "abcdef\n".as_bytes());
        font_builder.add_raw(Tag::new(b"tab2"), "foobar\n".as_bytes());
        font_builder.add_raw(Tag::new(b"tab3"), "foobaz\n".as_bytes());
        font_builder.add_raw(Tag::new(b"tab4"), "unchanged\n".as_bytes());
        font_builder.build()
    }

    #[test]
    fn table_keyed_patch_test() {
        let font = test_font();
        let font = FontRef::new(&font).unwrap();

        let r = apply_patch(
            &font,
            &[1, 2, 3, 4],
            table_keyed_patch().as_slice(),
            PatchEncoding::TableKeyed {
                fully_invalidating: false,
            },
        );

        let font = r.unwrap();
        let font = FontRef::new(&font).unwrap();

        assert_eq!(font.table_directory.num_tables(), 3);

        let target = "hijkabcdeflmnohijkabcdeflmno\n".as_bytes();
        assert_eq!(
            font.table_data(Tag::new(b"tab1")).unwrap().as_bytes(),
            target
        );
        assert_eq!(
            font.table_data(Tag::new(b"tab2")).unwrap().as_bytes(),
            target
        );
        assert_eq!(
            font.table_data(Tag::new(b"tab4")).unwrap().as_bytes(),
            "unchanged\n".as_bytes()
        );
    }

    #[test]
    fn table_keyed_patch_compat_id_mismatch() {
        let font = test_font();
        let font = FontRef::new(&font).unwrap();

        assert_eq!(
            Err(ReadError::ValidationError),
            apply_patch(
                &font,
                &[1, 2, 2, 4],
                table_keyed_patch().as_slice(),
                PatchEncoding::TableKeyed {
                    fully_invalidating: false,
                },
            )
        );
    }

    #[test]
    fn table_keyed_patch_bad_top_level_tag() {
        let font = test_font();
        let font = FontRef::new(&font).unwrap();

        let mut patch = table_keyed_patch();
        patch.write_at("tag", Tag::new(b"ifgk"));
        let patch = patch.as_slice();

        assert_eq!(
            Err(ReadError::ValidationError),
            apply_patch(
                &font,
                &[1, 2, 3, 4],
                patch,
                PatchEncoding::TableKeyed {
                    fully_invalidating: false,
                },
            )
        );
    }

    #[test]
    fn table_keyed_ignore_duplicates() {
        let font = test_font();
        let font = FontRef::new(&font).unwrap();

        // add a duplicate entry requesting dropping of tab2,
        // should be ignored.
        let mut patch = table_keyed_patch();
        patch.write_at("patch[2]", Tag::new(b"tab2"));
        let patch = patch.as_slice();

        let r = apply_patch(
            &font,
            &[1, 2, 3, 4],
            patch,
            PatchEncoding::TableKeyed {
                fully_invalidating: false,
            },
        );

        let font = r.unwrap();
        let font = FontRef::new(&font).unwrap();

        assert_eq!(font.table_directory.num_tables(), 4);

        let target = "hijkabcdeflmnohijkabcdeflmno\n".as_bytes();
        assert_eq!(
            font.table_data(Tag::new(b"tab1")).unwrap().as_bytes(),
            target
        );
        assert_eq!(
            font.table_data(Tag::new(b"tab2")).unwrap().as_bytes(),
            target
        );
        assert_eq!(
            font.table_data(Tag::new(b"tab3")).unwrap().as_bytes(),
            "foobaz\n".as_bytes()
        );
        assert_eq!(
            font.table_data(Tag::new(b"tab4")).unwrap().as_bytes(),
            "unchanged\n".as_bytes()
        );
    }

    // TODO non ascending patch offsets.
    // TODO out of bounds table patch entry.
    // TODO drop and replace flag set simultaneously.
    // TODO drop table that's not present.
    // TODO replace table that's not present.
    // TODO noop patch (table count = 0)
}
