//! The [glyf (Glyph Data)](https://docs.microsoft.com/en-us/typography/opentype/spec/glyf) table

use std::fmt;
use types::{F26Dot6, PathSink, Point};

include!("../../generated/generated_glyf.rs");

macro_rules! field_getter {
    ($field:ident, $ty:ty) => {
        pub fn $field(&self) -> $ty {
            match self {
                Self::Simple(table) => table.$field(),
                Self::Composite(table) => table.$field(),
            }
        }
    };
}

impl<'a> Glyph<'a> {
    field_getter!(number_of_contours, i16);
    field_getter!(x_min, i16);
    field_getter!(x_max, i16);
    field_getter!(y_min, i16);
    field_getter!(y_max, i16);
}

//NOTE: This code below was taken from an old implementation, and has a bunch
// of funny warts. It should be replaced at some point, but might be useful in
// the interim?

impl<'a> SimpleGlyph<'a> {
    /// Returns the total number of points.
    pub fn num_points(&self) -> usize {
        self.end_pts_of_contours()
            .last()
            .map(|last| last.get() as usize + 1)
            .unwrap_or(0)
    }

    /// Reads points and flags into the provided buffers.
    ///
    /// Drops all flag bits except on-curve. The lengths of the buffers must be
    /// equal to the value returned by [num_points](Self::num_points).
    ///
    /// ## Performance
    ///
    /// As the name implies, this is faster than using the iterator returned by
    /// [points](Self::points) so should be used when it is possible to
    /// preallocate buffers.
    pub fn read_points_fast(
        &self,
        points: &mut [Point<i32>],
        flags: &mut [u8],
    ) -> Result<(), ReadError> {
        let n_points = self.num_points();
        if points.len() != n_points || flags.len() != n_points {
            return Err(ReadError::InvalidArrayLen);
        }
        let mut cursor = FontData::new(self.glyph_data()).cursor();
        let mut i = 0;
        while i < n_points {
            let flag = cursor.read::<SimpleGlyphFlags>()?;
            let flag_bits = flag.bits();
            if flag.contains(SimpleGlyphFlags::REPEAT_FLAG) {
                let count = (cursor.read::<u8>()? as usize + 1).min(n_points - i);
                for f in &mut flags[i..i + count] {
                    *f = flag_bits;
                }
                i += count;
            } else {
                flags[i] = flag_bits;
                i += 1;
            }
        }
        let mut x = 0i32;
        for (&flag_bits, point) in flags.iter().zip(points.as_mut()) {
            let mut delta = 0i32;
            let flag = SimpleGlyphFlags::from_bits_truncate(flag_bits);
            if flag.contains(SimpleGlyphFlags::X_SHORT_VECTOR) {
                delta = cursor.read::<u8>()? as i32;
                if !flag.contains(SimpleGlyphFlags::X_IS_SAME_OR_POSITIVE_X_SHORT_VECTOR) {
                    delta = -delta;
                }
            } else if !flag.contains(SimpleGlyphFlags::X_IS_SAME_OR_POSITIVE_X_SHORT_VECTOR) {
                delta = cursor.read::<i16>()? as i32;
            }
            x = x.wrapping_add(delta);
            point.x = x;
        }
        let mut y = 0i32;
        for (flag_bits, point) in flags.iter_mut().zip(points.as_mut()) {
            let mut delta = 0i32;
            let flag = SimpleGlyphFlags::from_bits_truncate(*flag_bits);
            if flag.contains(SimpleGlyphFlags::Y_SHORT_VECTOR) {
                delta = cursor.read::<u8>()? as i32;
                if !flag.contains(SimpleGlyphFlags::Y_IS_SAME_OR_POSITIVE_Y_SHORT_VECTOR) {
                    delta = -delta;
                }
            } else if !flag.contains(SimpleGlyphFlags::Y_IS_SAME_OR_POSITIVE_Y_SHORT_VECTOR) {
                delta = cursor.read::<i16>()? as i32;
            }
            y = y.wrapping_add(delta);
            point.y = y;
            // Only keep the on-curve bit
            *flag_bits &= 1;
        }
        Ok(())
    }

    /// Returns an iterator over the points in the glyph.
    ///
    /// ## Performance
    ///
    /// This is slower than [read_points_fast](Self::read_points_fast) but
    /// provides access to the points without requiring a preallocated buffer.
    pub fn points(&self) -> impl Iterator<Item = CurvePoint> + 'a + Clone {
        self.points_impl()
            .unwrap_or_else(|| PointIter::new(&[], &[], &[]))
    }

    fn points_impl(&self) -> Option<PointIter<'a>> {
        let end_points = self.end_pts_of_contours();
        let n_points = end_points.last()?.get().checked_add(1)?;
        let data = self.glyph_data();
        let lens = resolve_coords_len(data, n_points).ok()?;
        let total_len = lens.flags + lens.x_coords + lens.y_coords;
        if data.len() < total_len as usize {
            return None;
        }

        let (flags, data) = data.split_at(lens.flags as usize);
        let (x_coords, y_coords) = data.split_at(lens.x_coords as usize);

        Some(PointIter::new(flags, x_coords, y_coords))
    }
}

/// Point with an associated on-curve flag in a simple glyph.
///
/// This type is a simpler representation of the data in the blob.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CurvePoint {
    /// X cooordinate.
    pub x: i16,
    /// Y cooordinate.
    pub y: i16,
    /// True if this is an on-curve point.
    pub on_curve: bool,
}

impl CurvePoint {
    /// Construct a new `CurvePoint`
    pub fn new(x: i16, y: i16, on_curve: bool) -> Self {
        Self { x, y, on_curve }
    }

    /// Convenience method to construct an on-curve point
    pub fn on_curve(x: i16, y: i16) -> Self {
        Self::new(x, y, true)
    }

    /// Convenience method to construct an off-curve point
    pub fn off_curve(x: i16, y: i16) -> Self {
        Self::new(x, y, false)
    }
}

#[derive(Clone)]
struct PointIter<'a> {
    flags: Cursor<'a>,
    x_coords: Cursor<'a>,
    y_coords: Cursor<'a>,
    flag_repeats: u8,
    cur_flags: SimpleGlyphFlags,
    cur_x: i16,
    cur_y: i16,
}

impl<'a> Iterator for PointIter<'a> {
    type Item = CurvePoint;
    fn next(&mut self) -> Option<Self::Item> {
        self.advance_flags()?;
        self.advance_points();
        let is_on_curve = self.cur_flags.contains(SimpleGlyphFlags::ON_CURVE_POINT);
        Some(CurvePoint::new(self.cur_x, self.cur_y, is_on_curve))
    }
}

impl<'a> PointIter<'a> {
    fn new(flags: &'a [u8], x_coords: &'a [u8], y_coords: &'a [u8]) -> Self {
        Self {
            flags: FontData::new(flags).cursor(),
            x_coords: FontData::new(x_coords).cursor(),
            y_coords: FontData::new(y_coords).cursor(),
            flag_repeats: 0,
            cur_flags: SimpleGlyphFlags::empty(),
            cur_x: 0,
            cur_y: 0,
        }
    }

    fn advance_flags(&mut self) -> Option<()> {
        if self.flag_repeats == 0 {
            self.cur_flags = SimpleGlyphFlags::from_bits_truncate(self.flags.read().ok()?);
            self.flag_repeats = self
                .cur_flags
                .contains(SimpleGlyphFlags::REPEAT_FLAG)
                .then(|| self.flags.read().ok())
                .flatten()
                .unwrap_or(0)
                + 1;
        }
        self.flag_repeats -= 1;
        Some(())
    }

    fn advance_points(&mut self) {
        let x_short = self.cur_flags.contains(SimpleGlyphFlags::X_SHORT_VECTOR);
        let x_same_or_pos = self
            .cur_flags
            .contains(SimpleGlyphFlags::X_IS_SAME_OR_POSITIVE_X_SHORT_VECTOR);
        let y_short = self.cur_flags.contains(SimpleGlyphFlags::Y_SHORT_VECTOR);
        let y_same_or_pos = self
            .cur_flags
            .contains(SimpleGlyphFlags::Y_IS_SAME_OR_POSITIVE_Y_SHORT_VECTOR);

        let delta_x = match (x_short, x_same_or_pos) {
            (true, false) => -(self.x_coords.read::<u8>().unwrap_or(0) as i16),
            (true, true) => self.x_coords.read::<u8>().unwrap_or(0) as i16,
            (false, false) => self.x_coords.read::<i16>().unwrap_or(0),
            _ => 0,
        };

        let delta_y = match (y_short, y_same_or_pos) {
            (true, false) => -(self.y_coords.read::<u8>().unwrap_or(0) as i16),
            (true, true) => self.y_coords.read::<u8>().unwrap_or(0) as i16,
            (false, false) => self.y_coords.read::<i16>().unwrap_or(0),
            _ => 0,
        };

        self.cur_x = self.cur_x.wrapping_add(delta_x);
        self.cur_y = self.cur_y.wrapping_add(delta_y);
    }
}

//taken from ttf_parser https://docs.rs/ttf-parser/latest/src/ttf_parser/tables/glyf.rs.html#1-677
/// Resolves coordinate arrays length.
///
/// The length depends on *Simple Glyph Flags*, so we have to process them all to find it.
fn resolve_coords_len(data: &[u8], points_total: u16) -> Result<FieldLengths, ReadError> {
    let mut cursor = FontData::new(data).cursor();
    let mut flags_left = u32::from(points_total);
    //let mut repeats;
    let mut x_coords_len = 0;
    let mut y_coords_len = 0;
    //let mut flags_seen = 0;
    while flags_left > 0 {
        let flags: SimpleGlyphFlags = cursor.read()?;

        // The number of times a glyph point repeats.
        let repeats = if flags.contains(SimpleGlyphFlags::REPEAT_FLAG) {
            let repeats: u8 = cursor.read()?;
            u32::from(repeats) + 1
        } else {
            1
        };

        if repeats > flags_left {
            return Err(ReadError::MalformedData("repeat count too large in glyf"));
        }

        // Non-obfuscated code below.
        // Branchless version is surprisingly faster.
        //
        // if flags.x_short() {
        //     // Coordinate is 1 byte long.
        //     x_coords_len += repeats;
        // } else if !flags.x_is_same_or_positive_short() {
        //     // Coordinate is 2 bytes long.
        //     x_coords_len += repeats * 2;
        // }
        // if flags.y_short() {
        //     // Coordinate is 1 byte long.
        //     y_coords_len += repeats;
        // } else if !flags.y_is_same_or_positive_short() {
        //     // Coordinate is 2 bytes long.
        //     y_coords_len += repeats * 2;
        // }
        let x_short = SimpleGlyphFlags::X_SHORT_VECTOR;
        let x_long = SimpleGlyphFlags::X_SHORT_VECTOR
            | SimpleGlyphFlags::X_IS_SAME_OR_POSITIVE_X_SHORT_VECTOR;
        let y_short = SimpleGlyphFlags::Y_SHORT_VECTOR;
        let y_long = SimpleGlyphFlags::Y_SHORT_VECTOR
            | SimpleGlyphFlags::Y_IS_SAME_OR_POSITIVE_Y_SHORT_VECTOR;
        x_coords_len += ((flags & x_short).bits() != 0) as u32 * repeats;
        x_coords_len += ((flags & x_long).bits() == 0) as u32 * repeats * 2;

        y_coords_len += ((flags & y_short).bits() != 0) as u32 * repeats;
        y_coords_len += ((flags & y_long).bits() == 0) as u32 * repeats * 2;

        flags_left -= repeats;
    }

    Ok(FieldLengths {
        flags: cursor.position()? as u32,
        x_coords: x_coords_len,
        y_coords: y_coords_len,
    })
    //Some((flags_len, x_coords_len, y_coords_len))
}

struct FieldLengths {
    flags: u32,
    x_coords: u32,
    y_coords: u32,
}

/// Transform for a composite component.
#[derive(Clone, Debug)]
pub struct Transform {
    /// X scale factor.
    pub xx: F2Dot14,
    /// YX skew factor.
    pub yx: F2Dot14,
    /// XY skew factor.
    pub xy: F2Dot14,
    /// Y scale factor.
    pub yy: F2Dot14,
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            xx: F2Dot14::from_f32(1.0),
            yx: F2Dot14::from_f32(0.0),
            xy: F2Dot14::from_f32(0.0),
            yy: F2Dot14::from_f32(1.0),
        }
    }
}

/// A reference to another glyph. Part of [CompositeGlyph].
pub struct Component {
    /// Component flags.
    pub flags: CompositeGlyphFlags,
    /// Glyph identifier.
    pub glyph: GlyphId,
    /// Anchor for component placement.
    pub anchor: Anchor,
    /// Component transformation matrix.
    pub transform: Transform,
}

/// Anchor position for a composite component.
#[derive(Debug, Clone, Copy)]
pub enum Anchor {
    Offset { x: i16, y: i16 },
    Point { base: u16, component: u16 },
}

impl<'a> CompositeGlyph<'a> {
    /// Returns an iterator over the components of the composite glyph.
    pub fn components(&self) -> impl Iterator<Item = Component> + 'a + Clone {
        ComponentIter {
            cur_flags: CompositeGlyphFlags::empty(),
            done: false,
            cursor: FontData::new(self.component_data()).cursor(),
        }
    }

    /// Returns the TrueType interpreter instructions.
    pub fn instructions(&self) -> Option<&'a [u8]> {
        ComponentIter {
            cur_flags: CompositeGlyphFlags::empty(),
            done: false,
            cursor: FontData::new(self.component_data()).cursor(),
        }
        .instructions()
    }
}

#[derive(Clone)]
struct ComponentIter<'a> {
    cur_flags: CompositeGlyphFlags,
    done: bool,
    cursor: Cursor<'a>,
}

impl<'a> ComponentIter<'a> {
    fn instructions(&mut self) -> Option<&'a [u8]> {
        while self.by_ref().next().is_some() {}
        if self
            .cur_flags
            .contains(CompositeGlyphFlags::WE_HAVE_INSTRUCTIONS)
        {
            let len = self.cursor.read::<u16>().ok()? as usize;
            self.cursor.read_array(len).ok()
        } else {
            None
        }
    }
}

impl Iterator for ComponentIter<'_> {
    type Item = Component;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        let flags: CompositeGlyphFlags = self.cursor.read().ok()?;
        self.cur_flags = flags;
        let glyph = self.cursor.read::<GlyphId>().ok()?;
        let args_are_words = flags.contains(CompositeGlyphFlags::ARG_1_AND_2_ARE_WORDS);
        let args_are_xy_values = flags.contains(CompositeGlyphFlags::ARGS_ARE_XY_VALUES);
        let anchor = match (args_are_xy_values, args_are_words) {
            (true, true) => Anchor::Offset {
                x: self.cursor.read().ok()?,
                y: self.cursor.read().ok()?,
            },
            (true, false) => Anchor::Offset {
                x: self.cursor.read::<i8>().ok()? as _,
                y: self.cursor.read::<i8>().ok()? as _,
            },
            (false, true) => Anchor::Point {
                base: self.cursor.read().ok()?,
                component: self.cursor.read().ok()?,
            },
            (false, false) => Anchor::Point {
                base: self.cursor.read::<u8>().ok()? as _,
                component: self.cursor.read::<u8>().ok()? as _,
            },
        };
        let mut transform = Transform::default();
        if flags.contains(CompositeGlyphFlags::WE_HAVE_A_SCALE) {
            transform.xx = self.cursor.read().ok()?;
            transform.yy = transform.xx;
        } else if flags.contains(CompositeGlyphFlags::WE_HAVE_AN_X_AND_Y_SCALE) {
            transform.xx = self.cursor.read().ok()?;
            transform.yy = self.cursor.read().ok()?;
        } else if flags.contains(CompositeGlyphFlags::WE_HAVE_A_TWO_BY_TWO) {
            transform.xx = self.cursor.read().ok()?;
            transform.yx = self.cursor.read().ok()?;
            transform.xy = self.cursor.read().ok()?;
            transform.yy = self.cursor.read().ok()?;
        }
        self.done = !flags.contains(CompositeGlyphFlags::MORE_COMPONENTS);

        Some(Component {
            flags,
            glyph,
            anchor,
            transform,
        })
    }
}

#[cfg(feature = "traversal")]
impl<'a> SomeTable<'a> for Component {
    fn type_name(&self) -> &str {
        "Component"
    }

    fn get_field(&self, idx: usize) -> Option<Field<'a>> {
        match idx {
            0 => Some(Field::new("flags", self.flags.bits())),
            1 => Some(Field::new("glyph", self.glyph)),
            2 => match self.anchor {
                Anchor::Point { base, .. } => Some(Field::new("base", base)),
                Anchor::Offset { x, .. } => Some(Field::new("x", x)),
            },
            3 => match self.anchor {
                Anchor::Point { component, .. } => Some(Field::new("component", component)),
                Anchor::Offset { y, .. } => Some(Field::new("y", y)),
            },
            _ => None,
        }
    }
}

/// Errors that can occur when converting an outline to a path.
#[derive(Clone, Debug)]
pub enum ToPathError {
    /// Contour end point at this index was less than its preceding end point.
    ContourOrder(usize),
    /// Expected a quadratic off-curve point at this index.
    ExpectedQuad(usize),
    /// Expected a quadratic off-curve or on-curve point at this index.
    ExpectedQuadOrOnCurve(usize),
    /// Expected a cubic off-curve point at this index.
    ExpectedCubic(usize),
}

impl fmt::Display for ToPathError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::ContourOrder(ix) => write!(
                f,
                "Contour end point at index {ix} was less than preceeding end point"
            ),
            Self::ExpectedQuad(ix) => write!(f, "Expected quadatic off-curve point at index {ix}"),
            Self::ExpectedQuadOrOnCurve(ix) => write!(
                f,
                "Expected quadatic off-curve or on-curve point at index {ix}"
            ),
            Self::ExpectedCubic(ix) => write!(f, "Expected cubic off-curve point at index {ix}"),
        }
    }
}

/// Converts a `glyf` outline described by points, flags and contour end points to a sequence of
/// path elements and and invokes the appropriate callback on the given sink for each.
///
/// The input points are expected in `F26Dot6` format as that is the standard result of scaling
/// a TrueType glyph. Output points are generated in `f32`.
pub fn to_path(
    points: &[Point<F26Dot6>],
    flags: &[u8],
    contours: &[u16],
    sink: &mut impl PathSink<f32>,
) -> Result<(), ToPathError> {
    fn to_f32(x: F26Dot6) -> f32 {
        x.to_f64() as f32
    }
    const FLAG_MASK: u8 = 0x3;
    const QUAD: u8 = 0x0;
    const ON: u8 = 0x1;
    const CUBIC: u8 = 0x2;
    const TWO: F26Dot6 = F26Dot6::from_i32(2);
    let mut count = 0usize;
    let mut last_was_close = false;
    for contour_ix in 0..contours.len() {
        let mut cur_ix = if contour_ix > 0 {
            contours[contour_ix - 1] as usize + 1
        } else {
            0
        };
        let mut last_ix = contours[contour_ix] as usize;
        if last_ix < cur_ix || last_ix >= points.len() {
            return Err(ToPathError::ContourOrder(contour_ix));
        }
        let mut v_start = points[cur_ix];
        let v_last = v_start;
        let mut flag = flags[cur_ix] & FLAG_MASK;
        if flag == CUBIC {
            return Err(ToPathError::ExpectedQuadOrOnCurve(cur_ix));
        }
        let mut step_point = true;
        if flag == QUAD {
            if flags[last_ix] & FLAG_MASK == ON {
                v_start = v_last;
                last_ix -= 1;
            } else {
                v_start = (v_start + v_last) / TWO;
            }
            step_point = false;
        }
        let p = v_start.map(to_f32);
        if count > 0 && !last_was_close {
            sink.close();
        }
        sink.move_to(p.x, p.y);
        count += 1;
        last_was_close = false;
        while cur_ix < last_ix {
            if step_point {
                cur_ix += 1;
            }
            step_point = true;
            flag = flags[cur_ix] & FLAG_MASK;
            match flag {
                ON => {
                    let p = points[cur_ix].map(to_f32);
                    sink.line_to(p.x, p.y);
                    count += 1;
                    last_was_close = false;
                    continue;
                }
                QUAD => {
                    let mut do_close_quad = true;
                    let mut v_control = points[cur_ix];
                    while cur_ix < last_ix {
                        cur_ix += 1;
                        let cur_point = points[cur_ix];
                        flag = flags[cur_ix] & FLAG_MASK;
                        if flag == ON {
                            let control = v_control.map(to_f32);
                            let point = cur_point.map(to_f32);
                            sink.quad_to(control.x, control.y, point.x, point.y);
                            count += 1;
                            last_was_close = false;
                            do_close_quad = false;
                            break;
                        }
                        if flag != QUAD {
                            return Err(ToPathError::ExpectedQuad(cur_ix));
                        }
                        let v_middle = (v_control + cur_point) / TWO;
                        let control = v_control.map(to_f32);
                        let point = v_middle.map(to_f32);
                        sink.quad_to(control.x, control.y, point.x, point.y);
                        count += 1;
                        last_was_close = false;
                        v_control = cur_point;
                    }
                    if do_close_quad {
                        let control = v_control.map(to_f32);
                        let point = v_start.map(to_f32);
                        sink.quad_to(control.x, control.y, point.x, point.y);
                        count += 1;
                        last_was_close = false;
                        break;
                    }
                    continue;
                }
                _ => {
                    if cur_ix + 1 > last_ix || (flags[cur_ix + 1] & FLAG_MASK != CUBIC) {
                        return Err(ToPathError::ExpectedCubic(cur_ix + 1));
                    }
                    let control0 = points[cur_ix].map(to_f32);
                    let control1 = points[cur_ix + 1].map(to_f32);
                    cur_ix += 2;
                    if cur_ix <= last_ix {
                        let point = points[cur_ix].map(to_f32);
                        sink.curve_to(
                            control0.x, control0.y, control1.x, control1.y, point.x, point.y,
                        );
                        count += 1;
                        last_was_close = false;
                        continue;
                    }
                    let point = v_start.map(to_f32);
                    sink.curve_to(
                        control0.x, control0.y, control1.x, control1.y, point.x, point.y,
                    );
                    count += 1;
                    last_was_close = false;
                    break;
                }
            }
        }
        if count > 0 && !last_was_close {
            sink.close();
            last_was_close = true;
        }
    }
    Ok(())
}

//NOTE: we want generated_glyf traversal to include this:
//7usize => {
//let this = self.sneaky_copy();
//Some(Field::new(
//"components",
//FieldType::offset_iter(move || {
//Box::new(
//this.iter_components()
//.map(|item| FieldType::ResolvedOffset(Ok(Box::new(item)))),
//) as Box<dyn Iterator<Item = FieldType<'a>> + 'a>
//}),
//))
//}

#[cfg(test)]
mod tests {
    use super::Glyph;
    use crate::test_data;
    use crate::{FontRef, GlyphId, TableProvider};

    #[test]
    fn simple_glyph() {
        let font = FontRef::new(test_data::test_fonts::COLR_GRADIENT_RECT).unwrap();
        let loca = font.loca(None).unwrap();
        let glyf = font.glyf().unwrap();
        let glyph = loca.get_glyf(GlyphId::new(0), &glyf).unwrap().unwrap();
        assert_eq!(glyph.number_of_contours(), 2);
        let simple_glyph = if let Glyph::Simple(simple) = glyph {
            simple
        } else {
            panic!("expected simple glyph");
        };
        assert_eq!(
            simple_glyph
                .end_pts_of_contours()
                .iter()
                .map(|x| x.get())
                .collect::<Vec<_>>(),
            &[3, 7]
        );
        assert_eq!(
            simple_glyph
                .points()
                .map(|pt| (pt.x, pt.y, pt.on_curve))
                .collect::<Vec<_>>(),
            &[
                (5, 0, true),
                (5, 100, true),
                (45, 100, true),
                (45, 0, true),
                (10, 5, true),
                (40, 5, true),
                (40, 95, true),
                (10, 95, true),
            ]
        );
    }
}
