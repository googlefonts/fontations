//! Reads and applies font patches to font binaries.
//!
//! Font patch formats are defined as part of the incremental font transfer specification:
//! <https://w3c.github.io/IFT/Overview.html#font-patch-formats>
//!
//! Two main types of font patches are implemented:
//! 1. Table Keyed Patch - these patches contain a per table brotli binary patch to be applied
//!    to the input font.
//! 2. Glyph Keyed - these patches contain blobs of data associated with combinations of
//!    glyph id + table. The patch inserts these blobs into the table at the location for
//!    the corresponding glyph id.

use crate::patchmap::{PatchEncoding, PatchUri};

use read_fonts::tables::ift::{CompatibilityId, TablePatchFlags};
use read_fonts::tables::ift::{TableKeyedPatch, TablePatch};
use read_fonts::types::Tag;
use read_fonts::{FontData, FontRef};
use read_fonts::{FontRead, ReadError};
use shared_brotli_patch_decoder::{shared_brotli_decode, DecodeError};
use std::collections::BTreeSet;
use write_fonts::FontBuilder;

/// An incremental font patch which can be used to extend a font.
///
/// See: <https://w3c.github.io/IFT/Overview.html#font-patch-formats>
pub enum IncrementalFontPatch<'a> {
    TableKeyed(TableKeyedPatch<'a>),
}

impl PatchUri {
    /// Resolves a PatchUri into the actual underlying patch given the data associated with the URI.
    pub fn into_patch<'a>(
        &self,
        patch_data: &'a [u8],
    ) -> Result<IncrementalFontPatch<'a>, PatchingError> {
        let patch = match self.encoding() {
            PatchEncoding::TableKeyed { .. } => IncrementalFontPatch::TableKeyed(
                TableKeyedPatch::read(FontData::new(patch_data))
                    .map_err(PatchingError::PatchParsingFailed)?,
            ),
            PatchEncoding::GlyphKeyed => {
                todo!()
            }
        };

        if *self.expected_compatibility_id() != patch.compatibility_id() {
            // Compatibility ids must match.
            return Err(PatchingError::IncompatiblePatch);
        }

        Ok(patch)
    }
}

/// A trait for types to which an incremental font transfer patch can be applied.
///
/// See: <https://w3c.github.io/IFT/Overview.html#font-patch-formats> for details on the format of patches.
pub trait IncrementalFontPatchBase {
    /// Apply an incremental font patch (<https://w3c.github.io/IFT/Overview.html#font-patch-formats>)
    ///
    /// Applies the patch to this base. In the base the patch is associated with the supplied
    /// expected_compatibility_id and has the specified encoding.
    ///
    /// Returns the byte data for the new font produced as a result of the patch application.
    fn apply_patch(&self, patch: IncrementalFontPatch) -> Result<Vec<u8>, PatchingError>;
}

/// An error that occurs while trying to apply an IFT patch to a font file.
#[derive(Debug, Clone, PartialEq)]
pub enum PatchingError {
    PatchParsingFailed(ReadError),
    FontParsingFailed(ReadError),
    IncompatiblePatch,
    NonIncrementalFont,
    InvalidPatch(&'static str),
}

impl std::fmt::Display for PatchingError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            PatchingError::PatchParsingFailed(err) => {
                write!(f, "Failed to parse patch file: {}", err)
            }
            PatchingError::FontParsingFailed(err) => {
                write!(f, "Failed to parse font file: {}", err)
            }
            PatchingError::IncompatiblePatch => {
                write!(f, "Compatibility ID of the patch does not match the font.")
            }
            PatchingError::NonIncrementalFont => {
                write!(
                    f,
                    "Can't patch font as it's not an incremental transfer font."
                )
            }
            PatchingError::InvalidPatch(msg) => write!(f, "Invalid patch file: '{msg}'"),
        }
    }
}

impl std::error::Error for PatchingError {}

impl IncrementalFontPatch<'_> {
    fn compatibility_id(&self) -> CompatibilityId {
        match self {
            IncrementalFontPatch::TableKeyed(patch_data) => patch_data.compatibility_id(),
        }
    }

    fn apply_to(&self, font: &FontRef) -> Result<Vec<u8>, PatchingError> {
        match self {
            IncrementalFontPatch::TableKeyed(patch_data) => {
                apply_table_keyed_patch(patch_data, font)
            }
        }
    }
}

impl IncrementalFontPatchBase for FontRef<'_> {
    fn apply_patch(&self, patch: IncrementalFontPatch) -> Result<Vec<u8>, PatchingError> {
        if self.table_data(Tag::new(b"IFT ")).is_none()
            && self.table_data(Tag::new(b"IFTX")).is_none()
        {
            // This base is not an incremental font, which is an error.
            // See: https://w3c.github.io/IFT/Overview.html#apply-table-keyed
            return Err(PatchingError::NonIncrementalFont);
        }

        patch.apply_to(self)
    }
}

impl IncrementalFontPatchBase for &[u8] {
    fn apply_patch(&self, patch: IncrementalFontPatch) -> Result<Vec<u8>, PatchingError> {
        let font_ref = FontRef::new(self).map_err(PatchingError::FontParsingFailed)?;
        font_ref.apply_patch(patch)
    }
}

fn apply_table_keyed_patch(
    patch: &TableKeyedPatch<'_>,
    font: &FontRef,
) -> Result<Vec<u8>, PatchingError> {
    if patch.format() != Tag::new(b"iftk") {
        return Err(PatchingError::InvalidPatch("Patch file tag is not 'iftk'"));
    }

    // brotli stream starts at the (u32 tag + u8 flags + u32 length) = 9th byte
    const STREAM_START: u32 = 9;
    let mut font_builder = FontBuilder::new();
    let mut processed_tables = BTreeSet::<Tag>::new();
    // TODO(garretrieger): enforce a max combined size of all decoded tables? say something in the spec about this?
    for i in 0..patch.patches_count() {
        let i = i as usize;
        let next = i + 1;

        let table_patch = patch
            .patches()
            .get(i)
            .map_err(PatchingError::PatchParsingFailed)?;
        let (Some(offset), Some(next_offset)) = (
            patch.patch_offsets().get(i),
            patch.patch_offsets().get(next),
        ) else {
            return Err(PatchingError::InvalidPatch("Missing patch offset."));
        };

        let offset = offset.get().to_u32();
        let next_offset = next_offset.get().to_u32();
        let Some(stream_length) = next_offset
            .checked_sub(offset)
            .and_then(|v| v.checked_sub(STREAM_START))
        else {
            return Err(PatchingError::InvalidPatch(
                "Patch offsets are not in sorted order.",
            ));
        };

        if stream_length as usize > table_patch.brotli_stream().len() {
            return Err(PatchingError::PatchParsingFailed(ReadError::OutOfBounds));
        }

        let tag = table_patch.tag();
        if !processed_tables.insert(tag) {
            // Table has already been processed.
            continue;
        }

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

fn apply_table_patch(
    font: &FontRef,
    table_patch: TablePatch,
    stream_length: u32,
    replacement: bool,
) -> Result<Vec<u8>, PatchingError> {
    let stream_length = stream_length as usize;
    let base_data = font.table_data(table_patch.tag());
    let stream = if table_patch.brotli_stream().len() >= stream_length {
        &table_patch.brotli_stream()[..stream_length]
    } else {
        return Err(PatchingError::InvalidPatch(
            "Brotli stream is larger then the maxUncompressedLength field.",
        ));
    };
    let r = match (base_data, replacement) {
        (Some(base_data), false) => shared_brotli_decode(
            stream,
            Some(base_data.as_bytes()),
            table_patch.max_uncompressed_length() as usize,
        ),
        (None, false) => {
            return Err(PatchingError::InvalidPatch(
                "Trying to patch a base table that doesn't exist.",
            ))
        }
        _ => shared_brotli_decode(stream, None, table_patch.max_uncompressed_length() as usize),
    };

    r.map_err(|decode_error| match decode_error {
        DecodeError::InitFailure => PatchingError::InvalidPatch("Failure to init brotli encoder."),
        DecodeError::InvalidStream => PatchingError::InvalidPatch("Malformed brotli stream."),
        DecodeError::InvalidDictionary => PatchingError::InvalidPatch("Malformed dictionary."),
        DecodeError::MaxSizeExceeded => PatchingError::InvalidPatch("Max size exceeded."),
        DecodeError::ExcessInputData => {
            PatchingError::InvalidPatch("Input brotli stream has excess bytes.")
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use font_test_data::ift::{noop_table_keyed_patch, table_keyed_patch};
    use read_fonts::FontRef;
    use read_fonts::ReadError;
    use write_fonts::FontBuilder;

    const IFT_TABLE: &[u8] = "IFT PATCH MAP".as_bytes();
    const TABLE_1_FINAL_STATE: &[u8] = "hijkabcdeflmnohijkabcdeflmno\n".as_bytes();
    const TABLE_2_FINAL_STATE: &[u8] = "foobarbaz foobarbaz foobarbaz\n".as_bytes();
    const TABLE_3_FINAL_STATE: &[u8] = "foobaz\n".as_bytes();
    const TABLE_4_FINAL_STATE: &[u8] = "unchanged\n".as_bytes();

    fn table_keyed_patch_uri() -> PatchUri {
        PatchUri::from_index(
            "",
            0,
            &CompatibilityId::from_u32s([1, 2, 3, 4]),
            PatchEncoding::TableKeyed {
                fully_invalidating: false,
            },
        )
    }

    fn test_font() -> Vec<u8> {
        let mut font_builder = FontBuilder::new();
        font_builder.add_raw(Tag::new(b"IFT "), IFT_TABLE);
        font_builder.add_raw(Tag::new(b"tab1"), "abcdef\n".as_bytes());
        font_builder.add_raw(Tag::new(b"tab2"), "foobar\n".as_bytes());
        font_builder.add_raw(Tag::new(b"tab3"), "foobaz\n".as_bytes());
        font_builder.add_raw(Tag::new(b"tab4"), "unchanged\n".as_bytes());
        font_builder.build()
    }

    #[test]
    fn table_keyed_patch_test() {
        let patch_data = table_keyed_patch();
        let patch = table_keyed_patch_uri()
            .into_patch(patch_data.as_slice())
            .unwrap();

        let r = test_font().as_slice().apply_patch(patch);

        let font = r.unwrap();
        let font = FontRef::new(&font).unwrap();

        assert_eq!(font.table_directory.num_tables(), 4);

        assert_eq!(
            font.table_data(Tag::new(b"IFT ")).unwrap().as_bytes(),
            IFT_TABLE
        );
        assert_eq!(
            font.table_data(Tag::new(b"tab1")).unwrap().as_bytes(),
            TABLE_1_FINAL_STATE
        );
        assert_eq!(
            font.table_data(Tag::new(b"tab2")).unwrap().as_bytes(),
            TABLE_2_FINAL_STATE
        );
        assert_eq!(
            font.table_data(Tag::new(b"tab4")).unwrap().as_bytes(),
            TABLE_4_FINAL_STATE
        );
    }

    #[test]
    fn noop_table_keyed_patch_test() {
        let patch_data = noop_table_keyed_patch();
        let patch = table_keyed_patch_uri()
            .into_patch(patch_data.as_slice())
            .unwrap();

        let r = test_font().as_slice().apply_patch(patch);

        let font = r.unwrap();
        let font = FontRef::new(&font).unwrap();
        let expected_font = test_font();
        let expected_font = FontRef::new(&expected_font).unwrap();

        assert_eq!(
            font.table_directory.num_tables(),
            expected_font.table_directory.num_tables()
        );

        for t in expected_font.table_directory.table_records() {
            let data = font.table_data(t.tag()).unwrap();
            let expected_data = expected_font.table_data(t.tag()).unwrap();
            assert_eq!(data.as_bytes(), expected_data.as_bytes());
        }
    }

    #[test]
    fn table_keyed_patch_compat_id_mismatch() {
        let uri = PatchUri::from_index(
            "",
            0,
            &CompatibilityId::from_u32s([1, 2, 2, 4]),
            PatchEncoding::TableKeyed {
                fully_invalidating: false,
            },
        );
        let patch_data = table_keyed_patch();
        assert_eq!(
            PatchingError::IncompatiblePatch,
            uri.into_patch(patch_data.as_slice()).err().unwrap()
        );
    }

    #[test]
    fn table_keyed_patch_bad_top_level_tag() {
        let mut patch_data = table_keyed_patch();
        patch_data.write_at("tag", Tag::new(b"ifgk"));
        let patch = table_keyed_patch_uri()
            .into_patch(patch_data.as_slice())
            .unwrap();

        assert_eq!(
            Err(PatchingError::InvalidPatch("Patch file tag is not 'iftk'")),
            test_font().as_slice().apply_patch(patch)
        );
    }

    #[test]
    fn table_keyed_ignore_duplicates() {
        // add a duplicate entry requesting dropping of tab2,
        // should be ignored.
        let mut patch_data = table_keyed_patch();
        patch_data.write_at("patch[2]", Tag::new(b"tab2"));
        let patch = table_keyed_patch_uri()
            .into_patch(patch_data.as_slice())
            .unwrap();

        let r = test_font().as_slice().apply_patch(patch);

        let font = r.unwrap();
        let font = FontRef::new(&font).unwrap();

        assert_eq!(font.table_directory.num_tables(), 5);

        assert_eq!(
            font.table_data(Tag::new(b"IFT ")).unwrap().as_bytes(),
            IFT_TABLE
        );
        assert_eq!(
            font.table_data(Tag::new(b"tab1")).unwrap().as_bytes(),
            TABLE_1_FINAL_STATE
        );
        assert_eq!(
            font.table_data(Tag::new(b"tab2")).unwrap().as_bytes(),
            TABLE_2_FINAL_STATE
        );
        assert_eq!(
            font.table_data(Tag::new(b"tab3")).unwrap().as_bytes(),
            TABLE_3_FINAL_STATE
        );
        assert_eq!(
            font.table_data(Tag::new(b"tab4")).unwrap().as_bytes(),
            TABLE_4_FINAL_STATE
        );
    }

    #[test]
    fn table_keyed_patch_unsorted_offsets() {
        // reorder the offsets
        let mut patch_data = table_keyed_patch();

        let offset = patch_data.offset_for("patch[1]") as u32;
        patch_data.write_at("patch_off[0]", offset);

        let offset = patch_data.offset_for("patch[0]") as u32;
        patch_data.write_at("patch_off[1]", offset);

        let patch = table_keyed_patch_uri()
            .into_patch(patch_data.as_slice())
            .unwrap();

        assert_eq!(
            Err(PatchingError::InvalidPatch(
                "Patch offsets are not in sorted order."
            )),
            test_font().as_slice().apply_patch(patch)
        );
    }

    #[test]
    fn table_keyed_patch_out_of_bounds_offsets() {
        // set last offset out of bounds
        let mut patch_data = table_keyed_patch();
        let offset = (patch_data.offset_for("end") + 5) as u32;
        patch_data.write_at("patch_off[3]", offset);

        let patch = table_keyed_patch_uri()
            .into_patch(patch_data.as_slice())
            .unwrap();

        assert_eq!(
            Err(PatchingError::PatchParsingFailed(ReadError::OutOfBounds)),
            test_font().as_slice().apply_patch(patch,)
        );
    }

    #[test]
    fn table_keyed_patch_drop_and_replace() {
        let mut patch_data = table_keyed_patch();
        patch_data.write_at("flags[2]", 3u8);

        let patch = table_keyed_patch_uri()
            .into_patch(patch_data.as_slice())
            .unwrap();

        // When DROP and REPLACE are both set DROP takes priority.
        let r = test_font().as_slice().apply_patch(patch);

        let font = r.unwrap();
        let font = FontRef::new(&font).unwrap();

        assert_eq!(font.table_directory.num_tables(), 4);

        assert_eq!(
            font.table_data(Tag::new(b"IFT ")).unwrap().as_bytes(),
            IFT_TABLE
        );
        assert_eq!(
            font.table_data(Tag::new(b"tab1")).unwrap().as_bytes(),
            TABLE_1_FINAL_STATE
        );
        assert_eq!(
            font.table_data(Tag::new(b"tab2")).unwrap().as_bytes(),
            TABLE_2_FINAL_STATE
        );
        assert_eq!(
            font.table_data(Tag::new(b"tab4")).unwrap().as_bytes(),
            TABLE_4_FINAL_STATE
        );
    }

    #[test]
    fn table_keyed_patch_missing_table() {
        let mut patch_data = table_keyed_patch();
        patch_data.write_at("flags[1]", 0u8);
        patch_data.write_at("patch[1]", Tag::new(b"tab5"));

        let patch = table_keyed_patch_uri()
            .into_patch(patch_data.as_slice())
            .unwrap();

        assert_eq!(
            Err(PatchingError::InvalidPatch(
                "Trying to patch a base table that doesn't exist."
            )),
            test_font().as_slice().apply_patch(patch),
        );
    }

    #[test]
    fn table_keyed_replace_missing_table() {
        let mut patch_data = table_keyed_patch();
        patch_data.write_at("patch[1]", Tag::new(b"tab5"));

        let patch = table_keyed_patch_uri()
            .into_patch(patch_data.as_slice())
            .unwrap();

        let r = test_font().as_slice().apply_patch(patch);

        let font = r.unwrap();
        let font = FontRef::new(&font).unwrap();

        assert_eq!(font.table_directory.num_tables(), 5);

        assert_eq!(
            font.table_data(Tag::new(b"IFT ")).unwrap().as_bytes(),
            IFT_TABLE
        );
        assert_eq!(
            font.table_data(Tag::new(b"tab1")).unwrap().as_bytes(),
            TABLE_1_FINAL_STATE
        );
        assert_eq!(
            font.table_data(Tag::new(b"tab2")).unwrap().as_bytes(),
            "foobar\n".as_bytes()
        );
        assert_eq!(
            font.table_data(Tag::new(b"tab4")).unwrap().as_bytes(),
            TABLE_4_FINAL_STATE
        );
        assert_eq!(
            font.table_data(Tag::new(b"tab5")).unwrap().as_bytes(),
            TABLE_2_FINAL_STATE
        );
    }

    #[test]
    fn table_keyed_drop_missing_table() {
        let mut patch_data = table_keyed_patch();
        patch_data.write_at("patch[2]", Tag::new(b"tab5"));

        let patch = table_keyed_patch_uri()
            .into_patch(patch_data.as_slice())
            .unwrap();

        let r = test_font().as_slice().apply_patch(patch);

        let font = r.unwrap();
        let font = FontRef::new(&font).unwrap();

        assert_eq!(font.table_directory.num_tables(), 5);

        assert_eq!(
            font.table_data(Tag::new(b"IFT ")).unwrap().as_bytes(),
            IFT_TABLE
        );
        assert_eq!(
            font.table_data(Tag::new(b"tab1")).unwrap().as_bytes(),
            TABLE_1_FINAL_STATE
        );
        assert_eq!(
            font.table_data(Tag::new(b"tab2")).unwrap().as_bytes(),
            TABLE_2_FINAL_STATE,
        );
        assert_eq!(
            font.table_data(Tag::new(b"tab3")).unwrap().as_bytes(),
            TABLE_3_FINAL_STATE
        );
        assert_eq!(
            font.table_data(Tag::new(b"tab4")).unwrap().as_bytes(),
            TABLE_4_FINAL_STATE
        );
    }

    #[test]
    fn table_keyed_patch_uncompressed_len_too_small() {
        let mut patch_data = table_keyed_patch();
        patch_data.write_at("decompressed_len[0]", 28u32);
        let patch = table_keyed_patch_uri()
            .into_patch(patch_data.as_slice())
            .unwrap();

        assert_eq!(
            Err(PatchingError::InvalidPatch("Max size exceeded.")),
            test_font().as_slice().apply_patch(patch),
        );
    }
}
