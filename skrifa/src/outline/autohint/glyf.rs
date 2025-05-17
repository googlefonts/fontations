//! Fast loading of glyf outlines for autohinting.

use super::super::glyf::{Outline as GlyfOutline, Outlines as GlyfOutlines};
use super::outline::{Contour, Outline, Point};
use crate::outline::DrawError;
use raw::{
    tables::glyf::{
        Anchor, CompositeGlyph, CompositeGlyphFlags, Glyph, PointFlags, PointWithFlags, SimpleGlyph,
    },
    types::{F2Dot14, Fixed, GlyphId},
    ReadError,
};

type PointI32 = raw::types::Point<i32>;

impl PointWithFlags<i32> for Point {
    fn x(&self) -> i32 {
        self.fx
    }

    fn y(&self) -> i32 {
        self.fy
    }

    fn x_mut(&mut self) -> &mut i32 {
        &mut self.fx
    }

    fn y_mut(&mut self) -> &mut i32 {
        &mut self.fy
    }

    fn flags(&self) -> PointFlags {
        self.flags
    }

    fn flags_mut(&mut self) -> &mut PointFlags {
        &mut self.flags
    }
}

impl Outline {
    pub(crate) fn fill_from_glyf<'a>(
        &mut self,
        outlines: &GlyfOutlines<'a>,
        outline: &GlyfOutline<'a>,
    ) -> Result<i32, DrawError> {
        self.points.clear();
        // self.points.try_reserve(outline.points + 4);
        // for point in self.points.as_mut_slice() {
        //     *point = Default::default();
        // }
        self.points.resize(outline.points);
        self.contours.clear();
        self.contours.resize(outline.contours);
        let mut loader = GlyfLoader {
            points: self.points.as_mut_slice(),
            contours: self.contours.as_mut_slice(),
            n_points: 0,
            n_contours: 0,
            glyf: outlines,
            phantom: [PointI32::default(); 4],
        };
        loader.load(&outline.glyph, outline.glyph_id, 0)?;
        let n_points = loader.n_points;
        let pp0x = loader.phantom[0].x;
        let advance = loader.phantom[1].x - loader.phantom[0].x;
        self.points.truncate(n_points);
        if pp0x != 0 {
            for point in self.points.as_mut_slice() {
                point.fx -= pp0x;
            }
        }
        Ok(advance)
    }
}

const PHANTOM_POINT_COUNT: usize = 4;
const GLYF_COMPOSITE_RECURSION_LIMIT: usize = 64;

struct GlyfLoader<'a> {
    points: &'a mut [Point],
    contours: &'a mut [Contour],
    n_points: usize,
    n_contours: usize,
    glyf: &'a GlyfOutlines<'a>,
    phantom: [PointI32; 4],
}

impl<'a> GlyfLoader<'a> {
    fn load(
        &mut self,
        glyph: &Option<Glyph>,
        glyph_id: GlyphId,
        recurse_depth: usize,
    ) -> Result<(), DrawError> {
        if recurse_depth > GLYF_COMPOSITE_RECURSION_LIMIT {
            return Err(DrawError::RecursionLimitExceeded(glyph_id));
        }
        let bounds = match &glyph {
            Some(glyph) => [glyph.x_min(), glyph.x_max(), glyph.y_min(), glyph.y_max()],
            _ => [0; 4],
        };
        let lsb = self.glyf.glyph_metrics.lsb(glyph_id, &[]);
        let advance = self.glyf.glyph_metrics.advance_width(glyph_id, &[]);
        let [ascent, descent] = [0, 0]; //outlines.os2_vmetrics.map(|x| x as i32);
        let tsb = ascent - bounds[3] as i32;
        let vadvance = ascent - descent;
        // The four "phantom" points as computed by FreeType.
        // See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/truetype/ttgload.c#L1365>
        // horizontal:
        self.phantom[0].x = bounds[0] as i32 - lsb;
        self.phantom[0].y = 0;
        self.phantom[1].x = self.phantom[0].x + advance;
        self.phantom[1].y = 0;
        // vertical:
        self.phantom[2].x = 0;
        self.phantom[2].y = bounds[3] as i32 + tsb;
        self.phantom[3].x = 0;
        self.phantom[3].y = self.phantom[2].y - vadvance;
        match glyph {
            Some(Glyph::Simple(simple)) => self.load_simple(simple, glyph_id),
            Some(Glyph::Composite(composite)) => {
                self.load_composite(composite, glyph_id, recurse_depth)
            }
            None => self.load_empty(glyph_id),
        }
    }

    fn load_empty(&mut self, _gid: GlyphId) -> Result<(), DrawError> {
        Ok(())
    }

    fn load_simple(&mut self, glyph: &SimpleGlyph, _glyph_id: GlyphId) -> Result<(), DrawError> {
        let points_start = self.n_points;
        let point_count = glyph.num_points();
        let points_end = points_start + point_count + PHANTOM_POINT_COUNT;
        let points_range = points_start..points_end;
        let points = &mut self.points[points_range.clone()];
        glyph.read_points_with_flags_fast(&mut points[0..point_count])?;
        let contours_start = self.n_contours;
        let contour_end_pts = glyph.end_pts_of_contours();
        let contour_count = contour_end_pts.len();
        let contours_end = contours_start + contour_count;
        let contours = &mut self.contours[contours_start..contours_end];
        // Read the contour end points, ensuring that they are properly
        // ordered.
        let mut last_end_pt = 0;
        for (end_pt, contour) in contour_end_pts.iter().zip(contours.iter_mut()) {
            let end_pt = end_pt.get();
            if end_pt < last_end_pt {
                return Err(ReadError::MalformedData(
                    "unordered contour end points in TrueType glyph",
                )
                .into());
            }
            contour.first_ix = last_end_pt + points_start as u16;
            if last_end_pt != 0 {
                contour.first_ix += 1;
            }
            last_end_pt = end_pt;
            contour.last_ix = end_pt + points_start as u16;
        }
        self.n_points += point_count;
        self.n_contours += contour_count;
        Ok(())
    }

    fn load_composite(
        &mut self,
        glyph: &CompositeGlyph,
        _glyph_id: GlyphId,
        recurse_depth: usize,
    ) -> Result<(), DrawError> {
        for (_i, component) in glyph.components().enumerate() {
            let phantom = self.phantom;
            let start_point = self.n_points;
            let component_glyph = self
                .glyf
                .loca
                .get_glyf(component.glyph.into(), &self.glyf.glyf)?;
            self.load(&component_glyph, component.glyph.into(), recurse_depth + 1)?;
            let end_point = self.n_points;
            if !component
                .flags
                .contains(CompositeGlyphFlags::USE_MY_METRICS)
            {
                // If the USE_MY_METRICS flag is missing, we restore the phantom points we
                // saved at the start of the loop.
                self.phantom = phantom;
            }
            // Prepares the transform components for our conversion math below.
            fn scale_component(x: F2Dot14) -> Fixed {
                Fixed::from_bits(x.to_bits() as i32 * 4)
            }
            let xform = &component.transform;
            let xx = scale_component(xform.xx);
            let yx = scale_component(xform.yx);
            let xy = scale_component(xform.xy);
            let yy = scale_component(xform.yy);
            let have_xform = component.flags.intersects(
                CompositeGlyphFlags::WE_HAVE_A_SCALE
                    | CompositeGlyphFlags::WE_HAVE_AN_X_AND_Y_SCALE
                    | CompositeGlyphFlags::WE_HAVE_A_TWO_BY_TWO,
            );
            if have_xform {
                let points = &mut self.points[start_point..end_point];
                for point in points {
                    let fx = Fixed::from_bits(point.fx);
                    let fy = Fixed::from_bits(point.fy);
                    let x = fx * xx + fy * xy;
                    let y = fx * yx + fy * yy;
                    point.fx = x.to_bits();
                    point.fy = y.to_bits();
                }
            }
            let anchor_offset = match component.anchor {
                Anchor::Offset { x, y } => {
                    let (mut x, mut y) = (x as i32, y as i32);
                    if have_xform
                        && component.flags
                            & (CompositeGlyphFlags::SCALED_COMPONENT_OFFSET
                                | CompositeGlyphFlags::UNSCALED_COMPONENT_OFFSET)
                            == CompositeGlyphFlags::SCALED_COMPONENT_OFFSET
                    {
                        // According to FreeType, this algorithm is a "guess"
                        // and works better than the one documented by Apple.
                        // https://github.com/freetype/freetype/blob/b1c90733ee6a04882b133101d61b12e352eeb290/src/truetype/ttgload.c#L1259
                        fn hypot(a: Fixed, b: Fixed) -> Fixed {
                            let a = a.to_bits().abs();
                            let b = b.to_bits().abs();
                            Fixed::from_bits(if a > b {
                                a + ((3 * b) >> 3)
                            } else {
                                b + ((3 * a) >> 3)
                            })
                        }
                        // FreeType uses a fixed point multiplication here.
                        x = (Fixed::from_bits(x) * hypot(xx, xy)).to_bits();
                        y = (Fixed::from_bits(y) * hypot(yy, yx)).to_bits();
                    }
                    (x, y)
                }
                Anchor::Point {
                    base: _,
                    component: _,
                } => {
                    // panic!("don't support Anchor::Point");
                    (0, 0)
                }
            };
            if anchor_offset.0 != 0 || anchor_offset.1 != 0 {
                for point in &mut self.points[start_point..end_point] {
                    point.fx += anchor_offset.0;
                    point.fy += anchor_offset.1;
                }
            }
        }
        Ok(())
    }
}
