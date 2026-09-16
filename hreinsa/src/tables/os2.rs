//! Sanitizer for the `OS/2` (OS/2 and Windows Metrics) table.

use font_types::Tag;

use crate::context::SanitizeContext;
use crate::error::{MessageLevel, SanitizeError};
use crate::tables::head::HeadState;

pub const TAG: Tag = Tag::new(b"OS/2");

/// Sanitized state of the `OS/2` table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Os2State {
    pub version: u16,
    pub x_avg_char_width: i16,
    pub us_weight_class: u16,
    pub us_width_class: u16,
    pub fs_type: u16,
    pub y_subscript_x_size: i16,
    pub y_subscript_y_size: i16,
    pub y_subscript_x_offset: i16,
    pub y_subscript_y_offset: i16,
    pub y_superscript_x_size: i16,
    pub y_superscript_y_size: i16,
    pub y_superscript_x_offset: i16,
    pub y_superscript_y_offset: i16,
    pub y_strikeout_size: i16,
    pub y_strikeout_position: i16,
    pub s_family_class: i16,
    pub panose: [u8; 10],
    pub ul_unicode_range_1: u32,
    pub ul_unicode_range_2: u32,
    pub ul_unicode_range_3: u32,
    pub ul_unicode_range_4: u32,
    pub ach_vend_id: [u8; 4],
    pub fs_selection: u16,
    pub us_first_char_index: u16,
    pub us_last_char_index: u16,
    pub s_typo_ascender: i16,
    pub s_typo_descender: i16,
    pub s_typo_line_gap: i16,
    pub us_win_ascent: u16,
    pub us_win_descent: u16,

    // Version >= 1
    pub ul_code_page_range_1: u32,
    pub ul_code_page_range_2: u32,

    // Version >= 2
    pub sx_height: i16,
    pub s_cap_height: i16,
    pub us_default_char: u16,
    pub us_break_char: u16,
    pub us_max_context: u16,

    // Version >= 5
    pub us_lower_optical_point_size: u16,
    pub us_upper_optical_point_size: u16,
}

impl Os2State {
    pub fn parse(
        data: &[u8],
        head: &mut HeadState,
        context: &mut dyn SanitizeContext,
    ) -> Result<Self, SanitizeError> {
        if data.len() < 78 {
            return Err(SanitizeError::TableParseError {
                tag: TAG,
                message: "Table too short for basic OS/2 elements".into(),
            });
        }

        let mut version = u16::from_be_bytes(data[0..2].try_into().unwrap());
        if version > 5 {
            return Err(SanitizeError::TableParseError {
                tag: TAG,
                message: format!("Unsupported OS/2 table version: {version}"),
            });
        }

        let x_avg_char_width = i16::from_be_bytes(data[2..4].try_into().unwrap());

        let raw_weight = u16::from_be_bytes(data[4..6].try_into().unwrap());
        let us_weight_class = if raw_weight < 1 {
            context.message(
                MessageLevel::Warning,
                &format!("OS/2: bad usWeightClass {raw_weight}, clamping to 1"),
            );
            1
        } else if raw_weight > 1000 {
            context.message(
                MessageLevel::Warning,
                &format!("OS/2: bad usWeightClass {raw_weight}, clamping to 1000"),
            );
            1000
        } else {
            raw_weight
        };

        let raw_width = u16::from_be_bytes(data[6..8].try_into().unwrap());
        let us_width_class = if raw_width < 1 {
            context.message(
                MessageLevel::Warning,
                &format!("OS/2: bad usWidthClass {raw_width}, clamping to 1"),
            );
            1
        } else if raw_width > 9 {
            context.message(
                MessageLevel::Warning,
                &format!("OS/2: bad usWidthClass {raw_width}, clamping to 9"),
            );
            9
        } else {
            raw_width
        };

        let mut fs_type = u16::from_be_bytes(data[8..10].try_into().unwrap());
        // Lowest 3 bits (bits 1, 2, 3) are mutually exclusive
        if fs_type & 0x2 != 0 {
            fs_type &= 0xFFF3;
        } else if fs_type & 0x4 != 0 {
            fs_type &= 0xFFF4;
        } else if fs_type & 0x8 != 0 {
            fs_type &= 0xFFF9;
        }
        // Mask reserved bits (allow 0..3, 8, 9)
        fs_type &= 0x030F;

        let clamp_zero = |val: i16, name: &str, ctx: &mut dyn SanitizeContext| -> i16 {
            if val < 0 {
                ctx.message(
                    MessageLevel::Warning,
                    &format!("OS/2: bad {name} {val}, setting to zero"),
                );
                0
            } else {
                val
            }
        };

        let y_subscript_x_size = clamp_zero(
            i16::from_be_bytes(data[10..12].try_into().unwrap()),
            "ySubscriptXSize",
            context,
        );
        let y_subscript_y_size = clamp_zero(
            i16::from_be_bytes(data[12..14].try_into().unwrap()),
            "ySubscriptYSize",
            context,
        );
        let y_subscript_x_offset = i16::from_be_bytes(data[14..16].try_into().unwrap());
        let y_subscript_y_offset = i16::from_be_bytes(data[16..18].try_into().unwrap());

        let y_superscript_x_size = clamp_zero(
            i16::from_be_bytes(data[18..20].try_into().unwrap()),
            "ySuperscriptXSize",
            context,
        );
        let y_superscript_y_size = clamp_zero(
            i16::from_be_bytes(data[20..22].try_into().unwrap()),
            "ySuperscriptYSize",
            context,
        );
        let y_superscript_x_offset = i16::from_be_bytes(data[22..24].try_into().unwrap());
        let y_superscript_y_offset = i16::from_be_bytes(data[24..26].try_into().unwrap());

        let y_strikeout_size = clamp_zero(
            i16::from_be_bytes(data[26..28].try_into().unwrap()),
            "yStrikeoutSize",
            context,
        );
        let y_strikeout_position = i16::from_be_bytes(data[28..30].try_into().unwrap());
        let s_family_class = i16::from_be_bytes(data[30..32].try_into().unwrap());

        let mut panose = [0u8; 10];
        panose.copy_from_slice(&data[32..42]);

        let ul_unicode_range_1 = u32::from_be_bytes(data[42..46].try_into().unwrap());
        let ul_unicode_range_2 = u32::from_be_bytes(data[46..50].try_into().unwrap());
        let ul_unicode_range_3 = u32::from_be_bytes(data[50..54].try_into().unwrap());
        let ul_unicode_range_4 = u32::from_be_bytes(data[54..58].try_into().unwrap());

        let mut ach_vend_id = [0u8; 4];
        ach_vend_id.copy_from_slice(&data[58..62]);

        let mut fs_selection = u16::from_be_bytes(data[62..64].try_into().unwrap());

        // If bit 6 (REGULAR) is set, clear bit 0 (ITALIC) and bit 5 (BOLD)
        if fs_selection & 0x40 != 0 {
            fs_selection &= 0xFFDE;
        }

        // Cross-table synchronization with head.mac_style
        if (fs_selection & 0x1 != 0) && (head.mac_style & 0x2 == 0) {
            context.message(
                MessageLevel::Warning,
                "OS/2: adjusting head.macStyle (italic) to match fsSelection",
            );
            head.mac_style |= 0x2;
        }
        if (fs_selection & 0x2 != 0) && (head.mac_style & 0x4 == 0) {
            context.message(
                MessageLevel::Warning,
                "OS/2: adjusting head.macStyle (underscore) to match fsSelection",
            );
            head.mac_style |= 0x4;
        }
        if (fs_selection & 0x40 != 0) && (head.mac_style & 0x3 != 0) {
            context.message(
                MessageLevel::Warning,
                "OS/2: adjusting head.macStyle (regular) to match fsSelection",
            );
            head.mac_style &= 0xFFFC;
        }

        if version < 4 && (fs_selection & 0x300 != 0) {
            context.message(
                MessageLevel::Warning,
                &format!("OS/2: fsSelection bits 8 and 9 must be unset for table version {version}"),
            );
        }
        fs_selection &= 0x03FF;

        let mut us_first_char_index = u16::from_be_bytes(data[64..66].try_into().unwrap());
        let us_last_char_index = u16::from_be_bytes(data[66..68].try_into().unwrap());
        if us_first_char_index > us_last_char_index {
            context.message(
                MessageLevel::Warning,
                &format!("OS/2: usFirstCharIndex {us_first_char_index} > usLastCharIndex {us_last_char_index}, syncing"),
            );
            us_first_char_index = us_last_char_index;
        }

        let s_typo_ascender = i16::from_be_bytes(data[68..70].try_into().unwrap());
        let s_typo_descender = i16::from_be_bytes(data[70..72].try_into().unwrap());
        let raw_linegap = i16::from_be_bytes(data[72..74].try_into().unwrap());
        let s_typo_line_gap = if raw_linegap < 0 {
            context.message(
                MessageLevel::Warning,
                &format!("OS/2: bad sTypoLineGap {raw_linegap}, setting to zero"),
            );
            0
        } else {
            raw_linegap
        };

        let us_win_ascent = u16::from_be_bytes(data[74..76].try_into().unwrap());
        let us_win_descent = u16::from_be_bytes(data[76..78].try_into().unwrap());

        let mut ul_code_page_range_1 = 0u32;
        let mut ul_code_page_range_2 = 0u32;
        if version >= 1 {
            if data.len() < 86 {
                context.message(
                    MessageLevel::Warning,
                    "OS/2: table too short for version 1, downgrading to version 0",
                );
                version = 0;
            } else {
                ul_code_page_range_1 = u32::from_be_bytes(data[78..82].try_into().unwrap());
                ul_code_page_range_2 = u32::from_be_bytes(data[82..86].try_into().unwrap());
            }
        }

        let mut sx_height = 0i16;
        let mut s_cap_height = 0i16;
        let mut us_default_char = 0u16;
        let mut us_break_char = 0u16;
        let mut us_max_context = 0u16;
        if version >= 2 {
            if data.len() < 96 {
                context.message(
                    MessageLevel::Warning,
                    "OS/2: table too short for version 2, downgrading to version 1",
                );
                version = 1;
            } else {
                sx_height = i16::from_be_bytes(data[86..88].try_into().unwrap());
                s_cap_height = i16::from_be_bytes(data[88..90].try_into().unwrap());
                us_default_char = u16::from_be_bytes(data[90..92].try_into().unwrap());
                us_break_char = u16::from_be_bytes(data[92..94].try_into().unwrap());
                us_max_context = u16::from_be_bytes(data[94..96].try_into().unwrap());
            }
        }

        let mut us_lower_optical_point_size = 0u16;
        let mut us_upper_optical_point_size = 0u16;
        if version >= 5 {
            if data.len() < 100 {
                context.message(
                    MessageLevel::Warning,
                    "OS/2: table too short for version 5, downgrading to version 2",
                );
                version = 2;
            } else {
                us_lower_optical_point_size =
                    u16::from_be_bytes(data[96..98].try_into().unwrap());
                us_upper_optical_point_size =
                    u16::from_be_bytes(data[98..100].try_into().unwrap());
            }
        }

        Ok(Self {
            version,
            x_avg_char_width,
            us_weight_class,
            us_width_class,
            fs_type,
            y_subscript_x_size,
            y_subscript_y_size,
            y_subscript_x_offset,
            y_subscript_y_offset,
            y_superscript_x_size,
            y_superscript_y_size,
            y_superscript_x_offset,
            y_superscript_y_offset,
            y_strikeout_size,
            y_strikeout_position,
            s_family_class,
            panose,
            ul_unicode_range_1,
            ul_unicode_range_2,
            ul_unicode_range_3,
            ul_unicode_range_4,
            ach_vend_id,
            fs_selection,
            us_first_char_index,
            us_last_char_index,
            s_typo_ascender,
            s_typo_descender,
            s_typo_line_gap,
            us_win_ascent,
            us_win_descent,
            ul_code_page_range_1,
            ul_code_page_range_2,
            sx_height,
            s_cap_height,
            us_default_char,
            us_break_char,
            us_max_context,
            us_lower_optical_point_size,
            us_upper_optical_point_size,
        })
    }

    pub fn serialize(&self) -> Vec<u8> {
        let size = match self.version {
            0 => 78,
            1 => 86,
            2..=4 => 96,
            _ => 100,
        };
        let mut bytes = Vec::with_capacity(size);
        bytes.extend_from_slice(&self.version.to_be_bytes());
        bytes.extend_from_slice(&self.x_avg_char_width.to_be_bytes());
        bytes.extend_from_slice(&self.us_weight_class.to_be_bytes());
        bytes.extend_from_slice(&self.us_width_class.to_be_bytes());
        bytes.extend_from_slice(&self.fs_type.to_be_bytes());
        bytes.extend_from_slice(&self.y_subscript_x_size.to_be_bytes());
        bytes.extend_from_slice(&self.y_subscript_y_size.to_be_bytes());
        bytes.extend_from_slice(&self.y_subscript_x_offset.to_be_bytes());
        bytes.extend_from_slice(&self.y_subscript_y_offset.to_be_bytes());
        bytes.extend_from_slice(&self.y_superscript_x_size.to_be_bytes());
        bytes.extend_from_slice(&self.y_superscript_y_size.to_be_bytes());
        bytes.extend_from_slice(&self.y_superscript_x_offset.to_be_bytes());
        bytes.extend_from_slice(&self.y_superscript_y_offset.to_be_bytes());
        bytes.extend_from_slice(&self.y_strikeout_size.to_be_bytes());
        bytes.extend_from_slice(&self.y_strikeout_position.to_be_bytes());
        bytes.extend_from_slice(&self.s_family_class.to_be_bytes());
        bytes.extend_from_slice(&self.panose);
        bytes.extend_from_slice(&self.ul_unicode_range_1.to_be_bytes());
        bytes.extend_from_slice(&self.ul_unicode_range_2.to_be_bytes());
        bytes.extend_from_slice(&self.ul_unicode_range_3.to_be_bytes());
        bytes.extend_from_slice(&self.ul_unicode_range_4.to_be_bytes());
        bytes.extend_from_slice(&self.ach_vend_id);
        bytes.extend_from_slice(&self.fs_selection.to_be_bytes());
        bytes.extend_from_slice(&self.us_first_char_index.to_be_bytes());
        bytes.extend_from_slice(&self.us_last_char_index.to_be_bytes());
        bytes.extend_from_slice(&self.s_typo_ascender.to_be_bytes());
        bytes.extend_from_slice(&self.s_typo_descender.to_be_bytes());
        bytes.extend_from_slice(&self.s_typo_line_gap.to_be_bytes());
        bytes.extend_from_slice(&self.us_win_ascent.to_be_bytes());
        bytes.extend_from_slice(&self.us_win_descent.to_be_bytes());

        if self.version >= 1 {
            bytes.extend_from_slice(&self.ul_code_page_range_1.to_be_bytes());
            bytes.extend_from_slice(&self.ul_code_page_range_2.to_be_bytes());
        }

        if self.version >= 2 {
            bytes.extend_from_slice(&self.sx_height.to_be_bytes());
            bytes.extend_from_slice(&self.s_cap_height.to_be_bytes());
            bytes.extend_from_slice(&self.us_default_char.to_be_bytes());
            bytes.extend_from_slice(&self.us_break_char.to_be_bytes());
            bytes.extend_from_slice(&self.us_max_context.to_be_bytes());
        }

        if self.version >= 5 {
            bytes.extend_from_slice(&self.us_lower_optical_point_size.to_be_bytes());
            bytes.extend_from_slice(&self.us_upper_optical_point_size.to_be_bytes());
        }

        bytes
    }
}
