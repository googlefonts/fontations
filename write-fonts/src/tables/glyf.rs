//! The [glyf (Glyph Data)](https://docs.microsoft.com/en-us/typography/opentype/spec/glyf) table

use crate::OtRound;
use kurbo::{BezPath, Rect, Shape};

use read_fonts::{
    tables::glyf::{Anchor, CompositeGlyphFlags, CurvePoint, SimpleGlyphFlags, Transform},
    types::GlyphId,
};

use crate::{
    from_obj::{FromObjRef, FromTableRef},
    util::MultiZip,
    FontWrite,
};

/// A single contour, comprising only line and quadratic bezier segments
#[derive(Clone, Debug, Default)]
pub struct Contour(Vec<CurvePoint>);

/// A Bounding box.
///
/// This should be the minimum rectangle which fully encloses the glyph outline;
/// importantly this can only be determined by computing the individual Bezier
/// segments, and cannot be determiend from points alone.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Bbox {
    pub x_min: i16,
    pub y_min: i16,
    pub x_max: i16,
    pub y_max: i16,
}

/// A simple (without components) glyph
#[derive(Clone, Debug)]
pub struct SimpleGlyph {
    pub bbox: Bbox,
    contours: Vec<Contour>,
    _instructions: Vec<u8>,
}

/// A glyph consisting of multiple component sub-glyphs
#[derive(Clone, Debug)]
pub struct CompositeGlyph {
    pub bbox: Bbox,
    components: Vec<Component>,
    _instructions: Vec<u8>,
}

/// A single component glyph (part of a [`CompositeGlyph`]).
#[derive(Clone, Debug)]
pub struct Component {
    pub glyph: GlyphId,
    pub anchor: Anchor,
    pub flags: ComponentFlags,
    pub transform: Transform,
}

/// Options that can be manually set for a given component.
///
/// This provides an easier interface for setting those flags that are not
/// calculated based on other properties of the glyph. For more information
/// on these flags, see [Component Glyph Flags](flags-spec) in the spec.
///
/// These eventually are combined with calculated flags into the
/// [`CompositeGlyphFlags`] bitset.
///
/// [flags-spec]: https://learn.microsoft.com/en-us/typography/opentype/spec/glyf#compositeGlyphFlags
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ComponentFlags {
    /// Round xy values to the nearest grid line
    pub round_xy_to_grid: bool,
    /// Use the advance/lsb/rsb values of this component for the whole
    /// composite glyph
    pub use_my_metrics: bool,
    /// The composite should have this component's offset scaled
    pub scaled_component_offset: bool,
    /// The composite should *not* have this component's offset scaled
    pub unscaled_component_offset: bool,
    /// If set, the components of the composite glyph overlap.
    pub overlap_compound: bool,
}

/// An error if an input curve is malformed
#[derive(Clone, Debug)]
pub enum BadKurbo {
    HasCubic,
    TooSmall,
    MissingMove,
    UnequalNumberOfElements(Vec<usize>),
    InconsistentPathElements(usize, Vec<&'static str>),
}

/// Point with an associated on-curve flag.
///
/// Similar to read_fonts::tables::glyf::CurvePoint, but uses kurbo::Point directly
/// thus it does not require (x, y) coordinates to be rounded to integers.
#[derive(Clone, Copy, Debug, PartialEq)]
struct ContourPoint {
    point: kurbo::Point,
    on_curve: bool,
}

impl ContourPoint {
    fn new(point: kurbo::Point, on_curve: bool) -> Self {
        Self { point, on_curve }
    }

    fn on_curve(point: kurbo::Point) -> Self {
        Self::new(point, true)
    }

    fn off_curve(point: kurbo::Point) -> Self {
        Self::new(point, false)
    }

    fn distance(&self, other: &Self) -> f64 {
        self.point.distance(other.point)
    }
}

impl From<ContourPoint> for CurvePoint {
    fn from(pt: ContourPoint) -> Self {
        let (x, y) = pt.point.ot_round();
        CurvePoint::new(x, y, pt.on_curve)
    }
}

/// A helper struct for building interpolatable contours
///
/// Holds a vec of contour, one contour per glyph.
#[derive(Clone, Debug, PartialEq)]
struct InterpolatableContourBuilder(Vec<Vec<ContourPoint>>);

impl InterpolatableContourBuilder {
    /// Create new set of interpolatable contours beginning at the provided points
    fn new(move_pts: &[kurbo::Point]) -> Self {
        assert!(!move_pts.is_empty());
        Self(
            move_pts
                .iter()
                .map(|pt| vec![ContourPoint::on_curve(*pt)])
                .collect(),
        )
    }

    /// Number of interpolatable contours (one per glyph)
    fn len(&self) -> usize {
        self.0.len()
    }

    /// Add a line segment to all contours
    fn line_to(&mut self, pts: &[kurbo::Point]) {
        assert_eq!(pts.len(), self.len());
        for (i, pt) in pts.iter().enumerate() {
            self.0[i].push(ContourPoint::on_curve(*pt));
        }
    }

    /// Add a quadratic curve segment to all contours
    fn quad_to(&mut self, pts: &[(kurbo::Point, kurbo::Point)]) {
        for (i, (p0, p1)) in pts.iter().enumerate() {
            self.0[i].push(ContourPoint::off_curve(*p0));
            self.0[i].push(ContourPoint::on_curve(*p1));
        }
    }

    /// The total number of points in each interpolatable contour
    fn num_points(&self) -> usize {
        let n = self.0[0].len();
        assert!(self.0.iter().all(|c| c.len() == n));
        n
    }

    /// The first point in each contour
    fn first(&self) -> impl Iterator<Item = &ContourPoint> {
        self.0.iter().map(|v| v.first().unwrap())
    }

    /// The last point in each contour
    fn last(&self) -> impl Iterator<Item = &ContourPoint> {
        self.0.iter().map(|v| v.last().unwrap())
    }

    /// Remove the last point from each contour
    fn remove_last(&mut self) {
        self.0.iter_mut().for_each(|c| {
            c.pop().unwrap();
        });
    }

    fn is_implicit_on_curve(&self, idx: usize) -> bool {
        self.0
            .iter()
            .all(|points| is_implicit_on_curve(points, idx))
    }

    /// Build the contours, dropping any on-curve points that can be implied in all contours
    fn build(self) -> Vec<Contour> {
        let num_contours = self.len();
        let num_points = self.num_points();
        let mut contours = vec![Contour::default(); num_contours];
        contours.iter_mut().for_each(|c| c.0.reserve(num_points));
        for point_idx in (0..num_points).filter(|point_idx| !self.is_implicit_on_curve(*point_idx))
        {
            for (contour_idx, contour) in contours.iter_mut().enumerate() {
                contour
                    .0
                    .push(CurvePoint::from(self.0[contour_idx][point_idx]));
            }
        }
        contours
    }
}

enum Sibling {
    Prev,
    Next,
}

/// Read the adjacent (prev/next) point.
///
/// offset is presumed +/- 1 to signify direction.
/// idx is presumed valid.
/// value + offset is presumed to fit both isize and usize; # points tends to be small.
fn wrapping_read_sibling(points: &[ContourPoint], idx: usize, sibling: Sibling) -> ContourPoint {
    let max_valid_idx = points.len() - 1;
    points[match (idx, sibling) {
        (_, Sibling::Next) if idx == max_valid_idx => 0,
        (_, Sibling::Prev) if idx == 0 => max_valid_idx,
        (_, Sibling::Next) => idx + 1,
        (_, Sibling::Prev) => idx - 1,
    }]
}

fn is_implicit_on_curve(points: &[ContourPoint], idx: usize) -> bool {
    let p1 = points[idx]; // user error if this is out of bounds
    if !p1.on_curve {
        return false;
    }
    let p0 = wrapping_read_sibling(points, idx, Sibling::Prev);
    let p2 = wrapping_read_sibling(points, idx, Sibling::Next);
    if p0.on_curve || p0.on_curve != p2.on_curve {
        return false;
    }
    // if the distance between p1 and p0 is approximately the same as the distance
    // between p2 and p1, then we can drop p1
    let p1p0 = p1.distance(&p0);
    let p2p1 = p2.distance(&p1);
    // should tolerance be a parameter?
    (p1p0 - p2p1).abs() < f32::EPSILON as f64
}

#[inline]
fn el_types(elements: &[kurbo::PathEl]) -> Vec<&'static str> {
    elements
        .iter()
        .map(|el| match el {
            kurbo::PathEl::MoveTo(_) => "M",
            kurbo::PathEl::LineTo(_) => "L",
            kurbo::PathEl::QuadTo(_, _) => "Q",
            kurbo::PathEl::CurveTo(_, _, _) => "C",
            kurbo::PathEl::ClosePath => "Z",
        })
        .collect()
}

pub fn simple_glyphs_from_kurbo(paths: &[BezPath]) -> Result<Vec<SimpleGlyph>, BadKurbo> {
    // check that all paths have the same number of elements so we can zip them together
    let num_elements: Vec<usize> = paths.iter().map(|path| path.elements().len()).collect();
    if num_elements.iter().any(|n| *n != num_elements[0]) {
        return Err(BadKurbo::UnequalNumberOfElements(num_elements));
    }
    let path_iters = MultiZip::new(paths.iter().map(|path| path.iter()).collect());
    let mut contours: Vec<InterpolatableContourBuilder> = Vec::new();
    let mut current: Option<InterpolatableContourBuilder> = None;
    let num_glyphs = paths.len();
    let mut pts = Vec::with_capacity(num_glyphs);
    let mut quad_pts = Vec::with_capacity(num_glyphs);
    for (i, elements) in path_iters.enumerate() {
        // All i-th path elements are expected to have the same types.
        // elements is never empty (if it were, MultiZip would have stopped), hence the unwrap
        let first_el = elements.first().unwrap();
        match first_el {
            kurbo::PathEl::MoveTo(_) => {
                // we have a new contour, flush the current one
                if let Some(prev) = current.take() {
                    contours.push(prev);
                }
                pts.clear();
                for el in &elements {
                    match el {
                        &kurbo::PathEl::MoveTo(pt) => {
                            pts.push(pt);
                        }
                        _ => {
                            return Err(BadKurbo::InconsistentPathElements(i, el_types(&elements)))
                        }
                    }
                }
                current = Some(InterpolatableContourBuilder::new(&pts));
            }
            kurbo::PathEl::LineTo(_) => {
                pts.clear();
                for el in &elements {
                    match el {
                        &kurbo::PathEl::LineTo(pt) => {
                            pts.push(pt);
                        }
                        _ => {
                            return Err(BadKurbo::InconsistentPathElements(i, el_types(&elements)))
                        }
                    }
                }
                current.as_mut().ok_or(BadKurbo::MissingMove)?.line_to(&pts)
            }
            kurbo::PathEl::QuadTo(_, _) => {
                quad_pts.clear();
                for el in &elements {
                    match el {
                        &kurbo::PathEl::QuadTo(p0, p1) => {
                            quad_pts.push((p0, p1));
                        }
                        _ => {
                            return Err(BadKurbo::InconsistentPathElements(i, el_types(&elements)))
                        }
                    }
                }
                current
                    .as_mut()
                    .ok_or(BadKurbo::MissingMove)?
                    .quad_to(&quad_pts)
            }
            kurbo::PathEl::CurveTo(_, _, _) => return Err(BadKurbo::HasCubic),
            kurbo::PathEl::ClosePath => {
                let contour = current.as_mut().ok_or(BadKurbo::MissingMove)?;
                // remove last point in closed path if has same coords as the move point
                // matches FontTools handling @ https://github.com/fonttools/fonttools/blob/3b9a73ff8379ab49d3ce35aaaaf04b3a7d9d1655/Lib/fontTools/pens/pointPen.py#L321-L323
                // FontTools has an else case to support UFO glif's choice to not include 'move' for closed paths that does not apply here.
                if contour.num_points() > 1 && contour.last().eq(contour.first()) {
                    contour.remove_last();
                }
            }
        }
    }
    contours.extend(current);

    let mut glyph_contours = vec![Vec::new(); num_glyphs];
    for builder in contours {
        assert_eq!(builder.len(), num_glyphs);
        for (i, contour) in builder.build().into_iter().enumerate() {
            glyph_contours[i].push(contour);
        }
    }

    let mut glyphs = Vec::new();
    for (contours, path) in glyph_contours.into_iter().zip(paths.iter()) {
        let bbox = path.bounding_box();
        glyphs.push(SimpleGlyph {
            bbox: bbox.into(),
            contours,
            _instructions: Default::default(),
        })
    }

    Ok(glyphs)
}

impl Contour {
    /// The total number of points in this contour
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// `true` if this contour is empty
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &CurvePoint> {
        self.0.iter()
    }
}

impl SimpleGlyph {
    /// Attempt to create a simple glyph from a kurbo `BezPath`
    ///
    /// The path may contain only line and quadratic bezier segments. The caller
    /// is responsible for converting any cubic segments to quadratics before
    /// calling.
    ///
    /// Returns an error if the input path is malformed; that is, if it is empty,
    /// contains cubic segments, or does not begin with a 'move' instruction.
    ///
    /// **Context**
    ///
    /// * In the glyf table simple (contour based) glyph paths implicitly close when rendering.
    /// * In font sources, and svg, open and closed paths are distinct.
    ///    * In SVG closure matters due to influence on strokes, <https://www.w3.org/TR/SVG11/paths.html#PathDataClosePathCommand>.
    /// * An explicit closePath joins the first/last points of a contour
    ///    * This is not the same as ending with some other drawing command whose endpoint is the contour startpoint
    /// * In FontTools endPath says I'm done with this subpath, [BezPath] has no endPath.
    ///
    /// Context courtesy of @anthrotype.
    pub fn from_kurbo(path: &BezPath) -> Result<Self, BadKurbo> {
        Ok(simple_glyphs_from_kurbo(std::slice::from_ref(path))?
            .pop()
            .unwrap())
    }

    /// Compute the flags and deltas for this glyph's points.
    ///
    /// This does not do the final binary encoding, and it also does not handle
    /// repeating flags, which doesn't really work when we're an iterator.
    ///
    // this is adapted from simon's implementation at
    // https://github.com/simoncozens/rust-font-tools/blob/105436d3a617ddbebd25f790b041ff506bd90d44/fonttools-rs/src/tables/glyf/glyph.rs#L268
    fn compute_point_deltas(
        &self,
    ) -> impl Iterator<Item = (SimpleGlyphFlags, CoordDelta, CoordDelta)> + '_ {
        // reused for x & y by passing in the flags
        fn flag_and_delta(
            value: i16,
            short_flag: SimpleGlyphFlags,
            same_or_pos: SimpleGlyphFlags,
        ) -> (SimpleGlyphFlags, CoordDelta) {
            const SHORT_MAX: i16 = u8::MAX as i16;
            const SHORT_MIN: i16 = -SHORT_MAX;
            match value {
                0 => (same_or_pos, CoordDelta::Skip),
                SHORT_MIN..=-1 => (short_flag, CoordDelta::Short(value.unsigned_abs() as u8)),
                1..=SHORT_MAX => (short_flag | same_or_pos, CoordDelta::Short(value as _)),
                _other => (SimpleGlyphFlags::empty(), CoordDelta::Long(value)),
            }
        }

        let (mut last_x, mut last_y) = (0, 0);
        let mut iter = self.contours.iter().flat_map(|c| c.iter());
        std::iter::from_fn(move || {
            let point = iter.next()?;
            let mut flag = SimpleGlyphFlags::empty();
            let d_x = point.x - last_x;
            let d_y = point.y - last_y;
            last_x = point.x;
            last_y = point.y;

            if point.on_curve {
                flag |= SimpleGlyphFlags::ON_CURVE_POINT;
            }
            let (x_flag, x_data) = flag_and_delta(
                d_x,
                SimpleGlyphFlags::X_SHORT_VECTOR,
                SimpleGlyphFlags::X_IS_SAME_OR_POSITIVE_X_SHORT_VECTOR,
            );
            let (y_flag, y_data) = flag_and_delta(
                d_y,
                SimpleGlyphFlags::Y_SHORT_VECTOR,
                SimpleGlyphFlags::Y_IS_SAME_OR_POSITIVE_Y_SHORT_VECTOR,
            );

            flag |= x_flag | y_flag;
            Some((flag, x_data, y_data))
        })
    }

    pub fn contours(&self) -> &[Contour] {
        &self.contours
    }
}

impl<'a> FromObjRef<read::tables::glyf::SimpleGlyph<'a>> for SimpleGlyph {
    fn from_obj_ref(from: &read::tables::glyf::SimpleGlyph, _data: read::FontData) -> Self {
        let bbox = Bbox {
            x_min: from.x_min(),
            y_min: from.y_min(),
            x_max: from.x_max(),
            y_max: from.y_max(),
        };
        let mut points = from.points();
        let mut last_end = 0;
        let mut contours = vec![];
        for end_pt in from.end_pts_of_contours() {
            let end = end_pt.get() as usize + 1;
            let count = end - last_end;
            last_end = end;
            contours.push(Contour(points.by_ref().take(count).collect()));
        }
        Self {
            bbox,
            contours,
            _instructions: from.instructions().to_owned(),
        }
    }
}

impl<'a> FromTableRef<read::tables::glyf::SimpleGlyph<'a>> for SimpleGlyph {}

/// A little helper for managing how we're representing a given delta
#[derive(Clone, Copy, Debug)]
enum CoordDelta {
    // this is a repeat (set in the flag) and so we write nothing
    Skip,
    Short(u8),
    Long(i16),
}

impl FontWrite for CoordDelta {
    fn write_into(&self, writer: &mut crate::TableWriter) {
        match self {
            CoordDelta::Skip => (),
            CoordDelta::Short(val) => val.write_into(writer),
            CoordDelta::Long(val) => val.write_into(writer),
        }
    }
}

/// A little helper for writing flags that may have a 'repeat' byte
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RepeatableFlag {
    flag: SimpleGlyphFlags,
    repeat: u8,
}

impl FontWrite for RepeatableFlag {
    fn write_into(&self, writer: &mut crate::TableWriter) {
        debug_assert_eq!(
            self.flag.contains(SimpleGlyphFlags::REPEAT_FLAG),
            self.repeat > 0
        );

        self.flag.bits().write_into(writer);
        if self.flag.contains(SimpleGlyphFlags::REPEAT_FLAG) {
            self.repeat.write_into(writer);
        }
    }
}

impl RepeatableFlag {
    /// given an iterator over raw flags, return an iterator over flags + repeat values
    // writing this as an iterator instead of just returning a vec is very marginal
    // gains, but I'm just in the habit at this point
    fn iter_from_flags(
        flags: impl IntoIterator<Item = SimpleGlyphFlags>,
    ) -> impl Iterator<Item = RepeatableFlag> {
        let mut iter = flags.into_iter();
        let mut prev = None;
        // if a flag repeats exactly once, then there is no (space) cost difference
        // between 1) using a repeat flag followed by a value of '1' and 2) just
        // repeating the flag (without setting the repeat bit).
        // It would be simplest for us to go with option 1), but fontmake goes
        // with 2). We like doing what fontmake does, so we add an extra step
        // where if we see a case where there's a single repeat, we split it into
        // two separate non-repeating flags.
        let mut decompose_single_repeat = None;

        std::iter::from_fn(move || loop {
            if let Some(repeat) = decompose_single_repeat.take() {
                return Some(repeat);
            }

            match (iter.next(), prev.take()) {
                (None, Some(RepeatableFlag { flag, repeat })) if repeat == 1 => {
                    let flag = flag & !SimpleGlyphFlags::REPEAT_FLAG;
                    decompose_single_repeat = Some(RepeatableFlag { flag, repeat: 0 });
                    return decompose_single_repeat;
                }
                (None, prev) => return prev,
                (Some(flag), None) => prev = Some(RepeatableFlag { flag, repeat: 0 }),
                (Some(flag), Some(mut last)) => {
                    if (last.flag & !SimpleGlyphFlags::REPEAT_FLAG) == flag && last.repeat < u8::MAX
                    {
                        last.repeat += 1;
                        last.flag |= SimpleGlyphFlags::REPEAT_FLAG;
                        prev = Some(last);
                    } else {
                        // split a single repeat into two non-repeat flags
                        if last.repeat == 1 {
                            last.flag &= !SimpleGlyphFlags::REPEAT_FLAG;
                            last.repeat = 0;
                            // stash the extra flag, which we'll use at the top
                            // of the next pass of the loop
                            decompose_single_repeat = Some(last);
                        }
                        prev = Some(RepeatableFlag { flag, repeat: 0 });
                        return Some(last);
                    }
                }
            }
        })
    }
}

impl FontWrite for SimpleGlyph {
    fn write_into(&self, writer: &mut crate::TableWriter) {
        assert!(self.contours.len() < i16::MAX as usize);
        assert!(self._instructions.len() < u16::MAX as usize);
        let n_contours = self.contours.len() as i16;
        n_contours.write_into(writer);
        self.bbox.write_into(writer);
        // now write end points of contours:
        let mut cur = 0;
        for contour in &self.contours {
            cur += contour.len();
            (cur as u16 - 1).write_into(writer);
        }
        (self._instructions.len() as u16).write_into(writer);
        self._instructions.write_into(writer);

        let deltas = self.compute_point_deltas().collect::<Vec<_>>();
        RepeatableFlag::iter_from_flags(deltas.iter().map(|(flag, _, _)| *flag))
            .for_each(|flag| flag.write_into(writer));
        deltas.iter().for_each(|(_, x, _)| x.write_into(writer));
        deltas.iter().for_each(|(_, _, y)| y.write_into(writer));
        writer.pad_to_2byte_aligned();
    }
}

impl crate::validate::Validate for SimpleGlyph {
    fn validate_impl(&self, ctx: &mut crate::codegen_prelude::ValidationCtx) {
        if self._instructions.len() > u16::MAX as usize {
            ctx.report("instructions len overflows");
        }
    }
}

impl Component {
    /// Create a new component.
    pub fn new(
        glyph: GlyphId,
        anchor: Anchor,
        transform: Transform,
        flags: impl Into<ComponentFlags>,
    ) -> Self {
        Component {
            glyph,
            anchor,
            flags: flags.into(),
            transform,
        }
    }
    /// Compute the flags for this glyph, excepting `MORE_COMPONENTS` and
    /// `WE_HAVE_INSTRUCTIONS`, which must be set manually
    fn compute_flag(&self) -> CompositeGlyphFlags {
        self.anchor.compute_flags() | self.transform.compute_flags() | self.flags.into()
    }

    /// like `FontWrite` but lets us pass in the flags that must be determined
    /// externally (WE_HAVE_INSTRUCTIONS and MORE_COMPONENTS)
    fn write_into(&self, writer: &mut crate::TableWriter, extra_flags: CompositeGlyphFlags) {
        let flags = self.compute_flag() | extra_flags;
        flags.bits().write_into(writer);
        self.glyph.write_into(writer);
        self.anchor.write_into(writer);
        self.transform.write_into(writer);
    }
}

/// An error that occurs if a `CompositeGlyph` is constructed with no components.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct NoComponents;

impl std::fmt::Display for NoComponents {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "A composite glyph must contain at least one component")
    }
}

impl std::error::Error for NoComponents {}

impl CompositeGlyph {
    /// Create a new composite glyph, with the provided component.
    ///
    /// The 'bbox' argument is the bounding box of the glyph after the transform
    /// has been applied.
    ///
    /// Additional components can be added with [`add_component`][Self::add_component]
    pub fn new(component: Component, bbox: impl Into<Bbox>) -> Self {
        Self {
            bbox: bbox.into(),
            components: vec![component],
            _instructions: Default::default(),
        }
    }

    /// Add a new component to this glyph
    ///
    /// The 'bbox' argument is the bounding box of the glyph after the transform
    /// has been applied.
    pub fn add_component(&mut self, component: Component, bbox: impl Into<Bbox>) {
        self.components.push(component);
        self.bbox = self.bbox.union(bbox.into());
    }

    /// Construct a `CompositeGlyph` from an iterator of `Component` and `Bbox`es.
    ///
    /// This returns an error if the iterator is empty; a CompositeGlyph must always
    /// contain at least one component.
    pub fn try_from_iter(
        source: impl IntoIterator<Item = (Component, Bbox)>,
    ) -> Result<Self, NoComponents> {
        let mut components = Vec::new();
        let mut union_box: Option<Bbox> = None;

        for (component, bbox) in source {
            components.push(component);
            union_box.get_or_insert(bbox).union(bbox);
        }

        if components.is_empty() {
            Err(NoComponents)
        } else {
            Ok(CompositeGlyph {
                bbox: union_box.unwrap(),
                components,
                _instructions: Default::default(),
            })
        }
    }

    pub fn components(&self) -> &[Component] {
        &self.components
    }
}

impl FontWrite for CompositeGlyph {
    fn write_into(&self, writer: &mut crate::TableWriter) {
        const N_CONTOURS: i16 = -1;
        N_CONTOURS.write_into(writer);
        self.bbox.write_into(writer);
        let (last, rest) = self
            .components
            .split_last()
            .expect("empty composites checked in validation");
        for comp in rest {
            comp.write_into(writer, CompositeGlyphFlags::MORE_COMPONENTS);
        }
        let last_flags = if self._instructions.is_empty() {
            CompositeGlyphFlags::empty()
        } else {
            CompositeGlyphFlags::WE_HAVE_INSTRUCTIONS
        };
        last.write_into(writer, last_flags);

        if !self._instructions.is_empty() {
            (self._instructions.len() as u16).write_into(writer);
            self._instructions.write_into(writer);
        }
        writer.pad_to_2byte_aligned();
    }
}

impl crate::validate::Validate for CompositeGlyph {
    fn validate_impl(&self, ctx: &mut crate::codegen_prelude::ValidationCtx) {
        if self.components.is_empty() {
            ctx.report("composite glyph must have components");
        }
        if self._instructions.len() > u16::MAX as usize {
            ctx.report("instructions len overflows");
        }
    }
}

impl FontWrite for Anchor {
    fn write_into(&self, writer: &mut crate::TableWriter) {
        let two_bytes = self
            .compute_flags()
            .contains(CompositeGlyphFlags::ARG_1_AND_2_ARE_WORDS);
        match self {
            Anchor::Offset { x, y } if !two_bytes => [*x as i8, *y as i8].write_into(writer),
            Anchor::Offset { x, y } => [*x, *y].write_into(writer),
            Anchor::Point { base, component } if !two_bytes => {
                [*base as u8, *component as u8].write_into(writer)
            }
            Anchor::Point { base, component } => [*base, *component].write_into(writer),
        }
    }
}

impl FontWrite for Transform {
    fn write_into(&self, writer: &mut crate::TableWriter) {
        let flags = self.compute_flags();
        if flags.contains(CompositeGlyphFlags::WE_HAVE_A_TWO_BY_TWO) {
            [self.xx, self.yx, self.xy, self.yy].write_into(writer);
        } else if flags.contains(CompositeGlyphFlags::WE_HAVE_AN_X_AND_Y_SCALE) {
            [self.xx, self.yy].write_into(writer);
        } else if flags.contains(CompositeGlyphFlags::WE_HAVE_A_SCALE) {
            self.xx.write_into(writer)
        }
    }
}

impl From<CompositeGlyphFlags> for ComponentFlags {
    fn from(src: CompositeGlyphFlags) -> ComponentFlags {
        ComponentFlags {
            round_xy_to_grid: src.contains(CompositeGlyphFlags::ROUND_XY_TO_GRID),
            use_my_metrics: src.contains(CompositeGlyphFlags::USE_MY_METRICS),
            scaled_component_offset: src.contains(CompositeGlyphFlags::SCALED_COMPONENT_OFFSET),
            unscaled_component_offset: src.contains(CompositeGlyphFlags::UNSCALED_COMPONENT_OFFSET),
            overlap_compound: src.contains(CompositeGlyphFlags::OVERLAP_COMPOUND),
        }
    }
}

impl From<ComponentFlags> for CompositeGlyphFlags {
    fn from(value: ComponentFlags) -> Self {
        value
            .round_xy_to_grid
            .then_some(CompositeGlyphFlags::ROUND_XY_TO_GRID)
            .unwrap_or_default()
            | value
                .use_my_metrics
                .then_some(CompositeGlyphFlags::USE_MY_METRICS)
                .unwrap_or_default()
            | value
                .scaled_component_offset
                .then_some(CompositeGlyphFlags::SCALED_COMPONENT_OFFSET)
                .unwrap_or_default()
            | value
                .unscaled_component_offset
                .then_some(CompositeGlyphFlags::UNSCALED_COMPONENT_OFFSET)
                .unwrap_or_default()
            | value
                .overlap_compound
                .then_some(CompositeGlyphFlags::OVERLAP_COMPOUND)
                .unwrap_or_default()
    }
}

impl Bbox {
    fn union(self, other: Bbox) -> Bbox {
        Bbox {
            x_min: self.x_min.min(other.x_min),
            y_min: self.y_min.min(other.y_min),
            x_max: self.x_max.max(other.x_max),
            y_max: self.y_max.max(other.x_max),
        }
    }
}

impl From<Rect> for Bbox {
    fn from(value: Rect) -> Self {
        Bbox {
            x_min: value.min_x().ot_round(),
            y_min: value.min_y().ot_round(),
            x_max: value.max_x().ot_round(),
            y_max: value.max_y().ot_round(),
        }
    }
}

impl FontWrite for Bbox {
    fn write_into(&self, writer: &mut crate::TableWriter) {
        let Bbox {
            x_min,
            y_min,
            x_max,
            y_max,
        } = *self;
        [x_min, y_min, x_max, y_max].write_into(writer)
    }
}

#[cfg(test)]
mod tests {
    use std::iter::from_fn;

    use kurbo::Affine;
    use read::{
        tables::glyf as read_glyf, types::GlyphId, FontData, FontRead, FontRef, TableProvider,
    };

    use super::*;

    #[test]
    #[should_panic(expected = "HasCubic")]
    fn bad_path_input() {
        let mut path = BezPath::new();
        path.move_to((0., 0.));
        path.curve_to((10., 10.), (20., 20.), (30., 30.));
        path.line_to((50., 50.));
        path.line_to((10., 10.));
        let _glyph = SimpleGlyph::from_kurbo(&path).unwrap();
    }

    fn simple_glyph_to_bezpath(glyph: &read::tables::glyf::SimpleGlyph) -> BezPath {
        use types::{F26Dot6, Pen};

        #[derive(Default)]
        struct Path(BezPath);

        impl Pen for Path {
            fn move_to(&mut self, x: f32, y: f32) {
                self.0.move_to((x as f64, y as f64));
            }

            fn line_to(&mut self, x: f32, y: f32) {
                self.0.line_to((x as f64, y as f64));
            }

            fn quad_to(&mut self, x0: f32, y0: f32, x1: f32, y1: f32) {
                self.0
                    .quad_to((x0 as f64, y0 as f64), (x1 as f64, y1 as f64));
            }

            fn curve_to(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, x2: f32, y2: f32) {
                self.0.curve_to(
                    (x0 as f64, y0 as f64),
                    (x1 as f64, y1 as f64),
                    (x2 as f64, y2 as f64),
                );
            }

            fn close(&mut self) {
                self.0.close_path();
            }
        }

        let contours = glyph
            .end_pts_of_contours()
            .iter()
            .map(|x| x.get())
            .collect::<Vec<_>>();
        let num_points = glyph.num_points();
        let mut points = vec![Default::default(); num_points];
        let mut flags = vec![Default::default(); num_points];
        glyph.read_points_fast(&mut points, &mut flags).unwrap();
        let points = points
            .into_iter()
            .map(|point| point.map(F26Dot6::from_i32))
            .collect::<Vec<_>>();
        let mut path = Path::default();
        read::tables::glyf::to_path(&points, &flags, &contours, &mut path).unwrap();
        path.0
    }

    // For `indexToLocFormat == 0` (short version), offset divided by 2 is stored, so add a padding
    // byte if the length is not even to ensure our computed bytes match those of our test glyphs.
    fn pad_for_loca_format(loca: &read::tables::loca::Loca, mut bytes: Vec<u8>) -> Vec<u8> {
        if matches!(loca, read::tables::loca::Loca::Short(_)) && bytes.len() & 1 != 0 {
            bytes.push(0);
        }
        bytes
    }

    #[test]
    fn read_write_simple() {
        let font = FontRef::new(font_test_data::SIMPLE_GLYF).unwrap();
        let loca = font.loca(None).unwrap();
        let glyf = font.glyf().unwrap();
        let read_glyf::Glyph::Simple(orig) = loca.get_glyf(GlyphId::new(0), &glyf).unwrap().unwrap() else { panic!("not a simple glyph") };
        let orig_bytes = orig.offset_data();

        let ours = SimpleGlyph::from_table_ref(&orig);
        let bytes = pad_for_loca_format(&loca, crate::dump_table(&ours).unwrap());
        let ours = read_glyf::SimpleGlyph::read(FontData::new(&bytes)).unwrap();

        let our_points = ours.points().collect::<Vec<_>>();
        let their_points = orig.points().collect::<Vec<_>>();
        assert_eq!(our_points, their_points);
        assert_eq!(orig_bytes.as_ref(), bytes);
        assert_eq!(orig.glyph_data(), ours.glyph_data());
        assert_eq!(orig_bytes.len(), bytes.len());
    }

    #[test]
    fn round_trip_simple() {
        let font = FontRef::new(font_test_data::SIMPLE_GLYF).unwrap();
        let loca = font.loca(None).unwrap();
        let glyf = font.glyf().unwrap();
        let read_glyf::Glyph::Simple(orig) = loca.get_glyf(GlyphId::new(2), &glyf).unwrap().unwrap() else { panic!("not a simple glyph") };
        let orig_bytes = orig.offset_data();

        let bezpath = simple_glyph_to_bezpath(&orig);

        let ours = SimpleGlyph::from_kurbo(&bezpath).unwrap();
        let bytes = pad_for_loca_format(&loca, crate::dump_table(&ours).unwrap());
        let ours = read_glyf::SimpleGlyph::read(FontData::new(&bytes)).unwrap();

        let our_points = ours.points().collect::<Vec<_>>();
        let their_points = orig.points().collect::<Vec<_>>();
        assert_eq!(our_points, their_points);
        assert_eq!(orig_bytes.as_ref(), bytes);
        assert_eq!(orig.glyph_data(), ours.glyph_data());
        assert_eq!(orig_bytes.len(), bytes.len());
    }

    #[test]
    fn round_trip_multi_contour() {
        let font = FontRef::new(font_test_data::VAZIRMATN_VAR).unwrap();
        let loca = font.loca(None).unwrap();
        let glyf = font.glyf().unwrap();
        let read_glyf::Glyph::Simple(orig) = loca.get_glyf(GlyphId::new(1), &glyf).unwrap().unwrap() else { panic!("not a simple glyph") };
        let orig_bytes = orig.offset_data();

        let bezpath = simple_glyph_to_bezpath(&orig);

        let ours = SimpleGlyph::from_kurbo(&bezpath).unwrap();
        let bytes = pad_for_loca_format(&loca, crate::dump_table(&ours).unwrap());
        let ours = read_glyf::SimpleGlyph::read(FontData::new(&bytes)).unwrap();

        let our_points = ours.points().collect::<Vec<_>>();
        let their_points = orig.points().collect::<Vec<_>>();
        dbg!(
            SimpleGlyphFlags::from_bits(1),
            SimpleGlyphFlags::from_bits(9)
        );
        assert_eq!(our_points, their_points);
        assert_eq!(orig.glyph_data(), ours.glyph_data());
        assert_eq!(orig_bytes.len(), bytes.len());
        assert_eq!(orig_bytes.as_ref(), bytes);
    }

    #[test]
    fn simple_glyph_open_path() {
        let mut path = BezPath::new();
        path.move_to((20., -100.));
        path.quad_to((1337., 1338.), (-50., -69.0));
        path.quad_to((13., 255.), (-255., 256.));
        // even if the last point is on top of the first, the path was not deliberately closed
        // hence there is going to be an extra point (6, not 5 in total)
        path.line_to((20., -100.));

        let glyph = SimpleGlyph::from_kurbo(&path).unwrap();
        let bytes = crate::dump_table(&glyph).unwrap();
        let read = read_fonts::tables::glyf::SimpleGlyph::read(FontData::new(&bytes)).unwrap();
        assert_eq!(read.number_of_contours(), 1);
        assert_eq!(read.num_points(), 6);
        assert_eq!(read.end_pts_of_contours(), &[5]);
        let points = read.points().collect::<Vec<_>>();
        assert_eq!(points[0].x, 20);
        assert_eq!(points[0].y, -100);
        assert!(points[0].on_curve);
        assert_eq!(points[1].x, 1337);
        assert_eq!(points[1].y, 1338);
        assert!(!points[1].on_curve);
        assert_eq!(points[4].x, -255);
        assert_eq!(points[4].y, 256);
        assert!(points[4].on_curve);
        assert_eq!(points[5].x, 20);
        assert_eq!(points[5].y, -100);
        assert!(points[5].on_curve);
    }

    #[test]
    fn simple_glyph_closed_path_implicit_vs_explicit_closing_line() {
        let mut path1 = BezPath::new();
        path1.move_to((20., -100.));
        path1.quad_to((1337., 1338.), (-50., -69.0));
        path1.quad_to((13., 255.), (-255., 256.));
        path1.close_path();

        let mut path2 = BezPath::new();
        path2.move_to((20., -100.));
        path2.quad_to((1337., 1338.), (-50., -69.0));
        path2.quad_to((13., 255.), (-255., 256.));
        // this line_to (absent from path1) makes no difference since in both cases the
        // path is closed with a close_path (5 points in total, not 6)
        path2.line_to((20., -100.));
        path2.close_path();

        for path in &[path1, path2] {
            let glyph = SimpleGlyph::from_kurbo(path).unwrap();
            let bytes = crate::dump_table(&glyph).unwrap();
            let read = read_fonts::tables::glyf::SimpleGlyph::read(FontData::new(&bytes)).unwrap();
            assert_eq!(read.number_of_contours(), 1);
            assert_eq!(read.num_points(), 5);
            assert_eq!(read.end_pts_of_contours(), &[4]);
            let points = read.points().collect::<Vec<_>>();
            assert_eq!(points[0].x, 20);
            assert_eq!(points[0].y, -100);
            assert!(points[0].on_curve);
            assert_eq!(points[1].x, 1337);
            assert_eq!(points[1].y, 1338);
            assert!(!points[1].on_curve);
            assert_eq!(points[4].x, -255);
            assert_eq!(points[4].y, 256);
            assert!(points[4].on_curve);
        }
    }

    #[test]
    fn keep_single_point_contours() {
        // single points may be meaningless, but are also harmless
        let mut path = BezPath::new();
        path.move_to((0.0, 0.0));
        // path.close_path();  // doesn't really matter if this is closed
        path.move_to((1.0, 2.0));
        path.close_path();

        let glyph = SimpleGlyph::from_kurbo(&path).unwrap();
        let bytes = crate::dump_table(&glyph).unwrap();
        let read = read_fonts::tables::glyf::SimpleGlyph::read(FontData::new(&bytes)).unwrap();
        assert_eq!(read.number_of_contours(), 2);
        assert_eq!(read.num_points(), 2);
        assert_eq!(read.end_pts_of_contours(), &[0, 1]);
        let points = read.points().collect::<Vec<_>>();
        assert_eq!(points[0].x, 0);
        assert_eq!(points[0].y, 0);
        assert!(points[0].on_curve);
        assert_eq!(points[1].x, 1);
        assert_eq!(points[1].y, 2);
        assert!(points[0].on_curve);
    }

    #[test]
    fn compile_repeatable_flags() {
        let mut path = BezPath::new();
        path.move_to((20., -100.));
        path.line_to((25., -90.));
        path.line_to((50., -69.));
        path.line_to((80., -20.));

        let glyph = SimpleGlyph::from_kurbo(&path).unwrap();
        let flags = glyph
            .compute_point_deltas()
            .map(|x| x.0)
            .collect::<Vec<_>>();
        let r_flags = RepeatableFlag::iter_from_flags(flags.iter().copied()).collect::<Vec<_>>();

        assert_eq!(r_flags.len(), 2, "{r_flags:?}");
        let bytes = crate::dump_table(&glyph).unwrap();
        let read = read_glyf::SimpleGlyph::read(FontData::new(&bytes)).unwrap();
        assert_eq!(read.number_of_contours(), 1);
        assert_eq!(read.num_points(), 4);
        assert_eq!(read.end_pts_of_contours(), &[3]);
        let points = read.points().collect::<Vec<_>>();
        assert_eq!(points[0].x, 20);
        assert_eq!(points[0].y, -100);
        assert_eq!(points[1].x, 25);
        assert_eq!(points[1].y, -90);
        assert_eq!(points[2].x, 50);
        assert_eq!(points[2].y, -69);
        assert_eq!(points[3].x, 80);
        assert_eq!(points[3].y, -20);
    }

    #[test]
    #[should_panic(expected = "UnequalNumberOfElements")]
    fn simple_glyphs_from_kurbo_unequal_number_of_elements() {
        let mut path1 = BezPath::new();
        path1.move_to((0., 0.));
        path1.line_to((1., 1.));
        path1.line_to((2., 2.));
        path1.line_to((0., 0.));
        path1.close_path();
        assert_eq!(path1.elements().len(), 5);

        let mut path2 = BezPath::new();
        path2.move_to((3., 3.));
        path2.line_to((4., 4.));
        path2.line_to((5., 5.));
        path2.line_to((6., 6.));
        path2.line_to((3., 3.));
        path2.close_path();
        assert_eq!(path2.elements().len(), 6);

        simple_glyphs_from_kurbo(&[path1, path2]).unwrap();
    }

    #[test]
    #[should_panic(expected = "InconsistentPathElements")]
    fn simple_glyphs_from_kurbo_inconsistent_path_elements() {
        let mut path1 = BezPath::new();
        path1.move_to((0., 0.));
        path1.line_to((1., 1.));
        path1.quad_to((2., 2.), (0., 0.));
        path1.close_path();
        let mut path2 = BezPath::new();
        path2.move_to((3., 3.));
        path2.quad_to((4., 4.), (5., 5.));
        path2.line_to((3., 3.));
        path2.close_path();

        simple_glyphs_from_kurbo(&[path1, path2]).unwrap();
    }

    fn make_interpolatable_paths(
        num_paths: usize,
        el_types: &str,
        last_pt_equal_move: bool,
    ) -> Vec<BezPath> {
        let mut paths = Vec::new();
        // we don't care about the actual coordinate values, just use a counter
        // that yields 0.0, 1.0, 2.0, 3.0, etc.
        let mut start = 0.0;
        let mut points = from_fn(move || {
            let value = start;
            start += 1.0;
            Some((value, value))
        });
        let el_types = el_types.chars().collect::<Vec<_>>();
        assert!(!el_types.is_empty());
        for _ in 0..num_paths {
            let mut path = BezPath::new();
            let mut start_pt = None;
            // use peekable iterator so we can look ahead to next el_type
            let mut el_types_iter = el_types.iter().peekable();
            while let Some(&el_type) = el_types_iter.next() {
                let next_el_type = el_types_iter.peek().map(|x| **x).unwrap_or('M');
                match el_type {
                    'M' => {
                        start_pt = points.next();
                        path.move_to(start_pt.unwrap());
                    }
                    'L' => {
                        if matches!(next_el_type, 'Z' | 'M') && last_pt_equal_move {
                            path.line_to(start_pt.unwrap());
                        } else {
                            path.line_to(points.next().unwrap());
                        }
                    }
                    'Q' => {
                        let p1 = points.next().unwrap();
                        let p2 = if matches!(next_el_type, 'Z' | 'M') && last_pt_equal_move {
                            start_pt.unwrap()
                        } else {
                            points.next().unwrap()
                        };
                        path.quad_to(p1, p2);
                    }
                    'Z' => {
                        path.close_path();
                        start_pt = None;
                    }
                    _ => panic!("Usupported element type {:?}", el_type),
                }
            }
            paths.push(path);
        }
        assert_eq!(paths.len(), num_paths);
        paths
    }

    fn assert_contour_points(glyph: &SimpleGlyph, all_points: Vec<Vec<CurvePoint>>) {
        let expected_num_contours = all_points.len();
        assert_eq!(glyph.contours().len(), expected_num_contours);
        for (contour, expected_points) in glyph.contours().iter().zip(all_points.iter()) {
            let points = contour.iter().copied().collect::<Vec<_>>();
            assert_eq!(points, *expected_points);
        }
    }

    #[test]
    fn simple_glyphs_from_kurbo_3_lines_closed() {
        let paths = make_interpolatable_paths(2, "MLLLZ", true);
        let glyphs = simple_glyphs_from_kurbo(&paths).unwrap();

        assert_contour_points(
            &glyphs[0],
            vec![vec![
                CurvePoint::on_curve(0, 0),
                CurvePoint::on_curve(1, 1),
                CurvePoint::on_curve(2, 2),
            ]],
        );
        assert_contour_points(
            &glyphs[1],
            vec![vec![
                CurvePoint::on_curve(3, 3),
                CurvePoint::on_curve(4, 4),
                CurvePoint::on_curve(5, 5),
            ]],
        );
    }

    #[test]
    fn simple_glyphs_from_kurbo_3_lines_implicitly_closed() {
        let paths = make_interpolatable_paths(2, "MLLZ", false);
        let glyphs = simple_glyphs_from_kurbo(&paths).unwrap();

        assert_contour_points(
            &glyphs[0],
            vec![vec![
                CurvePoint::on_curve(0, 0),
                CurvePoint::on_curve(1, 1),
                CurvePoint::on_curve(2, 2),
            ]],
        );
        assert_contour_points(
            &glyphs[1],
            vec![vec![
                CurvePoint::on_curve(3, 3),
                CurvePoint::on_curve(4, 4),
                CurvePoint::on_curve(5, 5),
            ]],
        );
    }

    #[test]
    fn simple_glyphs_from_kurbo_2_quads_closed() {
        let paths = make_interpolatable_paths(2, "MQQZ", true);
        let glyphs = simple_glyphs_from_kurbo(&paths).unwrap();

        assert_contour_points(
            &glyphs[0],
            vec![vec![
                CurvePoint::on_curve(0, 0),
                CurvePoint::off_curve(1, 1),
                // CurvePoint::on_curve(2, 2),  // implied oncurve point dropped
                CurvePoint::off_curve(3, 3),
            ]],
        );
        assert_contour_points(
            &glyphs[1],
            vec![vec![
                CurvePoint::on_curve(4, 4),
                CurvePoint::off_curve(5, 5),
                // CurvePoint::on_curve(6, 6),  // implied
                CurvePoint::off_curve(7, 7),
            ]],
        );
    }

    #[test]
    fn simple_glyphs_from_kurbo_2_quads_1_line_implictly_closed() {
        let paths = make_interpolatable_paths(2, "MQQZ", false);
        let glyphs = simple_glyphs_from_kurbo(&paths).unwrap();

        assert_contour_points(
            &glyphs[0],
            vec![vec![
                CurvePoint::on_curve(0, 0),
                CurvePoint::off_curve(1, 1),
                // CurvePoint::on_curve(2, 2),
                CurvePoint::off_curve(3, 3),
                CurvePoint::on_curve(4, 4),
            ]],
        );
        assert_contour_points(
            &glyphs[1],
            vec![vec![
                CurvePoint::on_curve(5, 5),
                CurvePoint::off_curve(6, 6),
                // CurvePoint::on_curve(7, 7),
                CurvePoint::off_curve(8, 8),
                CurvePoint::on_curve(9, 9),
            ]],
        );
    }

    #[test]
    fn simple_glyphs_from_kurbo_multiple_contours_mixed_segments() {
        let paths = make_interpolatable_paths(4, "MLQQZMQLQLZ", true);
        let glyphs = simple_glyphs_from_kurbo(&paths).unwrap();

        assert_contour_points(
            &glyphs[0],
            vec![
                vec![
                    CurvePoint::on_curve(0, 0),
                    CurvePoint::on_curve(1, 1),
                    CurvePoint::off_curve(2, 2),
                    // CurvePoint::on_curve(3, 3),
                    CurvePoint::off_curve(4, 4),
                ],
                vec![
                    CurvePoint::on_curve(5, 5),
                    CurvePoint::off_curve(6, 6),
                    CurvePoint::on_curve(7, 7),
                    CurvePoint::on_curve(8, 8),
                    CurvePoint::off_curve(9, 9),
                    CurvePoint::on_curve(10, 10),
                ],
            ],
        );
    }

    #[test]
    fn simple_glyphs_from_kurbo_all_quad_off_curves() {
        let mut path1 = BezPath::new();
        path1.move_to((0.0, 1.0));
        path1.quad_to((1.0, 1.0), (1.0, 0.0));
        path1.quad_to((1.0, -1.0), (0.0, -1.0));
        path1.quad_to((-1.0, -1.0), (-1.0, 0.0));
        path1.quad_to((-1.0, 1.0), (0.0, 1.0));
        path1.close_path();

        let mut path2 = path1.clone();
        path2.apply_affine(Affine::scale(2.0));

        let glyphs = simple_glyphs_from_kurbo(&[path1, path2]).unwrap();

        assert_contour_points(
            &glyphs[0],
            vec![vec![
                CurvePoint::off_curve(1, 1),
                CurvePoint::off_curve(1, -1),
                CurvePoint::off_curve(-1, -1),
                CurvePoint::off_curve(-1, 1),
            ]],
        );
        assert_contour_points(
            &glyphs[1],
            vec![vec![
                CurvePoint::off_curve(2, 2),
                CurvePoint::off_curve(2, -2),
                CurvePoint::off_curve(-2, -2),
                CurvePoint::off_curve(-2, 2),
            ]],
        );
    }

    #[test]
    fn simple_glyphs_from_kurbo_keep_on_curve_unless_impliable_for_all() {
        let mut path1 = BezPath::new();
        path1.move_to((0.0, 0.0));
        path1.quad_to((0.0, 1.0), (1.0, 1.0)); // on-curve equidistant from prev/next off-curves
        path1.quad_to((2.0, 1.0), (2.0, 0.0));
        path1.line_to((0.0, 0.0));
        path1.close_path();

        assert_contour_points(
            &SimpleGlyph::from_kurbo(&path1).unwrap(),
            vec![vec![
                CurvePoint::on_curve(0, 0),
                CurvePoint::off_curve(0, 1),
                // CurvePoint::on_curve(1, 1),  // implied
                CurvePoint::off_curve(2, 1),
                CurvePoint::on_curve(2, 0),
            ]],
        );

        let mut path2 = BezPath::new();
        path2.move_to((0.0, 0.0));
        path2.quad_to((0.0, 2.0), (2.0, 2.0)); // on-curve NOT equidistant from prev/next off-curves
        path2.quad_to((3.0, 2.0), (3.0, 0.0));
        path2.line_to((0.0, 0.0));
        path2.close_path();

        let glyphs = simple_glyphs_from_kurbo(&[path1, path2]).unwrap();

        assert_contour_points(
            &glyphs[0],
            vec![vec![
                CurvePoint::on_curve(0, 0),
                CurvePoint::off_curve(0, 1),
                CurvePoint::on_curve(1, 1), // NOT implied
                CurvePoint::off_curve(2, 1),
                CurvePoint::on_curve(2, 0),
            ]],
        );
        assert_contour_points(
            &glyphs[1],
            vec![vec![
                CurvePoint::on_curve(0, 0),
                CurvePoint::off_curve(0, 2),
                CurvePoint::on_curve(2, 2), // NOT implied
                CurvePoint::off_curve(3, 2),
                CurvePoint::on_curve(3, 0),
            ]],
        );
    }

    #[test]
    fn simple_glyphs_from_kurbo_2_lines_open() {
        let paths = make_interpolatable_paths(2, "MLL", false);
        let glyphs = simple_glyphs_from_kurbo(&paths).unwrap();

        assert_contour_points(
            &glyphs[0],
            vec![vec![
                CurvePoint::on_curve(0, 0),
                CurvePoint::on_curve(1, 1),
                CurvePoint::on_curve(2, 2),
            ]],
        );
        assert_contour_points(
            &glyphs[1],
            vec![vec![
                CurvePoint::on_curve(3, 3),
                CurvePoint::on_curve(4, 4),
                CurvePoint::on_curve(5, 5),
            ]],
        );
    }

    #[test]
    fn simple_glyphs_from_kurbo_3_lines_open_duplicate_last_pt() {
        let paths = make_interpolatable_paths(2, "MLLL", true);
        let glyphs = simple_glyphs_from_kurbo(&paths).unwrap();

        assert_contour_points(
            &glyphs[0],
            vec![vec![
                CurvePoint::on_curve(0, 0),
                CurvePoint::on_curve(1, 1),
                CurvePoint::on_curve(2, 2),
                CurvePoint::on_curve(0, 0),
            ]],
        );
        assert_contour_points(
            &glyphs[1],
            vec![vec![
                CurvePoint::on_curve(3, 3),
                CurvePoint::on_curve(4, 4),
                CurvePoint::on_curve(5, 5),
                CurvePoint::on_curve(3, 3),
            ]],
        );
    }

    #[test]
    fn simple_glyphs_from_kurbo_4_lines_closed_duplicate_last_pt() {
        for implicit_closing_line in &[true, false] {
            // both (closed) paths contain 4 line segments each, but the first path
            // looks like a triangle because the last segment has zero length (i.e.
            // last and first points are duplicates).
            let mut path1 = BezPath::new();
            path1.move_to((0.0, 0.0));
            path1.line_to((0.0, 1.0));
            path1.line_to((1.0, 1.0));
            path1.line_to((0.0, 0.0));
            if !*implicit_closing_line {
                path1.line_to((0.0, 0.0));
            }
            path1.close_path();

            let mut path2 = BezPath::new();
            path2.move_to((0.0, 0.0));
            path2.line_to((0.0, 2.0));
            path2.line_to((2.0, 2.0));
            path2.line_to((2.0, 0.0));
            if !*implicit_closing_line {
                path2.line_to((0.0, 0.0));
            }
            path2.close_path();

            let glyphs = simple_glyphs_from_kurbo(&[path1, path2]).unwrap();

            assert_contour_points(
                &glyphs[0],
                vec![vec![
                    CurvePoint::on_curve(0, 0),
                    CurvePoint::on_curve(0, 1),
                    CurvePoint::on_curve(1, 1),
                    CurvePoint::on_curve(0, 0), // duplicate last point retained
                ]],
            );
            assert_contour_points(
                &glyphs[1],
                vec![vec![
                    CurvePoint::on_curve(0, 0),
                    CurvePoint::on_curve(0, 2),
                    CurvePoint::on_curve(2, 2),
                    CurvePoint::on_curve(2, 0),
                ]],
            );
        }
    }

    #[test]
    fn simple_glyphs_from_kurbo_2_quads_1_line_closed_duplicate_last_pt() {
        for implicit_closing_line in &[true, false] {
            // the closed paths contain 2 quads and 1 line segments, but in the first path
            // the last segment has zero length (i.e. last and first points are duplicates).
            let mut path1 = BezPath::new();
            path1.move_to((0.0, 0.0));
            path1.quad_to((0.0, 1.0), (1.0, 1.0));
            path1.quad_to((1.0, 0.0), (0.0, 0.0));
            if !*implicit_closing_line {
                path1.line_to((0.0, 0.0));
            }
            path1.close_path();

            let mut path2 = BezPath::new();
            path2.move_to((0.0, 0.0));
            path2.quad_to((0.0, 2.0), (2.0, 2.0));
            path2.quad_to((2.0, 1.0), (1.0, 0.0));
            if !*implicit_closing_line {
                path2.line_to((0.0, 0.0));
            }
            path2.close_path();

            let glyphs = simple_glyphs_from_kurbo(&[path1, path2]).unwrap();

            assert_contour_points(
                &glyphs[0],
                vec![vec![
                    CurvePoint::on_curve(0, 0),
                    CurvePoint::off_curve(0, 1),
                    CurvePoint::on_curve(1, 1),
                    CurvePoint::off_curve(1, 0),
                    CurvePoint::on_curve(0, 0), // duplicate last point retained
                ]],
            );
            assert_contour_points(
                &glyphs[1],
                vec![vec![
                    CurvePoint::on_curve(0, 0),
                    CurvePoint::off_curve(0, 2),
                    CurvePoint::on_curve(2, 2),
                    CurvePoint::off_curve(2, 1),
                    CurvePoint::on_curve(1, 0),
                ]],
            );
        }
    }

    #[test]
    fn repeatable_flags_basic() {
        let flags = [
            SimpleGlyphFlags::ON_CURVE_POINT,
            SimpleGlyphFlags::X_SHORT_VECTOR,
            SimpleGlyphFlags::X_SHORT_VECTOR,
        ];
        let repeatable = RepeatableFlag::iter_from_flags(flags).collect::<Vec<_>>();
        let expected = flags
            .into_iter()
            .map(|flag| RepeatableFlag { flag, repeat: 0 })
            .collect::<Vec<_>>();

        // even though we have a repeating flag at the end, we should still produce
        // three flags, since we don't bother with repeat counts < 2.
        assert_eq!(repeatable, expected);
    }

    #[test]
    fn repeatable_flags_repeats() {
        let some_dupes = std::iter::repeat(SimpleGlyphFlags::ON_CURVE_POINT).take(4);
        let many_dupes = std::iter::repeat(SimpleGlyphFlags::Y_SHORT_VECTOR).take(257);
        let repeatable =
            RepeatableFlag::iter_from_flags(some_dupes.chain(many_dupes)).collect::<Vec<_>>();
        assert_eq!(repeatable.len(), 3);
        assert_eq!(
            repeatable[0],
            RepeatableFlag {
                flag: SimpleGlyphFlags::ON_CURVE_POINT | SimpleGlyphFlags::REPEAT_FLAG,
                repeat: 3
            }
        );
        assert_eq!(
            repeatable[1],
            RepeatableFlag {
                flag: SimpleGlyphFlags::Y_SHORT_VECTOR | SimpleGlyphFlags::REPEAT_FLAG,
                repeat: u8::MAX,
            }
        );

        assert_eq!(
            repeatable[2],
            RepeatableFlag {
                flag: SimpleGlyphFlags::Y_SHORT_VECTOR,
                repeat: 0,
            }
        )
    }

    #[test]
    fn roundtrip_composite() {
        let font = FontRef::new(font_test_data::VAZIRMATN_VAR).unwrap();
        let loca = font.loca(None).unwrap();
        let glyf = font.glyf().unwrap();
        let read_glyf::Glyph::Composite(orig) = loca.get_glyf(GlyphId::new(2), &glyf).unwrap().unwrap() else { panic!("not a composite glyph") };

        let bbox = Bbox {
            x_min: orig.x_min(),
            y_min: orig.y_min(),
            x_max: orig.x_max(),
            y_max: orig.y_max(),
        };
        let mut iter = orig
            .components()
            .map(|comp| Component::new(comp.glyph, comp.anchor, comp.transform, comp.flags));
        let mut composite = CompositeGlyph::new(iter.next().unwrap(), bbox);
        composite.add_component(iter.next().unwrap(), bbox);
        composite._instructions = orig.instructions().unwrap_or_default().to_vec();
        assert!(iter.next().is_none());
        let bytes = crate::dump_table(&composite).unwrap();
        let ours = read::tables::glyf::CompositeGlyph::read(FontData::new(&bytes)).unwrap();

        let our_comps = ours.components().collect::<Vec<_>>();
        let orig_comps = orig.components().collect::<Vec<_>>();
        assert_eq!(our_comps.len(), orig_comps.len());
        assert_eq!(our_comps.len(), 2);
        assert_eq!(&our_comps[0], &orig_comps[0]);
        assert_eq!(&our_comps[1], &orig_comps[1]);
        assert_eq!(ours.instructions(), orig.instructions());
        assert_eq!(orig.offset_data().len(), bytes.len());

        assert_eq!(orig.offset_data().as_ref(), bytes);
    }
}
