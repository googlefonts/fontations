//! Sanitizer for the `cmap` (Character to Glyph Index Mapping) table.

use font_types::Tag;
use read_fonts::tables::cmap::{Cmap, CmapSubtable};
use read_fonts::{FontData, FontRead};

use crate::context::SanitizeContext;
use crate::error::SanitizeError;
use crate::tables::maxp::MaxpState;

pub const TAG: Tag = Tag::new(b"cmap");

/// Sanitizer for the `cmap` table.
pub struct CmapState;

impl CmapState {
    pub fn sanitize<'a>(
        data: &'a [u8],
        maxp: &MaxpState,
        _context: &mut dyn SanitizeContext,
    ) -> Result<&'a [u8], SanitizeError> {
        if data.len() < 4 {
            return Err(SanitizeError::TableParseError {
                tag: TAG,
                message: "Table too short for cmap header".into(),
            });
        }

        let version = u16::from_be_bytes(data[0..2].try_into().unwrap());
        if version != 0 {
            return Err(SanitizeError::TableParseError {
                tag: TAG,
                message: format!("Unsupported cmap version: {version}"),
            });
        }

        let num_tables = u16::from_be_bytes(data[2..4].try_into().unwrap());
        if num_tables == 0 {
            return Err(SanitizeError::TableParseError {
                tag: TAG,
                message: "cmap has 0 encoding records".into(),
            });
        }

        let cmap = Cmap::read(FontData::new(data)).map_err(|e| SanitizeError::TableParseError {
            tag: TAG,
            message: format!("Failed to parse cmap table: {e:?}"),
        })?;

        let num_glyphs = maxp.num_glyphs as u32;

        for record in cmap.encoding_records() {
            let subtable = match record.subtable(cmap.offset_data()) {
                Ok(s) => s,
                Err(e) => {
                    return Err(SanitizeError::TableParseError {
                        tag: TAG,
                        message: format!("Failed to read cmap subtable: {e:?}"),
                    });
                }
            };

            match subtable {
                CmapSubtable::Format0(f0) => {
                    for &gid in f0.glyph_id_array() {
                        if gid as u32 >= num_glyphs {
                            return Err(SanitizeError::TableParseError {
                                tag: TAG,
                                message: format!(
                                    "Format 0 subtable contains out of bounds glyph ID {gid} >= {num_glyphs}"
                                ),
                            });
                        }
                    }
                }
                CmapSubtable::Format4(f4) => {
                    let seg_count = f4.seg_count_x2() as usize / 2;
                    if seg_count == 0 {
                        return Err(SanitizeError::TableParseError {
                            tag: TAG,
                            message: "Format 4 subtable segCount is 0".into(),
                        });
                    }

                    let end_codes = f4.end_code();
                    let start_codes = f4.start_code();
                    if end_codes.len() != seg_count || start_codes.len() != seg_count {
                        return Err(SanitizeError::TableParseError {
                            tag: TAG,
                            message: "Format 4 segment arrays mismatch segCount".into(),
                        });
                    }

                    if end_codes[seg_count - 1].get() != 0xFFFF
                        || start_codes[seg_count - 1].get() != 0xFFFF
                    {
                        return Err(SanitizeError::TableParseError {
                            tag: TAG,
                            message: "Format 4 subtable final segment must be 0xFFFF".into(),
                        });
                    }

                    for i in 0..seg_count {
                        let start = start_codes[i].get();
                        let end = end_codes[i].get();
                        if start > end {
                            _context.message(
                                crate::error::MessageLevel::Warning,
                                &format!("cmap: Format 4 startCode {start} > endCode {end} at segment {i}"),
                            );
                        }
                        if i > 0 && start <= end_codes[i - 1].get() {
                            return Err(SanitizeError::TableParseError {
                                tag: TAG,
                                message: format!(
                                    "Format 4 segments out of order or overlapping at segment {i}"
                                ),
                            });
                        }
                    }
                }
                CmapSubtable::Format6(f6) => {
                    for gid in f6.glyph_id_array() {
                        let gid = gid.get();
                        if gid as u32 >= num_glyphs {
                            return Err(SanitizeError::TableParseError {
                                tag: TAG,
                                message: format!(
                                    "Format 6 subtable contains out of bounds glyph ID {gid} >= {num_glyphs}"
                                ),
                            });
                        }
                    }
                }
                CmapSubtable::Format12(f12) => {
                    let mut prev_end: Option<u32> = None;
                    for group in f12.groups() {
                        let start = group.start_char_code();
                        let end = group.end_char_code();
                        let start_gid = group.start_glyph_id();

                        if start > end || end > 0x0010_FFFF {
                            return Err(SanitizeError::TableParseError {
                                tag: TAG,
                                message: format!(
                                    "Format 12 group has invalid code range [{start:#x}, {end:#x}]"
                                ),
                            });
                        }

                        if let Some(prev) = prev_end {
                            if start <= prev {
                                return Err(SanitizeError::TableParseError {
                                    tag: TAG,
                                    message: format!(
                                        "Format 12 groups not ascending ({start:#x} <= {prev:#x})"
                                    ),
                                });
                            }
                        }
                        prev_end = Some(end);

                        let count = end - start + 1;
                        if start_gid.saturating_add(count) > num_glyphs {
                            return Err(SanitizeError::TableParseError {
                                tag: TAG,
                                message: format!(
                                    "Format 12 group maps to glyph ID >= {num_glyphs}"
                                ),
                            });
                        }
                    }
                }
                _ => {}
            }
        }

        Ok(data)
    }
}
