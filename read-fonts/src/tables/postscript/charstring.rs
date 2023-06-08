//! Parsing for PostScript charstrings.

use super::{BlendState, Error, Index, Stack};
use crate::{
    types::{Fixed, Pen},
    Cursor,
};

/// Maximum nesting depth for subroutine calls.
///
/// See "Appendix B Type 2 Charstring Implementation Limits" at
/// <https://adobe-type-tools.github.io/font-tech-notes/pdfs/5177.Type2.pdf#page=33>
pub const NESTING_DEPTH_LIMIT: u32 = 10;

/// Trait for processing commands resulting from charstring evaluation.
///
/// During processing, the path construction operators (see "4.1 Path
/// Construction Operators" at <https://adobe-type-tools.github.io/font-tech-notes/pdfs/5177.Type2.pdf#page=15>)
/// are simplified into the basic move, line, curve and close commands.
///
/// This also has optional callbacks for processing hint operators. See "4.3
/// Hint Operators" at <https://adobe-type-tools.github.io/font-tech-notes/pdfs/5177.Type2.pdf#page=21>
/// for more detail.
#[allow(unused_variables)]
pub trait CommandSink {
    // Path construction operators.
    fn move_to(&mut self, x: Fixed, y: Fixed);
    fn line_to(&mut self, x: Fixed, y: Fixed);
    fn curve_to(&mut self, cx0: Fixed, cy0: Fixed, cx1: Fixed, cy1: Fixed, x: Fixed, y: Fixed);
    fn close(&mut self);
    // Hint operators.
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
}

/// Command sink that sends the results of charstring evaluation to a [Pen].
pub struct PenSink<'a, P>(&'a mut P);

impl<'a, P> PenSink<'a, P> {
    pub fn new(pen: &'a mut P) -> Self {
        Self(pen)
    }
}

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
/// If the Private DICT associated with this charstring contains local
/// subroutines, then the `subrs` index must be provided, otherwise
/// `Error::MissingSubroutines` will be returned if a callsubr operator
/// is present.
///
/// If evaluating a CFF2 charstring and the top-level table contains an
/// item variation store, then `blend_state` must be provided, otherwise
/// `Error::MissingBlendState` will be returned if a blend operator is
/// present.
pub fn evaluate(
    charstring_data: &[u8],
    global_subrs: Index,
    subrs: Option<Index>,
    blend_state: Option<BlendState>,
    sink: &mut impl CommandSink,
) -> Result<(), Error> {
    let mut evaluator = Evaluator::new(global_subrs, subrs, blend_state, sink);
    evaluator.evaluate(charstring_data, 0)?;
    Ok(())
}

/// Transient state for evaluating a charstring and handling recursive
/// subroutine calls.
struct Evaluator<'a, S> {
    global_subrs: Index<'a>,
    subrs: Option<Index<'a>>,
    blend_state: Option<BlendState<'a>>,
    sink: &'a mut S,
    is_open: bool,
    have_read_width: bool,
    stem_count: usize,
    x: Fixed,
    y: Fixed,
    stack: Stack,
}

impl<'a, S> Evaluator<'a, S>
where
    S: CommandSink,
{
    fn new(
        global_subrs: Index<'a>,
        subrs: Option<Index<'a>>,
        blend_state: Option<BlendState<'a>>,
        sink: &'a mut S,
    ) -> Self {
        Self {
            global_subrs,
            subrs,
            blend_state,
            sink,
            is_open: false,
            have_read_width: false,
            stem_count: 0,
            stack: Stack::new(),
            x: Fixed::ZERO,
            y: Fixed::ZERO,
        }
    }

    fn evaluate(&mut self, charstring_data: &[u8], nesting_depth: u32) -> Result<(), Error> {
        if nesting_depth > NESTING_DEPTH_LIMIT {
            return Err(Error::CharstringNestingDepthLimitExceeded);
        }
        let mut cursor = crate::FontData::new(charstring_data).cursor();
        while cursor.remaining_bytes() != 0 {
            let b0 = cursor.read::<u8>()?;
            match b0 {
                // See "3.2 Charstring Number Encoding" <https://adobe-type-tools.github.io/font-tech-notes/pdfs/5177.Type2.pdf#page=12>
                //
                // Push an integer to the stack
                28 | 32..=254 => {
                    self.stack.push(super::dict::parse_int(&mut cursor, b0)?)?;
                }
                // Push a fixed point value to the stack
                255 => {
                    let num = Fixed::from_bits(cursor.read::<i32>()?);
                    self.stack.push(num)?;
                }
                _ => {
                    let operator = Operator::read(&mut cursor, b0)?;
                    if !self.evaluate_operator(operator, &mut cursor, nesting_depth)? {
                        break;
                    }
                }
            }
        }
        Ok(())
    }

    /// Evaluates a single charstring operator.
    ///
    /// Returns `Ok(true)` if evaluation should continue.
    fn evaluate_operator(
        &mut self,
        operator: Operator,
        cursor: &mut Cursor,
        nesting_depth: u32,
    ) -> Result<bool, Error> {
        use Operator::*;
        match operator {
            // The following "flex" operators are intended to emit
            // either two curves or a straight line depending on
            // a "flex depth" parameter and the distance from the
            // joining point to the chord connecting the two
            // end points. In practice, we just emit the two curves,
            // following FreeType:
            // <https://gitlab.freedesktop.org/freetype/freetype/-/blob/80a507a6b8e3d2906ad2c8ba69329bd2fb2a85ef/src/psaux/psintrp.c#L335>
            //
            // Spec: <https://adobe-type-tools.github.io/font-tech-notes/pdfs/5177.Type2.pdf#page=18>
            Flex => {
                let args = self.stack.get_fixed_array::<12>(0)?;
                let dx1 = self.x + args[0];
                let dy1 = self.y + args[1];
                let dx2 = dx1 + args[2];
                let dy2 = dy1 + args[3];
                let dx3 = dx2 + args[4];
                let dy3 = dy2 + args[5];
                let dx4 = dx3 + args[6];
                let dy4 = dy3 + args[7];
                let dx5 = dx4 + args[8];
                let dy5 = dy4 + args[9];
                self.x = dx5 + args[10];
                self.y = dy5 + args[11];
                self.sink.curve_to(dx1, dy1, dx2, dy2, dx3, dy3);
                self.sink.curve_to(dx4, dy4, dx5, dy5, self.x, self.y);
                self.stack.clear();
            }
            // Spec: <https://adobe-type-tools.github.io/font-tech-notes/pdfs/5177.Type2.pdf#page=19>
            HFlex => {
                let args = self.stack.get_fixed_array::<7>(0)?;
                let dx1 = self.x + args[0];
                let dy1 = self.y;
                let dx2 = dx1 + args[1];
                let dy2 = dy1 + args[2];
                let dx3 = dx2 + args[3];
                let dy3 = dy2;
                let dx4 = dx3 + args[4];
                let dy4 = dy2;
                let dx5 = dx4 + args[5];
                let dy5 = self.y;
                self.x = dx5 + args[6];
                self.sink.curve_to(dx1, dy1, dx2, dy2, dx3, dy3);
                self.sink.curve_to(dx4, dy4, dx5, dy5, self.x, self.y);
                self.stack.clear();
            }
            // Spec: <https://adobe-type-tools.github.io/font-tech-notes/pdfs/5177.Type2.pdf#page=19>
            HFlex1 => {
                let args = self.stack.get_fixed_array::<9>(0)?;
                let dx1 = self.x + args[0];
                let dy1 = self.y + args[1];
                let dx2 = dx1 + args[2];
                let dy2 = dy1 + args[3];
                let dx3 = dx2 + args[4];
                let dy3 = dy2;
                let dx4 = dx3 + args[5];
                let dy4 = dy2;
                let dx5 = dx4 + args[6];
                let dy5 = dy4 + args[7];
                self.x = dx5 + args[8];
                self.sink.curve_to(dx1, dy1, dx2, dy2, dx3, dy3);
                self.sink.curve_to(dx4, dy4, dx5, dy5, self.x, self.y);
                self.stack.clear();
            }
            // Spec: <https://adobe-type-tools.github.io/font-tech-notes/pdfs/5177.Type2.pdf#page=20>
            Flex1 => {
                let args = self.stack.get_fixed_array::<11>(0)?;
                let dx1 = self.x + args[0];
                let dy1 = self.y + args[1];
                let dx2 = dx1 + args[2];
                let dy2 = dy1 + args[3];
                let dx3 = dx2 + args[4];
                let dy3 = dy2 + args[5];
                let dx4 = dx3 + args[6];
                let dy4 = dy3 + args[7];
                let dx5 = dx4 + args[8];
                let dy5 = dy4 + args[9];
                if (dx5 - self.x).abs() > (dy5 - self.y).abs() {
                    self.x = dx5 + args[10];
                } else {
                    self.y = dy5 + args[10];
                }
                self.sink.curve_to(dx1, dy1, dx2, dy2, dx3, dy3);
                self.sink.curve_to(dx4, dy4, dx5, dy5, self.x, self.y);
                self.stack.clear();
            }
            // Set the variation store index
            // <https://learn.microsoft.com/en-us/typography/opentype/spec/cff2charstr#syntax-for-font-variations-support-operators>
            VariationStoreIndex => {
                let blend_state = self.blend_state.as_mut().ok_or(Error::MissingBlendState)?;
                let store_index = self.stack.pop_i32()? as u16;
                blend_state.set_store_index(store_index)?;
            }
            // Apply blending to the current operand stack
            // <https://learn.microsoft.com/en-us/typography/opentype/spec/cff2charstr#syntax-for-font-variations-support-operators>
            Blend => {
                let blend_state = self.blend_state.as_ref().ok_or(Error::MissingBlendState)?;
                self.stack.apply_blend(blend_state)?;
            }
            // Return from the current subroutine
            // Spec: <https://adobe-type-tools.github.io/font-tech-notes/pdfs/5177.Type2.pdf#page=29>
            Return => {
                return Ok(false);
            }
            // End the current charstring
            // TODO: handle implied 'seac' operator
            // Spec: <https://adobe-type-tools.github.io/font-tech-notes/pdfs/5177.Type2.pdf#page=21>
            // FT: <https://gitlab.freedesktop.org/freetype/freetype/-/blob/80a507a6b8e3d2906ad2c8ba69329bd2fb2a85ef/src/psaux/psintrp.c#L2463>
            EndChar => {
                if !self.stack.is_empty() && !self.have_read_width {
                    self.have_read_width = true;
                    self.stack.clear();
                }
                if self.is_open {
                    self.is_open = false;
                    self.sink.close();
                }
                return Ok(false);
            }
            // Emits a sequence of stem hints
            // Spec: <https://adobe-type-tools.github.io/font-tech-notes/pdfs/5177.Type2.pdf#page=21>
            // FT: <https://gitlab.freedesktop.org/freetype/freetype/-/blob/80a507a6b8e3d2906ad2c8ba69329bd2fb2a85ef/src/psaux/psintrp.c#L777>
            HStem | VStem | HStemHm | VStemHm => {
                let mut i = 0;
                let len = if self.stack.len_is_odd() && !self.have_read_width {
                    self.have_read_width = true;
                    i = 1;
                    self.stack.len() - 1
                } else {
                    self.stack.len()
                };
                let is_horizontal = matches!(operator, HStem | HStemHm);
                let mut u = Fixed::ZERO;
                while i < self.stack.len() {
                    let args = self.stack.get_fixed_array::<2>(i)?;
                    u += args[0];
                    let w = args[1];
                    let v = u.wrapping_add(w);
                    if is_horizontal {
                        self.sink.hstem(u, v);
                    } else {
                        self.sink.vstem(u, v);
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
            // Spec: <https://adobe-type-tools.github.io/font-tech-notes/pdfs/5177.Type2.pdf#page=24>
            // FT: <https://gitlab.freedesktop.org/freetype/freetype/-/blob/80a507a6b8e3d2906ad2c8ba69329bd2fb2a85ef/src/psaux/psintrp.c#L2580>
            HintMask | CntrMask => {
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
                    let args = self.stack.get_fixed_array::<2>(i)?;
                    u += args[0];
                    let w = args[1];
                    let v = u + w;
                    self.sink.vstem(u, v);
                    u = v;
                    i += 2;
                }
                self.stem_count += len / 2;
                let count = (self.stem_count + 7) / 8;
                let mask = cursor.read_array::<u8>(count)?;
                if operator == HintMask {
                    self.sink.hint_mask(mask);
                } else {
                    self.sink.counter_mask(mask);
                }
                self.stack.clear();
            }
            // Starts a new subpath
            // Spec: <https://adobe-type-tools.github.io/font-tech-notes/pdfs/5177.Type2.pdf#page=16>
            // FT: <https://gitlab.freedesktop.org/freetype/freetype/-/blob/80a507a6b8e3d2906ad2c8ba69329bd2fb2a85ef/src/psaux/psintrp.c#L2653>
            RMoveTo => {
                let mut i = 0;
                if self.stack.len() == 3 && !self.have_read_width {
                    self.have_read_width = true;
                    i = 1;
                }
                if !self.is_open {
                    self.is_open = true;
                } else {
                    self.sink.close();
                }
                let args = self.stack.get_fixed_array::<2>(i)?;
                self.x += args[0];
                self.y += args[1];
                self.sink.move_to(self.x, self.y);
                self.stack.clear();
            }
            // Starts a new subpath by moving the current point in the
            // horizontal or vertical direction
            // Spec: <https://adobe-type-tools.github.io/font-tech-notes/pdfs/5177.Type2.pdf#page=16>
            // FT: <https://gitlab.freedesktop.org/freetype/freetype/-/blob/80a507a6b8e3d2906ad2c8ba69329bd2fb2a85ef/src/psaux/psintrp.c#L839>
            HMoveTo | VMoveTo => {
                let mut i = 0;
                if self.stack.len() == 2 && !self.have_read_width {
                    self.have_read_width = true;
                    i = 1;
                }
                if !self.is_open {
                    self.is_open = true;
                } else {
                    self.sink.close();
                }
                let value = self.stack.get_fixed(i)?;
                if operator == HMoveTo {
                    self.x += value;
                } else {
                    self.y += value;
                }
                self.sink.move_to(self.x, self.y);
                self.stack.clear();
            }
            // Emits a sequence of lines
            // Spec: <https://adobe-type-tools.github.io/font-tech-notes/pdfs/5177.Type2.pdf#page=16>
            // FT: <https://gitlab.freedesktop.org/freetype/freetype/-/blob/80a507a6b8e3d2906ad2c8ba69329bd2fb2a85ef/src/psaux/psintrp.c#L863>
            RLineTo => {
                let mut i = 0;
                while i < self.stack.len() {
                    let args = self.stack.get_fixed_array::<2>(i)?;
                    self.x += args[0];
                    self.y += args[1];
                    self.sink.line_to(self.x, self.y);
                    i += 2;
                }
                self.stack.clear();
            }
            // Emits a sequence of alternating horizontal and vertical
            // lines
            // Spec: <https://adobe-type-tools.github.io/font-tech-notes/pdfs/5177.Type2.pdf#page=16>
            // FT: <https://gitlab.freedesktop.org/freetype/freetype/-/blob/80a507a6b8e3d2906ad2c8ba69329bd2fb2a85ef/src/psaux/psintrp.c#L885>
            HLineTo | VLineTo => {
                let mut is_x = operator == HLineTo;
                for i in 0..self.stack.len() {
                    let value = self.stack.get_fixed(i)?;
                    if is_x {
                        self.x += value;
                    } else {
                        self.y += value;
                    }
                    is_x = !is_x;
                    self.sink.line_to(self.x, self.y);
                }
                self.stack.clear();
            }
            // Emits curves that start and end horizontal, unless
            // the stack count is odd, in which case the first
            // curve may start with a vertical tangent
            // Spec: <https://adobe-type-tools.github.io/font-tech-notes/pdfs/5177.Type2.pdf#page=17>
            // FT: <https://gitlab.freedesktop.org/freetype/freetype/-/blob/80a507a6b8e3d2906ad2c8ba69329bd2fb2a85ef/src/psaux/psintrp.c#L2789>
            HhCurveTo => {
                let mut i = 0;
                if self.stack.len_is_odd() {
                    self.y += self.stack.get_fixed(0)?;
                    i += 1;
                }
                while i < self.stack.len() {
                    let args = self.stack.get_fixed_array::<4>(i)?;
                    let x1 = self.x + args[0];
                    let y1 = self.y;
                    let x2 = x1 + args[1];
                    let y2 = y1 + args[2];
                    self.x = x2 + args[3];
                    self.y = y2;
                    self.sink.curve_to(x1, y1, x2, y2, self.x, self.y);
                    i += 4;
                }
                self.stack.clear();
            }
            // Alternates between curves with horizontal and vertical
            // tangents
            // Spec: <https://adobe-type-tools.github.io/font-tech-notes/pdfs/5177.Type2.pdf#page=17>
            // FT: <https://gitlab.freedesktop.org/freetype/freetype/-/blob/80a507a6b8e3d2906ad2c8ba69329bd2fb2a85ef/src/psaux/psintrp.c#L2834>
            HvCurveTo | VhCurveTo => {
                let count = self.stack.len();
                let mut i = (count & !2) - count;
                let mut alternate = operator == HvCurveTo;
                while i < count {
                    let (x1, x2, x3, y1, y2, y3);
                    if alternate {
                        let args = self.stack.get_fixed_array::<4>(i)?;
                        x1 = self.x + args[0];
                        y1 = self.y;
                        x2 = x1 + args[1];
                        y2 = y1 + args[2];
                        y3 = y2 + args[3];
                        x3 = if count - i == 5 {
                            let x3 = x2 + self.stack.get_fixed(i + 4)?;
                            i += 1;
                            x3
                        } else {
                            x2
                        };
                        alternate = false;
                    } else {
                        let args = self.stack.get_fixed_array::<4>(i)?;
                        x1 = self.x;
                        y1 = args[0];
                        x2 = x1 + args[1];
                        y2 = y1 + args[2];
                        x3 = x2 + args[3];
                        y3 = if count - i == 5 {
                            let y3 = y2 + self.stack.get_fixed(i + 4)?;
                            i += 1;
                            y3
                        } else {
                            y2
                        };
                        alternate = true;
                    }
                    self.sink.curve_to(x1, y1, x2, y2, x3, y3);
                    self.x = x3;
                    self.y = y3;
                    i += 4;
                }
                self.stack.clear();
            }
            // Emits a sequence of curves possibly followed by a line
            // Spec: <https://adobe-type-tools.github.io/font-tech-notes/pdfs/5177.Type2.pdf#page=17>
            // FT: <https://gitlab.freedesktop.org/freetype/freetype/-/blob/80a507a6b8e3d2906ad2c8ba69329bd2fb2a85ef/src/psaux/psintrp.c#L915>
            RrCurveTo | RCurveLine => {
                let count = self.stack.len();
                let mut i = 0;
                while i + 6 <= count {
                    let args = self.stack.get_fixed_array::<6>(i)?;
                    let x1 = self.x + args[0];
                    let y1 = self.y + args[1];
                    let x2 = x1 + args[2];
                    let y2 = y1 + args[3];
                    self.x = x2 + args[4];
                    self.y = y2 + args[5];
                    self.sink.curve_to(x1, y1, x2, y2, self.x, self.y);
                    i += 6;
                }
                if operator == RCurveLine {
                    let [dx, dy] = self.stack.get_fixed_array::<2>(i)?;
                    self.x += dx;
                    self.y += dy;
                    self.sink.line_to(self.x, self.y);
                }
                self.stack.clear();
            }
            // Emits a sequence of lines followed by a curve
            // Spec: <https://adobe-type-tools.github.io/font-tech-notes/pdfs/5177.Type2.pdf#page=18>
            // FT: <https://gitlab.freedesktop.org/freetype/freetype/-/blob/80a507a6b8e3d2906ad2c8ba69329bd2fb2a85ef/src/psaux/psintrp.c#L2702>
            RLineCurve => {
                let mut i = 0;
                while i < self.stack.len() - 6 {
                    let [dx, dy] = self.stack.get_fixed_array::<2>(i)?;
                    self.x += dx;
                    self.y += dy;
                    self.sink.line_to(self.x, self.y);
                    i += 2;
                }
                let args = self.stack.get_fixed_array::<6>(i)?;
                let x1 = self.x + args[0];
                let y1 = self.y + args[1];
                let x2 = x1 + args[2];
                let y2 = y1 + args[3];
                self.x = x2 + args[4];
                self.y = y2 + args[5];
                self.sink.curve_to(x1, y1, x2, y2, self.x, self.y);
                self.stack.clear();
            }
            // Emits curves that start and end vertical, unless
            // the stack count is odd, in which case the first
            // curve may start with a horizontal tangent
            // Spec: <https://adobe-type-tools.github.io/font-tech-notes/pdfs/5177.Type2.pdf#page=18>
            // FT: <https://gitlab.freedesktop.org/freetype/freetype/-/blob/80a507a6b8e3d2906ad2c8ba69329bd2fb2a85ef/src/psaux/psintrp.c#L2744>
            VvCurveTo => {
                let mut i = 0;
                if self.stack.len_is_odd() {
                    self.x += self.stack.get_fixed(0)?;
                    i += 1;
                }
                while i < self.stack.len() {
                    let args = self.stack.get_fixed_array::<4>(i)?;
                    let x1 = self.x;
                    let y1 = self.y + args[0];
                    let x2 = x1 + args[1];
                    let y2 = y1 + args[2];
                    self.x = x2;
                    self.y = y2 + args[3];
                    self.sink.curve_to(x1, y1, x2, y2, self.x, self.y);
                    i += 4;
                }
                self.stack.clear();
            }
            // Call local or global subroutine
            // Spec: <https://adobe-type-tools.github.io/font-tech-notes/pdfs/5177.Type2.pdf#page=29>
            // FT: <https://gitlab.freedesktop.org/freetype/freetype/-/blob/80a507a6b8e3d2906ad2c8ba69329bd2fb2a85ef/src/psaux/psintrp.c#L972>
            CallSubr | CallGsubr => {
                let subrs_index = if operator == CallSubr {
                    self.subrs.as_ref().ok_or(Error::MissingSubroutines)?
                } else {
                    &self.global_subrs
                };
                let biased_index = (self.stack.pop_i32()? + subrs_index.subr_bias()) as usize;
                let subr_charstring_data = subrs_index.get(biased_index)?;
                self.evaluate(subr_charstring_data, nesting_depth + 1)?;
            }
        }
        Ok(true)
    }
}

/// PostScript charstring operator.
///
/// See <https://learn.microsoft.com/en-us/typography/opentype/spec/cff2charstr#appendix-a-cff2-charstring-command-codes>
// TODO: This is currently missing legacy math and logical operators.
// fonttools doesn't even implement these: <https://github.com/fonttools/fonttools/blob/65598197c8afd415781f6667a7fb647c2c987fff/Lib/fontTools/misc/psCharStrings.py#L409>
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum Operator {
    HStem,
    VStem,
    VMoveTo,
    RLineTo,
    HLineTo,
    VLineTo,
    RrCurveTo,
    CallSubr,
    Return,
    EndChar,
    VariationStoreIndex,
    Blend,
    HStemHm,
    HintMask,
    CntrMask,
    RMoveTo,
    HMoveTo,
    VStemHm,
    RCurveLine,
    RLineCurve,
    VvCurveTo,
    HhCurveTo,
    CallGsubr,
    VhCurveTo,
    HvCurveTo,
    HFlex,
    Flex,
    HFlex1,
    Flex1,
}

impl Operator {
    fn read(cursor: &mut Cursor, b0: u8) -> Result<Self, Error> {
        // Escape opcode for accessing two byte operators
        const ESCAPE: u8 = 12;
        let (opcode, operator) = if b0 == ESCAPE {
            let b1 = cursor.read::<u8>()?;
            (b1, Self::from_two_byte_opcode(b1))
        } else {
            (b0, Self::from_opcode(b0))
        };
        operator.ok_or(Error::InvalidCharstringOperator(opcode))
    }

    /// Creates an operator from the given opcode.
    fn from_opcode(opcode: u8) -> Option<Self> {
        use Operator::*;
        Some(match opcode {
            1 => HStem,
            3 => VStem,
            4 => VMoveTo,
            5 => RLineTo,
            6 => HLineTo,
            7 => VLineTo,
            8 => RrCurveTo,
            10 => CallSubr,
            11 => Return,
            14 => EndChar,
            15 => VariationStoreIndex,
            16 => Blend,
            18 => HStemHm,
            19 => HintMask,
            20 => CntrMask,
            21 => RMoveTo,
            22 => HMoveTo,
            23 => VStemHm,
            24 => RCurveLine,
            25 => RLineCurve,
            26 => VvCurveTo,
            27 => HhCurveTo,
            29 => CallGsubr,
            30 => VhCurveTo,
            31 => HvCurveTo,
            _ => return None,
        })
    }

    /// Creates an operator from the given extended opcode.
    ///
    /// These are preceded by a byte containing the escape value of 12.
    pub fn from_two_byte_opcode(opcode: u8) -> Option<Self> {
        use Operator::*;
        Some(match opcode {
            34 => HFlex,
            35 => Flex,
            36 => HFlex1,
            37 => Flex1,
            _ => return None,
        })
    }
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
