//! Sanitizer for the `maxp` (Maximum Profile) table.

use font_types::Tag;

use crate::context::SanitizeContext;
use crate::error::{MessageLevel, SanitizeError};

pub const TAG: Tag = Tag::new(b"maxp");

/// Sanitized and mutable state of the `maxp` table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaxpState {
    pub is_version_1: bool,
    pub num_glyphs: u16,
    pub max_points: u16,
    pub max_contours: u16,
    pub max_composite_points: u16,
    pub max_composite_contours: u16,
    pub max_zones: u16,
    pub max_twilight_points: u16,
    pub max_storage: u16,
    pub max_function_defs: u16,
    pub max_instruction_defs: u16,
    pub max_stack_elements: u16,
    pub max_size_of_instructions: u16,
    pub max_component_elements: u16,
    pub max_component_depth: u16,
}

impl MaxpState {
    pub fn parse(data: &[u8], context: &mut dyn SanitizeContext) -> Result<Self, SanitizeError> {
        if data.len() < 6 {
            return Err(SanitizeError::TableParseError {
                tag: TAG,
                message: "Table too short to read version and numGlyphs".into(),
            });
        }

        let raw_version = u32::from_be_bytes(data[0..4].try_into().unwrap());
        let major = raw_version >> 16;
        if major > 1 {
            return Err(SanitizeError::TableParseError {
                tag: TAG,
                message: format!("Unsupported table version {raw_version:#010x}"),
            });
        }

        let num_glyphs = u16::from_be_bytes(data[4..6].try_into().unwrap());
        if num_glyphs == 0 {
            return Err(SanitizeError::TableParseError {
                tag: TAG,
                message: "numGlyphs is 0".into(),
            });
        }

        // Version 0.5 (typically for CFF)
        if raw_version == 0x00005000 {
            return Ok(Self {
                is_version_1: false,
                num_glyphs,
                max_points: 0,
                max_contours: 0,
                max_composite_points: 0,
                max_composite_contours: 0,
                max_zones: 0,
                max_twilight_points: 0,
                max_storage: 0,
                max_function_defs: 0,
                max_instruction_defs: 0,
                max_stack_elements: 0,
                max_size_of_instructions: 0,
                max_component_elements: 0,
                max_component_depth: 0,
            });
        }

        if raw_version != 0x00010000 {
            context.message(
                MessageLevel::Warning,
                &format!(
                    "maxp: unexpected version {raw_version:#010x}; attempting to read as version 1.0"
                ),
            );
        }

        // Check if full version 1.0 table is present (32 bytes total)
        if data.len() < 32 {
            context.message(
                MessageLevel::Warning,
                "maxp: failed to read version 1.0 fields, downgrading to version 0.5",
            );
            return Ok(Self {
                is_version_1: false,
                num_glyphs,
                max_points: 0,
                max_contours: 0,
                max_composite_points: 0,
                max_composite_contours: 0,
                max_zones: 0,
                max_twilight_points: 0,
                max_storage: 0,
                max_function_defs: 0,
                max_instruction_defs: 0,
                max_stack_elements: 0,
                max_size_of_instructions: 0,
                max_component_elements: 0,
                max_component_depth: 0,
            });
        }

        let max_points = u16::from_be_bytes(data[6..8].try_into().unwrap());
        let max_contours = u16::from_be_bytes(data[8..10].try_into().unwrap());
        let max_composite_points = u16::from_be_bytes(data[10..12].try_into().unwrap());
        let max_composite_contours = u16::from_be_bytes(data[12..14].try_into().unwrap());
        let mut max_zones = u16::from_be_bytes(data[14..16].try_into().unwrap());
        let max_twilight_points = u16::from_be_bytes(data[16..18].try_into().unwrap());
        let max_storage = u16::from_be_bytes(data[18..20].try_into().unwrap());
        let max_function_defs = u16::from_be_bytes(data[20..22].try_into().unwrap());
        let max_instruction_defs = u16::from_be_bytes(data[22..24].try_into().unwrap());
        let max_stack_elements = u16::from_be_bytes(data[24..26].try_into().unwrap());
        let max_size_of_instructions = u16::from_be_bytes(data[26..28].try_into().unwrap());
        let max_component_elements = u16::from_be_bytes(data[28..30].try_into().unwrap());
        let max_component_depth = u16::from_be_bytes(data[30..32].try_into().unwrap());

        if max_zones < 1 {
            context.message(
                MessageLevel::Warning,
                &format!("maxp: bad maxZones {max_zones}, setting to 1"),
            );
            max_zones = 1;
        } else if max_zones > 2 {
            context.message(
                MessageLevel::Warning,
                &format!("maxp: bad maxZones {max_zones}, setting to 2"),
            );
            max_zones = 2;
        }

        Ok(Self {
            is_version_1: true,
            num_glyphs,
            max_points,
            max_contours,
            max_composite_points,
            max_composite_contours,
            max_zones,
            max_twilight_points,
            max_storage,
            max_function_defs,
            max_instruction_defs,
            max_stack_elements,
            max_size_of_instructions,
            max_component_elements,
            max_component_depth,
        })
    }

    /// Serialize the sanitized maxp state into bytes.
    pub fn serialize(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(if self.is_version_1 { 32 } else { 6 });
        let version_val: u32 = if self.is_version_1 {
            0x00010000
        } else {
            0x00005000
        };
        bytes.extend_from_slice(&version_val.to_be_bytes());
        bytes.extend_from_slice(&self.num_glyphs.to_be_bytes());

        if self.is_version_1 {
            bytes.extend_from_slice(&self.max_points.to_be_bytes());
            bytes.extend_from_slice(&self.max_contours.to_be_bytes());
            bytes.extend_from_slice(&self.max_composite_points.to_be_bytes());
            bytes.extend_from_slice(&self.max_composite_contours.to_be_bytes());
            bytes.extend_from_slice(&self.max_zones.to_be_bytes());
            bytes.extend_from_slice(&self.max_twilight_points.to_be_bytes());
            bytes.extend_from_slice(&self.max_storage.to_be_bytes());
            bytes.extend_from_slice(&self.max_function_defs.to_be_bytes());
            bytes.extend_from_slice(&self.max_instruction_defs.to_be_bytes());
            bytes.extend_from_slice(&self.max_stack_elements.to_be_bytes());
            bytes.extend_from_slice(&self.max_size_of_instructions.to_be_bytes());
            bytes.extend_from_slice(&self.max_component_elements.to_be_bytes());
            bytes.extend_from_slice(&self.max_component_depth.to_be_bytes());
        }

        bytes
    }
}
