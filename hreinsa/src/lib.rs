//! # hreinsa
//!
//! A Rust port of [OpenType Sanitizer (OTS)](https://github.com/khaledhosny/ots).
//! Parses, validates, sanitizes, and serializes untrusted OpenType font files.

pub mod container;
pub mod context;
pub mod error;
pub mod tables;

pub use context::{DefaultContext, NullContext, SanitizeContext, TableAction};
pub use error::{MessageLevel, SanitizeError};

use container::{SfntDirectory, TtcHeader};

/// Sanitize an OpenType font using default options and context.
///
/// If the input is a font collection (`.ttc`), all subfonts within the collection
/// will be sanitized and a new valid collection will be returned.
pub fn sanitize(data: &[u8]) -> Result<Vec<u8>, SanitizeError> {
    let mut context = DefaultContext::new();
    sanitize_font(data, None, &mut context)
}

/// Sanitize an OpenType font or TrueType Collection with a custom `SanitizeContext`
/// and optional collection font `index`.
///
/// If `index` is `Some(i)` and `data` is a font collection, only the font at that index
/// will be sanitized and returned as a standalone SFNT font.
pub fn sanitize_font(
    data: &[u8],
    index: Option<u32>,
    context: &mut dyn SanitizeContext,
) -> Result<Vec<u8>, SanitizeError> {
    if let Some(ttc) = TtcHeader::try_parse(data)? {
        if let Some(idx) = index {
            if idx >= ttc.num_fonts {
                return Err(SanitizeError::TtcIndexOutOfBounds {
                    index: idx,
                    num_fonts: ttc.num_fonts,
                });
            }
            let offset = ttc.font_offsets[idx as usize] as usize;
            let dir = SfntDirectory::parse(data, offset, context)?;
            tables::sanitize_tables(data, &dir, context)
        } else {
            // Sanitize each subfont in the TTC and rebuild a collection
            let mut sanitized_subfonts = Vec::with_capacity(ttc.num_fonts as usize);
            for i in 0..ttc.num_fonts as usize {
                let offset = ttc.font_offsets[i] as usize;
                let dir = SfntDirectory::parse(data, offset, context)?;
                let font_bytes = tables::sanitize_tables(data, &dir, context)?;
                sanitized_subfonts.push(font_bytes);
            }

            // Assemble TTC header
            // ttcf (4) + version 1.0 (4) + num_fonts (4) + offsets (num_fonts * 4)
            let header_len = 12 + (ttc.num_fonts as usize) * 4;
            let mut current_offset = header_len;
            let mut out_offsets = Vec::with_capacity(ttc.num_fonts as usize);

            for sf in &sanitized_subfonts {
                out_offsets.push(current_offset as u32);
                let padded_len = (sf.len() + 3) & !3;
                current_offset += padded_len;
            }

            let mut out = Vec::with_capacity(current_offset);
            out.extend_from_slice(b"ttcf");
            out.extend_from_slice(&0x00010000u32.to_be_bytes());
            out.extend_from_slice(&ttc.num_fonts.to_be_bytes());
            for off in out_offsets {
                out.extend_from_slice(&off.to_be_bytes());
            }

            for sf in sanitized_subfonts {
                out.extend_from_slice(&sf);
                let rem = (4 - (sf.len() & 3)) % 4;
                out.extend_from_slice(&[0u8; 4][..rem]);
            }

            Ok(out)
        }
    } else {
        let dir = SfntDirectory::parse(data, 0, context)?;
        tables::sanitize_tables(data, &dir, context)
    }
}
