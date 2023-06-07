//! Parsing for PostScript charstrings.

use super::{BlendState, Error, Index, Stack};
use crate::types::{Fixed, Pen};

/// Maximum nesting depth for subroutine calls.
///
/// See "Appendix B Type 2 Charstring Implementation Limits" in
/// <https://adobe-type-tools.github.io/font-tech-notes/pdfs/5177.Type2.pdf>
pub const NESTING_DEPTH_LIMIT: u32 = 10;

/// Trait for processing commands resulting from charstring evaluation.
#[allow(unused_variables)]
pub trait CommandSink {
    /// Horizontal stem hint at `y` with height `dy`.
    fn hstem(&mut self, y: Fixed, dy: Fixed) {}
    /// Vertical stem hint at `x` with width `dx`.
    fn vstem(&mut self, x: Fixed, dx: Fixed) {}
    /// Bitmask defining the hints that should be made active for the
    /// commands that follow.
    fn hint_mask(&mut self, mask: &[u8]) {}
    /// Bitmask defining the counter hints that should be made active for the
    /// commands that follow.
    fn counter_mask(&mut self, mask: &[u8]) {}
    fn move_to(&mut self, x: Fixed, y: Fixed);
    fn line_to(&mut self, x: Fixed, y: Fixed);
    fn curve_to(&mut self, cx0: Fixed, cy0: Fixed, cx1: Fixed, cy1: Fixed, x: Fixed, y: Fixed);
    fn close(&mut self);
}

/// Command sink that sends the results of charstring evaluation to a [Pen].
pub struct PenSink<'a, P>(&'a mut P);

impl<'a, P> CommandSink for PenSink<'a, P>
where
    P: Pen,
{
    fn move_to(&mut self, x: Fixed, y: Fixed) {
        self.0.move_to(x.to_f64() as f32, y.to_f64() as f32);
    }

    fn line_to(&mut self, x: Fixed, y: Fixed) {
        self.0.line_to(x.to_f64() as f32, y.to_f64() as f32);
    }

    fn curve_to(&mut self, cx0: Fixed, cy0: Fixed, cx1: Fixed, cy1: Fixed, x: Fixed, y: Fixed) {
        self.0.curve_to(
            cx0.to_f64() as f32,
            cy0.to_f64() as f32,
            cx1.to_f64() as f32,
            cy1.to_f64() as f32,
            x.to_f64() as f32,
            y.to_f64() as f32,
        );
    }

    fn close(&mut self) {
        self.0.close();
    }
}

/// Evaluates the given charstring and emits the resulting commands to the
/// specified sink.
///
/// If evaluating a CFF2 charstring and the top-level table contains an
/// item variation store, then `blend_state` must be provided.
pub fn evaluate(
    charstring_data: &[u8],
    global_subrs: Index,
    subrs: Option<Index>,
    blend_state: Option<BlendState>,
    sink: &mut impl CommandSink,
) -> Result<(), Error> {
    let mut evaluator = Evaluator::new(global_subrs, subrs, blend_state);
    evaluator.evaluate(charstring_data, Fixed::ZERO, Fixed::ZERO, sink, 0)?;
    Ok(())
}

struct Evaluator<'a> {
    global_subrs: Index<'a>,
    subrs: Option<Index<'a>>,
    blend_state: Option<BlendState<'a>>,
    is_open: bool,
    have_read_width: bool,
    stem_count: usize,
    stack: Stack,
}

impl<'a> Evaluator<'a> {
    fn new(
        global_subrs: Index<'a>,
        subrs: Option<Index<'a>>,
        blend_state: Option<BlendState<'a>>,
    ) -> Self {
        Self {
            global_subrs,
            subrs,
            blend_state,
            is_open: false,
            have_read_width: false,
            stem_count: 0,
            stack: Stack::new(),
        }
    }

    fn evaluate(
        &mut self,
        charstring_data: &[u8],
        mut x: Fixed,
        mut y: Fixed,
        sink: &mut impl CommandSink,
        nesting_depth: u32,
    ) -> Result<(Fixed, Fixed), Error> {
        if nesting_depth > NESTING_DEPTH_LIMIT {
            return Err(Error::CharstringNestingDepthLimitExceeded);
        }
        use ops::*;
        let mut cursor = crate::FontData::new(charstring_data).cursor();
        while cursor.remaining_bytes() != 0 {
            let op = cursor.read::<u8>()?;
            if op == ESCAPE {
                let two_byte_op = cursor.read::<u8>()?;
                // The following "flex" operators are intended to emit
                // either two curves or a straight line depending on
                // a "flex depth" parameter and the distance from the
                // joining point to the chord connecting the two
                // end points. In practice, we just emit the two curves,
                // following FreeType:
                // <https://gitlab.freedesktop.org/freetype/freetype/-/blob/80a507a6b8e3d2906ad2c8ba69329bd2fb2a85ef/src/psaux/psintrp.c#L335>
                match two_byte_op {
                    HFLEX => {
                        let dx1 = x + self.stack.get_fixed(0)?;
                        let dy1 = y;
                        let dx2 = dx1 + self.stack.get_fixed(1)?;
                        let dy2 = dy1 + self.stack.get_fixed(2)?;
                        let dx3 = dx2 + self.stack.get_fixed(3)?;
                        let dy3 = dy2;
                        let dx4 = dx3 + self.stack.get_fixed(4)?;
                        let dy4 = dy2;
                        let dx5 = dx4 + self.stack.get_fixed(5)?;
                        let dy5 = y;
                        x = dx5 + self.stack.get_fixed(6)?;
                        sink.curve_to(dx1, dy1, dx2, dy2, dx3, dy3);
                        sink.curve_to(dx4, dy4, dx5, dy5, x, y);
                        self.stack.clear();
                    }
                    FLEX => {
                        let dx1 = x + self.stack.get_fixed(0)?;
                        let dy1 = y + self.stack.get_fixed(1)?;
                        let dx2 = dx1 + self.stack.get_fixed(2)?;
                        let dy2 = dy1 + self.stack.get_fixed(3)?;
                        let dx3 = dx2 + self.stack.get_fixed(4)?;
                        let dy3 = dy2 + self.stack.get_fixed(5)?;
                        let dx4 = dx3 + self.stack.get_fixed(6)?;
                        let dy4 = dy3 + self.stack.get_fixed(7)?;
                        let dx5 = dx4 + self.stack.get_fixed(8)?;
                        let dy5 = dy4 + self.stack.get_fixed(9)?;
                        x = dx5 + self.stack.get_fixed(10)?;
                        y = dy5 + self.stack.get_fixed(11)?;
                        sink.curve_to(dx1, dy1, dx2, dy2, dx3, dy3);
                        sink.curve_to(dx4, dy4, dx5, dy5, x, y);
                        self.stack.clear();
                    }
                    HFLEX1 => {
                        let dx1 = x + self.stack.get_fixed(0)?;
                        let dy1 = y + self.stack.get_fixed(1)?;
                        let dx2 = dx1 + self.stack.get_fixed(2)?;
                        let dy2 = dy1 + self.stack.get_fixed(3)?;
                        let dx3 = dx2 + self.stack.get_fixed(4)?;
                        let dy3 = dy2;
                        let dx4 = dx3 + self.stack.get_fixed(5)?;
                        let dy4 = dy2;
                        let dx5 = dx4 + self.stack.get_fixed(6)?;
                        let dy5 = dy4 + self.stack.get_fixed(7)?;
                        x = dx5 + self.stack.get_fixed(8)?;
                        sink.curve_to(dx1, dy1, dx2, dy2, dx3, dy3);
                        sink.curve_to(dx4, dy4, dx5, dy5, x, y);
                        self.stack.clear();
                    }
                    FLEX1 => {
                        let dx1 = x + self.stack.get_fixed(0)?;
                        let dy1 = y + self.stack.get_fixed(1)?;
                        let dx2 = dx1 + self.stack.get_fixed(2)?;
                        let dy2 = dy1 + self.stack.get_fixed(3)?;
                        let dx3 = dx2 + self.stack.get_fixed(4)?;
                        let dy3 = dy2 + self.stack.get_fixed(5)?;
                        let dx4 = dx3 + self.stack.get_fixed(6)?;
                        let dy4 = dy3 + self.stack.get_fixed(7)?;
                        let dx5 = dx4 + self.stack.get_fixed(8)?;
                        let dy5 = dy4 + self.stack.get_fixed(9)?;
                        if (dx5 - x).abs() > (dy5 - y).abs() {
                            x = dx5 + self.stack.get_fixed(10)?;
                        } else {
                            y = dy5 + self.stack.get_fixed(10)?;
                        }
                        sink.curve_to(dx1, dy1, dx2, dy2, dx3, dy3);
                        sink.curve_to(dx4, dy4, dx5, dy5, x, y);
                        self.stack.clear();
                    }
                    _ => return Err(Error::InvalidCharstringOperator(two_byte_op)),
                }
            } else {
                match op {
                    // Set the variation store index
                    VSINDEX => {
                        let blend_state =
                            self.blend_state.as_mut().ok_or(Error::MissingBlendState)?;
                        let store_index = self.stack.pop_i32()? as u16;
                        blend_state.set_store_index(store_index)?;
                    }
                    // Apply blending to the current operand stack
                    BLEND => {
                        let blend_state =
                            self.blend_state.as_ref().ok_or(Error::MissingBlendState)?;
                        self.stack.apply_blend(blend_state)?;
                    }
                    // Push an integer to the stack
                    28 | 32..=254 => {
                        self.stack.push(super::dict::parse_int(&mut cursor, op)?)?;
                    }
                    // Push a fixed point value to the stack
                    255 => {
                        let num = Fixed::from_bits(cursor.read::<i32>()?);
                        self.stack.push(num)?;
                    }
                    // Return from the current subroutine
                    RETURN => {
                        break;
                    }
                    // End the current charstring
                    // TODO: handle implied 'seac' operator
                    // <https://gitlab.freedesktop.org/freetype/freetype/-/blob/80a507a6b8e3d2906ad2c8ba69329bd2fb2a85ef/src/psaux/psintrp.c#L2463>
                    ENDCHAR => {
                        if !self.stack.is_empty() && !self.have_read_width {
                            self.have_read_width = true;
                            self.stack.clear();
                        }
                        if self.is_open {
                            self.is_open = false;
                            sink.close();
                        }
                        break;
                    }
                    // Emits a sequence of stem hints
                    // <https://gitlab.freedesktop.org/freetype/freetype/-/blob/80a507a6b8e3d2906ad2c8ba69329bd2fb2a85ef/src/psaux/psintrp.c#L777>
                    HSTEM | VSTEM | HSTEMHM | VSTEMHM => {
                        let mut i = 0;
                        let len = if self.stack.len_is_odd() && !self.have_read_width {
                            self.have_read_width = true;
                            i = 1;
                            self.stack.len() - 1
                        } else {
                            self.stack.len()
                        };
                        let is_horizontal = op == HSTEM || op == HSTEMHM;
                        let mut u = Fixed::ZERO;
                        while i < self.stack.len() {
                            u += self.stack.get_fixed(i)?;
                            let w = self.stack.get_fixed(i + 1)?;
                            let v = u.wrapping_add(w);
                            if is_horizontal {
                                sink.hstem(u, v);
                            } else {
                                sink.vstem(u, v);
                            }
                            u = v;
                            i += 2;
                        }
                        self.stem_count += len / 2;
                        self.stack.clear();
                    }
                    // Applies a hint or counter mask.
                    // If there are arguments on the stack, this is also an
                    // implied series of VSTEMHM operators.
                    // Hint and counter masks are bitstrings that determine
                    // the currently active set of hints.
                    // <https://gitlab.freedesktop.org/freetype/freetype/-/blob/80a507a6b8e3d2906ad2c8ba69329bd2fb2a85ef/src/psaux/psintrp.c#L2580>
                    HINTMASK | CNTRMASK => {
                        let mut i = 0;
                        let len = if self.stack.len_is_odd() && !self.have_read_width {
                            self.have_read_width = true;
                            i = 1;
                            self.stack.len() - 1
                        } else {
                            self.stack.len()
                        };
                        let mut u = Fixed::ZERO;
                        while i < self.stack.len() {
                            u += self.stack.get_fixed(i)?;
                            let w = self.stack.get_fixed(i + 1)?;
                            let v = u + w;
                            sink.vstem(u, v);
                            u = v;
                            i += 2;
                        }
                        self.stem_count += len / 2;
                        let count = (self.stem_count + 7) / 8;
                        let mask = cursor.read_array::<u8>(count)?;
                        if op == HINTMASK {
                            sink.hint_mask(mask);
                        } else {
                            sink.counter_mask(mask);
                        }
                        self.stack.clear();
                    }
                    // Starts a new subpath
                    // <https://gitlab.freedesktop.org/freetype/freetype/-/blob/80a507a6b8e3d2906ad2c8ba69329bd2fb2a85ef/src/psaux/psintrp.c#L2653>
                    RMOVETO => {
                        let mut i = 0;
                        if self.stack.len() == 3 && !self.have_read_width {
                            self.have_read_width = true;
                            i = 1;
                        }
                        if !self.is_open {
                            self.is_open = true;
                        } else {
                            sink.close();
                        }
                        x += self.stack.get_fixed(i)?;
                        y += self.stack.get_fixed(i + 1)?;
                        sink.move_to(x, y);
                        self.stack.clear();
                    }
                    // Starts a new subpath by moving the current point in the
                    // horizontal or vertical direction
                    // <https://gitlab.freedesktop.org/freetype/freetype/-/blob/80a507a6b8e3d2906ad2c8ba69329bd2fb2a85ef/src/psaux/psintrp.c#L839>
                    HMOVETO | VMOVETO => {
                        let mut i = 0;
                        if self.stack.len() == 2 && !self.have_read_width {
                            self.have_read_width = true;
                            i = 1;
                        }
                        if !self.is_open {
                            self.is_open = true;
                        } else {
                            sink.close();
                        }
                        if op == HMOVETO {
                            x += self.stack.get_fixed(i)?;
                        } else {
                            y += self.stack.get_fixed(i)?;
                        }
                        sink.move_to(x, y);
                        self.stack.clear();
                    }
                    // Emits a sequence of lines
                    // <https://gitlab.freedesktop.org/freetype/freetype/-/blob/80a507a6b8e3d2906ad2c8ba69329bd2fb2a85ef/src/psaux/psintrp.c#L863>
                    RLINETO => {
                        let mut i = 0;
                        while i < self.stack.len() {
                            x += self.stack.get_fixed(i)?;
                            y += self.stack.get_fixed(i + 1)?;
                            sink.line_to(x, y);
                            i += 2;
                        }
                        self.stack.clear();
                    }
                    // Emits a sequence of alternating horizontal and vertical
                    // lines
                    // <https://gitlab.freedesktop.org/freetype/freetype/-/blob/80a507a6b8e3d2906ad2c8ba69329bd2fb2a85ef/src/psaux/psintrp.c#L885>
                    HLINETO | VLINETO => {
                        let mut is_x = op == HLINETO;
                        for i in 0..self.stack.len() {
                            let value = self.stack.get_fixed(i)?;
                            if is_x {
                                x += value;
                            } else {
                                y += value;
                            }
                            is_x = !is_x;
                            sink.line_to(x, y);
                        }
                        self.stack.clear();
                    }
                    // Emits a sequence of curves possibly followed by a line
                    // <https://gitlab.freedesktop.org/freetype/freetype/-/blob/80a507a6b8e3d2906ad2c8ba69329bd2fb2a85ef/src/psaux/psintrp.c#L915>
                    RRCURVETO | RCURVELINE => {
                        let count = self.stack.len();
                        let mut i = 0;
                        while i + 6 <= count {
                            let x1 = x + self.stack.get_fixed(i)?;
                            let y1 = y + self.stack.get_fixed(i + 1)?;
                            let x2 = x1 + self.stack.get_fixed(i + 2)?;
                            let y2 = y1 + self.stack.get_fixed(i + 3)?;
                            x = x2 + self.stack.get_fixed(i + 4)?;
                            y = y2 + self.stack.get_fixed(i + 5)?;
                            sink.curve_to(x1, y1, x2, y2, x, y);
                            i += 6;
                        }
                        if op == RCURVELINE {
                            x += self.stack.get_fixed(i)?;
                            y += self.stack.get_fixed(i + 1)?;
                            sink.line_to(x, y);
                        }
                        self.stack.clear();
                    }
                    // Emits a sequence of lines followed by a curve
                    // <https://gitlab.freedesktop.org/freetype/freetype/-/blob/80a507a6b8e3d2906ad2c8ba69329bd2fb2a85ef/src/psaux/psintrp.c#L2702>
                    RLINECURVE => {
                        let mut i = 0;
                        while i < self.stack.len() - 6 {
                            x += self.stack.get_fixed(i)?;
                            y += self.stack.get_fixed(i + 1)?;
                            sink.line_to(x, y);
                            i += 2;
                        }
                        let x1 = x + self.stack.get_fixed(i)?;
                        let y1 = y + self.stack.get_fixed(i + 1)?;
                        let x2 = x1 + self.stack.get_fixed(i + 2)?;
                        let y2 = y1 + self.stack.get_fixed(i + 3)?;
                        x = x2 + self.stack.get_fixed(i + 4)?;
                        y = y2 + self.stack.get_fixed(i + 5)?;
                        sink.curve_to(x1, y1, x2, y2, x, y);
                        self.stack.clear();
                    }
                    // Emits curves that start and end vertical, unless
                    // the stack count is odd, in which case the first
                    // curve may start with a horizontal tangent
                    // <https://gitlab.freedesktop.org/freetype/freetype/-/blob/80a507a6b8e3d2906ad2c8ba69329bd2fb2a85ef/src/psaux/psintrp.c#L2744>
                    VVCURVETO => {
                        let mut i = 0;
                        if self.stack.len_is_odd() {
                            x += self.stack.get_fixed(0)?;
                            i += 1;
                        }
                        while i < self.stack.len() {
                            let x1 = x;
                            let y1 = y + self.stack.get_fixed(i)?;
                            let x2 = x1 + self.stack.get_fixed(i + 1)?;
                            let y2 = y1 + self.stack.get_fixed(i + 2)?;
                            x = x2;
                            y = y2 + self.stack.get_fixed(i + 3)?;
                            sink.curve_to(x1, y1, x2, y2, x, y);
                            i += 4;
                        }
                        self.stack.clear();
                    }
                    // Emits curves that start and end horizontal, unless
                    // the stack count is odd, in which case the first
                    // curve may start with a vertical tangent
                    // <https://gitlab.freedesktop.org/freetype/freetype/-/blob/80a507a6b8e3d2906ad2c8ba69329bd2fb2a85ef/src/psaux/psintrp.c#L2789>
                    HHCURVETO => {
                        let mut i = 0;
                        if self.stack.len_is_odd() {
                            y += self.stack.get_fixed(0)?;
                            i += 1;
                        }
                        while i < self.stack.len() {
                            let x1 = x + self.stack.get_fixed(i)?;
                            let y1 = y;
                            let x2 = x1 + self.stack.get_fixed(i + 1)?;
                            let y2 = y1 + self.stack.get_fixed(i + 2)?;
                            x = x2 + self.stack.get_fixed(i + 3)?;
                            y = y2;
                            sink.curve_to(x1, y1, x2, y2, x, y);
                            i += 4;
                        }
                        self.stack.clear();
                    }
                    // Alternates between curves with horizontal and vertical
                    // tangents
                    // <https://gitlab.freedesktop.org/freetype/freetype/-/blob/80a507a6b8e3d2906ad2c8ba69329bd2fb2a85ef/src/psaux/psintrp.c#L2834>
                    HVCURVETO | VHCURVETO => {
                        let count = self.stack.len();
                        let mut i = (count & !2) - count;
                        let mut alternate = op == HVCURVETO;
                        while i < count {
                            let (x1, x2, x3, y1, y2, y3);
                            if alternate {
                                x1 = x + self.stack.get_fixed(i)?;
                                y1 = y;
                                x2 = x1 + self.stack.get_fixed(i + 1)?;
                                y2 = y1 + self.stack.get_fixed(i + 2)?;
                                y3 = y2 + self.stack.get_fixed(i + 3)?;
                                x3 = if count - i == 5 {
                                    x2 + self.stack.get_fixed(i + 4)?
                                } else {
                                    x2
                                };
                                alternate = false;
                            } else {
                                x1 = x;
                                y1 = self.stack.get_fixed(i)?;
                                x2 = x1 + self.stack.get_fixed(i + 1)?;
                                y2 = y1 + self.stack.get_fixed(i + 2)?;
                                x3 = x2 + self.stack.get_fixed(i + 3)?;
                                y3 = if count - i == 5 {
                                    y2 + self.stack.get_fixed(i + 4)?
                                } else {
                                    y2
                                };
                                alternate = true;
                            }
                            sink.curve_to(x1, y1, x2, y2, x3, y3);
                            x = x3;
                            y = y3;
                            i += 4;
                        }
                        self.stack.clear();
                    }
                    // Call local or global subroutine
                    // <https://gitlab.freedesktop.org/freetype/freetype/-/blob/80a507a6b8e3d2906ad2c8ba69329bd2fb2a85ef/src/psaux/psintrp.c#L972>
                    CALLSUBR | CALLGSUBR => {
                        let subrs_index = if op == CALLSUBR {
                            self.subrs.as_ref().unwrap()
                        } else {
                            &self.global_subrs
                        };
                        let biased_index =
                            (self.stack.pop_i32()? + subrs_index.subr_bias()) as usize;
                        let subr_charstring_data = subrs_index.get(biased_index)?;
                        let pos =
                            self.evaluate(subr_charstring_data, x, y, sink, nesting_depth + 1)?;
                        x = pos.0;
                        y = pos.1;
                    }
                    _ => return Err(Error::InvalidCharstringOperator(op)),
                }
            }
        }
        Ok((x, y))
    }
}

/// Charstring operators.
/// See <https://learn.microsoft.com/en-us/typography/opentype/spec/cff2charstr#appendix-a-cff2-charstring-command-codes>
// TODO: This is currently missing legacy math and logical operators.
// fonttools doesn't even implement these: <https://github.com/fonttools/fonttools/blob/65598197c8afd415781f6667a7fb647c2c987fff/Lib/fontTools/misc/psCharStrings.py#L409>
mod ops {
    // One byte operators
    pub const HSTEM: u8 = 1;
    pub const VSTEM: u8 = 3;
    pub const VMOVETO: u8 = 4;
    pub const RLINETO: u8 = 5;
    pub const HLINETO: u8 = 6;
    pub const VLINETO: u8 = 7;
    pub const RRCURVETO: u8 = 8;
    pub const CALLSUBR: u8 = 10;
    pub const RETURN: u8 = 11;
    pub const ENDCHAR: u8 = 14;
    pub const VSINDEX: u8 = 15;
    pub const BLEND: u8 = 16;
    pub const HSTEMHM: u8 = 18;
    pub const HINTMASK: u8 = 19;
    pub const CNTRMASK: u8 = 20;
    pub const RMOVETO: u8 = 21;
    pub const HMOVETO: u8 = 22;
    pub const VSTEMHM: u8 = 23;
    pub const RCURVELINE: u8 = 24;
    pub const RLINECURVE: u8 = 25;
    pub const VVCURVETO: u8 = 26;
    pub const HHCURVETO: u8 = 27;
    pub const CALLGSUBR: u8 = 29;
    pub const VHCURVETO: u8 = 30;
    pub const HVCURVETO: u8 = 31;

    // Escape code to trigger processing of a two byte operator
    pub const ESCAPE: u8 = 12;

    /// Two byte operators
    pub const HFLEX: u8 = 34;
    pub const FLEX: u8 = 35;
    pub const HFLEX1: u8 = 36;
    pub const FLEX1: u8 = 37;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{tables::variations::ItemVariationStore, types::F2Dot14, FontData, FontRead};

    #[derive(Copy, Clone, PartialEq, Debug)]
    enum Command {
        MoveTo(Fixed, Fixed),
        LineTo(Fixed, Fixed),
        CurveTo(Fixed, Fixed, Fixed, Fixed, Fixed, Fixed),
        Close,
    }

    #[derive(PartialEq, Default, Debug)]
    struct CaptureCommandSink(Vec<Command>);

    impl CommandSink for CaptureCommandSink {
        fn move_to(&mut self, x: Fixed, y: Fixed) {
            self.0.push(Command::MoveTo(x, y))
        }

        fn line_to(&mut self, x: Fixed, y: Fixed) {
            self.0.push(Command::LineTo(x, y))
        }

        fn curve_to(&mut self, cx0: Fixed, cy0: Fixed, cx1: Fixed, cy1: Fixed, x: Fixed, y: Fixed) {
            self.0.push(Command::CurveTo(cx0, cy0, cx1, cy1, x, y))
        }

        fn close(&mut self) {
            self.0.push(Command::Close)
        }
    }

    #[test]
    fn cff2_example_subr() {
        use Command::*;
        let charstring = &font_test_data::cff2::EXAMPLE[0xc8..=0xe1];
        let empty_index_bytes = [0u8; 8];
        let store =
            ItemVariationStore::read(FontData::new(&font_test_data::cff2::EXAMPLE[18..])).unwrap();
        let global_subrs = Index::new(&empty_index_bytes, true).unwrap();
        let coords = &[F2Dot14::from_f32(0.0)];
        let blend_state = BlendState::new(store, coords, 0).unwrap();
        let mut commands = CaptureCommandSink::default();
        evaluate(
            charstring,
            global_subrs,
            None,
            Some(blend_state),
            &mut commands,
        )
        .unwrap();
        // 50 50 100 1 blend 0 rmoveto
        // 500 -100 -200 1 blend hlineto
        // 500 vlineto
        // -500 100 200 1 blend hlineto
        //
        // applying blends at default location results in:
        // 50 0 rmoveto
        // 500 hlineto
        // 500 vlineto
        // -500 hlineto
        //
        // applying relative operators:
        // 50 0 moveto
        // 550 0 lineto
        // 550 500 lineto
        // 50 500 lineto
        let expected = &[
            MoveTo(Fixed::from_f64(50.0), Fixed::ZERO),
            LineTo(Fixed::from_f64(550.0), Fixed::ZERO),
            LineTo(Fixed::from_f64(550.0), Fixed::from_f64(500.0)),
            LineTo(Fixed::from_f64(50.0), Fixed::from_f64(500.0)),
        ];
        assert_eq!(&commands.0, expected);
    }
}
