//! Sanitizer for the `hhea` (Horizontal Header) table.

use font_types::Tag;

use crate::context::SanitizeContext;
use crate::error::{MessageLevel, SanitizeError};
use crate::tables::maxp::MaxpState;

pub const TAG: Tag = Tag::new(b"hhea");
pub const HHEA_TABLE_LEN: usize = 36;

/// Sanitized state of the `hhea` table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HheaState {
    pub ascent: i16,
    pub descent: i16,
    pub linegap: i16,
    pub adv_width_max: u16,
    pub min_left_side_bearing: i16,
    pub min_right_side_bearing: i16,
    pub x_max_extent: i16,
    pub caret_slope_rise: i16,
    pub caret_slope_run: i16,
    pub caret_offset: i16,
    pub number_of_h_metrics: u16,
}

impl HheaState {
    pub fn parse(
        data: &[u8],
        maxp: &MaxpState,
        context: &mut dyn SanitizeContext,
    ) -> Result<Self, SanitizeError> {
        if data.len() < HHEA_TABLE_LEN {
            return Err(SanitizeError::TableParseError {
                tag: TAG,
                message: "Table too short for hhea".into(),
            });
        }

        let version = u32::from_be_bytes(data[0..4].try_into().unwrap());
        if version >> 16 != 1 {
            return Err(SanitizeError::TableParseError {
                tag: TAG,
                message: format!("Unsupported majorVersion: {}", version >> 16),
            });
        }

        let raw_ascent = i16::from_be_bytes(data[4..6].try_into().unwrap());
        let ascent = if raw_ascent < 0 {
            context.message(
                MessageLevel::Warning,
                &format!("hhea: negative ascent {raw_ascent}, setting to 0"),
            );
            0
        } else {
            raw_ascent
        };

        let descent = i16::from_be_bytes(data[6..8].try_into().unwrap());

        let raw_linegap = i16::from_be_bytes(data[8..10].try_into().unwrap());
        let linegap = if raw_linegap < 0 {
            context.message(
                MessageLevel::Warning,
                &format!("hhea: negative linegap {raw_linegap}, setting to 0"),
            );
            0
        } else {
            raw_linegap
        };

        let adv_width_max = u16::from_be_bytes(data[10..12].try_into().unwrap());
        let min_left_side_bearing = i16::from_be_bytes(data[12..14].try_into().unwrap());
        let min_right_side_bearing = i16::from_be_bytes(data[14..16].try_into().unwrap());
        let x_max_extent = i16::from_be_bytes(data[16..18].try_into().unwrap());
        let caret_slope_rise = i16::from_be_bytes(data[18..20].try_into().unwrap());
        let caret_slope_run = i16::from_be_bytes(data[20..22].try_into().unwrap());
        let caret_offset = i16::from_be_bytes(data[22..24].try_into().unwrap());

        // Skip 8 reserved bytes at 24..32

        let metric_data_format = i16::from_be_bytes(data[32..34].try_into().unwrap());
        if metric_data_format != 0 {
            return Err(SanitizeError::TableParseError {
                tag: TAG,
                message: format!("Unsupported metricDataFormat: {metric_data_format}"),
            });
        }

        let number_of_h_metrics = u16::from_be_bytes(data[34..36].try_into().unwrap());
        if number_of_h_metrics == 0 {
            return Err(SanitizeError::TableParseError {
                tag: TAG,
                message: "No metrics (numberOfHMetrics is 0)".into(),
            });
        }
        if number_of_h_metrics > maxp.num_glyphs {
            return Err(SanitizeError::TableParseError {
                tag: TAG,
                message: format!(
                    "Bad numberOfHMetrics {number_of_h_metrics} > numGlyphs {}",
                    maxp.num_glyphs
                ),
            });
        }

        Ok(Self {
            ascent,
            descent,
            linegap,
            adv_width_max,
            min_left_side_bearing,
            min_right_side_bearing,
            x_max_extent,
            caret_slope_rise,
            caret_slope_run,
            caret_offset,
            number_of_h_metrics,
        })
    }

    pub fn serialize(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(HHEA_TABLE_LEN);
        bytes.extend_from_slice(&0x00010000u32.to_be_bytes());
        bytes.extend_from_slice(&self.ascent.to_be_bytes());
        bytes.extend_from_slice(&self.descent.to_be_bytes());
        bytes.extend_from_slice(&self.linegap.to_be_bytes());
        bytes.extend_from_slice(&self.adv_width_max.to_be_bytes());
        bytes.extend_from_slice(&self.min_left_side_bearing.to_be_bytes());
        bytes.extend_from_slice(&self.min_right_side_bearing.to_be_bytes());
        bytes.extend_from_slice(&self.x_max_extent.to_be_bytes());
        bytes.extend_from_slice(&self.caret_slope_rise.to_be_bytes());
        bytes.extend_from_slice(&self.caret_slope_run.to_be_bytes());
        bytes.extend_from_slice(&self.caret_offset.to_be_bytes());
        bytes.extend_from_slice(&[0u8; 8]); // reserved
        bytes.extend_from_slice(&0i16.to_be_bytes()); // metricDataFormat
        bytes.extend_from_slice(&self.number_of_h_metrics.to_be_bytes());
        bytes
    }
}
