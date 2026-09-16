//! Sanitizer for the `hmtx` (Horizontal Metrics) table.

use font_types::Tag;

use crate::context::SanitizeContext;
use crate::error::{MessageLevel, SanitizeError};
use crate::tables::hhea::HheaState;
use crate::tables::maxp::MaxpState;

pub const TAG: Tag = Tag::new(b"hmtx");

/// Sanitized `hmtx` table bytes.
pub struct HmtxState;

impl HmtxState {
    pub fn sanitize<'a>(
        data: &'a [u8],
        hhea: &HheaState,
        maxp: &MaxpState,
        context: &mut dyn SanitizeContext,
    ) -> Result<&'a [u8], SanitizeError> {
        let num_metrics = hhea.number_of_h_metrics as usize;
        let num_glyphs = maxp.num_glyphs as usize;

        if num_metrics > num_glyphs {
            return Err(SanitizeError::TableParseError {
                tag: TAG,
                message: format!("numberOfHMetrics {num_metrics} > numGlyphs {num_glyphs}"),
            });
        }

        let num_side_bearings = num_glyphs - num_metrics;
        let expected_len = num_metrics * 4 + num_side_bearings * 2;

        if data.len() < expected_len {
            return Err(SanitizeError::TableParseError {
                tag: TAG,
                message: format!(
                    "Table too short for hmtx (expected at least {expected_len} bytes, got {})",
                    data.len()
                ),
            });
        }

        if data.len() > expected_len {
            context.message(
                MessageLevel::Warning,
                &format!(
                    "hmtx: table length {} exceeds required {expected_len} bytes; truncating",
                    data.len()
                ),
            );
        }

        Ok(&data[..expected_len])
    }
}
