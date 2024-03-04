//! Miscellaneous instructions.
//!
//! Implements 3 instructions.
//!
//! See <https://learn.microsoft.com/en-us/typography/opentype/spec/tt_instructions#miscellaneous-instructions>

use super::{Engine, OpResult};

impl<'a> Engine<'a> {
    /// Get information.
    ///
    /// GETINFO[] (0x88)
    ///
    /// Pops: selector: integer
    /// Pushes: result: integer
    ///
    /// GETINFO is used to obtain data about the font scaler version and the
    /// characteristics of the current glyph. The instruction pops a selector
    /// used to determine the type of information desired and pushes a result
    /// onto the stack.    
    ///
    /// See <https://learn.microsoft.com/en-us/typography/opentype/spec/tt_instructions#get-information>
    /// and <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/truetype/ttinterp.c#L6689>
    pub(super) fn op_getinfo(&mut self) -> OpResult {
        let selector = self.value_stack.pop()?;
        let mut result = 0;
        // Interpreter version (selector bit: 0, result bits: 0-7)
        const VERSION_SELECTOR_BIT: i32 = 1 << 0;
        if (selector & VERSION_SELECTOR_BIT) != 0 {
            result = 42;
        }
        // Glyph rotated (selector bit: 1, result bit: 8)
        const GLYPH_ROTATED_SELECTOR_BIT: i32 = 1 << 1;
        if (selector & GLYPH_ROTATED_SELECTOR_BIT) != 0 && self.graphics_state.is_rotated {
            const GLYPH_ROTATED_RESULT_BIT: i32 = 1 << 8;
            result |= GLYPH_ROTATED_RESULT_BIT;
        }
        // Glyph stretched (selector bit: 2, result bit: 9)
        const GLYPH_STRETCHED_SELECTOR_BIT: i32 = 1 << 2;
        if (selector & GLYPH_STRETCHED_SELECTOR_BIT) != 0 && self.graphics_state.is_stretched {
            const GLYPH_STRETCHED_RESULT_BIT: i32 = 1 << 9;
            result |= GLYPH_STRETCHED_RESULT_BIT;
        }
        // Font variations (selector bit: 3, result bit: 10)
        const FONT_VARIATIONS_SELECTOR_BIT: i32 = 1 << 3;
        if (selector & FONT_VARIATIONS_SELECTOR_BIT) != 0 && self.axis_count != 0 {
            const FONT_VARIATIONS_RESULT_BIT: i32 = 1 << 10;
            result |= FONT_VARIATIONS_RESULT_BIT;
        }
        // The following only apply for smooth hinting.
        if self.graphics_state.mode.is_smooth() {
            // Subpixel hinting [cleartype enabled] (selector bit: 6, result bit: 13)
            // (always enabled)
            const SUBPIXEL_HINTING_SELECTOR_BIT: i32 = 1 << 6;
            if (selector & SUBPIXEL_HINTING_SELECTOR_BIT) != 0 {
                const SUBPIXEL_HINTING_RESULT_BIT: i32 = 1 << 13;
                result |= SUBPIXEL_HINTING_RESULT_BIT;
            }
            // Vertical LCD subpixels? (selector bit: 8, result bit: 15)
            const VERTICAL_LCD_SELECTOR_BIT: i32 = 1 << 8;
            if (selector & VERSION_SELECTOR_BIT) != 0 && self.graphics_state.mode.is_vertical_lcd()
            {
                const VERTICAL_LCD_RESULT_BIT: i32 = 1 << 15;
                result |= VERTICAL_LCD_RESULT_BIT;
            }
            // Subpixel positioned? (selector bit: 10, result bit: 17)
            // (always enabled)
            const SUBPIXEL_POSITIONED_SELECTOR_BIT: i32 = 1 << 10;
            if (selector & SUBPIXEL_POSITIONED_SELECTOR_BIT) != 0 {
                const SUBPIXEL_POSITIONED_RESULT_BIT: i32 = 1 << 17;
                result |= SUBPIXEL_POSITIONED_RESULT_BIT;
            }
            // Symmetrical smoothing (selector bit: 11, result bit: 18)
            // Note: FreeType always enables this but we deviate when our own
            // preserve linear metrics flag is enabled.
            const SYMMETRICAL_SMOOTHING_SELECTOR_BIT: i32 = 1 << 11;
            if (selector & SYMMETRICAL_SMOOTHING_SELECTOR_BIT) != 0
                && !self.graphics_state.mode.preserve_linear_metrics()
            {
                const SYMMETRICAL_SMOOTHING_RESULT_BIT: i32 = 1 << 18;
                result |= SYMMETRICAL_SMOOTHING_RESULT_BIT;
            }
            // ClearType hinting and grayscale rendering (selector bit: 12, result bit: 19)
            const GRAYSCALE_CLEARTYPE_SELECTOR_BIT: i32 = 1 << 12;
            if (selector & GRAYSCALE_CLEARTYPE_SELECTOR_BIT) != 0
                && self.graphics_state.mode.is_grayscale_cleartype()
            {
                const GRAYSCALE_CLEARTYPE_RESULT_BIT: i32 = 1 << 19;
                result |= GRAYSCALE_CLEARTYPE_RESULT_BIT;
            }
        }
        self.value_stack.push(result)
    }

    /// Get variation.
    ///
    /// GETVARIATION[] (0x91)
    ///
    /// Pushes: Normalized axes coordinates, one for each axis in the font.
    ///
    /// GETVARIATION is used to obtain the current normalized variation
    /// coordinates for each axis. The coordinate for the first axis, as
    /// defined in the 'fvar' table, is pushed first on the stack, followed
    /// by each consecutive axis until the coordinate for the last axis is
    /// on the stack.   
    ///
    /// See <https://learn.microsoft.com/en-us/typography/opentype/spec/tt_instructions#get-variation>
    /// and <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/truetype/ttinterp.c#L6813>
    pub(super) fn op_getvariation(&mut self) -> OpResult {
        // For non-variable fonts, this falls back to IDEF resolution.
        let axis_count = self.axis_count as usize;
        if axis_count != 0 {
            // Make sure we push `axis_count` coords regardless of the value
            // provided by the user.
            for coord in self
                .coords
                .iter()
                .copied()
                .chain(std::iter::repeat(Default::default()))
                .take(axis_count)
            {
                self.value_stack.push(coord.to_bits() as i32)?;
            }
            Ok(())
        } else {
            self.op_unknown(0x91)
        }
    }

    /// Get data.
    ///
    /// GETDATA[] (0x92)
    ///
    /// Pushes: 17
    ///
    /// Undocumented and nobody knows what this does. FreeType just
    /// returns 17 for variable fonts and falls back to IDEF lookup
    /// otherwise.
    ///
    /// See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/truetype/ttinterp.c#L6851>    
    pub(super) fn op_getdata(&mut self) -> OpResult {
        if self.axis_count != 0 {
            self.value_stack.push(17)
        } else {
            self.op_unknown(0x92)
        }
    }
}
