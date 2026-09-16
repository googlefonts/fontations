//! Sanitizer for the `loca` (Index to Location) table.

use font_types::Tag;

use crate::context::SanitizeContext;
use crate::error::SanitizeError;
use crate::tables::head::HeadState;
use crate::tables::maxp::MaxpState;

pub const TAG: Tag = Tag::new(b"loca");

/// Sanitized state of the `loca` table containing byte offsets for all glyphs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocaState {
    pub offsets: Vec<u32>,
}

impl LocaState {
    pub fn parse(
        data: &[u8],
        head: &HeadState,
        maxp: &MaxpState,
        _context: &mut dyn SanitizeContext,
    ) -> Result<Self, SanitizeError> {
        let num_glyphs = maxp.num_glyphs as usize;
        let count = num_glyphs + 1;
        let mut offsets = Vec::with_capacity(count);

        if head.index_to_loc_format == 0 {
            let expected_len = count * 2;
            if data.len() < expected_len {
                return Err(SanitizeError::TableParseError {
                    tag: TAG,
                    message: format!(
                        "Table too short for short loca: expected {expected_len}, got {}",
                        data.len()
                    ),
                });
            }

            let mut last_offset = 0u32;
            for i in 0..count {
                let off = u16::from_be_bytes(data[i * 2..i * 2 + 2].try_into().unwrap()) as u32;
                if off < last_offset {
                    return Err(SanitizeError::TableParseError {
                        tag: TAG,
                        message: format!(
                            "Out of order offset {off} < {last_offset} for glyph {i}"
                        ),
                    });
                }
                last_offset = off;
                offsets.push(off * 2);
            }
        } else {
            let expected_len = count * 4;
            if data.len() < expected_len {
                return Err(SanitizeError::TableParseError {
                    tag: TAG,
                    message: format!(
                        "Table too short for long loca: expected {expected_len}, got {}",
                        data.len()
                    ),
                });
            }

            let mut last_offset = 0u32;
            for i in 0..count {
                let off = u32::from_be_bytes(data[i * 4..i * 4 + 4].try_into().unwrap());
                if off < last_offset {
                    return Err(SanitizeError::TableParseError {
                        tag: TAG,
                        message: format!(
                            "Out of order offset {off} < {last_offset} for glyph {i}"
                        ),
                    });
                }
                last_offset = off;
                offsets.push(off);
            }
        }

        Ok(Self { offsets })
    }

    pub fn serialize(&self, head: &HeadState) -> Vec<u8> {
        if head.index_to_loc_format == 0 {
            let mut bytes = Vec::with_capacity(self.offsets.len() * 2);
            for &off in &self.offsets {
                let short_off = (off >> 1) as u16;
                bytes.extend_from_slice(&short_off.to_be_bytes());
            }
            bytes
        } else {
            let mut bytes = Vec::with_capacity(self.offsets.len() * 4);
            for &off in &self.offsets {
                bytes.extend_from_slice(&off.to_be_bytes());
            }
            bytes
        }
    }
}
