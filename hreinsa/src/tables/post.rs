//! Sanitizer for the `post` (PostScript) table.

use font_types::Tag;

use crate::context::SanitizeContext;
use crate::error::{MessageLevel, SanitizeError};
use crate::tables::maxp::MaxpState;

pub const TAG: Tag = Tag::new(b"post");
pub const POST_HEADER_LEN: usize = 32;

/// Sanitized state of the `post` table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostState {
    pub version: u32,
    pub italic_angle: u32,
    pub underline_position: i16,
    pub underline_thickness: i16,
    pub is_fixed_pitch: u32,
    pub glyph_name_indices: Vec<u16>,
    pub names: Vec<Vec<u8>>,
}

impl PostState {
    pub fn parse(
        data: &[u8],
        maxp: &MaxpState,
        has_cff: bool,
        context: &mut dyn SanitizeContext,
    ) -> Result<Self, SanitizeError> {
        if data.len() < POST_HEADER_LEN {
            return Err(SanitizeError::TableParseError {
                tag: TAG,
                message: "Table too short for post header".into(),
            });
        }

        let mut version = u32::from_be_bytes(data[0..4].try_into().unwrap());
        if version != 0x00010000 && version != 0x00020000 && version != 0x00030000 {
            return Err(SanitizeError::TableParseError {
                tag: TAG,
                message: format!("Unsupported post table version {version:#010x}"),
            });
        }

        let italic_angle = u32::from_be_bytes(data[4..8].try_into().unwrap());
        let underline_position = i16::from_be_bytes(data[8..10].try_into().unwrap());
        let raw_thickness = i16::from_be_bytes(data[10..12].try_into().unwrap());
        let underline_thickness = if raw_thickness < 0 {
            context.message(
                MessageLevel::Warning,
                &format!("post: negative underlineThickness {raw_thickness}, setting to 1"),
            );
            1
        } else {
            raw_thickness
        };

        let is_fixed_pitch = u32::from_be_bytes(data[12..16].try_into().unwrap());
        // Memory usage fields at 16..32 are ignored and will be zeroed out

        // CFF fonts must use v3.0 post table per OpenType spec
        if has_cff && version != 0x00030000 {
            context.message(
                MessageLevel::Warning,
                &format!(
                    "post: font with CFF requires version 0x00030000; upgrading from {version:#010x}"
                ),
            );
            version = 0x00030000;
        }

        if version == 0x00010000 || version == 0x00030000 {
            return Ok(Self {
                version,
                italic_angle,
                underline_position,
                underline_thickness,
                is_fixed_pitch,
                glyph_name_indices: Vec::new(),
                names: Vec::new(),
            });
        }

        // Version 2.0 with glyph names
        if data.len() < POST_HEADER_LEN + 2 {
            return Err(SanitizeError::TableParseError {
                tag: TAG,
                message: "Table too short for post v2.0 numberOfGlyphs".into(),
            });
        }

        let num_glyphs =
            u16::from_be_bytes(data[POST_HEADER_LEN..POST_HEADER_LEN + 2].try_into().unwrap());

        if num_glyphs == 0 {
            if maxp.num_glyphs > 258 {
                return Err(SanitizeError::TableParseError {
                    tag: TAG,
                    message: "Cannot have 0 glyphs in post v2 if maxp numGlyphs > 258".into(),
                });
            }
            context.message(
                MessageLevel::Warning,
                "post: numberOfGlyphs is 0, downgrading to version 1.0",
            );
            return Ok(Self {
                version: 0x00010000,
                italic_angle,
                underline_position,
                underline_thickness,
                is_fixed_pitch,
                glyph_name_indices: Vec::new(),
                names: Vec::new(),
            });
        }

        if num_glyphs != maxp.num_glyphs {
            return Err(SanitizeError::TableParseError {
                tag: TAG,
                message: format!(
                    "post: numberOfGlyphs {num_glyphs} does not match maxp.numGlyphs {}",
                    maxp.num_glyphs
                ),
            });
        }

        let indices_end = POST_HEADER_LEN + 2 + (num_glyphs as usize) * 2;
        if data.len() < indices_end {
            return Err(SanitizeError::TableParseError {
                tag: TAG,
                message: "Table too short for post v2 glyph name indices".into(),
            });
        }

        let mut glyph_name_indices = Vec::with_capacity(num_glyphs as usize);
        for i in 0..num_glyphs as usize {
            let off = POST_HEADER_LEN + 2 + i * 2;
            let index = u16::from_be_bytes(data[off..off + 2].try_into().unwrap());
            glyph_name_indices.push(index);
        }

        // Parse Pascal strings
        let mut string_cursor = indices_end;
        let mut names = Vec::new();
        while string_cursor < data.len() {
            let str_len = data[string_cursor] as usize;
            string_cursor += 1;
            if string_cursor + str_len > data.len() {
                return Err(SanitizeError::TableParseError {
                    tag: TAG,
                    message: format!("Pascal string length {str_len} overruns post table"),
                });
            }
            let str_bytes = &data[string_cursor..string_cursor + str_len];
            if str_bytes.contains(&0) {
                return Err(SanitizeError::TableParseError {
                    tag: TAG,
                    message: "PostScript glyph name contains NUL byte".into(),
                });
            }
            names.push(str_bytes.to_vec());
            string_cursor += str_len;
        }

        // Check bounds of all string indices
        for (i, &idx) in glyph_name_indices.iter().enumerate() {
            if idx >= 258 {
                let custom_idx = (idx - 258) as usize;
                if custom_idx >= names.len() {
                    return Err(SanitizeError::TableParseError {
                        tag: TAG,
                        message: format!(
                            "Bad glyph name index {idx} for glyph {i} (only {} custom names)",
                            names.len()
                        ),
                    });
                }
            }
        }

        Ok(Self {
            version,
            italic_angle,
            underline_position,
            underline_thickness,
            is_fixed_pitch,
            glyph_name_indices,
            names,
        })
    }

    pub fn serialize(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(POST_HEADER_LEN);
        bytes.extend_from_slice(&self.version.to_be_bytes());
        bytes.extend_from_slice(&self.italic_angle.to_be_bytes());
        bytes.extend_from_slice(&self.underline_position.to_be_bytes());
        bytes.extend_from_slice(&self.underline_thickness.to_be_bytes());
        bytes.extend_from_slice(&self.is_fixed_pitch.to_be_bytes());
        bytes.extend_from_slice(&[0u8; 16]); // 4 zero memory fields

        if self.version == 0x00020000 {
            bytes.extend_from_slice(&(self.glyph_name_indices.len() as u16).to_be_bytes());
            for &idx in &self.glyph_name_indices {
                bytes.extend_from_slice(&idx.to_be_bytes());
            }
            for name in &self.names {
                bytes.push(name.len() as u8);
                bytes.extend_from_slice(name);
            }
        }

        bytes
    }
}
