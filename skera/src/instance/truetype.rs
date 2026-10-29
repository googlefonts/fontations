//! TrueType instancing keeps point order, component references, and bytecode.
//! Follows HarfBuzz's gvar accumulator and OT/glyf/Glyph.hh point traversal,
//! glyph compilation, bounds, and phantom-point metric updates.
use super::{
    metrics,
    tuple::{compile_gvar, TupleVariations},
};
use crate::SubsetError;
use read_fonts::{
    tables::{glyf::Glyph as ReadGlyph, gvar::GlyphDelta, variations::TupleVariation},
    types::{GlyphId, Tag},
    FontRead, FontRef, TableProvider,
};
use std::collections::BTreeMap;
use write_fonts::{
    from_obj::ToOwnedTable,
    tables::glyf::{Anchor, Bbox, Glyph},
};

type Point = [f32; 2];
pub(super) struct TrueTypeContext<'a, 'b> {
    pub input: &'a super::FontContext<'b>,
    pub metrics: &'a mut metrics::MetricTables,
}
pub(super) struct TrueTypeTables {
    pub glyf: Vec<u8>,
    pub loca: Vec<u8>,
    pub gvar: super::TableChange,
    pub cvar: super::TableChange,
    pub cvt: Option<Vec<u8>>,
}
struct GvarGlyphContext<'a, 'b> {
    input: &'a super::FontContext<'b>,
    gid: GlyphId,
    points: &'a mut Vec<Point>,
    original_points: &'a [Point],
    contours: &'a [usize],
}
struct CvarContext<'a, 'b> {
    input: &'a super::FontContext<'b>,
    cvt: &'a mut Vec<u8>,
}
fn error(tag: &[u8; 4]) -> SubsetError {
    SubsetError::SubsetTableError(Tag::new(tag))
}
fn round(value: f32) -> i32 {
    super::round_f32(value) as i32
}
fn coord(value: f32) -> i16 {
    round(value).clamp(-32768, 32767) as i16
}

fn put(data: &mut [u8], offset: usize, value: i16) -> Result<(), SubsetError> {
    data.get_mut(offset..offset + 2)
        .ok_or_else(|| error(b"head"))?
        .copy_from_slice(&value.to_be_bytes());
    Ok(())
}

struct InstanceGlyph {
    glyph: Glyph,
    points: Vec<Point>,
    phantoms: [Point; 4],
}

/// Expand sparse tuples before changing the default outline. Phantom points
/// and composite component offsets never participate in IUP interpolation.
fn calc_inferred_deltas(
    tuple: &TupleVariation<GlyphDelta>,
    points: &[Point],
    contours: &[usize],
    scalar: f32,
) -> Result<Vec<Point>, SubsetError> {
    let mut deltas = vec![[0.; 2]; points.len()];
    let mut touched = vec![false; points.len()];
    for delta in tuple.deltas() {
        let i = delta.position as usize;
        let out = deltas.get_mut(i).ok_or_else(|| error(b"gvar"))?;
        *out = [delta.x_delta as f32 * scalar, delta.y_delta as f32 * scalar];
        touched[i] = true;
    }
    let mut start = 0;
    for &end in contours {
        if end < start || end >= points.len().saturating_sub(4) {
            return Err(error(b"glyf"));
        }
        let refs: Vec<_> = (start..=end).filter(|&i| touched[i]).collect();
        for (j, &a) in refs.iter().enumerate() {
            let b = refs[(j + 1) % refs.len()];
            let mut i = if a == end { start } else { a + 1 };
            while i != b {
                for axis in 0..2 {
                    deltas[i][axis] = super::iup_delta(
                        points[i][axis],
                        points[a][axis],
                        points[b][axis],
                        deltas[a][axis],
                        deltas[b][axis],
                    );
                }
                i = if i == end { start } else { i + 1 };
            }
        }
        start = end + 1;
    }
    Ok(deltas)
}

// Match HarfBuzz's dense fast path and sparse-tuple scratch-buffer flushing.
fn any_private_points(bytes: &[u8], axis_count: usize) -> Result<bool, SubsetError> {
    let count = u16::from_be_bytes(
        bytes
            .get(..2)
            .ok_or_else(|| error(b"gvar"))?
            .try_into()
            .unwrap(),
    ) & 0xfff;
    let mut pos = 4;
    let mut any = false;
    for _ in 0..count {
        let flags = u16::from_be_bytes(
            bytes
                .get(pos + 2..pos + 4)
                .ok_or_else(|| error(b"gvar"))?
                .try_into()
                .unwrap(),
        );
        any |= flags & 0x2000 != 0;
        pos += 4 + 2
            * axis_count
            * (usize::from(flags & 0x8000 != 0) + 2 * usize::from(flags & 0x4000 != 0));
        if pos > bytes.len() {
            return Err(error(b"gvar"));
        }
    }
    Ok(any)
}

fn add_deltas(points: &mut [Point], deltas: &[Point]) {
    for (point, delta) in points.iter_mut().zip(deltas) {
        for axis in 0..2 {
            point[axis] += delta[axis];
        }
    }
}

impl super::Instance<GvarGlyphContext<'_, '_>> for read_fonts::tables::gvar::Gvar<'_> {
    type Output = TupleVariations;
    fn instance(
        &self,
        context: &mut GvarGlyphContext<'_, '_>,
    ) -> Result<Self::Output, SubsetError> {
        let gvar = self;
        let axes = context.input.axes;
        let gid = context.gid;
        let points = &mut *context.points;
        let orig_points = context.original_points;
        let contours = context.contours;
        let mut residual = TupleVariations::new();

        if let Some(data) = gvar.glyph_variation_data(gid).map_err(|_| error(b"gvar"))? {
            let private = any_private_points(
                gvar.data_for_gid(gid)
                    .map_err(|_| error(b"gvar"))?
                    .ok_or_else(|| error(b"gvar"))?
                    .as_bytes(),
                axes.pinned.len(),
            )?;
            let mut pending = vec![[0.; 2]; points.len()];
            let mut flush = false;
            for tuple in data.tuples() {
                if !axes.all_pinned() {
                    let mut deltas = vec![[0.; 2]; orig_points.len()];
                    let mut referenced = vec![false; orig_points.len()];
                    for d in tuple.deltas() {
                        let i = d.position as usize;
                        *deltas.get_mut(i).ok_or_else(|| error(b"gvar"))? =
                            [d.x_delta as f32, d.y_delta as f32];
                        referenced[i] = true;
                    }
                    residual.add(&tuple, axes.pinned.len(), deltas, referenced)?;
                }
                let Some(scalar) = tuple.compute_scalar_f32(&axes.metric_coords) else {
                    continue;
                };
                let deltas = calc_inferred_deltas(&tuple, orig_points, contours, scalar)?;
                if tuple.has_deltas_for_all_points() && !private {
                    add_deltas(points, &deltas);
                    continue;
                }
                if !tuple.has_deltas_for_all_points() {
                    if flush {
                        add_deltas(points, &pending);
                    }
                    pending.fill([0.; 2]);
                }
                add_deltas(&mut pending, &deltas);
                flush = true;
            }
            if flush {
                add_deltas(points, &pending);
            }
        }

        Ok(residual)
    }
}

impl super::Instance<TrueTypeContext<'_, '_>> for read_fonts::tables::glyf::Glyf<'_> {
    type Output = TrueTypeTables;
    fn instance(&self, context: &mut TrueTypeContext<'_, '_>) -> Result<Self::Output, SubsetError> {
        use super::TableChange;
        let font = context.input.font;
        let axes = context.input.axes;
        let tables = &mut *context.metrics;
        let optimize_iup = context
            .input
            .flags
            .contains(crate::SubsetFlags::SUBSET_FLAGS_OPTIMIZE_IUP_DELTAS);
        let glyf = self;
        let loca = font.loca(None).map_err(|_| error(b"loca"))?;
        let count = font.maxp().map_err(|_| error(b"maxp"))?.num_glyphs() as usize;
        let hmtx = font.hmtx().map_err(|_| error(b"hmtx"))?;
        let vmtx = font.vmtx().ok();
        let gvar = font
            .data_for_tag(Tag::new(b"gvar"))
            .map(|_| font.gvar().map_err(|_| error(b"gvar")))
            .transpose()?;
        if gvar
            .as_ref()
            .is_some_and(|g| g.axis_count() as usize != axes.pinned.len())
        {
            return Err(error(b"gvar"));
        }
        let mut glyphs = Vec::with_capacity(count);
        let mut variations = Vec::with_capacity(count);
        for i in 0..count {
            let gid = GlyphId::new(i as u32);
            let source = loca
                .get(gid, glyf)
                .ok_or_else(|| error(b"glyf"))?
                .into_glyph();
            let mut points = Vec::new();
            let mut contours = Vec::new();
            let glyph = match source {
                None => Glyph::Empty,
                Some(ReadGlyph::Simple(g)) => {
                    points.extend(g.points().map(|p| [p.x as f32, p.y as f32]));
                    if points.len() != g.num_points() {
                        return Err(error(b"glyf"));
                    }
                    contours.extend(g.end_pts_of_contours().iter().map(|n| n.get() as usize));
                    Glyph::Simple(g.to_owned_table())
                }
                Some(ReadGlyph::Composite(g)) => {
                    let owned: write_fonts::tables::glyf::CompositeGlyph = g.to_owned_table();
                    points.extend(owned.components().iter().map(|c| match c.anchor {
                        Anchor::Offset { x, y } => [x as f32, y as f32],
                        Anchor::Point { .. } => [0.; 2],
                    }));
                    Glyph::Composite(owned)
                }
            };
            let bbox = glyph.bbox().unwrap_or_default();
            let left = bbox.x_min as f32 - hmtx.side_bearing(gid).unwrap_or(0) as f32;
            let top = bbox.y_max as f32
                + vmtx.as_ref().and_then(|v| v.side_bearing(gid)).unwrap_or(0) as f32;
            points.extend([
                [left, 0.],
                [left + hmtx.advance(gid).unwrap_or(0) as f32, 0.],
                [0., top],
                [
                    0.,
                    top - vmtx.as_ref().and_then(|v| v.advance(gid)).unwrap_or(0) as f32,
                ],
            ]);
            let orig_points = points.clone();
            let mut residual = TupleVariations::new();
            if let Some(gvar) = &gvar {
                let mut input = GvarGlyphContext {
                    input: context.input,
                    gid,
                    points: &mut points,
                    original_points: &orig_points,
                    contours: &contours,
                };
                residual = gvar.instance(&mut input)?;
            }
            if !axes.all_pinned() {
                let mut iup_points = optimize_iup.then(|| orig_points.clone());
                residual.instantiate(
                    font,
                    axes,
                    Some((&orig_points, &contours)),
                    iup_points.as_deref_mut(),
                )?;
                if let Some(points) = iup_points {
                    residual.optimize(&points, &contours, matches!(glyph, Glyph::Composite(_)))?;
                }
            }
            variations.push(residual);
            let phantoms = points.split_off(points.len() - 4).try_into().unwrap();
            glyphs.push(InstanceGlyph {
                glyph,
                points,
                phantoms,
            });
        }
        let mut bounds = Vec::with_capacity(count);
        for gid in 0..count {
            let mut budget = 1_000_000usize;
            let points = get_points(gid, &glyphs, 0, &mut budget)?;
            // A composite remains a glyph for header statistics even when all
            // its components have no contour points, as in HarfBuzz's compiler.
            bounds.push(point_bounds(&points).or_else(|| {
                matches!(&glyphs[gid].glyph, Glyph::Composite(_)).then_some(Bbox::default())
            }));
        }
        let mut glyf_out = Vec::new();
        let mut offsets = Vec::with_capacity(count + 1);
        for (g, bbox) in glyphs.iter_mut().zip(&bounds) {
            offsets.push(u32::try_from(glyf_out.len()).map_err(|_| error(b"glyf"))?);
            match &mut g.glyph {
                Glyph::Empty => {}
                Glyph::Simple(simple) => {
                    simple.bbox = bbox.unwrap_or_default();
                    // Encode coordinate differences with wrapping arithmetic: the
                    // format stores signed 16-bit deltas even across the full range.
                    encode_simple(simple, &g.points, &mut glyf_out)?;
                }
                Glyph::Composite(composite) => {
                    composite.bbox = bbox.unwrap_or_default();
                    for (component, point) in composite.components_mut().iter_mut().zip(&g.points) {
                        if matches!(component.anchor, Anchor::Offset { .. }) {
                            component.anchor = Anchor::Offset {
                                x: coord(point[0]),
                                y: coord(point[1]),
                            };
                        }
                    }
                    encode_composite(composite, &mut glyf_out)?;
                }
            }
            if glyf_out.len() % 2 != 0 {
                glyf_out.push(0);
            }
        }
        offsets.push(u32::try_from(glyf_out.len()).map_err(|_| error(b"glyf"))?);
        let short = glyf_out.len() <= 0x1fffe;
        let mut loca_out = Vec::new();
        for offset in offsets {
            if short {
                loca_out.extend(((offset / 2) as u16).to_be_bytes());
            } else {
                loca_out.extend(offset.to_be_bytes());
            }
        }
        let gvar = if axes.all_pinned() {
            TableChange::Remove
        } else if gvar.is_some() {
            TableChange::Replace(compile_gvar(
                &variations,
                axes.pinned.iter().filter(|&&p| !p).count() as u16,
            )?)
        } else {
            TableChange::Keep
        };
        let head = tables.head.as_mut().ok_or_else(|| error(b"head"))?;
        put(head, 50, i16::from(!short))?;
        let union = bounds
            .iter()
            .flatten()
            .copied()
            .reduce(Bbox::union)
            .unwrap_or_default();
        for (i, v) in [union.x_min, union.y_min, union.x_max, union.y_max]
            .into_iter()
            .enumerate()
        {
            put(head, 36 + i * 2, v)?;
        }
        let all_lsb = glyphs
            .iter()
            .zip(&bounds)
            .all(|(g, b)| b.is_none() || round(g.phantoms[0][0]) == 0);
        let flags = font.head().map_err(|_| error(b"head"))?.flags().bits();
        put(
            head,
            16,
            ((flags & !2) | if all_lsb { 2 } else { 0 }) as i16,
        )?;
        write_metrics(font, &glyphs, &bounds, tables)?;
        let (cvar, cvt) = if let Some(data) = font.data_for_tag(Tag::new(b"cvar")) {
            let cvar = read_fonts::tables::cvar::Cvar::read(data).map_err(|_| error(b"cvar"))?;
            let mut cvt = font
                .data_for_tag(Tag::new(b"cvt "))
                .ok_or_else(|| error(b"cvt "))?
                .as_bytes()
                .to_vec();
            let mut input = CvarContext {
                input: context.input,
                cvt: &mut cvt,
            };
            (cvar.instance(&mut input)?, Some(cvt))
        } else {
            (TableChange::Keep, None)
        };
        metrics::finish(font, axes, tables)?;
        Ok(TrueTypeTables {
            glyf: glyf_out,
            loca: loca_out,
            gvar,
            cvar,
            cvt,
        })
    }
}

fn point_bounds(points: &[Point]) -> Option<Bbox> {
    let first = *points.first()?;
    let mut min = first;
    let mut max = first;
    for point in points {
        for axis in 0..2 {
            min[axis] = min[axis].min(point[axis]);
            max[axis] = max[axis].max(point[axis]);
        }
    }
    Some(Bbox {
        x_min: coord(min[0]),
        y_min: coord(min[1]),
        x_max: coord(max[0]),
        y_max: coord(max[1]),
    })
}

// Resolve continuous component points for bounds before rounding the stored
// coordinates. A depth and work bound also rejects cycles and explosive trees.
fn get_points(
    gid: usize,
    glyphs: &[InstanceGlyph],
    depth: usize,
    budget: &mut usize,
) -> Result<Vec<Point>, SubsetError> {
    if depth > 64 {
        return Err(error(b"glyf"));
    }
    let g = glyphs.get(gid).ok_or_else(|| error(b"glyf"))?;
    *budget = budget
        .checked_sub(g.points.len() + 1)
        .ok_or_else(|| error(b"glyf"))?;
    let Glyph::Composite(composite) = &g.glyph else {
        return Ok(g.points.clone());
    };
    let mut points: Vec<Point> = Vec::new();
    for (component, offset) in composite.components().iter().zip(&g.points) {
        let mut child = get_points(component.glyph.to_u32() as usize, glyphs, depth + 1, budget)?;
        let t = component.transform;
        let scaled =
            component.flags.scaled_component_offset && !component.flags.unscaled_component_offset;
        let trans = if matches!(component.anchor, Anchor::Offset { .. }) {
            *offset
        } else {
            [0.; 2]
        };
        for p in &mut child {
            if scaled {
                p[0] += trans[0];
                p[1] += trans[1];
            }
            *p = [
                p[0] * t.xx.to_f32() + p[1] * t.xy.to_f32(),
                p[0] * t.yx.to_f32() + p[1] * t.yy.to_f32(),
            ];
            if !scaled {
                p[0] += trans[0];
                p[1] += trans[1];
            }
        }
        if let Anchor::Point { base, component } = component.anchor {
            let a = points.get(base as usize).ok_or_else(|| error(b"glyf"))?;
            let b = child
                .get(component as usize)
                .ok_or_else(|| error(b"glyf"))?;
            let delta = [a[0] - b[0], a[1] - b[1]];
            for p in &mut child {
                p[0] += delta[0];
                p[1] += delta[1];
            }
        }
        points.extend(child);
    }
    Ok(points)
}

// Composite glyph records contain no offsets. Append them without constructing
// a table graph for every glyph, while retaining write-fonts' owned glyph model.
fn encode_composite(
    glyph: &write_fonts::tables::glyf::CompositeGlyph,
    out: &mut Vec<u8>,
) -> Result<(), SubsetError> {
    use read_fonts::tables::glyf::CompositeGlyphFlags as Flags;
    let instructions = glyph.instructions();
    if glyph.components().is_empty() || instructions.len() > u16::MAX as usize {
        return Err(error(b"glyf"));
    }
    let start = out.len();
    out.extend((-1i16).to_be_bytes());
    for value in [
        glyph.bbox.x_min,
        glyph.bbox.y_min,
        glyph.bbox.x_max,
        glyph.bbox.y_max,
    ] {
        out.extend(value.to_be_bytes());
    }
    for (i, component) in glyph.components().iter().enumerate() {
        let mut flags: Flags = component.flags.into();
        flags |= component.anchor.compute_flags() | component.transform.compute_flags();
        if component.glyph.to_u32() > u16::MAX as u32 {
            flags.insert(Flags::GID_IS_24_BIT);
        } else {
            flags.remove(Flags::GID_IS_24_BIT);
        }
        if i + 1 < glyph.components().len() {
            flags |= Flags::MORE_COMPONENTS;
        } else if !instructions.is_empty() {
            flags |= Flags::WE_HAVE_INSTRUCTIONS;
        }
        out.extend(flags.bits().to_be_bytes());
        if flags.contains(Flags::GID_IS_24_BIT) {
            out.extend(
                read_fonts::types::Uint24::checked_new(component.glyph.to_u32())
                    .ok_or_else(|| error(b"glyf"))?
                    .to_be_bytes(),
            );
        } else {
            out.extend(
                u16::try_from(component.glyph.to_u32())
                    .map_err(|_| error(b"glyf"))?
                    .to_be_bytes(),
            );
        }
        let wide = flags.contains(Flags::ARG_1_AND_2_ARE_WORDS);
        match component.anchor {
            Anchor::Offset { x, y } => {
                if wide {
                    out.extend(x.to_be_bytes());
                    out.extend(y.to_be_bytes());
                } else {
                    out.extend([x as u8, y as u8]);
                }
            }
            Anchor::Point { base, component } => {
                if wide {
                    out.extend(base.to_be_bytes());
                    out.extend(component.to_be_bytes());
                } else {
                    out.extend([base as u8, component as u8]);
                }
            }
        }
        let t = component.transform;
        if flags.contains(Flags::WE_HAVE_A_TWO_BY_TWO) {
            for v in [t.xx, t.yx, t.xy, t.yy] {
                out.extend(v.to_bits().to_be_bytes());
            }
        } else if flags.contains(Flags::WE_HAVE_AN_X_AND_Y_SCALE) {
            for v in [t.xx, t.yy] {
                out.extend(v.to_bits().to_be_bytes());
            }
        } else if flags.contains(Flags::WE_HAVE_A_SCALE) {
            out.extend(t.xx.to_bits().to_be_bytes());
        }
    }
    if !instructions.is_empty() {
        out.extend((instructions.len() as u16).to_be_bytes());
        out.extend(instructions);
    }
    if (out.len() - start) % 2 != 0 {
        out.push(0);
    }
    Ok(())
}

fn encode_simple(
    g: &mut write_fonts::tables::glyf::SimpleGlyph,
    points: &[Point],
    out: &mut Vec<u8>,
) -> Result<(), SubsetError> {
    if g.contours.is_empty() && g.instructions.is_empty() {
        return Ok(());
    }
    let count: usize = g.contours.iter().map(|c| c.len()).sum();
    if count != points.len()
        || count > usize::from(u16::MAX) + 1
        || g.contours.iter().any(|c| c.is_empty())
        || g.contours.len() > i16::MAX as usize
        || g.instructions.len() > u16::MAX as usize
    {
        return Err(error(b"glyf"));
    }
    let mut points = points.iter();
    for contour in &mut g.contours {
        // Reuse the contour allocation while updating coordinates in place.
        let mut curve_points: Vec<_> = std::mem::take(contour).into();
        for source in &mut curve_points {
            let point = points.next().unwrap();
            source.x = coord(point[0]);
            source.y = coord(point[1]);
        }
        *contour = curve_points.into();
    }
    out.extend(write_fonts::dump_table(g).map_err(|_| error(b"glyf"))?);
    Ok(())
}

fn write_metrics(
    font: &FontRef,
    glyphs: &[InstanceGlyph],
    bounds: &[Option<Bbox>],
    tables: &mut metrics::MetricTables,
) -> Result<(), SubsetError> {
    for vertical in [false, true] {
        let (tag, header) = if vertical {
            (b"vmtx", b"vhea")
        } else {
            (b"hmtx", b"hhea")
        };
        if font.data_for_tag(Tag::new(tag)).is_none() {
            continue;
        }
        let mut out = Vec::new();
        let mut max_advance = 0;
        let mut min_leading = i32::MAX;
        let mut min_trailing = i32::MAX;
        let mut max_extent = i32::MIN;
        for (g, bbox) in glyphs.iter().zip(bounds) {
            let bbox = bbox.unwrap_or_default();
            let (advance, leading, extent) = if vertical {
                (
                    g.phantoms[2][1] - g.phantoms[3][1],
                    bbox.y_max as f32 - g.phantoms[2][1],
                    bbox.y_max as i32 - bbox.y_min as i32,
                )
            } else {
                (
                    g.phantoms[1][0] - g.phantoms[0][0],
                    g.phantoms[0][0] - bbox.x_min as f32,
                    bbox.x_max as i32 - bbox.x_min as i32,
                )
            };
            let advance = round(advance).clamp(0, 65535);
            let leading = round(-leading).clamp(-32768, 32767);
            out.extend((advance as u16).to_be_bytes());
            out.extend((leading as i16).to_be_bytes());
            max_advance = max_advance.max(advance);
            if bounds[out.len() / 4 - 1].is_some() {
                min_leading = min_leading.min(leading);
                min_trailing = min_trailing.min(advance - leading - extent);
                max_extent = max_extent.max(leading + extent);
            }
        }
        let h = if vertical {
            tables.vhea.as_mut()
        } else {
            tables.hhea.as_mut()
        }
        .ok_or_else(|| error(header))?;
        put(h, 10, max_advance as u16 as i16)?;
        if min_leading != i32::MAX {
            for (offset, value) in [(12, min_leading), (14, min_trailing), (16, max_extent)] {
                put(h, offset, value.clamp(-32768, 32767) as i16)?;
            }
        }
        put(h, 34, glyphs.len() as u16 as i16)?;
        if vertical {
            tables.vmtx = Some(out);
        } else {
            tables.hmtx = Some(out);
        }
    }
    Ok(())
}

impl super::Instance<CvarContext<'_, '_>> for read_fonts::tables::cvar::Cvar<'_> {
    type Output = super::TableChange;
    fn instance(&self, context: &mut CvarContext<'_, '_>) -> Result<Self::Output, SubsetError> {
        let cvar = self;
        let font = context.input.font;
        let axes = context.input.axes;
        let cvt = &mut *context.cvt;
        if cvt.len() % 2 != 0 {
            return Err(error(b"cvt "));
        }
        let mut deltas = vec![0f32; cvt.len() / 2];
        let mut residual = TupleVariations::new();
        let data = cvar
            .variation_data(axes.pinned.len() as u16)
            .ok_or_else(|| error(b"cvar"))?;
        for tuple in data.tuples() {
            if !axes.all_pinned() {
                let mut values = vec![[0.; 2]; deltas.len()];
                let mut referenced = vec![false; deltas.len()];
                for d in tuple.deltas() {
                    let i = d.position as usize;
                    values.get_mut(i).ok_or_else(|| error(b"cvar"))?[0] = d.value as f32;
                    referenced[i] = true;
                }
                residual.add(&tuple, axes.pinned.len(), values, referenced)?;
            }
            let Some(scalar) = tuple.compute_scalar_f32(&axes.metric_coords) else {
                continue;
            };
            for d in tuple.deltas() {
                *deltas
                    .get_mut(d.position as usize)
                    .ok_or_else(|| error(b"cvar"))? += scalar * d.value as f32;
            }
        }
        for (bytes, delta) in cvt.chunks_exact_mut(2).zip(deltas) {
            let value = i16::from_be_bytes(bytes.try_into().unwrap()) as f32;
            bytes.copy_from_slice(&coord(value + delta).to_be_bytes());
        }
        if axes.all_pinned() {
            Ok(super::TableChange::Remove)
        } else {
            residual.instantiate(font, axes, None, None)?;
            let data = residual.compile_bytes(&BTreeMap::new(), false)?;
            if data.is_empty() {
                Ok(super::TableChange::Remove)
            } else {
                let mut bytes = vec![0, 1, 0, 0];
                bytes.extend(data);
                Ok(super::TableChange::Replace(bytes))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use write_fonts::tables::glyf::{Component, ComponentFlags, CompositeGlyph, Transform};

    #[test]
    fn simple_glyph_serialization_keeps_cubic_flags() {
        let bytes = font_test_data::extended::simple_glyph(&[
            (0, 0, 0x81),
            (20, 80, 0x80),
            (60, 80, 0x80),
            (80, 0, 0x81),
        ]);
        let source = read_fonts::tables::glyf::SimpleGlyph::read(bytes.as_slice().into()).unwrap();
        let original = source.points().collect::<Vec<_>>();
        let points = original
            .iter()
            .map(|p| [p.x as f32, p.y as f32])
            .collect::<Vec<_>>();
        let mut owned: write_fonts::tables::glyf::SimpleGlyph = source.to_owned_table();
        let mut out = Vec::new();
        encode_simple(&mut owned, &points, &mut out).unwrap();
        let mut expected = write_fonts::dump_table(&owned).unwrap();
        expected.truncate(out.len()); // The caller supplies glyph alignment.
        assert_eq!(out, expected);
        let actual = read_fonts::tables::glyf::SimpleGlyph::read(out.as_slice().into()).unwrap();
        assert_eq!(actual.points().collect::<Vec<_>>(), original);
    }

    #[test]
    fn cyclic_composites_return_an_error() {
        let glyph = InstanceGlyph {
            glyph: Glyph::Composite(CompositeGlyph::new(
                Component::new(
                    read_fonts::types::GlyphId::new(0),
                    Anchor::Offset { x: 0, y: 0 },
                    Transform::default(),
                    ComponentFlags::default(),
                ),
                Bbox::default(),
            )),
            points: vec![[0.; 2]],
            phantoms: [[0.; 2]; 4],
        };
        assert!(get_points(0, &[glyph], 0, &mut 1000).is_err());
    }

    #[test]
    fn coincident_iup_references_with_different_deltas_do_not_move_the_gap() {
        assert_eq!(super::super::iup_delta(100., 50., 50., 10., 20.), 0.);
        assert_eq!(super::super::iup_delta(100., 50., 50., 10., 10.), 10.);
        assert_eq!(super::super::iup_delta(100., 200., 0., 20., 0.), 10.);
    }

    #[test]
    fn composite_glyph_ids_keep_their_width_and_reject_overflow() {
        use write_fonts::tables::glyf::{Component, ComponentFlags, CompositeGlyph, Transform};
        let glyph = |gid| {
            CompositeGlyph::new(
                Component::new(
                    GlyphId::new(gid),
                    Anchor::Offset { x: 0, y: 0 },
                    Transform::default(),
                    ComponentFlags::default(),
                ),
                Bbox::default(),
            )
        };
        for gid in [0xffff, 0x10000, 0xffffff] {
            let source = glyph(gid);
            let mut actual = Vec::new();
            encode_composite(&source, &mut actual).unwrap();
            assert_eq!(actual, write_fonts::dump_table(&source).unwrap());
            let borrowed =
                read_fonts::tables::glyf::CompositeGlyph::read(read_fonts::FontData::new(&actual))
                    .unwrap();
            assert_eq!(
                borrowed.components().next().unwrap().glyph,
                GlyphId::new(gid)
            );
        }
        assert!(encode_composite(&glyph(0x1000000), &mut Vec::new()).is_err());
    }

    #[test]
    fn direct_composite_bytes_match_writer_for_anchors_transforms_and_instructions() {
        use read_fonts::types::{F2Dot14, GlyphId};
        use write_fonts::tables::glyf::{Component, ComponentFlags, CompositeGlyph, Transform};
        for anchor in [
            Anchor::Offset { x: -128, y: 127 },
            Anchor::Offset { x: -129, y: 128 },
            Anchor::Point {
                base: 255,
                component: 0,
            },
            Anchor::Point {
                base: 256,
                component: 65535,
            },
        ] {
            for transform in [
                Transform::default(),
                Transform {
                    xx: F2Dot14::from_f32(0.5),
                    yy: F2Dot14::from_f32(0.5),
                    ..Default::default()
                },
                Transform {
                    xx: F2Dot14::from_f32(0.5),
                    yy: F2Dot14::from_f32(0.75),
                    ..Default::default()
                },
                Transform {
                    xy: F2Dot14::from_f32(0.25),
                    yx: F2Dot14::from_f32(-0.25),
                    ..Default::default()
                },
            ] {
                let component = Component::new(
                    GlyphId::new(13),
                    anchor,
                    transform,
                    ComponentFlags {
                        round_xy_to_grid: true,
                        use_my_metrics: true,
                        scaled_component_offset: true,
                        overlap_compound: true,
                        ..Default::default()
                    },
                );
                let mut glyph = CompositeGlyph::new(component, Bbox::default());
                glyph.set_instructions(&[0x01, 0x02, 0xff]);
                let expected = write_fonts::dump_table(&glyph).unwrap();
                let mut actual = vec![0xaa];
                encode_composite(&glyph, &mut actual).unwrap();
                assert_eq!(&actual[1..], expected);
            }
        }
    }
}
