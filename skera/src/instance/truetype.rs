//! TrueType instancing keeps point order, component references, and bytecode.
//! Follows HarfBuzz's gvar accumulator and OT/glyf/Glyph.hh point traversal,
//! glyph compilation, bounds, and phantom-point metric updates.
use super::{
    metrics,
    tuple::{compile_gvar, TupleVariations},
    AxisPlan,
};
use crate::SubsetError;
use std::collections::BTreeMap;
use write_fonts::{
    from_obj::ToOwnedTable,
    read::{
        tables::{
            glyf::{Glyph as ReadGlyph, PointFlags},
            gvar::GlyphDelta,
            variations::{TupleDelta, TupleVariation},
        },
        FontRead, FontRef, TableProvider,
    },
    tables::glyf::{Anchor, Bbox, Glyph},
    types::{F2Dot14, GlyphId, Tag},
};

type Point = [f32; 2];
type Tables = BTreeMap<Tag, Vec<u8>>;
fn error(tag: &[u8; 4]) -> SubsetError {
    SubsetError::SubsetTableError(Tag::new(tag))
}
fn round(value: f32) -> i32 {
    (value + 0.5).floor() as i32
}
fn coord(value: f32) -> i16 {
    round(value).clamp(-32768, 32767) as i16
}

// TupleVariationHeader::calculate_scalar accumulates in double precision;
// HarfBuzz's outline/CVT consumers cast the final result to float once.
fn tuple_scalar<T: TupleDelta>(tuple: &TupleVariation<T>, coords: &[F2Dot14]) -> Option<f32> {
    let peak = tuple.peak();
    if peak.len() != coords.len() {
        return None;
    }
    let start = tuple.intermediate_start();
    let end = tuple.intermediate_end();
    let mut scalar = 1f64;
    for (i, coord) in coords.iter().enumerate() {
        let v = coord.to_bits() as i32;
        let p = peak.get(i)?.to_bits() as i32;
        if p == 0 || v == p {
            continue;
        }
        if v == 0 {
            return None;
        }
        if let (Some(start), Some(end)) = (&start, &end) {
            let s = start.get(i)?.to_bits() as i32;
            let e = end.get(i)?.to_bits() as i32;
            if s > p || p > e || (s < 0 && e > 0) {
                continue;
            }
            if v < s || v > e {
                return None;
            }
            if v < p {
                scalar *= (v - s) as f64 / (p - s) as f64;
            } else {
                scalar *= (e - v) as f64 / (e - p) as f64;
            }
        } else {
            if v < p.min(0) || v > p.max(0) {
                return None;
            }
            scalar *= v as f64 / p as f64;
        }
    }
    Some(scalar as f32)
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
    curve_flags: Vec<PointFlags>,
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
                    deltas[i][axis] = interpolate(
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

fn interpolate(v: f32, a: f32, b: f32, da: f32, db: f32) -> f32 {
    if a == b {
        return if da == db { da } else { 0. };
    }
    if v <= a.min(b) {
        if a < b {
            da
        } else {
            db
        }
    } else if v >= a.max(b) {
        if a > b {
            da
        } else {
            db
        }
    } else {
        let r = (v - a) / (b - a);
        da + r * (db - da)
    }
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

pub(super) fn instance(
    font: &FontRef,
    axes: &AxisPlan,
    tables: &mut Tables,
    optimize_iup: bool,
) -> Result<(), SubsetError> {
    let glyf = font.glyf().map_err(|_| error(b"glyf"))?;
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
            .get(gid, &glyf)
            .ok_or_else(|| error(b"glyf"))?
            .into_glyph();
        let mut points = Vec::new();
        let mut contours = Vec::new();
        let mut curve_flags = Vec::new();
        let glyph = match source {
            None => Glyph::Empty,
            Some(ReadGlyph::Simple(g)) => {
                let mut raw_points =
                    vec![write_fonts::types::Point::<i32>::default(); g.num_points()];
                curve_flags.resize(g.num_points(), PointFlags::default());
                g.read_points_fast(&mut raw_points, &mut curve_flags)
                    .map_err(|_| error(b"glyf"))?;
                points.extend(raw_points.iter().map(|p| [p.x as f32, p.y as f32]));
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
        let top =
            bbox.y_max as f32 + vmtx.as_ref().and_then(|v| v.side_bearing(gid)).unwrap_or(0) as f32;
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
                    let Some(scalar) = tuple_scalar(&tuple, &axes.metric_coords) else {
                        continue;
                    };
                    let deltas = calc_inferred_deltas(&tuple, &orig_points, &contours, scalar)?;
                    if tuple.has_deltas_for_all_points() && !private {
                        add_deltas(&mut points, &deltas);
                        continue;
                    }
                    if !tuple.has_deltas_for_all_points() {
                        if flush {
                            add_deltas(&mut points, &pending);
                        }
                        pending.fill([0.; 2]);
                    }
                    add_deltas(&mut pending, &deltas);
                    flush = true;
                }
                if flush {
                    add_deltas(&mut points, &pending);
                }
            }
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
            curve_flags,
            phantoms,
        });
    }
    let mut bounds = Vec::with_capacity(count);
    for gid in 0..count {
        let mut budget = 1_000_000usize;
        let points = get_points(gid, &glyphs, 0, &mut budget)?;
        bounds.push(point_bounds(&points));
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
                encode_simple(simple, &g.points, &g.curve_flags, &mut glyf_out)?;
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
                glyf_out.extend(write_fonts::dump_table(composite).map_err(|_| error(b"glyf"))?);
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
    tables.insert(Tag::new(b"glyf"), glyf_out);
    tables.insert(Tag::new(b"loca"), loca_out);
    if axes.all_pinned() {
        tables.remove(&Tag::new(b"gvar"));
    } else if gvar.is_some() {
        tables.insert(
            Tag::new(b"gvar"),
            compile_gvar(
                &variations,
                axes.pinned.iter().filter(|&&p| !p).count() as u16,
            )?,
        );
    }
    let head = tables
        .get_mut(&Tag::new(b"head"))
        .ok_or_else(|| error(b"head"))?;
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
    instance_cvar(font, axes, tables)?;
    metrics::finish(font, axes, tables)
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
        let mut child = get_points(component.glyph.to_u16() as usize, glyphs, depth + 1, budget)?;
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

fn encode_simple(
    g: &write_fonts::tables::glyf::SimpleGlyph,
    points: &[Point],
    curve_flags: &[PointFlags],
    out: &mut Vec<u8>,
) -> Result<(), SubsetError> {
    if g.contours.is_empty() {
        return Ok(());
    }
    out.extend(
        i16::try_from(g.contours.len())
            .map_err(|_| error(b"glyf"))?
            .to_be_bytes(),
    );
    for v in [g.bbox.x_min, g.bbox.y_min, g.bbox.x_max, g.bbox.y_max] {
        out.extend(v.to_be_bytes());
    }
    let mut count = 0usize;
    for c in &g.contours {
        count += c.len();
        out.extend(
            u16::try_from(count - 1)
                .map_err(|_| error(b"glyf"))?
                .to_be_bytes(),
        );
    }
    out.extend(
        u16::try_from(g.instructions.len())
            .map_err(|_| error(b"glyf"))?
            .to_be_bytes(),
    );
    out.extend(&g.instructions);
    // HarfBuzz's simple-glyph compiler preserves the curve-kind bits while
    // rebuilding coordinate and repeat flags. The owned glyph representation
    // only stores on-curve booleans, so retain these bits from the reader.
    for (i, flags) in curve_flags.iter().enumerate() {
        out.push(
            u8::from(flags.is_on_curve())
                | if flags.is_off_curve_cubic() { 0x80 } else { 0 }
                | if i == 0 && g.overlaps { 0x40 } else { 0 },
        );
    }
    for axis in 0..2 {
        let mut prev = 0i16;
        for point in points {
            let v = coord(point[axis]);
            out.extend(v.wrapping_sub(prev).to_be_bytes());
            prev = v;
        }
    }
    Ok(())
}

fn write_metrics(
    font: &FontRef,
    glyphs: &[InstanceGlyph],
    bounds: &[Option<Bbox>],
    tables: &mut Tables,
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
        let h = tables
            .get_mut(&Tag::new(header))
            .ok_or_else(|| error(header))?;
        put(h, 10, max_advance as u16 as i16)?;
        if min_leading != i32::MAX {
            for (offset, value) in [(12, min_leading), (14, min_trailing), (16, max_extent)] {
                put(h, offset, value.clamp(-32768, 32767) as i16)?;
            }
        }
        put(h, 34, glyphs.len() as u16 as i16)?;
        tables.insert(Tag::new(tag), out);
    }
    Ok(())
}

fn instance_cvar(font: &FontRef, axes: &AxisPlan, tables: &mut Tables) -> Result<(), SubsetError> {
    let Some(bytes) = font.data_for_tag(Tag::new(b"cvar")) else {
        return Ok(());
    };
    let cvar = write_fonts::read::tables::cvar::Cvar::read(bytes).map_err(|_| error(b"cvar"))?;
    let cvt = tables
        .get_mut(&Tag::new(b"cvt "))
        .ok_or_else(|| error(b"cvt "))?;
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
        let Some(scalar) = tuple_scalar(&tuple, &axes.metric_coords) else {
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
        tables.remove(&Tag::new(b"cvar"));
    } else {
        residual.instantiate(font, axes, None, None)?;
        let data = residual.compile_bytes(&BTreeMap::new(), false)?;
        if data.is_empty() {
            tables.remove(&Tag::new(b"cvar"));
        } else {
            let mut bytes = vec![0, 1, 0, 0];
            bytes.extend(data);
            tables.insert(Tag::new(b"cvar"), bytes);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use write_fonts::tables::glyf::{Component, ComponentFlags, CompositeGlyph, Transform};

    #[test]
    fn cyclic_composites_return_an_error() {
        let glyph = InstanceGlyph {
            glyph: Glyph::Composite(CompositeGlyph::new(
                Component::new(
                    write_fonts::types::GlyphId16::new(0),
                    Anchor::Offset { x: 0, y: 0 },
                    Transform::default(),
                    ComponentFlags::default(),
                ),
                Bbox::default(),
            )),
            points: vec![[0.; 2]],
            curve_flags: Vec::new(),
            phantoms: [[0.; 2]; 4],
        };
        assert!(get_points(0, &[glyph], 0, &mut 1000).is_err());
    }

    #[test]
    fn coincident_iup_references_with_different_deltas_do_not_move_the_gap() {
        assert_eq!(interpolate(100., 50., 50., 10., 20.), 0.);
        assert_eq!(interpolate(100., 50., 50., 10., 10.), 10.);
        assert_eq!(interpolate(100., 200., 0., 20., 0.), 10.);
    }
}
