//! Sanitizer for the `head` (Font Header) table.

use font_types::Tag;

use crate::context::SanitizeContext;
use crate::error::{MessageLevel, SanitizeError};

pub const TAG: Tag = Tag::new(b"head");
pub const HEAD_MAGIC: u32 = 0x5F0F3CF5;
pub const HEAD_TABLE_LEN: usize = 54;

/// Sanitized and mutable state of the `head` table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadState {
    pub font_revision: u32,
    pub flags: u16,
    pub units_per_em: u16,
    pub created: i64,
    pub modified: i64,
    pub x_min: i16,
    pub y_min: i16,
    pub x_max: i16,
    pub y_max: i16,
    pub mac_style: u16,
    pub lowest_rec_ppem: u16,
    pub index_to_loc_format: i16,
}

impl HeadState {
    pub fn parse(data: &[u8], context: &mut dyn SanitizeContext) -> Result<Self, SanitizeError> {
        if data.len() < HEAD_TABLE_LEN {
            return Err(SanitizeError::TableParseError {
                tag: TAG,
                message: "Table too short for head table header".into(),
            });
        }

        let version = u32::from_be_bytes(data[0..4].try_into().unwrap());
        if version >> 16 != 1 {
            return Err(SanitizeError::TableParseError {
                tag: TAG,
                message: format!("Unsupported majorVersion: {}", version >> 16),
            });
        }

        let font_revision = u32::from_be_bytes(data[4..8].try_into().unwrap());
        // Skip checkSumAdjustment at 8..12
        let magic = u32::from_be_bytes(data[12..16].try_into().unwrap());
        if magic != HEAD_MAGIC {
            return Err(SanitizeError::TableParseError {
                tag: TAG,
                message: format!("Invalid magicNumber: {magic:#010x}"),
            });
        }

        let raw_flags = u16::from_be_bytes(data[16..18].try_into().unwrap());
        let flags = raw_flags & 0x381F; // allow bits 0..4, 11..13
        if flags != raw_flags {
            context.message(
                MessageLevel::Warning,
                &format!("head: masked reserved flags from {raw_flags:#06x} to {flags:#06x}"),
            );
        }

        let units_per_em = u16::from_be_bytes(data[18..20].try_into().unwrap());
        if !(16..=16384).contains(&units_per_em) {
            return Err(SanitizeError::TableParseError {
                tag: TAG,
                message: format!("unitsPerEm not in range [16, 16384]: {units_per_em}"),
            });
        }

        let created = i64::from_be_bytes(data[20..28].try_into().unwrap());
        let modified = i64::from_be_bytes(data[28..36].try_into().unwrap());

        let x_min = i16::from_be_bytes(data[36..38].try_into().unwrap());
        let y_min = i16::from_be_bytes(data[38..40].try_into().unwrap());
        let x_max = i16::from_be_bytes(data[40..42].try_into().unwrap());
        let y_max = i16::from_be_bytes(data[42..44].try_into().unwrap());

        if x_min > x_max {
            return Err(SanitizeError::TableParseError {
                tag: TAG,
                message: format!("Bad x dimension in font bounding box: ({x_min}, {x_max})"),
            });
        }
        if y_min > y_max {
            return Err(SanitizeError::TableParseError {
                tag: TAG,
                message: format!("Bad y dimension in font bounding box: ({y_min}, {y_max})"),
            });
        }

        let raw_mac_style = u16::from_be_bytes(data[44..46].try_into().unwrap());
        let mac_style = raw_mac_style & 0x7F; // allow bits 0..6
        if mac_style != raw_mac_style {
            context.message(
                MessageLevel::Warning,
                &format!(
                    "head: masked reserved macStyle bits from {raw_mac_style:#06x} to {mac_style:#06x}"
                ),
            );
        }

        let lowest_rec_ppem = u16::from_be_bytes(data[46..48].try_into().unwrap());
        // Skip fontDirectionHint at 48..50

        let index_to_loc_format = i16::from_be_bytes(data[50..52].try_into().unwrap());
        if index_to_loc_format != 0 && index_to_loc_format != 1 {
            return Err(SanitizeError::TableParseError {
                tag: TAG,
                message: format!("Bad indexToLocFormat: {index_to_loc_format}"),
            });
        }

        let glyph_data_format = i16::from_be_bytes(data[52..54].try_into().unwrap());
        if glyph_data_format != 0 {
            return Err(SanitizeError::TableParseError {
                tag: TAG,
                message: format!("Unsupported glyphDataFormat: {glyph_data_format}"),
            });
        }

        Ok(Self {
            font_revision,
            flags,
            units_per_em,
            created,
            modified,
            x_min,
            y_min,
            x_max,
            y_max,
            mac_style,
            lowest_rec_ppem,
            index_to_loc_format,
        })
    }

    /// Serialize the sanitized head table. Note: checkSumAdjustment is zeroed here,
    /// and will be updated by FontBuilder.
    pub fn serialize(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(HEAD_TABLE_LEN);
        bytes.extend_from_slice(&0x00010000u32.to_be_bytes());
        bytes.extend_from_slice(&self.font_revision.to_be_bytes());
        bytes.extend_from_slice(&0u32.to_be_bytes()); // checksum adjustment placeholder
        bytes.extend_from_slice(&HEAD_MAGIC.to_be_bytes());
        bytes.extend_from_slice(&self.flags.to_be_bytes());
        bytes.extend_from_slice(&self.units_per_em.to_be_bytes());
        bytes.extend_from_slice(&self.created.to_be_bytes());
        bytes.extend_from_slice(&self.modified.to_be_bytes());
        bytes.extend_from_slice(&self.x_min.to_be_bytes());
        bytes.extend_from_slice(&self.y_min.to_be_bytes());
        bytes.extend_from_slice(&self.x_max.to_be_bytes());
        bytes.extend_from_slice(&self.y_max.to_be_bytes());
        bytes.extend_from_slice(&self.mac_style.to_be_bytes());
        bytes.extend_from_slice(&self.lowest_rec_ppem.to_be_bytes());
        bytes.extend_from_slice(&2i16.to_be_bytes()); // fontDirectionHint is set to 2 per OTS
        bytes.extend_from_slice(&self.index_to_loc_format.to_be_bytes());
        bytes.extend_from_slice(&0i16.to_be_bytes()); // glyphDataFormat
        bytes
    }
}
