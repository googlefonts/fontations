use super::math;
use super::{Context, Outline, Point};
use crate::{Error, Hinting, NormalizedCoord, Result};

use read_fonts::tables::{
    glyf::{Anchor, CompositeGlyph, CompositeGlyphFlags, Glyf, Glyph, SimpleGlyph},
    hmtx::Hmtx,
    hvar::Hvar,
    loca::Loca,
};
use read_fonts::types::{BigEndian, F2Dot14, GlyphId, Tag};
use read_fonts::TableProvider;

/// Limit for recursion when loading composite glyphs.
const RECURSION_LIMIT: usize = 32;

/// TrueType glyph scaler for a specific font and configuration.
pub struct Scaler<'a> {
    /// Backing context.
    context: &'a mut Context,
    /// Current font data.
    font: Font<'a>,
    /// Font identifier for the hinting cache.
    font_id: Option<u64>,
    /// Current hinting cache slot.
    cache_slot: Option<CacheSlot>,
    /// True if the outline is begin scaled.
    is_scaled: bool,
    /// Size in pixels per em unit.
    ppem: u16,
    /// Scale factor from font units to 26.6 fixed point.
    scale: i32,
    /// Hint mode.
    hint: Option<Hinting>,
    /// Normalized variation coordinates.
    coords: &'a [NormalizedCoord],
}

impl<'a> Scaler<'a> {
    /// Creates a new scaler for extracting outlines with the specified font
    /// and configuration.
    pub fn new(
        context: &'a mut Context,
        font: &impl TableProvider<'a>,
        font_id: Option<u64>,
        size: f32,
        hint: Option<Hinting>,
        coords: &'a [NormalizedCoord],
    ) -> Result<Self> {
        let font = Font::new(font)?;
        let size = size.abs();
        let ppem = size as u16;
        let upem = font.units_per_em;
        let (is_scaled, scale) = if size != 0. && upem != 0 {
            (true, math::div((size * 64.) as i32, upem as i32))
        } else {
            (false, 0)
        };
        Ok(Self {
            context,
            font,
            font_id,
            cache_slot: None,
            is_scaled,
            ppem,
            scale,
            hint,
            coords,
        })
    }

    /// Loads an outline for the specified glyph identifier to the preallocated
    /// target.
    pub fn get_into(&mut self, glyph_id: GlyphId, outline: &mut Outline) -> Result<()> {
        outline.clear();
        self.context.unscaled.clear();
        self.context.original.clear();
        self.context.deltas.clear();
        if glyph_id.to_u16() >= self.font.glyph_count {
            return Err(Error::GlyphNotFound(glyph_id));
        }
        outline.is_scaled = self.is_scaled;
        GlyphScaler::new(self).load(glyph_id, outline, 0)
    }

    /// Loads an outline for the specified glyph identifier.
    pub fn get(&mut self, glyph_id: GlyphId) -> Result<Outline> {
        let mut outline = Outline::new();
        self.get_into(glyph_id, &mut outline)?;
        Ok(outline)
    }
}

/// State for loading a glyph.
struct GlyphScaler<'a, 'b> {
    /// Backing scaler.
    scaler: &'b mut Scaler<'a>,
    /// True if hinting is enabled.
    hint: bool,
    /// Phantom points. These are 4 extra points appended to the end of an
    /// outline that allow the bytecode interpreter to produce hinted
    /// metrics. 
    /// 
    /// See https://learn.microsoft.com/en-us/typography/opentype/spec/tt_instructing_glyphs#phantom-points
    phantom: [Point; 4],
}

impl<'a, 'b> GlyphScaler<'a, 'b> {
    pub fn new(scaler: &'b mut Scaler<'a>) -> Self {
        let hint = scaler.hint.is_some();
        Self {
            scaler,
            hint,
            phantom: Default::default(),
        }
    }
}

// Loading
impl<'a, 'b> GlyphScaler<'a, 'b> {
    fn load(
        &mut self,
        glyph_id: GlyphId,
        outline: &mut Outline,
        recurse_depth: usize,
    ) -> Result<()> {
        if recurse_depth > RECURSION_LIMIT {
            return Err(Error::RecursionLimitExceeded(glyph_id));
        }
        let Some(glyph) = self.scaler.font.glyph(glyph_id) else {
            return Err(Error::GlyphNotFound(glyph_id));
        };
        let glyph = match glyph {
            Some(glyph) => glyph,
            // This is a valid empty glyph
            None => return Ok(()),
        };
        let bounds = [glyph.x_min(), glyph.x_max(), glyph.y_min(), glyph.y_max()];
        self.setup(bounds, glyph_id);
        match glyph {
            Glyph::Simple(simple) => self.load_simple(&simple, glyph_id, outline, recurse_depth),
            Glyph::Composite(composite) => {
                self.load_composite(&composite, glyph_id, outline, recurse_depth)
            }
        }
    }

    fn load_simple(
        &mut self,
        simple: &SimpleGlyph,
        glyph_id: GlyphId,
        outline: &mut Outline,
        recurse_depth: usize,
    ) -> Result<()> {
        // The base indices of the points and contours for the current glyph.
        let point_base = outline.points.len();
        let contour_base = outline.contours.len();
        let end_pts = simple.end_pts_of_contours();
        let contour_count = end_pts.len();
        let contour_end = contour_base + contour_count;
        outline
            .contours
            .extend(end_pts.iter().map(|end_pt| end_pt.get()));
        let mut point_count = simple.num_points();
        outline.tags.resize(outline.tags.len() + point_count, 0);
        outline
            .points
            .resize(outline.points.len() + point_count, Point::default());
        simple.read_points_fast(
            &mut outline.points[point_base..],
            &mut outline.tags[point_base..],
        )?;
        let ins = simple.instructions();
        self.push_phantom(outline);
        point_count += 4;
        let point_end = point_base + point_count;
        // TODO: variations
        // if state.vary {
        //     self.unscaled.clear();
        //     self.unscaled.resize(point_count, Point::new(0, 0));
        //     self.original.clear();
        //     self.original.resize(point_count, Point::new(0, 0));
        //     if state.data.deltas(
        //         state.coords,
        //         glyph_id,
        //         &self.scaled[point_base..],
        //         &mut self.tags[point_base..],
        //         &self.contours[contour_base..],
        //         &mut self.unscaled[..],
        //         &mut self.original[..],
        //     ) {
        //         for (d, p) in self.original[..point_count]
        //             .iter()
        //             .zip(self.scaled[point_base..].iter_mut())
        //         {
        //             p.x += d.x;
        //             p.y += d.y;
        //         }
        //     }
        // }
        let hinted = self.hint && !ins.is_empty();
        if hinted {
            // Hinting requires a copy of the original unscaled points.
            self.scaler.context.unscaled.clear();
            self.scaler
                .context
                .unscaled
                .extend_from_slice(&outline.points[point_base..]);
        }
        let scale = self.scaler.scale;
        if self.scaler.is_scaled {
            // Apply the scale to each point.
            for p in &mut outline.points[point_base..] {
                p.x = math::mul(p.x, scale);
                p.y = math::mul(p.y, scale);
            }
            // Save the scaled phantom points.
            self.save_phantom(outline, point_base, point_count);
        }
        if hinted {
            // Hinting requires a copy of the scaled points. These are used
            // as references when modifying an outline.
            self.scaler.context.original.clear();
            self.scaler
                .context
                .original
                .extend_from_slice(&outline.points[point_base..point_end]);
            // When hinting, round the components of the phantom points.
            for p in &mut outline.points[point_end - 4..] {
                p.x = math::round(p.x);
                p.y = math::round(p.y);
            }
            // Apply hinting to the set of contours for this outline.
            if !self.hint(outline, point_base, contour_base, ins, false) {
                return Err(Error::HintingFailed(glyph_id));
            }
        }
        if point_base != 0 {
            // If we're not the first component, shift our contour end points.
            for c in &mut outline.contours[contour_base..contour_end] {
                *c += point_base as u16;
            }
        }
        // We're done with the phantom points, so drop them.
        outline.points.truncate(outline.points.len() - 4);
        outline.tags.truncate(outline.tags.len() - 4);
        Ok(())
    }

    fn load_composite(
        &mut self,
        composite: &CompositeGlyph,
        glyph_id: GlyphId,
        outline: &mut Outline,
        recurse_depth: usize,
    ) -> Result<()> {
        // The base indices of the points and contours for the current glyph.
        let point_base = outline.points.len();
        let contour_base = outline.contours.len();
        let scale = self.scaler.scale;
        if self.scaler.is_scaled {
            for p in self.phantom.iter_mut() {
                p.x = math::mul(p.x, scale);
                p.y = math::mul(p.y, scale);
            }
        }
        // TODO: variations
        // let delta_base = self.deltas.len();
        // let mut have_deltas = false;
        // let count = composite.components().count();
        // self.deltas.resize(delta_base + count, Point::new(0, 0));
        // if state.data.composite_deltas(
        //     state.coords,
        //     glyph_id,
        //     &mut self.deltas[delta_base..],
        // ) {
        //     have_deltas = true;
        // }
        for component in composite.components() {
            // Save a copy of our phantom points.
            let phantom = self.phantom;
            // Load the component glyph and keep track of the points range.
            let start_point = outline.points.len();
            self.load(component.glyph, outline, recurse_depth + 1)?;
            let end_point = outline.points.len();
            if !component
                .flags
                .contains(CompositeGlyphFlags::USE_MY_METRICS)
            {
                // The USE_MY_METRICS flag indicates that this component's phantom
                // points should override those of the composite glyph.
                self.phantom = phantom;
            }
            // Scaling does an internal conversion to 26.6 so we don't use the
            // fixed types in read-fonts. Maybe a better solution here? We want to match
            // FreeType semantics.
            fn f2dot14_to_fixed(x: F2Dot14) -> i32 {
                i16::from_be_bytes(x.to_be_bytes()) as i32 * 4
            }
            let xx = f2dot14_to_fixed(component.transform.xx);
            let yx = f2dot14_to_fixed(component.transform.yx);
            let xy = f2dot14_to_fixed(component.transform.xy);
            let yy = f2dot14_to_fixed(component.transform.yy);
            let have_xform = component.flags.intersects(
                CompositeGlyphFlags::WE_HAVE_A_SCALE
                    | CompositeGlyphFlags::WE_HAVE_AN_X_AND_Y_SCALE
                    | CompositeGlyphFlags::WE_HAVE_A_TWO_BY_TWO,
            );
            if have_xform {
                for p in &mut outline.points[start_point..end_point] {
                    let (x, y) = math::transform(p.x, p.y, xx, yx, xy, yy);
                    p.x = x;
                    p.y = y;
                }
            }
            let anchor = component.anchor;
            let (dx, dy) = match anchor {
                Anchor::Offset { x, y } => {
                    let (mut dx, mut dy) = (x as i32, y as i32);
                    if have_xform
                        && component.flags
                            & (CompositeGlyphFlags::SCALED_COMPONENT_OFFSET
                                | CompositeGlyphFlags::UNSCALED_COMPONENT_OFFSET)
                            == CompositeGlyphFlags::SCALED_COMPONENT_OFFSET
                    {
                        // This matches the computation done in FreeType which is
                        // based on a heuristic.
                        dx = math::mul(dx, math::hypot(xx, xy));
                        dy = math::mul(dy, math::hypot(yy, yx));
                    }
                    // TODO: variations
                    // if have_deltas {
                    //     let d = self.deltas[delta_base + i];
                    //     dx += d.x;
                    //     dy += d.y;
                    // }
                    if self.scaler.is_scaled {
                        dx = math::mul(dx, scale);
                        dy = math::mul(dy, scale);
                        if self.hint
                            && component
                                .flags
                                .contains(CompositeGlyphFlags::ROUND_XY_TO_GRID)
                        {
                            // Only round the y-coordinate, per FreeType.
                            dy = math::round(dy);
                        }
                    }
                    (dx, dy)
                }
                Anchor::Point { base, component } => {
                    let (a1, a2) = (base as usize, component as usize);
                    let pi1 = point_base + a1;
                    let pi2 = start_point + a2;
                    let p1 = outline
                        .points
                        .get(pi1)
                        .ok_or(Error::InvalidAnchorPoint(glyph_id))?;
                    let p2 = outline
                        .points
                        .get(pi2)
                        .ok_or(Error::InvalidAnchorPoint(glyph_id))?;
                    (p1.x.wrapping_sub(p2.x), p1.y.wrapping_sub(p2.y))
                }
            };
            if dx != 0 || dy != 0 {
                for p in &mut outline.points[start_point..end_point] {
                    p.x += dx;
                    p.y += dy;
                }
            }
        }
        if self.hint {
            let ins = composite.instructions().unwrap_or_default();
            // TODO: variations
            // self.deltas.resize(delta_base, Point::new(0, 0));
            if !ins.is_empty() {
                // Append the current phantom points to the outline.
                self.push_phantom(outline);
                // For composite glyphs, the unscaled and original points are simply
                // copies of the current point set.
                self.scaler.context.unscaled.clear();
                self.scaler
                    .context
                    .unscaled
                    .extend_from_slice(&outline.points[point_base..]);
                self.scaler.context.original.clear();
                self.scaler
                    .context
                    .original
                    .extend_from_slice(&outline.points[point_base..]);
                let point_end = outline.points.len();
                // Round the phantom points.
                for p in &mut outline.points[point_end - 4..] {
                    p.x = math::round(p.x);
                    p.y = math::round(p.y);
                }
                // Clear the "touched" flags that are used during IUP processing.
                const TOUCHED_FLAGS: u8 = 0x08 | 0x10;
                for tag in &mut outline.tags[point_base..] {
                    *tag &= !TOUCHED_FLAGS;
                }
                if !self.hint(outline, point_base, contour_base, ins, true) {
                    return Err(Error::HintingFailed(glyph_id));
                }
                // As in simple outlines, drop the phantom points.
                outline.points.truncate(outline.points.len() - 4);
                outline.tags.truncate(outline.tags.len() - 4);
            }
        }
        Ok(())
    }
}

// Hinting
impl<'a, 'b> GlyphScaler<'a, 'b> {
    fn hint(
        &mut self,
        outline: &mut Outline,
        point_base: usize,
        contour_base: usize,
        ins: &[u8],
        is_composite: bool,
    ) -> bool {
        true
    }
}

// Per-component setup.
impl<'a, 'b> GlyphScaler<'a, 'b> {
    fn setup(&mut self, bounds: [i16; 4], glyph_id: GlyphId) {
        let font = &self.scaler.font;
        let lsb = font.lsb(glyph_id, self.scaler.coords);
        let advance = font.advance_width(glyph_id, self.scaler.coords);
        // Vertical metrics aren't significant to the glyph loading process, so
        // they are ignored.
        let vadvance = 0;
        let tsb = 0;
        // The four "phantom" points as computed by FreeType.
        self.phantom[0].x = bounds[0] as i32 - lsb;
        self.phantom[0].y = 0;
        self.phantom[1].x = self.phantom[0].x + advance;
        self.phantom[1].y = 0;
        self.phantom[2].x = advance / 2;
        self.phantom[2].y = bounds[3] as i32 + tsb;
        self.phantom[3].x = advance / 2;
        self.phantom[3].y = self.phantom[2].y - vadvance;
    }
}

// Phantom point management.
impl<'a, 'b> GlyphScaler<'a, 'b> {
    fn push_phantom(&mut self, outline: &mut Outline) {
        for i in 0..4 {
            outline.points.push(self.phantom[i]);
            outline.tags.push(0);
        }
    }

    fn save_phantom(&mut self, outline: &mut Outline, point_base: usize, point_count: usize) {
        for i in 0..4 {
            self.phantom[3 - i] = outline.points[point_base + point_count - i - 1];
        }
    }
}

/// Slot for the hinting cache.
#[derive(Copy, Clone)]
enum CacheSlot {
    /// Uncached font.
    Uncached,
    /// Font and size cache indices.
    Cached(usize, usize),
}

// Cache management and hinting.
impl Context {
    /// Prepares for the cache for hinting.
    fn prepare_for_hinting(
        &mut self,
        font: &Font,
        font_id: Option<u64>,
        coords: &[NormalizedCoord],
        ppem: u16,
        scale: i32,
        mode: Hinting,
    ) -> Option<CacheSlot> {
        None
    }

    #[allow(clippy::too_many_arguments)]
    fn hint(
        &mut self,
        data: &Font,
        coords: &[NormalizedCoord],
        slot: CacheSlot,
        scaled: &mut [Point],
        tags: &mut [u8],
        contours: &mut [u16],
        phantom: &mut [Point],
        point_base: usize,
        contour_base: usize,
        ins: &[u8],
        is_composite: bool,
    ) -> bool {
        true
    }
}

/// Contains the tables and limits necessary for loading, scaling and hinting
/// a TrueType glyph.
#[derive(Clone)]
pub struct Font<'a> {
    pub glyf: Glyf<'a>,
    pub loca: Loca<'a>,
    pub hmtx: Hmtx<'a>,
    pub hvar: Option<Hvar<'a>>,
    pub fpgm: &'a [u8],
    pub prep: &'a [u8],
    pub cvt: &'a [BigEndian<i16>],
    pub units_per_em: u16,
    pub glyph_count: u16,
    pub max_storage: u16,
    pub max_stack: u16,
    pub max_function_defs: u16,
    pub max_instruction_defs: u16,
    pub max_twilight: u16,
    pub axis_count: u16,
}

impl<'a> Font<'a> {
    pub fn new(font: &impl TableProvider<'a>) -> Result<Self> {
        let glyf = font.glyf()?;
        let loca = font.loca(None)?;
        let hmtx = font.hmtx()?;
        let hvar = font.hvar().ok();
        let upem = font.head()?.units_per_em();
        let fpgm = font
            .data_for_tag(Tag::new(b"fpgm"))
            .map(|data| data.read_array(0..data.len()).unwrap())
            .unwrap_or_default();
        let prep = font
            .data_for_tag(Tag::new(b"prep"))
            .map(|data| data.read_array(0..data.len()).unwrap())
            .unwrap_or_default();
        let cvt = font
            .data_for_tag(Tag::new(b"cvt"))
            .and_then(|data| data.read_array(0..data.len()).ok())
            .unwrap_or_default();
        let maxp = font.maxp()?;
        let glyph_count = maxp.num_glyphs();
        let axis_count = font.fvar().map(|fvar| fvar.axis_count()).unwrap_or(0);
        Ok(Self {
            glyf,
            loca,
            hmtx,
            hvar,
            fpgm,
            prep,
            cvt,
            glyph_count,
            units_per_em: upem,
            max_storage: maxp.max_storage().unwrap_or(0),
            max_stack: maxp.max_stack_elements().unwrap_or(0),
            max_function_defs: maxp.max_function_defs().unwrap_or(0),
            max_instruction_defs: maxp.max_instruction_defs().unwrap_or(0),
            max_twilight: maxp.max_twilight_points().unwrap_or(0),
            axis_count,
        })
    }

    fn glyph(&self, gid: GlyphId) -> Option<Option<Glyph<'a>>> {
        self.loca.get_glyf(gid, &self.glyf).ok()
    }

    fn advance_width(&self, gid: GlyphId, coords: &[NormalizedCoord]) -> i32 {
        let default_advance = self
            .hmtx
            .h_metrics()
            .last()
            .map(|metric| metric.advance())
            .unwrap_or(0);
        let mut advance = self
            .hmtx
            .h_metrics()
            .get(gid.to_u16() as usize)
            .map(|metric| metric.advance())
            .unwrap_or(default_advance) as i32;
        if let Some(hvar) = &self.hvar {
            advance += hvar
                .advance_width_delta(gid, coords)
                // FreeType truncates metric deltas...
                .map(|delta| delta.to_f64() as i32)
                .unwrap_or(0);
        }
        advance
    }

    fn lsb(&self, gid: GlyphId, coords: &[NormalizedCoord]) -> i32 {
        let gid_index = gid.to_u16() as usize;
        let mut lsb = self
            .hmtx
            .h_metrics()
            .get(gid_index)
            .map(|metric| metric.side_bearing())
            .unwrap_or_else(|| {
                self.hmtx
                    .left_side_bearings()
                    .get(gid_index.saturating_sub(self.hmtx.h_metrics().len()))
                    .map(|lsb| lsb.get())
                    .unwrap_or(0)
            }) as i32;
        if let Some(hvar) = &self.hvar {
            lsb += hvar
                .lsb_delta(gid, coords)
                // FreeType truncates metric deltas...
                .map(|delta| delta.to_f64() as i32)
                .unwrap_or(0);
        }
        lsb
    }

    pub(crate) fn scale_cvt(&self, scale: Option<i32>, scaled_cvt: &mut Vec<i32>) {
        if scaled_cvt.len() < self.cvt.len() {
            scaled_cvt.resize(self.cvt.len(), 0);
        }
        for (src, dest) in self.cvt.iter().zip(scaled_cvt.iter_mut()) {
            *dest = src.get() as i32 * 64;
        }
        if let Some(scale) = scale {
            let scale = scale >> 6;
            for value in &mut scaled_cvt[..] {
                *value = super::math::mul(*value, scale);
            }
        }
    }
}
