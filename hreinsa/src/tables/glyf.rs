//! Sanitizer for the `glyf` (Glyph Data) table.

use std::borrow::Cow;
use font_types::Tag;

use crate::context::SanitizeContext;
use crate::error::{MessageLevel, SanitizeError};
use crate::tables::loca::LocaState;
use crate::tables::maxp::MaxpState;

pub const TAG: Tag = Tag::new(b"glyf");

const X_SHORT_VECTOR: u8 = 1 << 1;
const Y_SHORT_VECTOR: u8 = 1 << 2;
const REPEAT_FLAG: u8 = 1 << 3;
const X_IS_SAME_OR_POSITIVE_X_SHORT_VECTOR: u8 = 1 << 4;
const Y_IS_SAME_OR_POSITIVE_Y_SHORT_VECTOR: u8 = 1 << 5;

// Composite glyph flags
const ARG_1_AND_2_ARE_WORDS: u16 = 0x0001;
const WE_HAVE_A_SCALE: u16 = 0x0008;
const MORE_COMPONENTS: u16 = 0x0020;
const WE_HAVE_AN_X_AND_Y_SCALE: u16 = 0x0040;
const WE_HAVE_A_TWO_BY_TWO: u16 = 0x0080;
const WE_HAVE_INSTRUCTIONS: u16 = 0x0100;

/// Sanitizes the `glyf` table against the `loca` offsets and updates `maxp` limits if necessary.
pub struct GlyfState;

impl GlyfState {
    pub fn sanitize<'a>(
        data: &'a [u8],
        loca: &LocaState,
        maxp: &mut MaxpState,
        context: &mut dyn SanitizeContext,
    ) -> Result<Cow<'a, [u8]>, SanitizeError> {
        let num_glyphs = maxp.num_glyphs as usize;
        if loca.offsets.len() < num_glyphs + 1 {
            return Err(SanitizeError::TableParseError {
                tag: TAG,
                message: "loca offsets count mismatch".into(),
            });
        }

        let total_glyf_len = loca.offsets[num_glyphs] as usize;
        if data.len() < total_glyf_len {
            return Err(SanitizeError::TableParseError {
                tag: TAG,
                message: format!(
                    "glyf table too short for loca: expected at least {total_glyf_len}, got {}",
                    data.len()
                ),
            });
        }

        let mut output_data: Option<Vec<u8>> = None;

        for gid in 0..num_glyphs {
            let start = loca.offsets[gid] as usize;
            let end = loca.offsets[gid + 1] as usize;
            if start == end {
                // Empty glyph
                continue;
            }

            if end > data.len() || end < start {
                return Err(SanitizeError::TableParseError {
                    tag: TAG,
                    message: format!("Glyph {gid} loca range [{start}, {end}] out of bounds"),
                });
            }

            let glyph_bytes = &data[start..end];
            if glyph_bytes.len() < 10 {
                return Err(SanitizeError::TableParseError {
                    tag: TAG,
                    message: format!("Glyph {gid} length {} is less than header (10)", glyph_bytes.len()),
                });
            }

            let num_contours = i16::from_be_bytes(glyph_bytes[0..2].try_into().unwrap());
            let mut x_min = i16::from_be_bytes(glyph_bytes[2..4].try_into().unwrap());
            let mut y_min = i16::from_be_bytes(glyph_bytes[4..6].try_into().unwrap());
            let mut x_max = i16::from_be_bytes(glyph_bytes[6..8].try_into().unwrap());
            let mut y_max = i16::from_be_bytes(glyph_bytes[8..10].try_into().unwrap());

            if num_contours > 0 {
                // Simple glyph
                let n_contours = num_contours as usize;
                if maxp.is_version_1 && (num_contours as u16) > maxp.max_contours {
                    context.message(
                        MessageLevel::Warning,
                        &format!(
                            "glyf: glyph {gid} contours {num_contours} exceeds maxp.maxContours {}, updating",
                            maxp.max_contours
                        ),
                    );
                    maxp.max_contours = num_contours as u16;
                }

                let end_pts_len = n_contours * 2;
                if glyph_bytes.len() < 10 + end_pts_len + 2 {
                    return Err(SanitizeError::TableParseError {
                        tag: TAG,
                        message: format!("Glyph {gid} too short for contour end points"),
                    });
                }

                let mut last_pt = 0u16;
                for c in 0..n_contours {
                    let pt_idx = u16::from_be_bytes(
                        glyph_bytes[10 + c * 2..10 + c * 2 + 2].try_into().unwrap(),
                    );
                    if c > 0 && pt_idx <= last_pt {
                        return Err(SanitizeError::TableParseError {
                            tag: TAG,
                            message: format!("Glyph {gid} non-monotonic contour endpoint"),
                        });
                    }
                    last_pt = pt_idx;
                }
                let num_points = (last_pt as usize) + 1;

                if maxp.is_version_1 && (num_points as u16) > maxp.max_points {
                    context.message(
                        MessageLevel::Warning,
                        &format!(
                            "glyf: glyph {gid} points {num_points} exceeds maxp.maxPoints {}, updating",
                            maxp.max_points
                        ),
                    );
                    maxp.max_points = num_points as u16;
                }

                let mut cursor = 10 + end_pts_len;
                let instruction_len =
                    u16::from_be_bytes(glyph_bytes[cursor..cursor + 2].try_into().unwrap()) as usize;
                cursor += 2;

                if maxp.is_version_1 && (instruction_len as u16) > maxp.max_size_of_instructions {
                    maxp.max_size_of_instructions = instruction_len as u16;
                }

                if cursor + instruction_len > glyph_bytes.len() {
                    return Err(SanitizeError::TableParseError {
                        tag: TAG,
                        message: format!("Glyph {gid} instructions overrun glyph data"),
                    });
                }
                cursor += instruction_len;

                // Parse flags
                let mut flags = Vec::with_capacity(num_points);
                while flags.len() < num_points {
                    if cursor >= glyph_bytes.len() {
                        return Err(SanitizeError::TableParseError {
                            tag: TAG,
                            message: format!("Glyph {gid} flags overrun glyph data"),
                        });
                    }
                    let flag = glyph_bytes[cursor];
                    cursor += 1;

                    if flag & (1 << 7) != 0 {
                        return Err(SanitizeError::TableParseError {
                            tag: TAG,
                            message: format!("Glyph {gid} has reserved flag bit 7 set"),
                        });
                    }

                    flags.push(flag);

                    if flag & REPEAT_FLAG != 0 {
                        if cursor >= glyph_bytes.len() {
                            return Err(SanitizeError::TableParseError {
                                tag: TAG,
                                message: format!("Glyph {gid} repeat count truncated"),
                            });
                        }
                        let repeat = glyph_bytes[cursor] as usize;
                        cursor += 1;
                        if flags.len() + repeat > num_points {
                            return Err(SanitizeError::TableParseError {
                                tag: TAG,
                                message: format!("Glyph {gid} repeat count {repeat} exceeds point count"),
                            });
                        }
                        for _ in 0..repeat {
                            flags.push(flag);
                        }
                    }
                }

                // Parse x coordinates
                let mut cur_x: i16 = 0;
                let mut real_x_min = i16::MAX;
                let mut real_x_max = i16::MIN;

                for &flag in &flags {
                    if flag & X_SHORT_VECTOR != 0 {
                        if cursor >= glyph_bytes.len() {
                            return Err(SanitizeError::TableParseError {
                                tag: TAG,
                                message: format!("Glyph {gid} coordinates overrun"),
                            });
                        }
                        let dx = glyph_bytes[cursor] as i16;
                        cursor += 1;
                        if flag & X_IS_SAME_OR_POSITIVE_X_SHORT_VECTOR != 0 {
                            cur_x = cur_x.wrapping_add(dx);
                        } else {
                            cur_x = cur_x.wrapping_sub(dx);
                        }
                    } else if flag & X_IS_SAME_OR_POSITIVE_X_SHORT_VECTOR != 0 {
                        // x unchanged
                    } else {
                        if cursor + 2 > glyph_bytes.len() {
                            return Err(SanitizeError::TableParseError {
                                tag: TAG,
                                message: format!("Glyph {gid} coordinates overrun"),
                            });
                        }
                        let dx = i16::from_be_bytes(glyph_bytes[cursor..cursor + 2].try_into().unwrap());
                        cursor += 2;
                        cur_x = cur_x.wrapping_add(dx);
                    }
                    real_x_min = real_x_min.min(cur_x);
                    real_x_max = real_x_max.max(cur_x);
                }

                // Parse y coordinates
                let mut cur_y: i16 = 0;
                let mut real_y_min = i16::MAX;
                let mut real_y_max = i16::MIN;

                for &flag in &flags {
                    if flag & Y_SHORT_VECTOR != 0 {
                        if cursor >= glyph_bytes.len() {
                            return Err(SanitizeError::TableParseError {
                                tag: TAG,
                                message: format!("Glyph {gid} coordinates overrun"),
                            });
                        }
                        let dy = glyph_bytes[cursor] as i16;
                        cursor += 1;
                        if flag & Y_IS_SAME_OR_POSITIVE_Y_SHORT_VECTOR != 0 {
                            cur_y = cur_y.wrapping_add(dy);
                        } else {
                            cur_y = cur_y.wrapping_sub(dy);
                        }
                    } else if flag & Y_IS_SAME_OR_POSITIVE_Y_SHORT_VECTOR != 0 {
                        // y unchanged
                    } else {
                        if cursor + 2 > glyph_bytes.len() {
                            return Err(SanitizeError::TableParseError {
                                tag: TAG,
                                message: format!("Glyph {gid} coordinates overrun"),
                            });
                        }
                        let dy = i16::from_be_bytes(glyph_bytes[cursor..cursor + 2].try_into().unwrap());
                        cursor += 2;
                        cur_y = cur_y.wrapping_add(dy);
                    }
                    real_y_min = real_y_min.min(cur_y);
                    real_y_max = real_y_max.max(cur_y);
                }

                // Check bbox and adjust if needed
                let mut modified_bbox = false;
                if real_x_min < x_min {
                    x_min = real_x_min;
                    modified_bbox = true;
                }
                if real_x_max > x_max {
                    x_max = real_x_max;
                    modified_bbox = true;
                }
                if real_y_min < y_min {
                    y_min = real_y_min;
                    modified_bbox = true;
                }
                if real_y_max > y_max {
                    y_max = real_y_max;
                    modified_bbox = true;
                }

                if modified_bbox {
                    if output_data.is_none() {
                        output_data = Some(data[..total_glyf_len].to_vec());
                    }
                    if let Some(ref mut out) = output_data {
                        out[start + 2..start + 4].copy_from_slice(&x_min.to_be_bytes());
                        out[start + 4..start + 6].copy_from_slice(&y_min.to_be_bytes());
                        out[start + 6..start + 8].copy_from_slice(&x_max.to_be_bytes());
                        out[start + 8..start + 10].copy_from_slice(&y_max.to_be_bytes());
                    }
                }
            } else if num_contours == -1 {
                // Composite glyph
                let mut cursor = 10;
                let mut component_count = 0u16;
                let mut has_instructions = false;

                loop {
                    if cursor + 4 > glyph_bytes.len() {
                        return Err(SanitizeError::TableParseError {
                            tag: TAG,
                            message: format!("Glyph {gid} truncated composite component"),
                        });
                    }
                    let comp_flags =
                        u16::from_be_bytes(glyph_bytes[cursor..cursor + 2].try_into().unwrap());
                    let comp_gid =
                        u16::from_be_bytes(glyph_bytes[cursor + 2..cursor + 4].try_into().unwrap());
                    cursor += 4;
                    component_count += 1;

                    if comp_flags & WE_HAVE_INSTRUCTIONS != 0 {
                        has_instructions = true;
                    }

                    if comp_gid >= maxp.num_glyphs {
                        return Err(SanitizeError::TableParseError {
                            tag: TAG,
                            message: format!(
                                "Glyph {gid} composite component {comp_gid} >= numGlyphs {}",
                                maxp.num_glyphs
                            ),
                        });
                    }

                    let mut skip = if comp_flags & ARG_1_AND_2_ARE_WORDS != 0 {
                        4
                    } else {
                        2
                    };
                    if comp_flags & WE_HAVE_A_SCALE != 0 {
                        skip += 2;
                    } else if comp_flags & WE_HAVE_AN_X_AND_Y_SCALE != 0 {
                        skip += 4;
                    } else if comp_flags & WE_HAVE_A_TWO_BY_TWO != 0 {
                        skip += 8;
                    }

                    if cursor + skip > glyph_bytes.len() {
                        return Err(SanitizeError::TableParseError {
                            tag: TAG,
                            message: format!("Glyph {gid} composite component transform overrun"),
                        });
                    }
                    cursor += skip;

                    if comp_flags & MORE_COMPONENTS == 0 {
                        break;
                    }
                }

                if maxp.is_version_1 && component_count > maxp.max_component_elements {
                    maxp.max_component_elements = component_count;
                }

                // Check for composite instructions if WE_HAVE_INSTRUCTIONS was set
                if has_instructions {
                    if cursor + 2 > glyph_bytes.len() {
                        return Err(SanitizeError::TableParseError {
                            tag: TAG,
                            message: format!("Glyph {gid} truncated composite instruction length"),
                        });
                    }
                    let instr_len =
                        u16::from_be_bytes(glyph_bytes[cursor..cursor + 2].try_into().unwrap()) as usize;
                    cursor += 2;
                    if cursor + instr_len > glyph_bytes.len() {
                        return Err(SanitizeError::TableParseError {
                            tag: TAG,
                            message: format!("Glyph {gid} composite instructions overrun"),
                        });
                    }
                    if maxp.is_version_1 && (instr_len as u16) > maxp.max_size_of_instructions {
                        maxp.max_size_of_instructions = instr_len as u16;
                    }
                }
            } else {
                return Err(SanitizeError::TableParseError {
                    tag: TAG,
                    message: format!("Glyph {gid} invalid numberOfContours {num_contours}"),
                });
            }
        }

        match output_data {
            Some(vec) => Ok(Cow::Owned(vec)),
            None => Ok(Cow::Borrowed(&data[..total_glyf_len])),
        }
    }
}
